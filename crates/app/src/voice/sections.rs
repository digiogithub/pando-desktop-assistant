//! The `voice` and `audio` sections of the settings window (PDA-T-0126 registry, PDA-T-0138).

use std::rc::Rc;

use rust_i18n::t;

use super::controller::VoiceController;
use crate::settings_view::{Section, register_section};
use crate::ui::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, Render, Subscription, Window,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Voice,
    Audio,
}

struct Panel {
    ctl: Entity<VoiceController>,
    kind: Kind,
    _sub: Subscription,
}

impl Render for Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (voice, audio) = (self.kind == Kind::Voice, self.kind == Kind::Audio);
        self.ctl.update(cx, |c, cx| {
            if audio {
                c.refresh_devices_if_empty();
                c.keep_ticking(cx);
            }
            c.settings_body(cx, voice, audio)
        })
    }
}

struct VoiceSection {
    panel: Entity<Panel>,
    id: &'static str,
    title_key: &'static str,
}

impl Section for VoiceSection {
    fn id(&self) -> &'static str {
        self.id
    }

    fn title(&self) -> String {
        t!(self.title_key).to_string()
    }

    fn render(&self, _window: &mut Window, _cx: &mut App) -> AnyElement {
        self.panel.clone().into_any_element()
    }
}

/// Registers the `voice` and `audio` sections of the settings window.
pub fn register(cx: &mut App, ctl: &Entity<VoiceController>) {
    for (kind, id, title_key) in [
        (Kind::Voice, "voice", "voice.settings.title"),
        (Kind::Audio, "audio", "voice.settings.audio_title"),
    ] {
        let ctl = ctl.clone();
        let panel = cx.new(|cx| Panel {
            _sub: cx.observe(&ctl, |_, _, cx| cx.notify()),
            ctl,
            kind,
        });
        register_section(
            cx,
            Rc::new(VoiceSection {
                panel,
                id,
                title_key,
            }),
        );
    }
}
