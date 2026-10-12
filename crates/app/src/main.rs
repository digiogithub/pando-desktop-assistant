//! Entry point of `pando-assistant`.

use clap::Parser as _;

fn main() -> anyhow::Result<()> {
    let args = app::cli::Args::parse();
    if let Some(cmd) = args.command {
        return app::app::run_command(cmd);
    }
    app::app::run(&args)
}
