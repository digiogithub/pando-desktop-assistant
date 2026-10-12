//! Host and device lookup.

use cpal::traits::{DeviceTrait, HostTrait};

use crate::AudioError;

/// A selectable audio device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Stable id (falls back to the name when the backend has no id).
    pub id: String,
    /// Human readable name.
    pub name: String,
    pub is_default: bool,
}

/// Names of the compiled-in hosts (`Alsa`, `PulseAudio`, `PipeWire`, ...).
pub fn available_hosts() -> Vec<String> {
    cpal::available_hosts()
        .into_iter()
        .map(|h| format!("{h:?}"))
        .collect()
}

/// Open a host by name (case-insensitive); `None` is the platform default
/// (PipeWire when compiled in, then PulseAudio, then ALSA).
pub fn open_host(name: Option<&str>) -> Result<cpal::Host, AudioError> {
    match name.filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("default")) {
        None => Ok(cpal::default_host()),
        Some(n) => {
            let id = cpal::available_hosts()
                .into_iter()
                .find(|h| format!("{h:?}").eq_ignore_ascii_case(n))
                .ok_or_else(|| AudioError::HostUnavailable(n.to_string()))?;
            cpal::host_from_id(id).map_err(|e| AudioError::Backend(e.to_string()))
        }
    }
}

fn info(d: &cpal::Device, default_id: Option<&str>) -> DeviceInfo {
    let name = d.description().map(|x| x.to_string()).unwrap_or_default();
    let id = d
        .id()
        .map(|i| i.to_string())
        .unwrap_or_else(|_| name.clone());
    DeviceInfo {
        is_default: default_id.is_some_and(|x| x == id),
        id,
        name,
    }
}

fn list(host: &cpal::Host, input: bool) -> Vec<DeviceInfo> {
    let default = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    };
    let default_id = default.as_ref().map(|d| info(d, None).id);
    let devices: Vec<cpal::Device> = if input {
        host.input_devices()
            .map(Iterator::collect)
            .unwrap_or_default()
    } else {
        host.output_devices()
            .map(Iterator::collect)
            .unwrap_or_default()
    };
    devices
        .iter()
        .map(|d| info(d, default_id.as_deref()))
        .collect()
}

pub fn list_input_devices(host: Option<&str>) -> Result<Vec<DeviceInfo>, AudioError> {
    Ok(list(&open_host(host)?, true))
}

pub fn list_output_devices(host: Option<&str>) -> Result<Vec<DeviceInfo>, AudioError> {
    Ok(list(&open_host(host)?, false))
}

/// Find a device by id or name; `None` selects the host default.
pub(crate) fn find_device(
    host: &cpal::Host,
    wanted: Option<&str>,
    input: bool,
) -> Result<cpal::Device, AudioError> {
    let default = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    };
    let Some(w) = wanted.filter(|w| !w.is_empty()) else {
        return default.ok_or_else(|| AudioError::NoDevice("no default device".into()));
    };
    let devices: Vec<cpal::Device> = if input {
        host.input_devices()
            .map(Iterator::collect)
            .unwrap_or_default()
    } else {
        host.output_devices()
            .map(Iterator::collect)
            .unwrap_or_default()
    };
    devices
        .into_iter()
        .find(|d| {
            let i = info(d, None);
            i.id == w || i.name == w
        })
        .ok_or_else(|| AudioError::NoDevice(format!("device `{w}` not found")))
}
