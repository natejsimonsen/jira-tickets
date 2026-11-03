//! Pure derivation for the Team screen: one column per configured member.

use crate::jira::Issue;
use crate::store::jql_quote;
use chrono::{DateTime, Utc};

const UNASSIGNED_CAP: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub id: String,
    pub display: String,
    /// Their Jira account id, from a matching ticket's assignee (the configured `id` may be a display name).
    pub account_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Card<'a> {
    pub issue: &'a Issue,
    pub days_in_status: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Column<'a> {
    /// `None` is the Unassigned column.
    pub member: Option<Member>,
    pub in_progress: Vec<Card<'a>>,
    pub ready: Vec<Card<'a>>,
    pub none_found: bool,
}

/// True when the issue's assignee account id or display name equals `member_id`, case-insensitively.
pub fn matches(issue: &Issue, member_id: &str) -> bool {
    issue.assignee.as_ref().is_some_and(|u| u.account_id.eq_ignore_ascii_case(member_id) || u.display_name.eq_ignore_ascii_case(member_id))
}

/// Whole days between the Jira status-change date and `today`; `None` when absent or unparseable.
pub fn days_in_status(status_changed: Option<&str>, today: DateTime<Utc>) -> Option<i64> {
    let changed = DateTime::parse_from_str(status_changed?, "%Y-%m-%dT%H:%M:%S%.3f%z").ok()?;
    Some((today.date_naive() - changed.with_timezone(&Utc).date_naive()).num_days())
}

pub fn columns<'a>(members: &[String], in_progress: &[&'a Issue], team_open: &[&'a Issue], status_ready: &str, today: DateTime<Utc>) -> Vec<Column<'a>> {
    let card = |issue: &'a Issue| Card { issue, days_in_status: days_in_status(issue.status_changed.as_deref(), today) };
    let mut cols: Vec<Column<'a>> = members
        .iter()
        .map(|id| {
            let mut seen = std::collections::HashSet::new();
            let wip: Vec<Card<'a>> = in_progress
                .iter()
                .copied()
                .filter(|i| matches(i, id) && i.status.category == "indeterminate" && seen.insert(i.key.as_str()))
                .map(card)
                .collect();
            let ready: Vec<Card<'a>> = team_open.iter().copied().filter(|i| i.status.name == status_ready && matches(i, id)).map(card).collect();
            let any_match = in_progress.iter().chain(team_open).find(|i| matches(i, id));
            let who = any_match.and_then(|i| i.assignee.as_ref());
            let display = who.map_or_else(|| id.clone(), |u| u.display_name.clone());
            let account_id = who.map(|u| u.account_id.clone());
            let none_found = wip.is_empty() && ready.is_empty() && any_match.is_none();
            Column { member: Some(Member { id: id.clone(), display, account_id }), in_progress: wip, ready, none_found }
        })
        .collect();
    let unassigned: Vec<Card<'a>> =
        team_open.iter().copied().filter(|i| i.status.name == status_ready && i.assignee.is_none()).take(UNASSIGNED_CAP).map(card).collect();
    cols.push(Column { member: None, in_progress: Vec::new(), ready: unassigned, none_found: false });
    cols
}

/// The cursor moved onto a card: the column within `cols`, the row within that column's in-progress then ready
/// cards (0 when it has none, or there are no columns).
pub fn clamp_cursor(col: usize, row: usize, cols: &[Column]) -> (usize, usize) {
    let Some(last) = cols.len().checked_sub(1) else { return (0, 0) };
    let col = col.min(last);
    let n = cols[col].in_progress.len() + cols[col].ready.len();
    (col, row.min(n.saturating_sub(1)))
}

/// The card at `row` of a column: in-progress cards first, then ready.
pub fn card_at<'c, 'a>(col: &'c Column<'a>, row: usize) -> Option<&'c Card<'a>> {
    col.in_progress.iter().chain(&col.ready).nth(row)
}

/// The end of a column a move ran into: the cursor stays put there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    First,
    Last,
}

/// Which end a move of `d` rows from `row` runs into in a column of `len` cards, when the cursor cannot move at all
/// (j on the last card, k on the first). None for a move that lands, or an empty column.
pub fn end_hit(row: usize, len: usize, d: i32) -> Option<End> {
    if len == 0 {
        None
    } else if d > 0 && row + 1 >= len {
        Some(End::Last)
    } else if d < 0 && row == 0 {
        Some(End::First)
    } else {
        None
    }
}

pub fn jql_done_recent(projects: &[String], member_id: &str) -> String {
    let projects: Vec<String> = projects.iter().map(|p| jql_quote(p)).collect();
    format!("project in ({}) AND assignee = {} AND statusCategory = Done AND resolved >= -7d ORDER BY resolved DESC", projects.join(", "), jql_quote(member_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status, User};
    use chrono::{TimeZone, Utc};

    fn issue(key: &str, status: &str, cat: &str, who: Option<(&str, &str)>, changed: Option<&str>) -> Issue {
        Issue {
            key: key.into(),
            status: Status { name: status.into(), category: cat.into() },
            assignee: who.map(|(id, n)| User { account_id: id.into(), display_name: n.into() }),
            status_changed: changed.map(String::from),
            ..Default::default()
        }
    }
    fn today() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, 12, 0, 0).unwrap()
    }

    #[test]
    fn matches_by_id_or_name_case_insensitive() {
        let i = issue("CI-1", "Work in Progress", "indeterminate", Some(("abc", "Grace Hopper")), None);
        assert!(matches(&i, "abc"));
        assert!(matches(&i, "grace hopper"));
        assert!(!matches(&i, "nate"));
    }

    #[test]
    fn days_in_status_parses_jira_timestamps() {
        assert_eq!(days_in_status(Some("2026-10-01T10:00:00.000-0700"), today()), Some(7));
        assert_eq!(days_in_status(Some("garbage"), today()), None);
        assert_eq!(days_in_status(None, today()), None);
    }

    #[test]
    fn columns_group_in_progress_then_ready_then_unassigned() {
        let a = issue("CI-1", "Work in Progress", "indeterminate", Some(("abc", "Grace Hopper")), Some("2026-10-01T10:00:00.000-0700"));
        let b = issue("CI-2", "Ready for Work", "new", Some(("abc", "Grace Hopper")), None);
        let c = issue("CI-3", "Ready for Work", "new", None, None);
        let d = issue("CI-4", "New", "new", None, None);
        let cols = columns(&["abc".into(), "zzz".into()], &[&a], &[&b, &c, &d], "Ready for Work", today());
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0].member.as_ref().unwrap().display, "Grace Hopper");
        assert_eq!(cols[0].in_progress[0].issue.key, "CI-1");
        assert_eq!(cols[0].in_progress[0].days_in_status, Some(7));
        assert_eq!(cols[0].ready[0].issue.key, "CI-2");
        assert!(cols[1].none_found && cols[1].member.as_ref().unwrap().display == "zzz");
        assert!(cols[2].member.is_none());
        assert_eq!(cols[2].ready.iter().map(|c| c.issue.key.as_str()).collect::<Vec<_>>(), ["CI-3"]);
    }

    #[test]
    fn clamp_cursor_keeps_the_cursor_on_a_card() {
        let a = issue("CI-1", "Work in Progress", "indeterminate", Some(("abc", "Grace")), None);
        let b = issue("CI-2", "Ready for Work", "new", Some(("abc", "Grace")), None);
        let cols = columns(&["abc".into(), "zzz".into()], &[&a], &[&b], "Ready for Work", today());
        assert_eq!(clamp_cursor(0, 1, &cols), (0, 1), "in progress then ready");
        assert_eq!(clamp_cursor(0, 7, &cols), (0, 1));
        assert_eq!(clamp_cursor(1, 3, &cols), (1, 0), "an empty column holds row 0");
        assert_eq!(clamp_cursor(9, 0, &cols), (2, 0), "past the Unassigned column");
        assert_eq!(clamp_cursor(3, 4, &[]), (0, 0));
    }

    #[test]
    fn end_hit_only_when_the_cursor_cannot_move() {
        assert_eq!(end_hit(0, 1, 1), Some(End::Last), "j on the only card");
        assert_eq!(end_hit(2, 3, 2), Some(End::Last));
        assert_eq!(end_hit(1, 3, 2), None, "it still moves one");
        assert_eq!(end_hit(0, 3, -1), Some(End::First));
        assert_eq!(end_hit(1, 3, -1), None);
        assert_eq!(end_hit(0, 0, 1), None, "an empty column has no end");
        assert_eq!(end_hit(0, 2, 0), None);
    }

    #[test]
    fn done_recent_jql() {
        assert_eq!(
            jql_done_recent(&["CI".into(), "OPS".into()], "abc"),
            r#"project in ("CI", "OPS") AND assignee = "abc" AND statusCategory = Done AND resolved >= -7d ORDER BY resolved DESC"#
        );
    }
}
