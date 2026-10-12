//! The agents the decision model can name. Spoken names are matched loosely by the model, so
//! every agent gets a speakable, unique, kebab-case slug (`migracion-auth`) used as the
//! criterion key; the directory maps the slug back to the Pando task / session.

use decide::questions::AgentInfo;

/// One addressable agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    /// Speakable unique id.
    pub slug: String,
    /// Cell / task id in Pando (`primary` for the primary agent).
    pub cell_id: String,
    /// Display label.
    pub label: String,
    /// Status word (`running`, `waiting`, ...).
    pub status: String,
    /// Chat / ACP session, when known (approvals are keyed by it).
    pub session: Option<String>,
}

/// A snapshot of the agents at the moment a sentence was understood.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentDirectory {
    entries: Vec<AgentEntry>,
}

/// Lowercase, accent-free, dash-separated.
#[must_use]
pub fn slugify(label: &str) -> String {
    let mut out = String::new();
    let mut dash = true;
    for c in label.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            other => other,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out.chars()
        .take(40)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned()
}

impl AgentDirectory {
    /// Builds the directory from `(cell_id, label, status, session)` rows, deduplicating slugs
    /// (`build`, `build-2`, ...) and skipping the reserved `all` / `none`.
    pub fn from_rows(
        rows: impl IntoIterator<Item = (String, String, String, Option<String>)>,
    ) -> Self {
        let mut entries: Vec<AgentEntry> = Vec::new();
        for (cell_id, label, status, session) in rows {
            let mut base = slugify(&label);
            if base.is_empty() || base == "all" || base == "none" {
                base = format!("agent-{}", entries.len() + 1);
            }
            let mut slug = base.clone();
            let mut n = 2;
            while entries.iter().any(|e| e.slug == slug) {
                slug = format!("{base}-{n}");
                n += 1;
            }
            entries.push(AgentEntry {
                slug,
                cell_id,
                label,
                status,
                session,
            });
        }
        Self { entries }
    }

    /// Entries in order.
    #[must_use]
    pub fn entries(&self) -> &[AgentEntry] {
        &self.entries
    }

    /// True when no agent exists.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry for a slug.
    #[must_use]
    pub fn resolve(&self, slug: &str) -> Option<&AgentEntry> {
        self.entries.iter().find(|e| e.slug == slug)
    }

    /// The entry owning a session.
    #[must_use]
    pub fn by_session(&self, session: &str) -> Option<&AgentEntry> {
        self.entries
            .iter()
            .find(|e| e.session.as_deref() == Some(session))
    }

    /// The agents as the decision layer sees them.
    #[must_use]
    pub fn infos(&self) -> Vec<AgentInfo> {
        self.entries
            .iter()
            .map(|e| AgentInfo {
                id: e.slug.clone(),
                name: e.label.clone(),
                status: e.status.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, label: &str) -> (String, String, String, Option<String>) {
        (id.into(), label.into(), "running".into(), None)
    }

    #[test]
    fn slugs_are_speakable_and_unique() {
        assert_eq!(slugify("Migración Auth!"), "migracion-auth");
        assert_eq!(slugify("  --  "), "");
        let d = AgentDirectory::from_rows([
            row("1", "Build"),
            row("2", "build"),
            row("3", "all"),
            row("4", ""),
        ]);
        let slugs: Vec<_> = d.entries().iter().map(|e| e.slug.as_str()).collect();
        assert_eq!(slugs, ["build", "build-2", "agent-3", "agent-4"]);
        assert_eq!(d.resolve("build-2").map(|e| e.cell_id.as_str()), Some("2"));
        assert_eq!(d.infos()[0].name, "Build");
    }

    #[test]
    fn sessions_map_back_to_agents() {
        let d = AgentDirectory::from_rows([(
            "1".into(),
            "A".into(),
            "waiting".into(),
            Some("s1".into()),
        )]);
        assert_eq!(d.by_session("s1").map(|e| e.slug.as_str()), Some("a"));
        assert!(d.by_session("zz").is_none());
    }
}
