//! Application startup: directories, logging, settings, i18n, runtime and the main window.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Context as _;

use crate::cli::Args;
use crate::organism_view::demo::DemoView;
use crate::paths::AppDirs;
use crate::settings::{AppSettings, LoadOutcome};
use crate::ui::{self, AppContext as _};
use crate::views::main_view::MainView;
use crate::views::title_bar;
use crate::{fonts, i18n, logging, tokio_bridge};
use rust_i18n::t;

/// How long `--smoke-test` waits for the frames before failing.
const SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs the app until the last window closes (or, with `--smoke-test`, two frames are drawn).
pub fn run(args: &Args) -> anyhow::Result<()> {
    let started = Instant::now();
    let smoke_root = args.smoke_test.then(|| {
        std::env::temp_dir().join(format!("pando-assistant-smoke-{}", std::process::id()))
    });
    let dirs = match &smoke_root {
        Some(root) => AppDirs::with_root(root),
        None => AppDirs::from_project_dirs()?,
    };
    dirs.ensure()?;
    // Held until the end of `run` so the buffered log lines are flushed.
    let _log_guard = logging::init(&dirs.log_dir, args.log_level.as_deref())
        .map_err(|err| eprintln!("logging disabled: {err}"))
        .ok();
    logging::log_startup();

    let (settings, outcome) = AppSettings::load_or_default(&dirs.settings_file());
    match &outcome {
        LoadOutcome::Loaded | LoadOutcome::Missing => {}
        LoadOutcome::Recovered { backup } => {
            tracing::warn!(?backup, "settings.json was corrupt; started with defaults");
        }
    }
    // Tests of the smoke run must not depend on the OS language.
    i18n::apply(if args.smoke_test {
        Some("es")
    } else {
        settings.language.as_deref()
    });

    let failure: Rc<RefCell<Option<anyhow::Error>>> = Rc::default();
    let startup_failure = failure.clone();
    let smoke = args.smoke_test;
    let demo = args.demo;
    ui::application().run(move |cx| {
        if let Err(err) = start(cx, &settings, smoke, demo, started) {
            tracing::error!("startup failed: {err:#}");
            *startup_failure.borrow_mut() = Some(err);
            cx.quit();
        }
    });

    if let Some(root) = smoke_root {
        let _ = std::fs::remove_dir_all(root);
    }
    match failure.take() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Initializes the GPUI layer and opens the main window.
fn start(
    cx: &mut ui::App,
    settings: &AppSettings,
    smoke: bool,
    demo: bool,
    started: Instant,
) -> anyhow::Result<()> {
    ui::init(cx);
    crate::organism_view::bind_keys(cx);
    crate::cells::pane::bind_keys(cx);
    ui::kit_theme::install(cx);
    cx.set_reduce_motion(settings.reduce_motion);
    fonts::register(cx);
    tokio_bridge::init(cx).context("starting the tokio runtime")?;

    let tokio_handle = tokio_bridge::handle(cx);
    let pando_url = settings.pando_url.clone();
    let bounds = ui::Bounds::centered(
        None,
        ui::size(
            ui::px(title_bar::DEFAULT_SIZE.0),
            ui::px(title_bar::DEFAULT_SIZE.1),
        ),
        cx,
    );
    let options = title_bar::main_window_options(
        t!("window.title").to_string(),
        bounds,
        ui::size(ui::px(title_bar::MIN_SIZE.0), ui::px(title_bar::MIN_SIZE.1)),
    );
    let reduced_motion = settings.reduce_motion;
    let handle = if demo {
        ui::open_main_window(options, cx, |window, cx| {
            cx.new(|cx| DemoView::new(window, cx, reduced_motion))
        })
        .context("opening the demo window")?
        .0
    } else {
        ui::open_main_window(options, cx, |window, cx| {
            cx.new(|cx| MainView::new(window, cx, tokio_handle, pando_url))
        })
        .context("opening the main window")?
        .0
    };
    tracing::info!("window opened");

    handle.update(cx, |_, window, cx| {
        if let Some(specs) = window.gpu_specs() {
            tracing::info!(
                device = %specs.device_name,
                driver = %specs.driver_name,
                software = specs.is_software_emulated,
                "gpu_specs"
            );
        }
        if smoke {
            // Watchdog: a compositor that never delivers frames must fail the smoke test
            // instead of hanging.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(SMOKE_TIMEOUT).await;
                tracing::error!("smoke test: no frames rendered within {SMOKE_TIMEOUT:?}");
                std::process::exit(2);
            })
            .detach();
            // Two frames, so the first real paint has happened.
            window.on_next_frame(move |window, _| {
                window.on_next_frame(move |_, cx| {
                    tracing::info!(
                        "two frames rendered in {} ms",
                        started.elapsed().as_millis()
                    );
                    cx.quit();
                });
            });
        }
    })?;
    Ok(())
}
