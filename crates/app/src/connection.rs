//! Binds `pando-client`'s `ConnectionService` to the UI (PDA-T-0048).
//!
//! [`ConnectionHub`] owns the running service and forwards every state change into an
//! `async_channel`, which the main view drains with an `EventPump`. [`chip_spec`] and
//! [`needs_empty_state`] are the pure, unit-tested mapping from state to what the header chip and
//! the "sin Pando" screen show. All copy comes from the locale files.

use pando_client::auth::{default_ca_path, load_local_ca};
use pando_client::discovery::validate_url;
use pando_client::{ConnectionOptions, ConnectionService, ConnectionState};
use rust_i18n::t;
use tokio::runtime::Handle;

/// Visual weight of the header chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Working on it.
    Neutral,
    /// Connected.
    Ok,
    /// Reachable but needs attention (unauthorized, too old).
    Warn,
    /// Not reachable.
    Error,
}

/// What the header chip shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipSpec {
    /// Dot colour.
    pub tone: Tone,
    /// Localized label.
    pub label: String,
}

/// Maps a connection state to the header chip.
#[must_use]
pub fn chip_spec(state: &ConnectionState) -> ChipSpec {
    match state {
        ConnectionState::Connecting => ChipSpec {
            tone: Tone::Neutral,
            label: t!("pando.chip.connecting").to_string(),
        },
        ConnectionState::Connected { version, .. } => ChipSpec {
            tone: Tone::Ok,
            label: t!("pando.chip.connected", version = version).to_string(),
        },
        ConnectionState::Unavailable { .. } => ChipSpec {
            tone: Tone::Error,
            label: t!("pando.chip.unavailable").to_string(),
        },
        ConnectionState::Unauthorized => ChipSpec {
            tone: Tone::Warn,
            label: t!("pando.chip.unauthorized").to_string(),
        },
        ConnectionState::TooOld { .. } => ChipSpec {
            tone: Tone::Warn,
            label: t!("pando.chip.too_old").to_string(),
        },
    }
}

/// Whether the main area shows the "sin Pando" screen instead of the app.
#[must_use]
pub fn needs_empty_state(state: &ConnectionState) -> bool {
    !matches!(state, ConnectionState::Connected { .. })
}

/// One-line, localized explanation shown on the "sin Pando" screen.
#[must_use]
pub fn reason_text(state: &ConnectionState) -> String {
    match state {
        ConnectionState::Connecting => t!("pando.reason.connecting").to_string(),
        ConnectionState::Connected { .. } => String::new(),
        ConnectionState::Unavailable { .. } => t!("pando.reason.unavailable").to_string(),
        ConnectionState::Unauthorized => t!("pando.reason.unauthorized").to_string(),
        ConnectionState::TooOld { min, found } => {
            t!("pando.reason.too_old", found = found, min = min).to_string()
        }
    }
}

/// Why a manual URL was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid Pando URL: {0}")]
pub struct InvalidUrl(pub String);

/// Owns the running `ConnectionService` and mirrors its state into a channel.
#[derive(Debug)]
pub struct ConnectionHub {
    handle: Handle,
    tx: async_channel::Sender<ConnectionState>,
    service: Option<ConnectionService>,
    manual_url: Option<String>,
    options: ConnectionOptions,
}

impl ConnectionHub {
    /// A hub that sends states on `tx`; nothing runs until [`ConnectionHub::start`].
    #[must_use]
    pub fn new(handle: Handle, tx: async_channel::Sender<ConnectionState>) -> Self {
        let mut options = ConnectionOptions::default();
        options.discovery.root_certificate_pem =
            default_ca_path().and_then(|path| load_local_ca(&path).ok().flatten());
        Self {
            handle,
            tx,
            service: None,
            manual_url: None,
            options,
        }
    }

    /// Replaces the options (tests point discovery at a scratch registry).
    #[must_use]
    pub fn with_options(mut self, options: ConnectionOptions) -> Self {
        self.options = options;
        self
    }

    /// (Re)starts the service. `manual_url` skips discovery; it is validated first and a refused
    /// URL leaves the running service untouched.
    pub fn start(&mut self, manual_url: Option<&str>) -> Result<(), InvalidUrl> {
        let manual_url = manual_url.map(str::trim).filter(|u| !u.is_empty());
        if let Some(url) = manual_url {
            validate_url(url, self.options.discovery.allow_remote)
                .map_err(|e| InvalidUrl(e.to_string()))?;
        }
        self.manual_url = manual_url.map(str::to_owned);
        let mut options = self.options.clone();
        options.discovery.manual_url = self.manual_url.clone();

        // Dropping the old service aborts its task, which closes its watch channel and ends the
        // old forwarder below.
        self.service = None;
        let _guard = self.handle.enter();
        let service = ConnectionService::start(options);
        let mut rx = service.subscribe();
        let tx = self.tx.clone();
        self.handle.spawn(async move {
            loop {
                let state = rx.borrow_and_update().state.clone();
                if tx.send(state).await.is_err() || rx.changed().await.is_err() {
                    break;
                }
            }
        });
        self.service = Some(service);
        Ok(())
    }

    /// Retries now: restarts discovery with the current manual URL.
    pub fn retry(&mut self) {
        let url = self.manual_url.clone();
        // The URL was validated when it was set, so this cannot fail.
        let _ = self.start(url.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() {
        rust_i18n::set_locale("en");
    }

    #[test]
    fn chip_reflects_every_state() {
        en();
        let connected = chip_spec(&ConnectionState::Connected {
            version: "v1.3.0".into(),
            project: None,
        });
        assert_eq!(connected.tone, Tone::Ok);
        assert!(connected.label.contains("v1.3.0"), "{}", connected.label);
        assert_eq!(chip_spec(&ConnectionState::Connecting).tone, Tone::Neutral);
        assert_eq!(
            chip_spec(&ConnectionState::Unavailable { reason: "x".into() }).tone,
            Tone::Error
        );
        assert_eq!(chip_spec(&ConnectionState::Unauthorized).tone, Tone::Warn);
        let too_old = ConnectionState::TooOld {
            min: "1.0.0".into(),
            found: "0.9.0".into(),
        };
        assert_eq!(chip_spec(&too_old).tone, Tone::Warn);
        assert!(reason_text(&too_old).contains("0.9.0"));
    }

    #[test]
    fn empty_state_unless_connected() {
        assert!(needs_empty_state(&ConnectionState::Connecting));
        assert!(needs_empty_state(&ConnectionState::Unauthorized));
        assert!(!needs_empty_state(&ConnectionState::Connected {
            version: "v1".into(),
            project: None
        }));
    }

    #[test]
    fn chip_text_is_localized() {
        rust_i18n::set_locale("es");
        let es = chip_spec(&ConnectionState::Unavailable {
            reason: String::new(),
        })
        .label;
        en();
        let en_label = chip_spec(&ConnectionState::Unavailable {
            reason: String::new(),
        })
        .label;
        assert_ne!(es, en_label);
        assert!(!es.starts_with("pando."), "missing es key: {es}");
    }

    // Tests may block on a scratch runtime; the ban is for the GPUI main thread.
    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    async fn hub_reports_unavailable_for_a_dead_manual_url_and_rejects_bad_urls() {
        let (tx, rx) = async_channel::unbounded();
        let mut hub = ConnectionHub::new(Handle::current(), tx);
        assert!(hub.start(Some("ftp://nope")).is_err());
        assert!(
            hub.start(Some("https://example.com")).is_err(),
            "remote is off"
        );
        hub.start(Some("http://127.0.0.1:9"))
            .expect("local url is valid");
        let first = rx.recv().await.expect("initial state");
        assert_eq!(first, ConnectionState::Connecting);
        let next = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let state = rx.recv().await.expect("state");
                if !matches!(state, ConnectionState::Connecting) {
                    return state;
                }
            }
        })
        .await
        .expect("a verdict within 10 s");
        assert!(
            matches!(next, ConnectionState::Unavailable { .. }),
            "{next:?}"
        );
        assert!(needs_empty_state(&next));
        hub.retry();
        assert_eq!(
            rx.recv().await.expect("restart"),
            ConnectionState::Connecting
        );
    }
}
