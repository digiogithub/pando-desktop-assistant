//! Application startup: directories, logging, settings, i18n, runtime and the main window.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Context as _;

use crate::cli::Args;
use crate::organism_view::demo::DemoView;
use crate::paths::AppDirs;
use crate::settings::{AppSettings, LoadOutcome};
use crate::shell::controller::Msg;
use crate::shell::ipc;
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
    // Single instance: a second launch focuses the first and exits. Not for the smoke test
    // (throwaway root) nor the demo window.
    let (shell_tx, shell_rx) = async_channel::unbounded::<Msg>();
    let mut _instance = None;
    let shell_enabled = !smoke && !demo;
    if shell_enabled {
        match ipc::acquire(&dirs.data_dir, shell_tx.clone()) {
            Ok(ipc::Acquired::Primary(guard)) => _instance = Some(guard),
            Ok(ipc::Acquired::Secondary) => {
                let _ = ipc::send(&dirs.data_dir, &ipc::Command::Focus);
                tracing::info!("another instance is running; asked it to focus");
                return Ok(());
            }
            Err(err) => tracing::warn!("single-instance lock unavailable: {err}"),
        }
    }
    let shell = shell_enabled.then(|| ShellInit {
        inbox: shell_tx,
        rx: shell_rx,
        settings_path: dirs.settings_file(),
        keymap_path: dirs.config_dir.join("keymap.json"),
        minimized: args.minimized || settings.shell.start_minimized,
    });
    ui::application().run(move |cx| {
        if let Err(err) = start(cx, &settings, smoke, demo, shell, started) {
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

/// What the desktop shell needs at startup.
struct ShellInit {
    inbox: async_channel::Sender<Msg>,
    rx: async_channel::Receiver<Msg>,
    settings_path: std::path::PathBuf,
    keymap_path: std::path::PathBuf,
    minimized: bool,
}

/// Opens the main (or demo) window.
fn open_main(
    cx: &mut ui::App,
    settings: &AppSettings,
    demo: bool,
    tokio_handle: tokio::runtime::Handle,
) -> anyhow::Result<ui::AnyWindowHandle> {
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
    let pando_url = settings.pando_url.clone();
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
    Ok(handle)
}

/// Initializes the GPUI layer, the desktop shell and opens the main window.
fn start(
    cx: &mut ui::App,
    settings: &AppSettings,
    smoke: bool,
    demo: bool,
    shell: Option<ShellInit>,
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
    let mut start_hidden = false;
    if let Some(init) = shell {
        start_hidden = init.minimized;
        let shared = Rc::new(RefCell::new(settings.clone()));
        let (opener_settings, opener_handle) = (settings.clone(), tokio_handle.clone());
        let open_main_window: crate::shell::runtime::MainOpener = Rc::new(move |cx| {
            open_main(cx, &opener_settings, false, opener_handle.clone())
                .map_err(|e| tracing::error!("cannot open the main window: {e:#}"))
                .ok()
        });
        let (out_tx, out_rx) = async_channel::unbounded::<crate::shell::controller::Output>();
        // The voice orchestrator will consume these; until then they are only logged.
        cx.spawn(async move |_cx| {
            while let Ok(o) = out_rx.recv().await {
                tracing::info!(?o, "shell output");
            }
        })
        .detach();
        let fallback_inbox = init.inbox.clone();
        crate::shell::runtime::install(
            cx,
            init.rx,
            crate::shell::runtime::Params {
                inbox: init.inbox,
                settings: shared,
                settings_path: init.settings_path,
                open_main: open_main_window,
                outputs: out_tx,
                keymap_path: init.keymap_path,
            },
        );
        if start_hidden {
            // No tray within a few seconds means nobody could ever bring the window back.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(8)).await;
                cx.update(|cx| {
                    if !crate::shell::runtime::background_mode(cx) {
                        let _ = fallback_inbox.try_send(Msg::Command(ipc::Command::Show));
                    }
                });
            })
            .detach();
        }
    }
    if start_hidden {
        return Ok(());
    }
    let handle = open_main(cx, settings, demo, tokio_handle)?;

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

/// `pando-assistant ptt|show|hide|toggle-mini|quit`: sends the command to the running
/// instance (exit code 1 when none runs).
pub fn run_command(cmd: crate::cli::Cmd) -> anyhow::Result<()> {
    let dirs = AppDirs::from_project_dirs()?;
    match ipc::send(&dirs.data_dir, &cmd.to_ipc()) {
        Ok(reply) if reply.ok => Ok(()),
        Ok(reply) => anyhow::bail!(
            "the running instance refused the command: {}",
            reply.error.unwrap_or_default()
        ),
        Err(err) => anyhow::bail!("Pando Assistant is not running ({err})"),
    }
}
