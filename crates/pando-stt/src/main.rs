//! `pando-stt`: speech-to-text worker. Speaks `voice-proto` frames on stdin/stdout; logs go to stderr.
//! Never write anything else to stdout.

use std::io::{BufReader, BufWriter};

use pando_stt::stt::Stt;
use pando_stt::{SttError, Whistle, server};

fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("pando-stt {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut r = BufReader::new(stdin.lock());
    let mut w = BufWriter::new(stdout.lock());
    let mut load = |p: &std::path::Path| -> Result<Box<dyn Stt>, SttError> {
        Ok(Box::new(Whistle::load_file(p)?))
    };
    if let Err(e) = server::serve(&mut r, &mut w, &mut load) {
        eprintln!("pando-stt: protocol error: {e}");
        std::process::exit(1);
    }
}
