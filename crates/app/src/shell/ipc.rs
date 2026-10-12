//! Single-instance lock and local IPC (PDA-T-0124, PDA-T-0122).
//!
//! The first instance listens on `127.0.0.1:<ephemeral>` and writes `{port, token, pid}` to
//! `instance.json` in the data directory (mode 0600 on Unix). Anything that wants to talk to it
//! (a second launch, `pando-assistant ptt start`, a compositor binding) reads that file and sends
//! one JSON line carrying the token. Std only: no tokio, no GPUI, so a CLI invocation starts in
//! milliseconds. A stale file (the owner died) is detected by a failed connection and replaced.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Longest accepted request line.
const MAX_LINE: u64 = 16 * 1024;
/// Connect / read timeout of a client.
const TIMEOUT: Duration = Duration::from_secs(2);

/// What a hold-to-talk trigger asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PttAction {
    /// Start listening (key pressed).
    Start,
    /// Stop listening (key released).
    Stop,
    /// Flip.
    Toggle,
}

/// A command for the running instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Push-to-talk.
    Ptt {
        /// Which edge.
        action: PttAction,
    },
    /// Show and focus the main window (what a second launch sends).
    Focus,
    /// Show the main window.
    Show,
    /// Hide the main window.
    Hide,
    /// Show / hide the floating mini window.
    ToggleMini,
    /// Quit the application.
    Quit,
    /// Show the main window and select a cell (clicking an OS notification).
    FocusCell {
        /// Cell id.
        id: String,
    },
    /// Open the settings window.
    OpenSettings,
    /// Open the first-run assistant.
    OpenFirstRun,
    /// Hands-free listening on / off (tray).
    ToggleHandsFree,
    /// Mute / unmute the assistant's voice (tray).
    ToggleMute,
    /// Liveness probe.
    Ping,
}

/// Reply of the running instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    /// The command was accepted.
    pub ok: bool,
    /// Why not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Lock {
    port: u16,
    token: String,
    pid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    token: String,
    command: Command,
}

/// Path of the lock file inside `dir`.
#[must_use]
pub fn lock_path(dir: &Path) -> PathBuf {
    dir.join("instance.json")
}

fn random_token() -> String {
    use std::hash::{BuildHasher as _, Hasher as _, RandomState};
    let mut out = String::new();
    for i in 0..4_u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(i);
        h.write_u32(std::process::id());
        if let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            h.write_u128(d.as_nanos());
        }
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
    }
    #[cfg(not(unix))]
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

fn read_lock(dir: &Path) -> Option<Lock> {
    serde_json::from_slice(&std::fs::read(lock_path(dir)).ok()?).ok()
}

fn exchange(lock: &Lock, command: &Command) -> std::io::Result<Reply> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, lock.port));
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let req = Request {
        token: lock.token.clone(),
        command: command.clone(),
    };
    let mut line = serde_json::to_vec(&req).map_err(std::io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    let mut reply = String::new();
    BufReader::new(stream.take(MAX_LINE)).read_line(&mut reply)?;
    serde_json::from_str(&reply).map_err(std::io::Error::other)
}

/// Sends `command` to the running instance whose lock file is in `dir`.
///
/// `Err` when none is running (no file, or nobody answers).
pub fn send(dir: &Path, command: &Command) -> std::io::Result<Reply> {
    let lock = read_lock(dir).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Pando Assistant is not running",
        )
    })?;
    exchange(&lock, command)
}

/// Guard of the primary instance: removes the lock file when dropped.
#[derive(Debug)]
pub struct InstanceGuard {
    dir: PathBuf,
    token: String,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // Only remove our own lock.
        if read_lock(&self.dir).is_some_and(|l| l.token == self.token) {
            let _ = std::fs::remove_file(lock_path(&self.dir));
        }
    }
}

/// Result of [`acquire`].
#[derive(Debug)]
pub enum Acquired {
    /// This process is the instance; commands arrive on the channel given to `acquire`.
    Primary(InstanceGuard),
    /// Another instance is running and answered a ping.
    Secondary,
}

/// Becomes the primary instance, or reports that one is already running.
///
/// The listener thread forwards every authenticated command to `tx` and answers `ok`.
pub fn acquire<T>(dir: &Path, tx: async_channel::Sender<T>) -> std::io::Result<Acquired>
where
    T: From<Command> + Send + 'static,
{
    if let Some(lock) = read_lock(dir)
        && exchange(&lock, &Command::Ping).is_ok_and(|r| r.ok)
    {
        return Ok(Acquired::Secondary);
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let lock = Lock {
        port: listener.local_addr()?.port(),
        token: random_token(),
        pid: std::process::id(),
    };
    write_private(
        &lock_path(dir),
        &serde_json::to_vec(&lock).map_err(std::io::Error::other)?,
    )?;
    let token = lock.token.clone();
    std::thread::Builder::new()
        .name("pando-ipc".into())
        .spawn({
            let token = token.clone();
            move || serve(&listener, &token, &tx)
        })?;
    Ok(Acquired::Primary(InstanceGuard {
        dir: dir.to_path_buf(),
        token,
    }))
}

fn serve<T: From<Command>>(listener: &TcpListener, token: &str, tx: &async_channel::Sender<T>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(TIMEOUT));
        let _ = stream.set_write_timeout(Some(TIMEOUT));
        let reply = handle(&stream, token, tx);
        let mut stream = stream;
        if let Ok(mut line) = serde_json::to_vec(&reply) {
            line.push(b'\n');
            let _ = stream.write_all(&line);
        }
        if tx.is_closed() {
            break;
        }
    }
}

fn handle<T: From<Command>>(
    stream: &TcpStream,
    token: &str,
    tx: &async_channel::Sender<T>,
) -> Reply {
    let mut line = String::new();
    if BufReader::new(stream.take(MAX_LINE))
        .read_line(&mut line)
        .is_err()
    {
        return Reply {
            ok: false,
            error: Some("unreadable request".into()),
        };
    }
    let Ok(req) = serde_json::from_str::<Request>(&line) else {
        return Reply {
            ok: false,
            error: Some("malformed request".into()),
        };
    };
    if !constant_time_eq(req.token.as_bytes(), token.as_bytes()) {
        return Reply {
            ok: false,
            error: Some("bad token".into()),
        };
    }
    if req.command != Command::Ping && tx.send_blocking(T::from(req.command)).is_err() {
        return Reply {
            ok: false,
            error: Some("shutting down".into()),
        };
    }
    Reply {
        ok: true,
        error: None,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_instance_is_primary_and_receives_commands() {
        let dir = tempfile::tempdir().expect("dir");
        let (tx, rx) = async_channel::unbounded::<Command>();
        let guard = acquire(dir.path(), tx).expect("acquire");
        assert!(matches!(guard, Acquired::Primary(_)));
        let reply = send(
            dir.path(),
            &Command::Ptt {
                action: PttAction::Start,
            },
        )
        .expect("send");
        assert!(reply.ok);
        assert_eq!(
            rx.recv_blocking().expect("command"),
            Command::Ptt {
                action: PttAction::Start
            }
        );
        // A ping is answered but not forwarded.
        assert!(send(dir.path(), &Command::Ping).expect("ping").ok);
        assert!(rx.is_empty());
    }

    #[test]
    fn second_launch_is_secondary_and_a_dead_lock_is_taken_over() {
        let dir = tempfile::tempdir().expect("dir");
        let (tx, _rx) = async_channel::unbounded::<Command>();
        let first = acquire(dir.path(), tx.clone()).expect("first");
        assert!(matches!(
            acquire(dir.path(), tx.clone()).expect("second"),
            Acquired::Secondary
        ));
        // Simulate a crash: the lock file stays, nobody listens on its port.
        std::mem::forget(first);
        let lock = Lock {
            port: 9,
            token: "stale".into(),
            pid: 1,
        };
        std::fs::write(
            lock_path(dir.path()),
            serde_json::to_vec(&lock).expect("json"),
        )
        .expect("write");
        assert!(send(dir.path(), &Command::Ping).is_err());
        assert!(matches!(
            acquire(dir.path(), tx).expect("takeover"),
            Acquired::Primary(_)
        ));
    }

    #[test]
    fn wrong_token_is_refused_and_the_guard_removes_its_lock() {
        let dir = tempfile::tempdir().expect("dir");
        let (tx, rx) = async_channel::unbounded::<Command>();
        let Acquired::Primary(guard) = acquire(dir.path(), tx).expect("acquire") else {
            panic!("primary")
        };
        let mut lock = read_lock(dir.path()).expect("lock");
        lock.token = "wrong".into();
        let reply = exchange(&lock, &Command::Quit).expect("exchange");
        assert!(!reply.ok);
        assert!(
            rx.is_empty(),
            "an unauthenticated command must not be forwarded"
        );
        drop(guard);
        assert!(!lock_path(dir.path()).exists());
        assert!(send(dir.path(), &Command::Ping).is_err());
    }

    #[test]
    fn commands_round_trip_as_json() {
        let json = serde_json::to_string(&Command::Ptt {
            action: PttAction::Toggle,
        })
        .expect("json");
        assert_eq!(json, r#"{"cmd":"ptt","action":"toggle"}"#);
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"cmd":"toggle_mini"}"#).expect("parse"),
            Command::ToggleMini
        );
    }
}
