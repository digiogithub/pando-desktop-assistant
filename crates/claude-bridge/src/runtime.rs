//! Runtime files: random port and shared secret, written with mode 0600.
//!
//! Two files live in a private (0700) directory:
//! * `claude-bridge.port`: `port=<n>` and `pid=<n>` lines, read by the forwarder script;
//! * `claude-bridge.headers`: `X-Pando-Hook-Secret: <hex>`, passed to `curl -H @file` so the
//!   secret never appears in a process command line.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Name of the shared-secret request header.
pub const SECRET_HEADER: &str = "X-Pando-Hook-Secret";
/// Header the forwarder uses to pass `CLAUDE_CODE_MESSAGING_SOCKET`.
pub const SOCKET_HEADER: &str = "X-Claude-Msg-Socket";
/// Header the forwarder uses to pass `CLAUDE_CODE_MESSAGING_TOKEN`.
pub const TOKEN_HEADER: &str = "X-Claude-Msg-Token";

/// Locations of the runtime files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFiles {
    dir: PathBuf,
}

impl RuntimeFiles {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn port_file(&self) -> PathBuf {
        self.dir.join("claude-bridge.port")
    }

    pub fn headers_file(&self) -> PathBuf {
        self.dir.join("claude-bridge.headers")
    }

    /// Write both files (creating the directory 0700). Returns an error if any step fails.
    pub fn write(&self, port: u16, secret: &str) -> std::io::Result<()> {
        create_private_dir(&self.dir)?;
        write_private(
            &self.port_file(),
            format!("port={port}\npid={}\n", std::process::id()).as_bytes(),
        )?;
        write_private(
            &self.headers_file(),
            format!("{SECRET_HEADER}: {secret}\n").as_bytes(),
        )
    }

    /// Remove both files (best effort).
    pub fn remove(&self) {
        let _ = std::fs::remove_file(self.port_file());
        let _ = std::fs::remove_file(self.headers_file());
    }

    /// Read the port back (used by tests and diagnostics).
    pub fn read_port(&self) -> Option<u16> {
        std::fs::read_to_string(self.port_file())
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("port=")?.trim().parse().ok())
    }
}

/// 32 random bytes as 64 hex chars.
pub fn generate_secret() -> Result<String, getrandom::Error> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Atomically write `data` to `path` with mode 0600 (temp file + rename).
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_private_files() {
        let tmp = tempfile::tempdir().unwrap();
        let rt = RuntimeFiles::new(tmp.path().join("rt"));
        rt.write(4242, "abcd").unwrap();
        assert_eq!(rt.read_port(), Some(4242));
        let headers = std::fs::read_to_string(rt.headers_file()).unwrap();
        assert_eq!(headers, "X-Pando-Hook-Secret: abcd\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for p in [rt.port_file(), rt.headers_file()] {
                assert_eq!(
                    std::fs::metadata(p).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            assert_eq!(
                std::fs::metadata(rt.dir()).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        rt.remove();
        assert_eq!(rt.read_port(), None);
    }

    #[test]
    fn secrets_are_random_hex() {
        let a = generate_secret().unwrap();
        let b = generate_secret().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
