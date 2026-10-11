//! Entry point of `pando-assistant`.

use clap::Parser as _;

fn main() -> anyhow::Result<()> {
    app::app::run(&app::cli::Args::parse())
}
