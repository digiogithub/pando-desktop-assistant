//! `pando-tts`: text-to-speech worker (GPL-3 as a distributed binary, ADR 0007).
//! Speaks `voice-proto` frames on stdin/stdout; logs go to stderr. Never write anything else to stdout.

use std::io::{BufReader, BufWriter};

use pando_tts::{SherpaTts, Tts, TtsError, server};

fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("pando-tts {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let stdin = std::io::stdin();
    let mut r = BufReader::new(stdin.lock());
    let w = BufWriter::new(std::io::stdout());
    let load: Box<server::Loader> = Box::new(|dir, voice| -> Result<Box<dyn Tts>, TtsError> {
        Ok(Box::new(SherpaTts::load(dir, voice, 2)?))
    });
    if let Err(e) = server::serve(&mut r, Box::new(w), load) {
        eprintln!("pando-tts: protocol error: {e}");
        std::process::exit(1);
    }
}
