//! `decisions.jsonl` (`PDA-SP-0008.R5`): one JSON line per decision, rotated at 10 MB.
//!
//! The log is the eval set for choosing decision models (see [`crate::eval`]). It
//! stores the transcript, the state hash, every request (questions + state, so it can be
//! replayed) and response (answers with probabilities), latency, the policy outcome and
//! whether the command ran. It never contains credentials: the API key lives in the
//! transport only.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cascade::{CascadeFailure, CascadeRun, Decision, Exchange};
use crate::policy::Outcome;
use crate::state::state_hash;

/// Default rotation size.
pub const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// File name inside the data dir.
pub const FILE_NAME: &str = "decisions.jsonl";

/// One line of `decisions.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRecord {
    /// Unix time in milliseconds.
    pub ts_ms: u64,
    /// The transcript.
    pub transcript: String,
    /// FNV-1a hash of the first request's state (`""` when nothing was sent).
    pub state_hash: String,
    /// Model of the requests.
    pub model: String,
    /// Questions and answers, in order.
    pub exchanges: Vec<Exchange>,
    /// The decision, when the cascade completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    /// Error kind when the cascade failed (fail-open).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Total decision latency in milliseconds.
    pub latency_ms: u64,
    /// Policy outcome.
    pub policy: Outcome,
    /// Whether the command was executed (after confirmation, if any).
    pub executed: bool,
}

impl DecisionRecord {
    /// Build a record from a cascade result and the policy outcome.
    pub fn new(
        ts_ms: u64,
        transcript: impl Into<String>,
        result: &Result<CascadeRun, CascadeFailure>,
        policy: Outcome,
        executed: bool,
    ) -> Self {
        let (exchanges, decision, error) = match result {
            Ok(run) => (run.exchanges.clone(), Some(run.decision.clone()), None),
            Err(f) => (f.exchanges.clone(), None, Some(f.error.kind().to_string())),
        };
        let latency_ms = exchanges.iter().map(|e| e.latency_ms).sum();
        Self {
            ts_ms,
            transcript: transcript.into(),
            state_hash: exchanges
                .first()
                .map(|e| state_hash(&e.request.state))
                .unwrap_or_default(),
            model: exchanges
                .first()
                .map(|e| e.request.model.clone())
                .unwrap_or_default(),
            exchanges,
            decision,
            error,
            latency_ms,
            policy,
            executed,
        }
    }
}

/// Append-only log with single-backup rotation (`decisions.jsonl.1`).
#[derive(Debug, Clone)]
pub struct DecisionLog {
    path: PathBuf,
    max_bytes: u64,
}

impl DecisionLog {
    /// Log at `path`, rotated when a write would push it past `max_bytes`.
    pub fn new(path: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            path: path.into(),
            max_bytes,
        }
    }

    /// `<data_dir>/decisions.jsonl` with the 10 MB default.
    pub fn in_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(FILE_NAME), DEFAULT_MAX_BYTES)
    }

    /// The log path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The rotated backup path.
    pub fn backup_path(&self) -> PathBuf {
        let mut p = self.path.clone().into_os_string();
        p.push(".1");
        PathBuf::from(p)
    }

    /// Append one record as a single line, rotating first if needed. Creates the
    /// parent directory.
    pub fn append(&self, record: &DecisionRecord) -> io::Result<()> {
        let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
        line.push(b'\n');
        if let Some(dir) = self.path.parent()
            && !dir.as_os_str().is_empty()
        {
            fs::create_dir_all(dir)?;
        }
        let size = fs::metadata(&self.path).map_or(0, |m| m.len());
        if size > 0 && size + line.len() as u64 > self.max_bytes {
            fs::rename(&self.path, self.backup_path())?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(&line)
    }
}

/// Records of a log file, plus the number of lines that failed to parse.
pub fn read_log(path: &Path) -> io::Result<(Vec<DecisionRecord>, usize)> {
    let f = fs::File::open(path)?;
    let mut records = Vec::new();
    let mut bad = 0;
    for line in BufReader::new(f).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(r) => records.push(r),
            Err(_) => bad += 1,
        }
    }
    Ok((records, bad))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cascade::run;
    use crate::error::DecisionError;
    use crate::state::StateInput;
    use crate::transport::mock::{MockTransport, Reply};
    use crate::transport::{ClientOptions, SystemOneClient, block_on};

    fn sample(transcript: &str) -> DecisionRecord {
        let c = SystemOneClient::new(
            MockTransport::new(vec![Reply::Answer(vec![("intent", "question", 0.9)])]),
            ClientOptions::new("tev1:0.8b"),
        );
        let result = block_on(run(
            &c,
            StateInput {
                transcript: transcript.into(),
                ..StateInput::default()
            },
        ));
        DecisionRecord::new(1, transcript, &result, Outcome::unavailable(), false)
    }

    #[test]
    fn record_round_trips_and_holds_no_secrets() {
        let r = sample("explícame el error");
        assert_eq!(r.model, "tev1:0.8b");
        assert_eq!(r.state_hash.len(), 16);
        assert_eq!(r.exchanges.len(), 1);
        let line = serde_json::to_string(&r).unwrap();
        assert!(!line.contains('\n'));
        assert!(!line.to_lowercase().contains("authorization"));
        let back: DecisionRecord = serde_json::from_str(&line).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn failures_are_logged_with_error_kind() {
        let f = CascadeFailure {
            error: DecisionError::Timeout,
            exchanges: vec![],
        };
        let r = DecisionRecord::new(5, "hola", &Err(f), Outcome::unavailable(), false);
        assert_eq!(r.error.as_deref(), Some("timeout"));
        assert!(r.decision.is_none());
        assert_eq!(r.state_hash, "");
    }

    #[test]
    fn appends_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let rec = sample("¿cómo va bitácora?");
        let one = serde_json::to_vec(&rec).unwrap().len() as u64 + 1;
        let log = DecisionLog::new(dir.path().join("sub").join(FILE_NAME), one * 3);
        for _ in 0..3 {
            log.append(&rec).unwrap();
        }
        assert!(!log.backup_path().exists());
        assert_eq!(read_log(log.path()).unwrap().0.len(), 3);
        log.append(&rec).unwrap();
        assert!(log.backup_path().exists());
        assert_eq!(read_log(log.path()).unwrap().0.len(), 1);
        assert_eq!(read_log(&log.backup_path()).unwrap().0.len(), 3);
    }

    #[test]
    fn bad_lines_are_counted_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(FILE_NAME);
        let good = serde_json::to_string(&sample("hola")).unwrap();
        fs::write(&p, format!("{good}\nnot json\n\n{good}\n")).unwrap();
        let (records, bad) = read_log(&p).unwrap();
        assert_eq!((records.len(), bad), (2, 1));
    }

    #[test]
    fn in_dir_uses_the_standard_name() {
        let log = DecisionLog::in_dir(Path::new("/data"));
        assert_eq!(log.path(), Path::new("/data/decisions.jsonl"));
        assert_eq!(log.backup_path(), Path::new("/data/decisions.jsonl.1"));
    }
}
