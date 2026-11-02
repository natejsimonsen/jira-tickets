//! Pure derivation for the Me screen: the signed-in user's own tickets, in three sections.

use crate::jira::Issue;
use crate::teamview::{Card, days_in_status};
use chrono::{DateTime, Utc};
use std::cmp::Ordering;

/// One query for all three sections, by `currentUser()`, so it needs no config.
pub const JQL: &str = concat!(
    "assignee = currentUser() AND (statusCategory = \"In Progress\"",
    " OR (statusCategory = \"To Do\" AND sprint in openSprints())",
    " OR (statusCategory = Done AND resolved >= -7d)) ORDER BY updated DESC"
);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    InProgress,
    ToDo,
    Done,
}

impl Section {
    pub const ALL: [Section; 3] = [Section::InProgress, Section::ToDo, Section::Done];

    pub fn title(self) -> &'static str {
        match self {
            Section::InProgress => "In progress",
            Section::ToDo => "To do in open sprints",
            Section::Done => "Done in the last 7 days",
        }
    }

    /// The line an empty section shows.
    pub fn empty(self) -> &'static str {
        match self {
            Section::InProgress => "nothing in progress",
            Section::ToDo => "nothing to do in an open sprint",
            Section::Done => "nothing done in the last 7 days",
        }
    }

    /// The section a status category (`Status::category`) belongs in.
    fn of(category: &str) -> Section {
        match category {
            "indeterminate" => Section::InProgress,
            "done" => Section::Done,
            _ => Section::ToDo,
        }
    }
}

/// The three sections in `Section::ALL` order, each possibly empty. In progress and To do keep Jira's rank (the
/// board's order, unranked last); Done is newest first by when its status category changed.
pub fn sections<'a>(issues: &[&'a Issue], today: DateTime<Utc>) -> Vec<(Section, Vec<Card<'a>>)> {
    Section::ALL
        .iter()
        .map(|&s| {
            let mut rows: Vec<&Issue> = issues.iter().copied().filter(|i| Section::of(&i.status.category) == s).collect();
            match s {
                Section::Done => rows.sort_by(|a, b| by_changed_desc(a, b).then_with(|| a.key.cmp(&b.key))),
                _ => rows.sort_by(|a, b| by_rank(a, b).then_with(|| a.key.cmp(&b.key))),
            }
            let cards = rows.into_iter().map(|issue| Card { issue, days_in_status: days_in_status(issue.status_changed.as_deref(), today) }).collect();
            (s, cards)
        })
        .collect()
}

fn by_rank(a: &Issue, b: &Issue) -> Ordering {
    match (a.rank.is_empty(), b.rank.is_empty()) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        _ => a.rank.cmp(&b.rank),
    }
}

fn changed(i: &Issue) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(i.status_changed.as_deref()?, "%Y-%m-%dT%H:%M:%S%.3f%z").ok().map(|t| t.with_timezone(&Utc))
}

fn by_changed_desc(a: &Issue, b: &Issue) -> Ordering {
    match (changed(a), changed(b)) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// How many rows the sections hold.
pub fn row_count(sections: &[(Section, Vec<Card>)]) -> usize {
    sections.iter().map(|(_, c)| c.len()).sum()
}

/// The issue at flat row `row`, counting down through the sections in order.
pub fn issue_at<'a>(sections: &[(Section, Vec<Card<'a>>)], row: usize) -> Option<&'a Issue> {
    sections.iter().flat_map(|(_, c)| c.iter()).nth(row).map(|c| c.issue)
}

/// The cursor after moving `delta` rows, clamped to the rows there are (0 with none).
pub fn moved(cursor: usize, delta: i32, rows: usize) -> usize {
    let last = rows.saturating_sub(1);
    cursor.min(last).saturating_add_signed(delta as isize).min(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::Status;
    use chrono::TimeZone;

    fn issue(key: &str, cat: &str, rank: &str, changed: Option<&str>) -> Issue {
        Issue {
            key: key.into(),
            status: Status { name: "x".into(), category: cat.into() },
            rank: rank.into(),
            status_changed: changed.map(String::from),
            ..Default::default()
        }
    }

    fn today() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap()
    }

    fn keys<'a>(s: &'a [(Section, Vec<Card>)], i: usize) -> Vec<&'a str> {
        s[i].1.iter().map(|c| c.issue.key.as_str()).collect()
    }

    #[test]
    fn jql_uses_the_current_user_and_all_three_sections() {
        assert_eq!(
            JQL,
            r#"assignee = currentUser() AND (statusCategory = "In Progress" OR (statusCategory = "To Do" AND sprint in openSprints()) OR (statusCategory = Done AND resolved >= -7d)) ORDER BY updated DESC"#
        );
    }

    #[test]
    fn sections_group_by_category_and_sort() {
        let a = issue("CI-1", "indeterminate", "0|b", Some("2026-10-30T10:00:00.000+0000"));
        let b = issue("CI-2", "indeterminate", "0|a", None);
        let c = issue("CI-3", "new", "", None);
        let d = issue("CI-4", "new", "0|c", None);
        let e = issue("CI-5", "done", "0|a", Some("2026-10-28T10:00:00.000-0700"));
        let f = issue("CI-6", "done", "0|b", Some("2026-11-01T09:00:00.000+0000"));
        let g = issue("CI-7", "done", "", None);
        let s = sections(&[&a, &b, &c, &d, &e, &f, &g], today());
        assert_eq!(s.iter().map(|(s, _)| *s).collect::<Vec<_>>(), Section::ALL);
        assert_eq!(keys(&s, 0), ["CI-2", "CI-1"], "in progress by rank");
        assert_eq!(keys(&s, 1), ["CI-4", "CI-3"], "unranked last");
        assert_eq!(keys(&s, 2), ["CI-6", "CI-5", "CI-7"], "done newest first, undated last");
        assert_eq!(s[0].1[1].days_in_status, Some(3));
        assert_eq!(row_count(&s), 7);
        assert_eq!(issue_at(&s, 2).map(|i| i.key.as_str()), Some("CI-4"), "rows run on across sections");
        assert!(issue_at(&s, 7).is_none());
    }

    #[test]
    fn empty_sections_stay_listed() {
        let s = sections(&[], today());
        assert_eq!(s.len(), 3);
        assert!(s.iter().all(|(_, c)| c.is_empty()));
        assert_eq!(row_count(&s), 0);
    }

    #[test]
    fn cursor_moves_clamp() {
        assert_eq!(moved(0, 1, 3), 1);
        assert_eq!(moved(2, 5, 3), 2);
        assert_eq!(moved(1, -9, 3), 0);
        assert_eq!(moved(9, 0, 3), 2, "a cursor past a shrunk list comes back to its end");
        assert_eq!(moved(4, 1, 0), 0);
    }

    #[test]
    fn wording() {
        assert_eq!(Section::InProgress.title(), "In progress");
        assert_eq!(Section::ToDo.empty(), "nothing to do in an open sprint");
    }
}
