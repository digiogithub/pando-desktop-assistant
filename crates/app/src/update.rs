//! Auto-update with Velopack (PDA-T-0150).
//!
//! * [`init`] runs `VelopackApp::build().run()` first in `main` (install / update / uninstall
//!   hooks exit the process there).
//! * [`availability`] decides whether updating makes sense: not under Flatpak or a distro
//!   package (not installed by Velopack), not without a feed URL, not when disabled.
//! * [`start`] checks every `interval_hours` on a worker thread, downloads in the background
//!   and tells the shell (`Msg::UpdateReady`) so a toast offers "Reiniciar para actualizar";
//!   [`apply_and_restart`] applies the downloaded package.
//!
//! Pending: update from vX to vY on all 3 OSes needs real packaged releases (`vpk pack`) and a
//! hosted feed; the release pipeline (xtask release) owns that.

use std::sync::Mutex;
use std::time::Duration;

use async_channel::Sender;
use velopack::{UpdateCheck, UpdateManager, VelopackApp, VelopackAsset, sources::HttpSource};

use crate::settings::UpdateSettings;
use crate::shell::controller::Msg;

/// Must be the first call of `main`.
pub fn init() {
    VelopackApp::build().run();
}

/// Why updates are off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disabled {
    /// The user turned them off.
    Setting,
    /// No feed URL configured.
    NoFeed,
    /// Running inside Flatpak (the store updates it).
    Flatpak,
    /// Not installed by Velopack (deb/rpm/dev build): the package manager updates it.
    NotInstalled,
}

/// Whether the update service should run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// Check and download.
    Enabled,
    /// Off, and why.
    Disabled(Disabled),
}

/// Pure decision (`installed_by_velopack` comes from [`installed_by_velopack`]).
#[must_use]
pub fn decide(s: &UpdateSettings, flatpak: bool, installed_by_velopack: bool) -> Availability {
    if !s.enabled {
        Availability::Disabled(Disabled::Setting)
    } else if flatpak {
        Availability::Disabled(Disabled::Flatpak)
    } else if s.url.as_deref().is_none_or(|u| u.trim().is_empty()) {
        Availability::Disabled(Disabled::NoFeed)
    } else if !installed_by_velopack {
        Availability::Disabled(Disabled::NotInstalled)
    } else {
        Availability::Enabled
    }
}

/// Running under Flatpak (`FLATPAK_ID` or `/.flatpak-info`).
#[must_use]
pub fn is_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists()
}

/// Whether this binary was installed by Velopack (its manifest can be located).
#[must_use]
pub fn installed_by_velopack() -> bool {
    UpdateManager::new(HttpSource::new("http://localhost"), None, None).is_ok()
}

/// Availability for the running process.
#[must_use]
pub fn availability(s: &UpdateSettings) -> Availability {
    decide(s, is_flatpak(), installed_by_velopack())
}

static PENDING: Mutex<Option<(UpdateManager, VelopackAsset)>> = Mutex::new(None);

/// Starts the background checker; `Err(reason)` when updates are off.
pub fn start(s: &UpdateSettings, inbox: Sender<Msg>) -> Result<(), Disabled> {
    if let Availability::Disabled(why) = availability(s) {
        return Err(why);
    }
    let url = s.url.clone().unwrap_or_default();
    let every = Duration::from_secs(u64::from(s.interval_hours.max(1)) * 3600);
    let _ = std::thread::Builder::new()
        .name("pando-update".into())
        .spawn(move || {
            // First check a minute after startup, so it never competes with it.
            std::thread::sleep(Duration::from_secs(60));
            loop {
                if let Err(e) = check_once(&url, &inbox) {
                    tracing::warn!("update check failed: {e}");
                }
                if inbox.is_closed() {
                    return;
                }
                std::thread::sleep(every);
            }
        });
    Ok(())
}

fn check_once(url: &str, inbox: &Sender<Msg>) -> Result<(), velopack::Error> {
    let um = UpdateManager::new(HttpSource::new(url), None, None)?;
    if let Some(asset) = um.get_update_pending_restart() {
        announce(um, asset, inbox);
        return Ok(());
    }
    if let UpdateCheck::UpdateAvailable(info) = um.check_for_updates()? {
        um.download_updates(&info, None)?;
        announce(um, info.TargetFullRelease.clone(), inbox);
    }
    Ok(())
}

fn announce(um: UpdateManager, asset: VelopackAsset, inbox: &Sender<Msg>) {
    let version = asset.Version.to_string();
    if let Ok(mut p) = PENDING.lock() {
        *p = Some((um, asset));
    }
    let _ = inbox.try_send(Msg::UpdateReady { version });
}

/// Applies the downloaded update and restarts (the process exits on success).
pub fn apply_and_restart() -> Result<(), String> {
    let pending = PENDING.lock().map_err(|e| e.to_string())?.take();
    let Some((um, asset)) = pending else {
        return Err("no update downloaded".into());
    };
    um.apply_updates_and_restart(&asset)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(enabled: bool, url: Option<&str>) -> UpdateSettings {
        UpdateSettings {
            enabled,
            url: url.map(str::to_owned),
            interval_hours: 6,
        }
    }

    #[test]
    fn availability_rules() {
        let url = Some("https://updates.example/pando");
        assert_eq!(decide(&s(true, url), false, true), Availability::Enabled);
        assert_eq!(
            decide(&s(false, url), false, true),
            Availability::Disabled(Disabled::Setting)
        );
        assert_eq!(
            decide(&s(true, url), true, true),
            Availability::Disabled(Disabled::Flatpak)
        );
        assert_eq!(
            decide(&s(true, None), false, true),
            Availability::Disabled(Disabled::NoFeed)
        );
        assert_eq!(
            decide(&s(true, Some("  ")), false, true),
            Availability::Disabled(Disabled::NoFeed)
        );
        assert_eq!(
            decide(&s(true, url), false, false),
            Availability::Disabled(Disabled::NotInstalled)
        );
    }

    #[test]
    fn a_dev_build_is_not_installed_by_velopack() {
        assert!(!installed_by_velopack());
        assert!(apply_and_restart().is_err());
    }
}
