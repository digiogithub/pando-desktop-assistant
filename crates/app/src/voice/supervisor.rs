//! Supervisor of one voice worker process (`pando-stt` or `pando-tts`, ADR 0007; PDA-T-0090).
//!
//! A supervisor thread launches the worker, performs the `voice-proto` `Hello` handshake,
//! loads the model, forwards every [`FromWorker`] message to the app and restarts the worker
//! with exponential backoff when it dies. The process boundary is hidden behind [`Launcher`] /
//! [`Link`] so the state machine is tested without spawning anything.

use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Where supervisors push their events (a closure so the app can merge several sources).
pub type EventSink = Arc<dyn Fn(WorkerEvent) + Send + Sync>;

/// A sink that pushes into an `async_channel`.
#[must_use]
pub fn channel_sink(tx: async_channel::Sender<WorkerEvent>) -> EventSink {
    Arc::new(move |ev| {
        let _ = tx.send_blocking(ev);
    })
}

use voice_proto::{FromWorker, PROTOCOL_VERSION, Role, ToWorker, read_msg, write_msg};

/// Availability of a worker as shown in the header ("voz disponible / no disponible").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// Not started (voice disabled or the binary is missing).
    Stopped,
    /// Launching, handshaking or loading the model.
    Starting,
    /// Handshake done and model loaded.
    Ready,
    /// Died; waiting to be relaunched.
    Restarting,
    /// Gave up after too many consecutive failures.
    Failed,
}

impl WorkerState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Starting,
            2 => Self::Ready,
            3 => Self::Restarting,
            4 => Self::Failed,
            _ => Self::Stopped,
        }
    }

    /// True when requests can be sent.
    #[must_use]
    pub fn is_ready(self) -> bool {
        self == Self::Ready
    }
}

/// What the supervisor reports to the app.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkerEvent {
    /// The worker changed state; `detail` carries the reason of a failure.
    State {
        /// Which worker.
        role: Role,
        /// New state.
        state: WorkerState,
        /// Human readable (English, log level) cause when the change is a failure.
        detail: Option<String>,
    },
    /// A protocol message from the worker (`Hello` and `Ready` are consumed by the supervisor).
    Message {
        /// Which worker.
        role: Role,
        /// The message.
        msg: FromWorker,
    },
}

/// Both ends of a worker connection.
#[allow(missing_debug_implementations)]
pub struct Link {
    /// Worker stdout.
    pub reader: Box<dyn Read + Send>,
    /// Worker stdin.
    pub writer: Box<dyn Write + Send>,
    /// Stops the worker (kills the process). Called on restart and shutdown.
    pub kill: Box<dyn FnMut() + Send>,
}

/// Starts a worker process.
pub trait Launcher: Send + 'static {
    /// Launches a fresh worker.
    fn launch(&mut self) -> io::Result<Link>;
}

/// Launcher of a real child process.
#[derive(Debug, Clone)]
pub struct ProcessLauncher {
    /// Binary path.
    pub program: std::path::PathBuf,
    /// Extra arguments.
    pub args: Vec<String>,
}

impl Launcher for ProcessLauncher {
    fn launch(&mut self) -> io::Result<Link> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        if let Some(stderr) = child.stderr.take() {
            let name = self.program.display().to_string();
            std::thread::spawn(move || {
                use std::io::BufRead as _;
                for line in io::BufReader::new(stderr).lines().map_while(Result::ok) {
                    tracing::debug!(worker = %name, "{line}");
                }
            });
        }
        let child = Arc::new(Mutex::new(child));
        Ok(Link {
            reader: Box::new(io::BufReader::new(stdout)),
            writer: Box::new(io::BufWriter::new(stdin)),
            kill: Box::new(move || reap(&child)),
        })
    }
}

fn reap(child: &Arc<Mutex<Child>>) {
    if let Ok(mut c) = child.lock() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Where the worker binaries live: next to the app executable (packaged installs), then the
/// cargo target directory (development; test executables sit in `deps/`), then `PATH`.
#[must_use]
pub fn locate_worker(name: &str) -> std::path::PathBuf {
    let exe_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        dirs.push(dir.to_path_buf());
        if let Some(up) = dir.parent() {
            dirs.push(up.to_path_buf());
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter()
        .map(|d| d.join(&exe_name))
        .find(|c| c.is_file())
        .unwrap_or_else(|| std::path::PathBuf::from(exe_name))
}

/// Exponential restart backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// First delay.
    pub initial: Duration,
    /// Cap.
    pub max: Duration,
    /// Consecutive failures after which the supervisor gives up.
    pub max_failures: u32,
    /// A run that lasted this long resets the failure counter.
    pub healthy_after: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(30),
            max_failures: 6,
            healthy_after: Duration::from_secs(10),
        }
    }
}

impl Backoff {
    /// Delay before relaunching after `failures` consecutive failures (1-based).
    #[must_use]
    pub fn delay(&self, failures: u32) -> Duration {
        let shift = failures.saturating_sub(1).min(16);
        self.initial.saturating_mul(1 << shift).min(self.max)
    }
}

/// What to load right after the handshake.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadSpec {
    /// `LoadStt`.
    Stt {
        /// Model file or directory.
        model_path: String,
    },
    /// `LoadTts`.
    Tts {
        /// Model directory.
        model_dir: String,
        /// Voice.
        voice: String,
    },
}

impl LoadSpec {
    /// The worker role.
    #[must_use]
    pub fn role(&self) -> Role {
        match self {
            Self::Stt { .. } => Role::Stt,
            Self::Tts { .. } => Role::Tts,
        }
    }

    fn message(&self) -> ToWorker {
        match self {
            Self::Stt { model_path } => ToWorker::LoadStt {
                model_path: model_path.clone(),
            },
            Self::Tts { model_dir, voice } => ToWorker::LoadTts {
                model_dir: model_dir.clone(),
                voice: voice.clone(),
            },
        }
    }
}

/// Why sending a request failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SendError {
    /// The worker is not ready (starting, restarting, failed or stopped).
    #[error("the voice worker is not available")]
    Unavailable,
}

enum Inbox {
    Cmd(ToWorker),
    Wire(u64, Result<Option<FromWorker>, String>),
    Shutdown,
}

/// Handle to a supervised worker. Dropping it shuts the worker down.
#[allow(missing_debug_implementations)]
pub struct Supervisor {
    role: Role,
    state: Arc<AtomicU8>,
    tx: Sender<Inbox>,
    thread: Option<JoinHandle<()>>,
}

impl Supervisor {
    /// Starts supervising. Events are pushed to `events`.
    pub fn start(
        spec: LoadSpec,
        launcher: impl Launcher,
        backoff: Backoff,
        events: EventSink,
    ) -> Self {
        let role = spec.role();
        let state = Arc::new(AtomicU8::new(WorkerState::Stopped as u8));
        let (tx, rx) = channel();
        let ctx = Ctx {
            role,
            spec,
            backoff,
            events,
            state: state.clone(),
            tx: tx.clone(),
            rx,
        };
        let thread = std::thread::Builder::new()
            .name(format!("voice-supervisor-{role:?}"))
            .spawn(move || ctx.run(launcher))
            .ok();
        Self {
            role,
            state,
            tx,
            thread,
        }
    }

    /// Which worker this is.
    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> WorkerState {
        WorkerState::from_u8(self.state.load(Ordering::SeqCst))
    }

    /// Sends a request. Refused unless the worker is ready, so callers can report the outage
    /// instead of queueing audio for a dead process.
    pub fn send(&self, msg: ToWorker) -> Result<(), SendError> {
        if !self.state().is_ready() {
            return Err(SendError::Unavailable);
        }
        self.tx
            .send(Inbox::Cmd(msg))
            .map_err(|_| SendError::Unavailable)
    }

    /// Asks the worker to exit and joins the supervisor thread.
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Inbox::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Ctx {
    role: Role,
    spec: LoadSpec,
    backoff: Backoff,
    events: EventSink,
    state: Arc<AtomicU8>,
    tx: Sender<Inbox>,
    rx: Receiver<Inbox>,
}

/// How one incarnation ended.
enum Ended {
    Shutdown,
    Died(String),
}

impl Ctx {
    fn set(&self, state: WorkerState, detail: Option<String>) {
        self.state.store(state as u8, Ordering::SeqCst);
        (self.events)(WorkerEvent::State {
            role: self.role,
            state,
            detail,
        });
    }

    fn run(self, mut launcher: impl Launcher) {
        let mut failures = 0_u32;
        let mut generation = 0_u64;
        loop {
            self.set(WorkerState::Starting, None);
            generation += 1;
            let started = Instant::now();
            let ended = match launcher.launch() {
                Ok(link) => self.serve(link, generation),
                Err(e) => Ended::Died(format!("could not launch: {e}")),
            };
            match ended {
                Ended::Shutdown => {
                    self.set(WorkerState::Stopped, None);
                    return;
                }
                Ended::Died(reason) => {
                    if started.elapsed() >= self.backoff.healthy_after {
                        failures = 0;
                    }
                    failures += 1;
                    tracing::warn!(role = ?self.role, failures, "voice worker down: {reason}");
                    if failures >= self.backoff.max_failures {
                        self.set(WorkerState::Failed, Some(reason));
                        // Stay alive only to honour shutdown.
                        while let Ok(m) = self.rx.recv() {
                            if matches!(m, Inbox::Shutdown) {
                                break;
                            }
                        }
                        self.set(WorkerState::Stopped, None);
                        return;
                    }
                    self.set(WorkerState::Restarting, Some(reason));
                    if self.wait(self.backoff.delay(failures)) {
                        self.set(WorkerState::Stopped, None);
                        return;
                    }
                }
            }
        }
    }

    /// Sleeps `d`, dropping requests meanwhile; true when shutdown arrived.
    fn wait(&self, d: Duration) -> bool {
        let end = Instant::now() + d;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(Inbox::Shutdown) => return true,
                Ok(_) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return false,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return true,
            }
        }
    }

    fn serve(&self, link: Link, generation: u64) -> Ended {
        let Link {
            mut reader,
            mut writer,
            mut kill,
        } = link;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            loop {
                let item = read_msg::<_, FromWorker>(&mut reader).map_err(|e| e.to_string());
                let end = !matches!(item, Ok(Some(_)));
                if tx.send(Inbox::Wire(generation, item)).is_err() || end {
                    break;
                }
            }
        });
        let ended = self.converse(&mut writer, generation);
        if matches!(ended, Ended::Shutdown) {
            let _ = write_msg(&mut writer, &ToWorker::Shutdown);
        }
        kill();
        ended
    }

    fn converse(&self, writer: &mut Box<dyn Write + Send>, generation: u64) -> Ended {
        let died = |s: &str| Ended::Died(s.to_owned());
        if write_msg(
            writer,
            &ToWorker::Hello {
                version: PROTOCOL_VERSION,
            },
        )
        .is_err()
        {
            return died("could not write the handshake");
        }
        let mut greeted = false;
        let mut ready = false;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            // Until Ready the handshake has a deadline; afterwards we wait indefinitely.
            let msg = if ready {
                self.rx.recv().map_err(|_| ())
            } else {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.rx.recv_timeout(left) {
                    Ok(m) => Ok(m),
                    Err(_) => return died("the worker did not become ready in time"),
                }
            };
            let Ok(msg) = msg else {
                return Ended::Shutdown;
            };
            match msg {
                Inbox::Shutdown => return Ended::Shutdown,
                Inbox::Cmd(cmd) => {
                    if !ready {
                        continue;
                    }
                    if write_msg(writer, &cmd).is_err() {
                        return died("the worker closed its input");
                    }
                }
                Inbox::Wire(g, _) if g != generation => {}
                Inbox::Wire(_, Err(e)) => return Ended::Died(format!("protocol error: {e}")),
                Inbox::Wire(_, Ok(None)) => return died("the worker exited"),
                Inbox::Wire(_, Ok(Some(m))) => match m {
                    FromWorker::Hello { version, role } if !greeted => {
                        if let Err((_, why)) = voice_proto::check_version(version) {
                            return Ended::Died(why);
                        }
                        if role != self.role {
                            return Ended::Died(format!("expected {:?}, got {role:?}", self.role));
                        }
                        greeted = true;
                        if write_msg(writer, &self.spec.message()).is_err() {
                            return died("could not write the load request");
                        }
                    }
                    FromWorker::Ready if greeted && !ready => {
                        ready = true;
                        self.set(WorkerState::Ready, None);
                    }
                    FromWorker::Error { code, message, .. } if !ready => {
                        return Ended::Died(format!("{code:?}: {message}"));
                    }
                    other => {
                        (self.events)(WorkerEvent::Message {
                            role: self.role,
                            msg: other,
                        });
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    struct ChanReader {
        rx: mpsc::Receiver<Vec<u8>>,
        buf: Vec<u8>,
    }
    impl Read for ChanReader {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.buf.is_empty() {
                match self.rx.recv() {
                    Ok(b) => self.buf = b,
                    Err(_) => return Ok(0),
                }
            }
            let n = out.len().min(self.buf.len());
            out[..n].copy_from_slice(&self.buf[..n]);
            self.buf.drain(..n);
            Ok(n)
        }
    }
    struct ChanWriter(mpsc::Sender<Vec<u8>>);
    impl Write for ChanWriter {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0
                .send(b.to_vec())
                .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A scripted fake worker: replies to Hello/Load, echoes Transcribe as a transcript and
    /// dies when it sees `Health` if `die_on_health`.
    struct Fake {
        launches: Arc<AtomicU8>,
        die_on_health: bool,
        fail_first: u8,
        version: u16,
    }
    impl Launcher for Fake {
        fn launch(&mut self) -> io::Result<Link> {
            let n = self.launches.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_first {
                return Err(io::Error::other("no binary"));
            }
            let (to_app, from_worker) = mpsc::channel::<Vec<u8>>();
            let (to_worker, from_app) = mpsc::channel::<Vec<u8>>();
            let (die, version) = (self.die_on_health, self.version);
            std::thread::spawn(move || {
                let mut r = ChanReader {
                    rx: from_app,
                    buf: Vec::new(),
                };
                let mut w = ChanWriter(to_app);
                while let Ok(Some(m)) = read_msg::<_, ToWorker>(&mut r) {
                    let reply = match m {
                        ToWorker::Hello { .. } => Some(FromWorker::Hello {
                            version,
                            role: Role::Stt,
                        }),
                        ToWorker::LoadStt { .. } => Some(FromWorker::Ready),
                        ToWorker::Transcribe { id, .. } => Some(FromWorker::Transcript {
                            id,
                            text: "hola".into(),
                            lang: "es".into(),
                            words: vec![],
                            ttft_ms: None,
                        }),
                        ToWorker::Health if die => return,
                        ToWorker::Shutdown => return,
                        _ => None,
                    };
                    if let Some(f) = reply {
                        let _ = write_msg(&mut w, &f);
                    }
                }
            });
            Ok(Link {
                reader: Box::new(ChanReader {
                    rx: from_worker,
                    buf: Vec::new(),
                }),
                writer: Box::new(ChanWriter(to_worker)),
                kill: Box::new(|| {}),
            })
        }
    }

    fn fast() -> Backoff {
        Backoff {
            initial: Duration::from_millis(5),
            max: Duration::from_millis(20),
            max_failures: 3,
            healthy_after: Duration::from_secs(60),
        }
    }

    fn spec() -> LoadSpec {
        LoadSpec::Stt {
            model_path: "m".into(),
        }
    }

    fn wait_state(
        rx: &async_channel::Receiver<WorkerEvent>,
        want: WorkerState,
    ) -> Vec<WorkerEvent> {
        let mut seen = Vec::new();
        loop {
            let ev = rx.recv_blocking().unwrap_or(WorkerEvent::State {
                role: Role::Stt,
                state: WorkerState::Stopped,
                detail: None,
            });
            let hit = matches!(&ev, WorkerEvent::State { state, .. } if *state == want);
            let stopped = matches!(
                &ev,
                WorkerEvent::State {
                    state: WorkerState::Stopped,
                    ..
                }
            );
            seen.push(ev);
            if hit || stopped {
                return seen;
            }
        }
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let b = Backoff::default();
        assert_eq!(b.delay(1), Duration::from_millis(500));
        assert_eq!(b.delay(2), Duration::from_secs(1));
        assert_eq!(b.delay(3), Duration::from_secs(2));
        assert_eq!(b.delay(30), Duration::from_secs(30));
    }

    #[test]
    fn handshake_load_then_requests_are_served() {
        let (etx, erx) = async_channel::unbounded();
        let sup = Supervisor::start(
            spec(),
            Fake {
                launches: Arc::default(),
                die_on_health: false,
                fail_first: 0,
                version: PROTOCOL_VERSION,
            },
            fast(),
            channel_sink(etx),
        );
        wait_state(&erx, WorkerState::Ready);
        assert!(sup.state().is_ready());
        sup.send(ToWorker::Transcribe {
            id: 7,
            pcm: vec![],
            lang: None,
            keywords: vec![],
            word_ts: false,
        })
        .expect("ready");
        let ev = erx.recv_blocking().expect("event");
        assert!(matches!(
            ev,
            WorkerEvent::Message {
                msg: FromWorker::Transcript { id: 7, .. },
                ..
            }
        ));
    }

    #[test]
    fn a_killed_worker_is_restarted_and_the_outage_is_reported() {
        let (etx, erx) = async_channel::unbounded();
        let launches = Arc::new(AtomicU8::new(0));
        let sup = Supervisor::start(
            spec(),
            Fake {
                launches: launches.clone(),
                die_on_health: true,
                fail_first: 0,
                version: PROTOCOL_VERSION,
            },
            Backoff {
                max_failures: 10,
                ..fast()
            },
            channel_sink(etx),
        );
        wait_state(&erx, WorkerState::Ready);
        sup.send(ToWorker::Health).expect("ready");
        let seen = wait_state(&erx, WorkerState::Restarting);
        assert!(seen.iter().any(|e| matches!(
            e,
            WorkerEvent::State {
                state: WorkerState::Restarting,
                detail: Some(_),
                ..
            }
        )));
        wait_state(&erx, WorkerState::Ready);
        assert!(launches.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn requests_are_refused_while_down_and_failure_is_final_after_the_limit() {
        let (etx, erx) = async_channel::unbounded();
        let sup = Supervisor::start(
            spec(),
            Fake {
                launches: Arc::default(),
                die_on_health: false,
                fail_first: 200,
                version: PROTOCOL_VERSION,
            },
            fast(),
            channel_sink(etx),
        );
        wait_state(&erx, WorkerState::Failed);
        assert_eq!(sup.state(), WorkerState::Failed);
        assert_eq!(sup.send(ToWorker::Health), Err(SendError::Unavailable));
    }

    #[test]
    fn a_version_mismatch_is_a_failure() {
        let (etx, erx) = async_channel::unbounded();
        let _sup = Supervisor::start(
            spec(),
            Fake {
                launches: Arc::default(),
                die_on_health: false,
                fail_first: 0,
                version: 999,
            },
            fast(),
            channel_sink(etx),
        );
        let seen = wait_state(&erx, WorkerState::Failed);
        assert!(seen.iter().any(|e| matches!(
            e,
            WorkerEvent::State { detail: Some(d), .. } if d.contains("999")
        )));
    }

    #[test]
    fn shutdown_stops_cleanly() {
        let (etx, erx) = async_channel::unbounded();
        let mut sup = Supervisor::start(
            spec(),
            Fake {
                launches: Arc::default(),
                die_on_health: false,
                fail_first: 0,
                version: PROTOCOL_VERSION,
            },
            fast(),
            channel_sink(etx),
        );
        wait_state(&erx, WorkerState::Ready);
        sup.shutdown();
        assert_eq!(sup.state(), WorkerState::Stopped);
    }
}
