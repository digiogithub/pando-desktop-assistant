//! `pando-models`: command line over the model store. The app forwards
//! `pando-assistant models list|import|verify` to the same library calls.
//!
//! usage: pando-models list | verify [id] [--deep] | import <dir> | download <id> | path <id>

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use models::{HttpTransport, ModelState, ModelStore};

fn human(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.1} GiB", n as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} MiB", n as f64 / (1u64 << 20) as f64)
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("pando-models: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let store = ModelStore::open().map_err(|e| e.to_string())?;
    match args.first().map(String::as_str) {
        Some("list") | None => {
            println!("models directory: {}", store.root().display());
            for (e, s) in store.list() {
                let st = match s {
                    ModelState::Ok => "installed".to_string(),
                    ModelState::Missing => "missing".to_string(),
                    ModelState::Corrupt(why) => format!("CORRUPT ({why})"),
                };
                println!(
                    "{:<30} {:<5} {:>10}  {:<10} {}",
                    e.id,
                    e.kind,
                    human(e.size),
                    e.license,
                    st
                );
            }
            Ok(())
        }
        Some("verify") => {
            let deep = args.iter().any(|a| a == "--deep");
            let ids: Vec<String> = match args.get(1).filter(|a| !a.starts_with("--")) {
                Some(id) => vec![id.clone()],
                None => store
                    .manifest()
                    .models
                    .iter()
                    .map(|m| m.id.clone())
                    .collect(),
            };
            let mut bad = false;
            for id in ids {
                let s = store.verify(&id, deep).map_err(|e| e.to_string())?;
                println!("{id}: {s:?}");
                bad |= matches!(s, ModelState::Corrupt(_));
            }
            if bad {
                Err("some models are corrupt".into())
            } else {
                Ok(())
            }
        }
        Some("import") => {
            let dir = args.get(1).ok_or("import needs a folder")?;
            let results = store.import(std::path::Path::new(dir));
            if results.is_empty() {
                return Err(format!("no known model files found in {dir}"));
            }
            let mut bad = false;
            for (id, r) in results {
                match r {
                    Ok(()) => println!("{id}: imported"),
                    Err(e) => {
                        bad = true;
                        eprintln!("{id}: {e}");
                    }
                }
            }
            if bad {
                Err("some imports failed".into())
            } else {
                Ok(())
            }
        }
        Some("download") => {
            let id = args.get(1).ok_or("download needs a model id")?;
            let cancel = Arc::new(AtomicBool::new(false));
            let mut last = 0u64;
            let p = store
                .install(id, &HttpTransport::new(), &cancel, &mut |p| {
                    if p.downloaded - last > 8 << 20 || p.downloaded == p.total {
                        last = p.downloaded;
                        eprintln!("{id}: {} / {}", human(p.downloaded), human(p.total));
                    }
                })
                .map_err(|e| e.to_string())?;
            println!("{}", p.display());
            Ok(())
        }
        Some("path") => {
            let id = args.get(1).ok_or("path needs a model id")?;
            println!("{}", store.path(id).map_err(|e| e.to_string())?.display());
            Ok(())
        }
        Some(other) => Err(format!(
            "unknown command `{other}` (list, verify, import, download, path)"
        )),
    }
}
