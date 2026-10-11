//! Throwaway spikes for PDA-T-0024 (paths perf), PDA-T-0025 (mini window), PDA-T-0026 (hotkey+tray).
//! Build with `CARGO_TARGET_DIR=target/spikes`. Not part of the product workspace.

mod hotkey;
mod mini;
mod paths;
mod scene;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("paths") => paths::run(&args[1..]),
        Some("skia") => paths::run_skia(&args[1..]),
        Some("hotkey") => hotkey::run(&args[1..]),
        Some("mini") => mini::run(&args[1..]),
        _ => anyhow::bail!("usage: gpui-shell-spikes <paths|skia|mini> [options]"),
    }
}
