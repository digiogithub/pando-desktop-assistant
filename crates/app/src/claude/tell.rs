//! "Dile a <sesión> …" (PDA-T-0134): the voice route to send a message to a Claude Code
//! session. The decision layer has no `message` action, so the app recognises the lexical
//! pattern itself and resolves the session by name, never guessing between two sessions.

use claude_bridge::ExternalCell;

use crate::decision::directory::slugify;

/// A parsed "dile a X ..." sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tell {
    /// What the user called the session.
    pub target: String,
    /// What to say to it.
    pub body: String,
}

/// Parses `dile a <name> [que|:|,] <body>` / `dile a la sesión <name> ...`.
#[must_use]
pub fn parse(text: &str) -> Option<Tell> {
    let t = text.trim();
    let lower = t.to_lowercase();
    let rest = lower
        .strip_prefix("dile a ")
        .or_else(|| lower.strip_prefix("dile al "))?;
    let offset = lower.len() - rest.len();
    let original = &t[offset..];
    let original = original
        .strip_prefix("la sesión ")
        .or_else(|| original.strip_prefix("la sesion "))
        .or_else(|| original.strip_prefix("sesión "))
        .or_else(|| original.strip_prefix("sesion "))
        .unwrap_or(original);
    // The name ends at the first separator: ":" / "," or the word "que".
    let (target, body) = if let Some(i) = original.find([':', ',']) {
        (&original[..i], &original[i + 1..])
    } else {
        let lower_o = original.to_lowercase();
        let i = lower_o.find(" que ")?;
        (&original[..i], &original[i + 5..])
    };
    let (target, body) = (target.trim(), body.trim());
    if target.is_empty() || body.is_empty() {
        return None;
    }
    Some(Tell {
        target: target.to_owned(),
        body: body.to_owned(),
    })
}

/// The unique session whose slug equals the spoken name, else the unique one that contains it
/// (or is contained by it). Two candidates are ambiguous: `None`.
#[must_use]
pub fn best_session(sessions: &[ExternalCell], spoken: &str) -> Option<ExternalCell> {
    let want = slugify(spoken);
    if want.is_empty() {
        return None;
    }
    let exact: Vec<_> = sessions
        .iter()
        .filter(|s| slugify(&s.label) == want)
        .collect();
    if exact.len() == 1 {
        return exact.first().map(|s| (*s).clone());
    }
    if !exact.is_empty() {
        return None;
    }
    let loose: Vec<_> = sessions
        .iter()
        .filter(|s| {
            let slug = slugify(&s.label);
            !slug.is_empty() && (slug.contains(&want) || want.contains(&slug))
        })
        .collect();
    if loose.len() == 1 {
        loose.first().map(|s| (*s).clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use claude_bridge::CellStatus;

    use super::*;

    fn s(label: &str) -> ExternalCell {
        ExternalCell {
            session_id: label.into(),
            label: label.into(),
            cwd: "/".into(),
            project_id: None,
            status: CellStatus::Working,
            summary: None,
            waiting_message: None,
            messaging: None,
            notify_when_done: false,
        }
    }

    #[test]
    fn parses_the_spoken_forms() {
        assert_eq!(
            parse("dile a api que ejecute los tests"),
            Some(Tell {
                target: "api".into(),
                body: "ejecute los tests".into()
            })
        );
        assert_eq!(
            parse("Dile a la sesión Migración Auth: revisa los índices"),
            Some(Tell {
                target: "Migración Auth".into(),
                body: "revisa los índices".into()
            })
        );
        assert_eq!(parse("dile a api"), None);
        assert_eq!(parse("dile a api que"), None);
        assert_eq!(parse("cancela api"), None);
    }

    #[test]
    fn sessions_resolve_uniquely_or_not_at_all() {
        let all = [s("api"), s("api-web"), s("docs")];
        assert_eq!(
            best_session(&all, "API").map(|c| c.session_id),
            Some("api".into())
        );
        assert_eq!(
            best_session(&all, "docs").map(|c| c.session_id),
            Some("docs".into())
        );
        // "web" only appears in api-web.
        assert_eq!(
            best_session(&all, "web").map(|c| c.session_id),
            Some("api-web".into())
        );
        // "a" is contained in several: ambiguous.
        assert!(best_session(&all, "p").is_none());
        assert!(best_session(&all, "nada").is_none());
        assert!(
            best_session(&[s("x"), s("x")], "x").is_none(),
            "two exact matches are ambiguous"
        );
    }
}
