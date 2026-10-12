//! State of the session sidebar (PDA-T-0064): pages, local search, running dots, rename and
//! delete, all pure so a thousand sessions are cheap to filter and test.

use pando_client::chat::{Session, SessionList, SessionSummary};

/// Sessions per page ("cargar más" fetches the next one).
pub const PAGE_SIZE: u32 = 100;

/// Age bucket used to group the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bucket {
    /// Updated today.
    Today,
    /// Updated yesterday.
    Yesterday,
    /// Within the last seven days.
    Week,
    /// Older.
    Older,
}

impl Bucket {
    /// Locale key suffix.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Yesterday => "yesterday",
            Self::Week => "week",
            Self::Older => "older",
        }
    }
}

/// Buckets a Unix timestamp relative to `now` (both UTC seconds; day boundaries are UTC).
#[must_use]
pub fn bucket(updated_at: i64, now: i64) -> Bucket {
    let day = |t: i64| t.div_euclid(86_400);
    match day(now) - day(updated_at) {
        d if d <= 0 => Bucket::Today,
        1 => Bucket::Yesterday,
        2..=6 => Bucket::Week,
        _ => Bucket::Older,
    }
}

/// One line of the rendered list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRow {
    /// A bucket header.
    Header(Bucket),
    /// A session, by index into [`SessionsState::rows`].
    Session(usize),
    /// The "load more" row.
    LoadMore,
}

/// The sidebar's data.
#[derive(Debug, Clone, Default)]
pub struct SessionsState {
    rows: Vec<SessionSummary>,
    total: i64,
    has_more: bool,
    query: String,
    loading: bool,
    error: Option<String>,
}

/// The title to show for a session, `None` when it has neither title nor preview.
#[must_use]
pub fn display_title(s: &SessionSummary) -> Option<String> {
    let t = s.session.title.trim();
    if !t.is_empty() {
        return Some(t.to_owned());
    }
    s.prompt_preview
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| p.chars().take(80).collect())
}

impl SessionsState {
    /// Loaded sessions, newest first.
    #[must_use]
    pub fn rows(&self) -> &[SessionSummary] {
        &self.rows
    }

    /// Sessions on the server.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.total
    }

    /// More pages exist.
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.has_more
    }

    /// A page request is in flight.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Marks a request as started/finished.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// The last load error.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Records a load failure.
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
        self.loading = false;
    }

    /// Offset of the next page.
    #[must_use]
    pub fn next_offset(&self) -> u32 {
        u32::try_from(self.rows.len()).unwrap_or(u32::MAX)
    }

    /// The local search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Sets the local search text.
    pub fn set_query(&mut self, q: &str) {
        self.query = q.trim().to_owned();
    }

    /// Replaces everything with the first page.
    pub fn replace(&mut self, page: SessionList) {
        self.rows = page.sessions;
        self.total = page.total;
        self.has_more = page.has_more;
        self.loading = false;
        self.error = None;
    }

    /// Appends a further page, skipping ids already present.
    pub fn append(&mut self, page: SessionList) {
        for s in page.sessions {
            if !self.rows.iter().any(|r| r.session.id == s.session.id) {
                self.rows.push(s);
            }
        }
        self.total = page.total;
        self.has_more = page.has_more;
        self.loading = false;
        self.error = None;
    }

    /// Applies a refreshed first page without dropping the pages loaded below it: the head is
    /// replaced, the tail beyond it is kept (minus duplicates).
    pub fn refresh_head(&mut self, page: SessionList) {
        let head_ids: Vec<String> = page.sessions.iter().map(|s| s.session.id.clone()).collect();
        let tail: Vec<SessionSummary> = self
            .rows
            .iter()
            .skip(page.sessions.len())
            .filter(|s| !head_ids.contains(&s.session.id))
            .cloned()
            .collect();
        let had_tail = !tail.is_empty();
        self.rows = page.sessions;
        self.rows.extend(tail);
        self.total = page.total;
        // Keep "load more" when pages below the head were loaded and there may be more.
        self.has_more = if had_tail {
            i64::try_from(self.rows.len()).unwrap_or(i64::MAX) < page.total
        } else {
            page.has_more
        };
        self.loading = false;
        self.error = None;
    }

    /// A session created by the first `session` event appears at the top right away.
    pub fn insert_new(&mut self, id: &str, title: &str, running: bool) {
        if self.rows.iter().any(|s| s.session.id == id) {
            return;
        }
        self.rows.insert(
            0,
            SessionSummary {
                session: Session {
                    id: id.to_owned(),
                    title: title.to_owned(),
                    ..Session::default()
                },
                is_running: running,
                ..SessionSummary::default()
            },
        );
        self.total += 1;
    }

    /// Updates the cached title.
    pub fn set_title(&mut self, id: &str, title: &str) {
        if let Some(s) = self.rows.iter_mut().find(|s| s.session.id == id) {
            title.clone_into(&mut s.session.title);
        }
    }

    /// Updates the running flag.
    pub fn set_running(&mut self, id: &str, running: bool) {
        if let Some(s) = self.rows.iter_mut().find(|s| s.session.id == id) {
            s.is_running = running;
        }
    }

    /// Removes a session (deleted).
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.rows.len();
        self.rows.retain(|s| s.session.id != id);
        let removed = self.rows.len() != before;
        if removed {
            self.total = (self.total - 1).max(0);
        }
        removed
    }

    /// Looks a session up.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SessionSummary> {
        self.rows.iter().find(|s| s.session.id == id)
    }

    /// Indices of the rows matching the local search (title, preview and id), in list order.
    #[must_use]
    pub fn matching(&self) -> Vec<usize> {
        let q = self.query.to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                q.is_empty()
                    || display_title(s).is_some_and(|t| t.to_lowercase().contains(&q))
                    || s.prompt_preview
                        .as_deref()
                        .is_some_and(|p| p.to_lowercase().contains(&q))
                    || s.session.id.to_lowercase().starts_with(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The flat list the sidebar draws: bucket headers, sessions and a final "load more".
    #[must_use]
    pub fn layout(&self, now: i64) -> Vec<ListRow> {
        let mut out = Vec::new();
        let mut last: Option<Bucket> = None;
        for i in self.matching() {
            let b = bucket(self.rows[i].session.updated_at, now);
            if last != Some(b) {
                out.push(ListRow::Header(b));
                last = Some(b);
            }
            out.push(ListRow::Session(i));
        }
        if self.has_more {
            out.push(ListRow::LoadMore);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: &str, title: &str, updated: i64) -> SessionSummary {
        SessionSummary {
            session: Session {
                id: id.into(),
                title: title.into(),
                updated_at: updated,
                ..Session::default()
            },
            ..SessionSummary::default()
        }
    }

    fn page(n: usize, offset: usize, total: i64) -> SessionList {
        SessionList {
            sessions: (offset..offset + n)
                .map(|i| {
                    summary(
                        &format!("s{i}"),
                        &format!("Session {i}"),
                        1_000_000 - i as i64,
                    )
                })
                .collect(),
            total,
            has_more: ((offset + n) as i64) < total,
            ..SessionList::default()
        }
    }

    #[test]
    fn buckets_by_utc_day() {
        let now = 10 * 86_400 + 3_600;
        assert_eq!(bucket(now - 10, now), Bucket::Today);
        assert_eq!(bucket(9 * 86_400 + 5, now), Bucket::Yesterday);
        assert_eq!(bucket(6 * 86_400, now), Bucket::Week);
        assert_eq!(bucket(2 * 86_400, now), Bucket::Older);
    }

    #[test]
    fn pages_append_without_duplicates() {
        let mut s = SessionsState::default();
        s.replace(page(100, 0, 250));
        assert!(s.has_more());
        assert_eq!(s.next_offset(), 100);
        s.append(page(100, 100, 250));
        s.append(page(100, 100, 250)); // a retried page adds nothing
        assert_eq!(s.rows().len(), 200);
        s.append(page(50, 200, 250));
        assert!(!s.has_more());
    }

    #[test]
    fn refresh_keeps_the_loaded_tail() {
        let mut s = SessionsState::default();
        s.replace(page(100, 0, 250));
        s.append(page(100, 100, 250));
        let mut fresh = page(100, 0, 251);
        fresh
            .sessions
            .insert(0, summary("new", "Brand new", 2_000_000));
        fresh.sessions.pop();
        s.refresh_head(fresh);
        assert_eq!(s.rows()[0].session.id, "new");
        assert_eq!(
            s.rows().len(),
            200,
            "tail kept, duplicates dropped: {}",
            s.rows().len()
        );
        assert!(s.has_more());
    }

    #[test]
    fn a_new_session_appears_at_the_top_once() {
        let mut s = SessionsState::default();
        s.replace(page(3, 0, 3));
        s.insert_new("fresh", "", true);
        s.insert_new("fresh", "", true);
        assert_eq!(s.rows()[0].session.id, "fresh");
        assert_eq!(s.rows().len(), 4);
        assert!(s.rows()[0].is_running);
        s.set_title("fresh", "Hello");
        assert_eq!(display_title(&s.rows()[0]).as_deref(), Some("Hello"));
    }

    #[test]
    fn search_is_local_and_case_insensitive() {
        let mut s = SessionsState::default();
        s.replace(page(5, 0, 5));
        s.set_query("  SESSION 3 ");
        assert_eq!(s.matching(), vec![3]);
        s.set_query("s4");
        assert_eq!(s.matching(), vec![4], "id prefix also matches");
        s.set_query("");
        assert_eq!(s.matching().len(), 5);
    }

    #[test]
    fn untitled_sessions_fall_back_to_the_preview() {
        let mut x = summary("a", "", 0);
        assert_eq!(display_title(&x), None);
        x.prompt_preview = Some("  fix the build ".into());
        assert_eq!(display_title(&x).as_deref(), Some("fix the build"));
    }

    #[test]
    fn remove_updates_the_total() {
        let mut s = SessionsState::default();
        s.replace(page(3, 0, 3));
        assert!(s.remove("s1"));
        assert!(!s.remove("s1"));
        assert_eq!((s.rows().len(), s.total()), (2, 2));
    }

    #[test]
    fn thousand_sessions_layout_is_cheap() {
        let mut s = SessionsState::default();
        for p in 0..10 {
            s.append(page(100, p * 100, 1000));
        }
        let t = std::time::Instant::now();
        let rows = s.layout(1_000_000);
        assert!(rows.len() >= 1000);
        assert!(t.elapsed() < std::time::Duration::from_millis(250));
    }

    #[test]
    fn layout_groups_and_ends_with_load_more() {
        let mut s = SessionsState::default();
        let mut p = page(2, 0, 5);
        p.sessions[0].session.updated_at = 5 * 86_400;
        p.sessions[1].session.updated_at = 4 * 86_400;
        s.replace(p);
        let rows = s.layout(5 * 86_400 + 10);
        assert_eq!(rows[0], ListRow::Header(Bucket::Today));
        assert_eq!(rows[2], ListRow::Header(Bucket::Yesterday));
        assert_eq!(rows.last(), Some(&ListRow::LoadMore));
    }
}
