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
    // A `.cact` file is Whistle; a directory is a Parakeet model (feature `parakeet`).
    let mut load = |p: &std::path::Path| -> Result<Box<dyn Stt>, SttError> {
        if p.is_dir() {
            #[cfg(feature = "parakeet")]
            return Ok(Box::new(pando_stt::parakeet::Parakeet::load(p, 4)?));
            #[cfg(not(feature = "parakeet"))]
            return Err(SttError::Unsupported(
                "Parakeet (build pando-stt with --features parakeet)",
            ));
        }
        Ok(Box::new(Whistle::load_file(p)?))
    };
    if let Err(e) = server::serve(&mut r, &mut w, &mut load) {
        eprintln!("pando-stt: protocol error: {e}");
        std::process::exit(1);
    }
}
