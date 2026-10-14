//! Pure grooming model: pre-check candidates, verdict prefill, promotion planning, session queues and thresholds.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::config;
use crate::jira::Issue;
use crate::report::{Badge, Report};

/// Why a ticket needs more detail before it can be promoted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Code {
    UnclearWhat,
    UnclearWhen,
    Empty,
}

impl Code {
    /// The kebab-case slug used in reports.
    pub fn slug(self) -> &'static str {
        match self {
            Code::UnclearWhat => "unclear-what",
            Code::UnclearWhen => "unclear-when",
            Code::Empty => "empty",
        }
    }

    /// Parses a report slug; unknown slugs give `None`.
    pub fn parse(s: &str) -> Option<Code> {
        [Code::UnclearWhat, Code::UnclearWhen, Code::Empty].into_iter().find(|c| c.slug() == s)
    }
}

/// A grooming verdict for one ticket.
#[derive(Clone, PartialEq, Debug)]
pub enum Verdict {
    Ready,
    NeedsDetail(Vec<Code>),
}

/// A `New` ticket considered for promotion, with its position in the rank-ordered list.
pub struct Candidate<'a> {
    pub issue: &'a Issue,
    pub rank_pos: u32,
    pub age: String,
}

/// `new (Nd)` when created within 14 days of `today`, otherwise `older` (also for unparsable dates).
pub fn age_tag(created: &str, today: chrono::NaiveDate) -> String {
    let date = created.get(..10).and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    match date.map(|d| (today - d).num_days()) {
        Some(days) if (0..=14).contains(&days) => format!("new ({days}d)"),
        _ => "older".to_string(),
    }
}

/// Team tickets in `status_new`, keeping input order; `rank_pos` is 1-based over the whole input.
pub fn new_candidates<'a>(issues: &[&'a Issue], team_label: &str, status_new: &str, today: chrono::NaiveDate) -> Vec<Candidate<'a>> {
    issues
        .iter()
        .enumerate()
        .filter(|(_, i)| i.labels.iter().any(|l| l == team_label) && i.status.name == status_new)
        .map(|(n, i)| Candidate { issue: i, rank_pos: n as u32 + 1, age: age_tag(&i.created, today) })
        .collect()
}

/// Initial verdict for `key` taken from the previous report, if it had one.
pub fn prefill(report: Option<&Report>, key: &str) -> Option<Verdict> {
    match report?.badge_for(key)? {
        Badge::Promoted => Some(Verdict::Ready),
        Badge::NeedsDetail(codes) => Some(Verdict::NeedsDetail(codes.iter().filter_map(|c| Code::parse(c)).collect())),
        Badge::Struck => None,
    }
}

/// The outcome of applying a promotion cap to a set of verdicts.
pub struct Promotion {
    pub promoted: Vec<String>,
    pub held_back: Vec<String>,
    pub needs_detail: Vec<(String, Vec<Code>)>,
}

/// Splits verdicts (in rank order) into the first `cap` ready keys, the remaining ready keys, and needs-detail entries.
pub fn plan_promotion(verdicts: &[(String, Verdict)], cap: usize) -> Promotion {
    let mut p = Promotion { promoted: vec![], held_back: vec![], needs_detail: vec![] };
    for (key, v) in verdicts {
        match v {
            Verdict::Ready if p.promoted.len() < cap => p.promoted.push(key.clone()),
            Verdict::Ready => p.held_back.push(key.clone()),
            Verdict::NeedsDetail(codes) => p.needs_detail.push((key.clone(), codes.clone())),
        }
    }
    p
}

/// A live-session decision on a ticket.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    Accept,
    Bounce,
    Park,
}

/// One epic's tickets for the session rail.
pub struct EpicQueue<'a> {
    pub epic: Option<String>,
    pub summary: String,
    pub ready: Vec<&'a Issue>,
    pub planning: Vec<&'a Issue>,
    pub new_count: usize,
}

/// Groups team tickets by epic; most Planning tickets first, then epic key, with "(no epic)" last among equals.
pub fn epic_queues<'a>(issues: &[&'a Issue], team_label: &str, g: &config::Grooming) -> Vec<EpicQueue<'a>> {
    let mut by_epic: BTreeMap<Option<String>, EpicQueue<'a>> = BTreeMap::new();
    for i in issues.iter().filter(|i| i.labels.iter().any(|l| l == team_label)) {
        let q = by_epic.entry(i.parent_key.clone()).or_insert_with(|| EpicQueue {
            epic: i.parent_key.clone(),
            summary: i.parent_key.as_ref().and_then(|_| i.parent_summary.clone()).unwrap_or_else(|| "(no epic)".to_string()),
            ready: vec![],
            planning: vec![],
            new_count: 0,
        });
        match i.status.name.as_str() {
            s if s == g.status_ready => q.ready.push(i),
            s if s == g.status_planning => q.planning.push(i),
            s if s == g.status_new => q.new_count += 1,
            _ => {}
        }
    }
    let mut out: Vec<_> = by_epic.into_values().collect();
    out.sort_by_key(|q| (Reverse(q.planning.len()), q.epic.is_none(), q.epic.clone()));
    out
}

/// `(before, after)` anchors for a ghost inserted at `ghost_index` in the Ready queue.
pub fn placement_target(ready_keys: &[String], ghost_index: usize) -> (Option<String>, Option<String>) {
    match ready_keys.get(ghost_index) {
        Some(k) => (Some(k.clone()), None),
        None => (None, ready_keys.last().cloned()),
    }
}

/// Traffic-light level for a gauge or timer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Ok,
    Amber,
    Red,
}

/// Level of the Ready queue size against its soft and hard ceilings (strictly above each).
pub fn ceiling_level(ready_total: usize, soft: usize, hard: usize) -> Level {
    if ready_total > hard {
        Level::Red
    } else if ready_total > soft {
        Level::Amber
    } else {
        Level::Ok
    }
}

/// Level of the per-ticket timer: amber from 180 s, red from 300 s.
pub fn timer_level(elapsed_secs: u64) -> Level {
    match elapsed_secs {
        300.. => Level::Red,
        180.. => Level::Amber,
        _ => Level::Ok,
    }
}

/// A bounce comment must be non-blank and at most two lines.
pub fn bounce_comment_ok(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && t.lines().count() <= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status};
    use chrono::NaiveDate;

    fn issue(key: &str, status: &str, parent: Option<&str>, created: &str, labels: &[&str]) -> Issue {
        Issue {
            key: key.into(),
            status: Status { name: status.into(), category: "new".into() },
            parent_key: parent.map(String::from),
            parent_summary: parent.map(|p| format!("Epic {p}")),
            created: created.into(),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
    }

    #[test]
    fn age_tags() {
        assert_eq!(age_tag("2026-10-05T10:00:00.000-0700", today()), "new (3d)");
        assert_eq!(age_tag("2026-09-01T10:00:00.000-0700", today()), "older");
        assert_eq!(age_tag("garbage", today()), "older");
    }

    #[test]
    fn candidates_are_team_new_with_rank_positions() {
        let a = issue("CI-1", "New", Some("E-1"), "2026-10-07T00:00:00.000-0700", &["PLAT"]);
        let b = issue("CI-2", "Planning", Some("E-1"), "2026-10-07T00:00:00.000-0700", &["PLAT"]);
        let c = issue("CI-3", "New", None, "2026-01-01T00:00:00.000-0700", &["TOOLS"]);
        let d = issue("CI-4", "New", None, "2026-01-01T00:00:00.000-0700", &["PLAT"]);
        let cands = new_candidates(&[&a, &b, &c, &d], "PLAT", "New", today());
        assert_eq!(
            cands.iter().map(|c| (c.issue.key.as_str(), c.rank_pos, c.age.as_str())).collect::<Vec<_>>(),
            [("CI-1", 1, "new (1d)"), ("CI-4", 4, "older")]
        );
    }

    #[test]
    fn codes_round_trip_and_prefill() {
        assert_eq!(Code::parse("unclear-when"), Some(Code::UnclearWhen));
        assert_eq!(Code::Empty.slug(), "empty");
        let r = crate::report::parse(
            "# x — 2026-10-01\n\n## Planning — by epic\n\n### [E-1 Epic](u?issueParent=1)\n\n- [CI-1](u) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### (no epic)\n\n- [CI-2](u) — b · rank 2 · created 2026-09-01 · missing: unclear-what, empty\n",
        );
        assert_eq!(prefill(Some(&r), "CI-1"), Some(Verdict::Ready));
        assert_eq!(prefill(Some(&r), "CI-2"), Some(Verdict::NeedsDetail(vec![Code::UnclearWhat, Code::Empty])));
        assert_eq!(prefill(Some(&r), "CI-9"), None);
        assert_eq!(prefill(None, "CI-1"), None);
    }

    #[test]
    fn promotion_respects_cap_and_rank_order() {
        let v: Vec<(String, Verdict)> =
            (1..=4).map(|i| (format!("CI-{i}"), if i == 2 { Verdict::NeedsDetail(vec![Code::UnclearWhen]) } else { Verdict::Ready })).collect();
        let p = plan_promotion(&v, 2);
        assert_eq!(p.promoted, ["CI-1", "CI-3"]);
        assert_eq!(p.held_back, ["CI-4"]);
        assert_eq!(p.needs_detail, [("CI-2".to_string(), vec![Code::UnclearWhen])]);
    }

    #[test]
    fn epic_queues_order_and_counts() {
        let g = crate::config::Grooming::default();
        let i1 = issue("CI-1", "Planning", Some("E-2"), "", &["PLAT"]);
        let i2 = issue("CI-2", "Ready for Work", Some("E-1"), "", &["PLAT"]);
        let i3 = issue("CI-3", "Planning", Some("E-1"), "", &["PLAT"]);
        let i4 = issue("CI-4", "Planning", Some("E-1"), "", &["PLAT"]);
        let i5 = issue("CI-5", "New", Some("E-3"), "", &["PLAT"]);
        let q = epic_queues(&[&i1, &i2, &i3, &i4, &i5], "PLAT", &g);
        assert_eq!(q.iter().map(|e| e.epic.clone().unwrap()).collect::<Vec<_>>(), ["E-1", "E-2", "E-3"]);
        assert_eq!(q[0].ready.len(), 1);
        assert_eq!(q[0].planning.len(), 2);
        assert_eq!(q[2].new_count, 1);
    }

    #[test]
    fn placement_targets() {
        let r: Vec<String> = ["A", "B", "C"].map(String::from).to_vec();
        assert_eq!(placement_target(&r, 0), (Some("A".into()), None));
        assert_eq!(placement_target(&r, 1), (Some("B".into()), None));
        assert_eq!(placement_target(&r, 3), (None, Some("C".into())));
        assert_eq!(placement_target(&[], 0), (None, None));
    }

    #[test]
    fn levels_and_bounce_text() {
        assert_eq!(ceiling_level(9, 10, 15), Level::Ok);
        assert_eq!(ceiling_level(11, 10, 15), Level::Amber);
        assert_eq!(ceiling_level(16, 10, 15), Level::Red);
        assert_eq!(timer_level(179), Level::Ok);
        assert_eq!(timer_level(180), Level::Amber);
        assert_eq!(timer_level(300), Level::Red);
        assert!(bounce_comment_ok("Needs a success condition.\nWhich clusters?"));
        assert!(!bounce_comment_ok("  "));
        assert!(!bounce_comment_ok("a\nb\nc"));
    }
}
