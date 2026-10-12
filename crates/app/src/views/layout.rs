//! Main-window layouts (PDA-T-0116): Orb (organism), Chat, Split (both with a draggable divider).
//! Pure helpers; the layout and the split ratio are persisted in the settings.

use serde::{Deserialize, Serialize};

crate::ui::actions!(layout, [ShowOrb, ShowChat, ShowSplit]);

/// Key context of the main view.
pub const KEY_CONTEXT: &str = "MainView";

/// Registers Ctrl+1 / Ctrl+2 / Ctrl+3 (Cmd on macOS) for the three layouts.
pub fn bind_keys(cx: &mut crate::ui::App) {
    let m = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let table: [(String, Box<dyn crate::ui::Action>); 3] = [
        (format!("{m}-1"), Box::new(ShowOrb)),
        (format!("{m}-2"), Box::new(ShowChat)),
        (format!("{m}-3"), Box::new(ShowSplit)),
    ];
    let bindings: Vec<_> = table
        .into_iter()
        .filter_map(|(k, a)| crate::ui::key_binding(&k, a, Some(KEY_CONTEXT)).ok())
        .collect();
    cx.bind_keys(bindings);
}

/// What the body shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// The organism with its chrome.
    #[default]
    Orb,
    /// The chat only.
    Chat,
    /// Organism and chat side by side.
    Split,
}

impl Layout {
    /// All layouts in shortcut order (Ctrl+1, Ctrl+2, Ctrl+3).
    pub const ALL: [Layout; 3] = [Layout::Orb, Layout::Chat, Layout::Split];

    /// Locale id (`layout.<id>`).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Orb => "orb",
            Self::Chat => "chat",
            Self::Split => "split",
        }
    }

    /// The organism is on screen (otherwise its frame loop is not driven at all).
    #[must_use]
    pub fn shows_organism(self) -> bool {
        matches!(self, Self::Orb | Self::Split)
    }

    /// The chat is on screen.
    #[must_use]
    pub fn shows_chat(self) -> bool {
        matches!(self, Self::Chat | Self::Split)
    }

    /// The next layout in the cycle.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Orb => Self::Chat,
            Self::Chat => Self::Split,
            Self::Split => Self::Orb,
        }
    }
}

/// Smallest and largest share of the width the organism side may take, in permille.
pub const SPLIT_MIN: u16 = 250;
/// See [`SPLIT_MIN`].
pub const SPLIT_MAX: u16 = 750;
/// Default share of the organism side.
pub const SPLIT_DEFAULT: u16 = 550;

/// Keeps a split ratio inside its bounds.
#[must_use]
pub fn clamp_permille(p: i64) -> u16 {
    p.clamp(i64::from(SPLIT_MIN), i64::from(SPLIT_MAX)) as u16
}

/// Ratio (permille) for a divider dragged to `x` inside `[left, left + total]`.
#[must_use]
pub fn permille_from_drag(x: f32, left: f32, total: f32) -> u16 {
    if total <= 1.0 {
        return SPLIT_DEFAULT;
    }
    clamp_permille((((x - left) / total) * 1000.0).round() as i64)
}

/// Widths of the two sides for a total width and ratio, minus a divider of `divider` px.
#[must_use]
pub fn split_widths(total: f32, permille: u16, divider: f32) -> (f32, f32) {
    let usable = (total - divider).max(0.0);
    let left = usable * f32::from(clamp_permille(i64::from(permille))) / 1000.0;
    (left, usable - left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_cycle_and_visibility() {
        assert_eq!(Layout::Orb.next().next().next(), Layout::Orb);
        assert!(Layout::Orb.shows_organism() && !Layout::Orb.shows_chat());
        assert!(!Layout::Chat.shows_organism() && Layout::Chat.shows_chat());
        assert!(Layout::Split.shows_organism() && Layout::Split.shows_chat());
        assert_eq!(Layout::default(), Layout::Orb);
    }

    #[test]
    fn serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&Layout::Split).expect("json"),
            "\"split\""
        );
        assert_eq!(
            serde_json::from_str::<Layout>("\"chat\"").expect("parse"),
            Layout::Chat
        );
    }

    #[test]
    fn drag_is_clamped() {
        assert_eq!(permille_from_drag(500.0, 0.0, 1000.0), 500);
        assert_eq!(permille_from_drag(10.0, 0.0, 1000.0), SPLIT_MIN);
        assert_eq!(permille_from_drag(990.0, 0.0, 1000.0), SPLIT_MAX);
        assert_eq!(permille_from_drag(5.0, 0.0, 0.0), SPLIT_DEFAULT);
        let (l, r) = split_widths(1006.0, 500, 6.0);
        assert!((l - 500.0).abs() < 1e-3 && (r - 500.0).abs() < 1e-3);
    }
}
