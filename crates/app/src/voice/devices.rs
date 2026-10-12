//! Microphone problems and device selection (PDA-T-0086).
//!
//! `audio::Capture` reports failures as `CaptureEvent::Error(String)` and retries every
//! second, so access that is granted later (macOS TCC prompt, Windows privacy toggle, a
//! portal) starts working without a restart. This module classifies those messages into what
//! the sheet should say and builds the device list for the selector.

use audio::DeviceInfo;

/// What is wrong with the microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicProblem {
    /// The OS refused access.
    PermissionDenied,
    /// No input device exists.
    NoDevice,
    /// Anything else.
    Other,
}

/// Classifies a capture error message.
#[must_use]
pub fn classify(message: &str) -> MicProblem {
    let m = message.to_lowercase();
    if m.contains("permission")
        || m.contains("denied")
        || m.contains("not authorized")
        || m.contains("access is")
        || m.contains("eacces")
    {
        MicProblem::PermissionDenied
    } else if m.contains("no audio device") || m.contains("no device") || m.contains("not found") {
        MicProblem::NoDevice
    } else {
        MicProblem::Other
    }
}

/// Deep link to the OS microphone privacy page, where one exists.
#[must_use]
pub fn privacy_settings_url() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
    } else if cfg!(target_os = "windows") {
        Some("ms-settings:privacy-microphone")
    } else {
        None
    }
}

/// One selectable device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceChoice {
    /// Stable id stored in the settings; `None` = follow the system default.
    pub id: Option<String>,
    /// Label shown to the user.
    pub name: String,
    /// The system default device.
    pub is_default: bool,
}

/// Builds the selector entries: "system default" first, then every device.
#[must_use]
pub fn choices(devices: &[DeviceInfo], default_label: &str) -> Vec<DeviceChoice> {
    let mut out = vec![DeviceChoice {
        id: None,
        name: default_label.to_owned(),
        is_default: true,
    }];
    out.extend(devices.iter().map(|d| DeviceChoice {
        id: Some(d.id.clone()),
        name: d.name.clone(),
        is_default: d.is_default,
    }));
    out
}

/// True when the stored device is still present (otherwise fall back to the default).
#[must_use]
pub fn is_available(devices: &[DeviceInfo], stored: Option<&str>) -> bool {
    stored.is_none_or(|id| devices.iter().any(|d| d.id == id || d.name == id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, def: bool) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: format!("Mic {id}"),
            is_default: def,
        }
    }

    #[test]
    fn classifies_messages() {
        assert_eq!(
            classify("Permission denied (os error 13)"),
            MicProblem::PermissionDenied
        );
        assert_eq!(
            classify("device not authorized by TCC"),
            MicProblem::PermissionDenied
        );
        assert_eq!(
            classify("no audio device available: none"),
            MicProblem::NoDevice
        );
        assert_eq!(classify("stream exploded"), MicProblem::Other);
    }

    #[test]
    fn default_comes_first_and_stored_devices_are_checked() {
        let devices = [dev("a", false), dev("b", true)];
        let c = choices(&devices, "Predeterminado");
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].id, None);
        assert!(is_available(&devices, None));
        assert!(is_available(&devices, Some("b")));
        assert!(!is_available(&devices, Some("gone")));
    }
}
