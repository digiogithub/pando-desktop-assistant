//! State behind the `decisión · <model>` chip and the fail-open rule (PDA-T-0099): if the
//! decision model is down, voice keeps working as a plain conversation with the chat model.

use pando_client::events::{DecisionModelConfig, RouterHealth};
use rust_i18n::t;

use super::transport::DecisionEndpoint;

/// What the chip shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionStatus {
    /// Config not loaded yet.
    Unknown,
    /// Healthy.
    Available {
        /// Model name.
        model: String,
    },
    /// Down or failing.
    Unavailable {
        /// Why (English, log level).
        reason: String,
    },
}

/// Everything known about the decision model.
#[derive(Debug, Clone, Default)]
pub struct DecisionState {
    config: Option<DecisionModelConfig>,
    health: Option<RouterHealth>,
    last_call_failed: bool,
}

impl DecisionState {
    /// New config from `/config/decision-model` (also when it changed on `/config/events`).
    pub fn set_config(&mut self, config: DecisionModelConfig) {
        self.config = Some(config);
        self.last_call_failed = false;
    }

    /// New answer of `/decision-model/router/health`.
    pub fn set_health(&mut self, health: RouterHealth) {
        self.health = Some(health);
    }

    /// Records the outcome of a real cascade (a failure flips the chip immediately).
    pub fn record_call(&mut self, failed: bool) {
        self.last_call_failed = failed;
    }

    /// The configured model.
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.config
            .as_ref()
            .map(|c| c.router.model.as_str())
            .filter(|m| !m.is_empty())
    }

    /// The chip status.
    #[must_use]
    pub fn status(&self) -> DecisionStatus {
        let Some(cfg) = &self.config else {
            return DecisionStatus::Unknown;
        };
        if cfg.router.model.is_empty() {
            return DecisionStatus::Unavailable {
                reason: "no decision model configured".into(),
            };
        }
        if let Some(h) = &self.health
            && !h.ok
        {
            let reason = h
                .problems
                .first()
                .or_else(|| h.report.problems.first())
                .cloned()
                .unwrap_or_else(|| "health check failed".into());
            return DecisionStatus::Unavailable { reason };
        }
        if self.last_call_failed {
            return DecisionStatus::Unavailable {
                reason: "the last decision call failed".into(),
            };
        }
        DecisionStatus::Available {
            model: cfg.router.model.clone(),
        }
    }

    /// Whether to run the cascade for the next sentence. A failed *call* is retried (it may
    /// have been a blip); a *health report* that says down skips it, so a dead Ollama does not
    /// add 400 ms to every sentence.
    #[must_use]
    pub fn should_try(&self) -> bool {
        match (&self.config, &self.health) {
            (None, _) => false,
            (Some(c), _) if c.router.model.is_empty() => false,
            (_, Some(h)) if !h.ok => false,
            _ => true,
        }
    }

    /// The transport endpoint, when configured.
    #[must_use]
    pub fn endpoint(&self, env: impl Fn(&str) -> Option<String>) -> Option<DecisionEndpoint> {
        DecisionEndpoint::from_config(self.config.as_ref()?, env)
    }

    /// Text of the chip.
    #[must_use]
    pub fn chip_label(&self) -> String {
        match self.status() {
            DecisionStatus::Unknown => t!("voice.decision.unknown").to_string(),
            DecisionStatus::Available { model } => {
                t!("voice.decision.available", model = model).to_string()
            }
            DecisionStatus::Unavailable { .. } => t!("voice.decision.unavailable").to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(model: &str) -> DecisionModelConfig {
        let mut c = DecisionModelConfig::default();
        c.router.model = model.into();
        c.router.effective_base_url = "http://h".into();
        c
    }

    fn health(ok: bool) -> RouterHealth {
        RouterHealth {
            ok,
            problems: if ok {
                vec![]
            } else {
                vec!["connection refused".into()]
            },
            ..RouterHealth::default()
        }
    }

    #[test]
    fn unknown_until_configured_then_available() {
        crate::i18n::apply(Some("es"));
        let mut s = DecisionState::default();
        assert_eq!(s.status(), DecisionStatus::Unknown);
        assert!(!s.should_try());
        s.set_config(cfg("tev1:0.8b"));
        assert_eq!(
            s.status(),
            DecisionStatus::Available {
                model: "tev1:0.8b".into()
            }
        );
        assert!(s.chip_label().contains("tev1:0.8b"));
    }

    #[test]
    fn a_dead_router_flips_the_chip_and_skips_the_cascade() {
        crate::i18n::apply(Some("es"));
        let mut s = DecisionState::default();
        s.set_config(cfg("m"));
        s.set_health(health(false));
        assert!(
            matches!(s.status(), DecisionStatus::Unavailable { ref reason } if reason == "connection refused")
        );
        assert!(!s.should_try());
        assert_eq!(s.chip_label(), t!("voice.decision.unavailable"));
        s.set_health(health(true));
        assert!(s.should_try());
    }

    #[test]
    fn a_failed_call_flips_the_chip_but_is_retried() {
        let mut s = DecisionState::default();
        s.set_config(cfg("m"));
        s.record_call(true);
        assert!(matches!(s.status(), DecisionStatus::Unavailable { .. }));
        assert!(s.should_try());
        s.record_call(false);
        assert!(matches!(s.status(), DecisionStatus::Available { .. }));
    }

    #[test]
    fn a_config_change_clears_the_failure() {
        let mut s = DecisionState::default();
        s.set_config(cfg("m"));
        s.record_call(true);
        s.set_config(cfg("other"));
        assert_eq!(s.model(), Some("other"));
        assert!(matches!(s.status(), DecisionStatus::Available { .. }));
    }

    #[test]
    fn no_model_means_unavailable() {
        let mut s = DecisionState::default();
        s.set_config(cfg(""));
        assert!(matches!(s.status(), DecisionStatus::Unavailable { .. }));
        assert!(!s.should_try());
    }
}
