//! Resumable, verified download.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::{ModelEntry, ModelError};

/// Cancellation flag shared with the UI.
pub type Cancel = Arc<AtomicBool>;

/// Download progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
}

impl std::fmt::Debug for Fetched {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fetched(resumed {})", self.resumed)
    }
}

/// A response body that starts at the byte the caller asked for (when `resumed`).
pub struct Fetched {
    /// The server honoured the Range request (206); otherwise the body starts at byte 0.
    pub resumed: bool,
    pub body: Box<dyn Read>,
}

/// HTTP abstraction so the logic is testable without a network.
pub trait Transport {
    /// GET `url` starting at byte `from` (0 = whole file).
    fn get(&self, url: &str, from: u64) -> Result<Fetched, String>;
}

/// SHA-256 of a file, lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Path of the partial file next to `dest`.
pub fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Download `entry` to `dest` (a file). Resumes `dest.part` when present; verifies size and SHA-256;
/// on a hash mismatch the partial data is deleted and `HashMismatch` is returned. On success `dest`
/// is atomically renamed into place.
pub fn download(
    entry: &ModelEntry,
    dest: &Path,
    transport: &dyn Transport,
    cancel: &Cancel,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), ModelError> {
    let id = entry.id.clone();
    let fail = |message: String| ModelError::Download {
        id: id.clone(),
        message,
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = part_path(dest);
    let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if have > entry.size {
        std::fs::remove_file(&part)?;
        have = 0;
    }
    if have < entry.size {
        let fetched = transport.get(&entry.url, have).map_err(&fail)?;
        let mut body = fetched.body;
        let mut file = if fetched.resumed && have > 0 {
            OpenOptions::new().append(true).open(&part)?
        } else {
            have = 0;
            File::create(&part)?
        };
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            if cancel.load(Ordering::SeqCst) {
                file.flush()?;
                return Err(ModelError::Cancelled(id)); // the .part stays for a later resume
            }
            let n = body.read(&mut buf).map_err(|e| fail(e.to_string()))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            have += n as u64;
            on_progress(Progress {
                downloaded: have,
                total: entry.size,
            });
            if have > entry.size {
                break; // more data than expected: caught by the size check below
            }
        }
        file.flush()?;
    }
    let got_size = std::fs::metadata(&part)?.len();
    if got_size != entry.size {
        if got_size > entry.size {
            std::fs::remove_file(&part)?;
        }
        return Err(ModelError::SizeMismatch {
            id,
            expected: entry.size,
            got: got_size,
        });
    }
    let got = sha256_file(&part)?;
    if got != entry.sha256 {
        std::fs::remove_file(&part)?;
        let _ = std::fs::remove_file(dest);
        return Err(ModelError::HashMismatch {
            id,
            expected: entry.sha256.clone(),
            got,
        });
    }
    std::fs::rename(&part, dest)?;
    on_progress(Progress {
        downloaded: entry.size,
        total: entry.size,
    });
    Ok(())
}

/// reqwest (rustls) blocking transport with HTTP Range support.
#[cfg(feature = "http")]
#[derive(Debug, Default)]
pub struct HttpTransport {
    client: Option<reqwest::blocking::Client>,
}

#[cfg(feature = "http")]
impl HttpTransport {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("pando-assistant/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(20))
            .build()
            .ok();
        Self { client }
    }
}

#[cfg(feature = "http")]
impl Transport for HttpTransport {
    fn get(&self, url: &str, from: u64) -> Result<Fetched, String> {
        let client = self.client.as_ref().ok_or("no HTTP client")?;
        let mut req = client.get(url);
        if from > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={from}-"));
        }
        let resp = req.send().map_err(|e| e.to_string())?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status} for {url}"));
        }
        Ok(Fetched {
            resumed: status == reqwest::StatusCode::PARTIAL_CONTENT,
            body: Box::new(resp),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::Mutex;

    /// Serves `data`; records the `from` of every request.
    pub struct Fake {
        pub data: Vec<u8>,
        pub range_support: bool,
        pub requests: Mutex<Vec<u64>>,
        /// Cut the first response after this many bytes (simulated dropped connection).
        pub cut_after: Mutex<Option<usize>>,
    }

    impl Fake {
        pub fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                range_support: true,
                requests: Mutex::default(),
                cut_after: Mutex::default(),
            }
        }
    }

    impl Transport for Fake {
        fn get(&self, _url: &str, from: u64) -> Result<Fetched, String> {
            self.requests.lock().expect("lock").push(from);
            let (resumed, start) = if from > 0 && self.range_support {
                (true, from as usize)
            } else {
                (false, 0)
            };
            let mut body = self.data[start..].to_vec();
            if let Some(n) = self.cut_after.lock().expect("lock").take() {
                body.truncate(n);
            }
            Ok(Fetched {
                resumed,
                body: Box::new(Cursor::new(body)),
            })
        }
    }

    pub fn entry_for(data: &[u8]) -> ModelEntry {
        let mut h = Sha256::new();
        h.update(data);
        ModelEntry {
            id: "t".into(),
            kind: "stt".into(),
            title: "t".into(),
            url: "https://example.invalid/t".into(),
            file: Some("t.bin".into()),
            archive: None,
            sha256: hex(&h.finalize()),
            size: data.len() as u64,
            license: "MIT".into(),
            attribution: String::new(),
            bundled: false,
            required_files: vec![],
        }
    }

    fn no_cancel() -> Cancel {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn downloads_and_verifies() {
        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("m/t.bin");
        let fake = Fake::new(data.clone());
        let mut last = None;
        download(&entry_for(&data), &dest, &fake, &no_cancel(), &mut |p| {
            last = Some(p)
        })
        .expect("ok");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
        assert!(!part_path(&dest).exists());
        assert_eq!(
            last,
            Some(Progress {
                downloaded: 1_000_000,
                total: 1_000_000
            })
        );
    }

    #[test]
    fn wrong_hash_deletes_the_file_and_reports_clearly() {
        let data = vec![7u8; 5000];
        let mut e = entry_for(&data);
        e.sha256 = "0".repeat(64);
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("t.bin");
        let err = download(&e, &dest, &Fake::new(data), &no_cancel(), &mut |_| {})
            .expect_err("must fail");
        assert!(matches!(err, ModelError::HashMismatch { .. }), "{err}");
        assert!(err.to_string().contains("deleted"));
        assert!(!dest.exists() && !part_path(&dest).exists());
    }

    #[test]
    fn resumes_a_partial_download() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 199) as u8).collect();
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("t.bin");
        let e = entry_for(&data);
        let fake = Fake::new(data.clone());
        *fake.cut_after.lock().expect("lock") = Some(100_000);
        let first = download(&e, &dest, &fake, &no_cancel(), &mut |_| {});
        assert!(
            matches!(first, Err(ModelError::SizeMismatch { got: 100_000, .. })),
            "{first:?}"
        );
        assert_eq!(
            std::fs::metadata(part_path(&dest)).expect("part").len(),
            100_000
        );
        download(&e, &dest, &fake, &no_cancel(), &mut |_| {}).expect("resume");
        assert_eq!(*fake.requests.lock().expect("lock"), vec![0, 100_000]);
        assert_eq!(std::fs::read(&dest).expect("read"), data);
    }

    #[test]
    fn server_without_range_restarts_cleanly() {
        let data = vec![9u8; 50_000];
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("t.bin");
        std::fs::write(part_path(&dest), &data[..10_000]).expect("seed part");
        let mut fake = Fake::new(data.clone());
        fake.range_support = false;
        download(&entry_for(&data), &dest, &fake, &no_cancel(), &mut |_| {}).expect("ok");
        assert_eq!(std::fs::read(&dest).expect("read"), data);
    }

    #[test]
    fn corrupt_partial_is_detected_by_the_hash() {
        let data = vec![1u8; 20_000];
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("t.bin");
        std::fs::write(part_path(&dest), vec![2u8; 5_000]).expect("seed part"); // wrong bytes
        let err = download(
            &entry_for(&data),
            &dest,
            &Fake::new(data),
            &no_cancel(),
            &mut |_| {},
        )
        .expect_err("hash must fail");
        assert!(matches!(err, ModelError::HashMismatch { .. }));
        assert!(!part_path(&dest).exists());
    }

    #[test]
    fn cancel_keeps_the_part_file() {
        let data = vec![3u8; 2_000_000];
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("t.bin");
        let cancel: Cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let err = download(
            &entry_for(&data),
            &dest,
            &Fake::new(data),
            &cancel,
            &mut |p| {
                if p.downloaded > 300_000 {
                    c2.store(true, Ordering::SeqCst);
                }
            },
        )
        .expect_err("cancelled");
        assert!(matches!(err, ModelError::Cancelled(_)));
        assert!(part_path(&dest).exists());
    }

    #[test]
    fn oversized_response_is_rejected() {
        let data = vec![4u8; 1000];
        let mut e = entry_for(&data);
        e.size = 500;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("t.bin");
        let err =
            download(&e, &dest, &Fake::new(data), &no_cancel(), &mut |_| {}).expect_err("size");
        assert!(matches!(err, ModelError::SizeMismatch { .. }));
        assert!(!dest.exists());
    }
}
