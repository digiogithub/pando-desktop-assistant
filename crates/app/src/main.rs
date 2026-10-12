//! Entry point of `pando-assistant`.

use clap::Parser as _;

fn main() -> anyhow::Result<()> {
    // Velopack install/update hooks must run before anything else.
    app::update::init();
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if app::decision::eval_cli::handles(&raw) {
        let log = app::paths::AppDirs::from_project_dirs()
            .map(|d| {
                decide::log::DecisionLog::in_dir(&d.data_dir)
                    .path()
                    .to_path_buf()
            })
            .unwrap_or_default();
        return match app::decision::eval_cli::run(&raw, &log) {
            Ok(report) => {
                println!("{report}");
                Ok(())
            }
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(2);
            }
        };
    }
    let args = app::cli::Args::parse();
    if let Some(cmd) = args.command {
        return app::app::run_command(cmd);
    }
    app::app::run(&args)
}
