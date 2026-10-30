//! Pure grooming model: pre-check candidates, verdict prefill, promotion planning, session queues and thresholds.

use std::collections::BTreeMap;

use crate::config;
use crate::jira::{Issue, Sprint};
use crate::report::{Badge, EpicRef, NeedsDetailEpic, NeedsDetailRow, PlanningRow, PromotedEpic, Report};
use crate::roadmap::{self, Horizon};

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

/// Team tickets in `status_new`, keeping the input's rank order. `rank_pos` is the 1-based position within that
/// team + `New` list (the pre-check query's `labels = TEAM AND status = New ORDER BY rank`), so other statuses and
/// teams in the input do not count. Epics are numbered too; callers drop them afterwards (`filter::is_action_item`).
pub fn new_candidates<'a>(issues: &[&'a Issue], team_label: &str, status_new: &str, today: chrono::NaiveDate) -> Vec<Candidate<'a>> {
    issues
        .iter()
        .filter(|i| i.labels.iter().any(|l| l == team_label) && i.status.name == status_new)
        .enumerate()
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

/// The verdict after a code key (`w`, `n`, `e`): adds `code` to a needs-detail verdict or takes it away; taking away
/// the last code leaves the ticket ready. An unjudged or ready ticket starts a needs-detail verdict with it.
pub fn toggle_code(v: Option<&Verdict>, code: Code) -> Verdict {
    match v {
        Some(Verdict::NeedsDetail(codes)) if codes.contains(&code) => {
            let rest: Vec<Code> = codes.iter().copied().filter(|c| *c != code).collect();
            if rest.is_empty() { Verdict::Ready } else { Verdict::NeedsDetail(rest) }
        }
        Some(Verdict::NeedsDetail(codes)) => Verdict::NeedsDetail(codes.iter().copied().chain([code]).collect()),
        _ => Verdict::NeedsDetail(vec![code]),
    }
}

/// The promotion plan over the candidates in rank order, each with its verdict. Verdicts for keys that are no
/// longer candidates (an old report's) are ignored, and unjudged candidates are left out.
pub fn promote_order(verdicts: &BTreeMap<String, Verdict>, candidates_in_rank_order: &[String], cap: usize) -> Promotion {
    let judged: Vec<(String, Verdict)> = candidates_in_rank_order.iter().filter_map(|k| verdicts.get(k).map(|v| (k.clone(), v.clone()))).collect();
    plan_promotion(&judged, cap)
}

/// Candidates with no verdict yet.
pub fn unjudged(verdicts: &BTreeMap<String, Verdict>, candidates_in_rank_order: &[String]) -> usize {
    candidates_in_rank_order.iter().filter(|k| !verdicts.contains_key(*k)).count()
}

/// A candidate as the report needs it, taken when Promote starts.
#[derive(Clone, Debug)]
pub struct RunRow {
    pub key: String,
    pub summary: String,
    pub rank: u32,
    pub age: String,
    /// `YYYY-MM-DD`.
    pub created: String,
    pub epic: Option<EpicRef>,
}

/// Quotes a JQL value unless it is one plain word.
fn jql_value(v: &str) -> String {
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { v.to_string() } else { crate::store::jql_quote(v) }
}

/// The pre-check's query as the report header names it: the team's `New` tickets in rank order.
pub fn pre_check_jql(project: &str, team_label: &str, status_new: &str) -> String {
    format!("project = {} AND labels = {} AND status = {} ORDER BY rank ASC", jql_value(project), jql_value(team_label), jql_value(status_new))
}

/// The tickets already in Ready for Work under the promoted epics, in rank order.
pub fn siblings_jql(project: &str, status_ready: &str, parents: &[String]) -> String {
    format!(
        "project = {} AND status = {} AND parent in ({}) ORDER BY Rank ASC",
        crate::store::jql_quote(project),
        crate::store::jql_quote(status_ready),
        parents.join(", ")
    )
}

/// The report's "Planning — by epic": the promoted rows grouped by epic in stage order (roadmap, no epic, ongoing,
/// other; see `StageKey`). Each epic lists its Ready for Work `siblings` (in rank order) with their 1-based
/// position among that epic's Ready tickets, and carries its stage note.
pub fn promoted_epics(promoted: &[&RunRow], siblings: &[Issue], ctx: &StageContext) -> Vec<PromotedEpic> {
    let mut by_epic: Vec<(StageKey, PromotedEpic)> = Vec::new();
    for r in promoted {
        let key = r.epic.as_ref().map(|e| e.key.as_str());
        let at = match by_epic.iter().position(|(_, e)| e.epic.as_ref().map(|e| e.key.as_str()) == key) {
            Some(i) => i,
            None => {
                let existing_ready = key
                    .map(|k| {
                        siblings
                            .iter()
                            .filter(|s| s.parent_key.as_deref() == Some(k))
                            .enumerate()
                            .map(|(n, s)| (s.key.clone(), s.summary.clone(), n as u32 + 1))
                            .collect()
                    })
                    .unwrap_or_default();
                let sk = stage_of(key, r.epic.as_ref().map(|e| e.summary.as_str()), ctx);
                let epic = r.epic.clone().map(|mut e| {
                    e.note = sk.note();
                    e
                });
                by_epic.push((sk, PromotedEpic { epic, existing_ready, candidates: vec![] }));
                by_epic.len() - 1
            }
        };
        by_epic[at].1.candidates.push(PlanningRow {
            key: r.key.clone(),
            summary: r.summary.clone(),
            rank: Some(r.rank),
            age: Some(r.age.clone()),
            epic: key.map(String::from),
        });
    }
    by_epic.sort_by(|a, b| a.0.cmp(&b.0));
    by_epic.into_iter().map(|(_, e)| e).collect()
}

/// The report's "Needs detail": the first `cap` rows in rank order, grouped by epic in stage order (rows keep rank
/// order), and the count before the cap.
pub fn needs_detail_epics(rows: &[(&RunRow, Vec<Code>)], cap: usize, ctx: &StageContext) -> (Vec<NeedsDetailEpic>, usize) {
    let mut out: Vec<(StageKey, NeedsDetailEpic)> = Vec::new();
    for (r, codes) in rows.iter().take(cap) {
        let key = r.epic.as_ref().map(|e| e.key.as_str());
        let row = NeedsDetailRow {
            key: r.key.clone(),
            summary: r.summary.clone(),
            rank: Some(r.rank),
            created: Some(r.created.clone()),
            codes: codes.iter().map(|c| c.slug().to_string()).collect(),
            epic: key.map(String::from),
        };
        match out.iter_mut().find(|(_, e)| e.epic.as_ref().map(|e| e.key.as_str()) == key) {
            Some((_, e)) => e.rows.push(row),
            None => {
                let sk = stage_of(key, r.epic.as_ref().map(|e| e.summary.as_str()), ctx);
                let epic = r.epic.clone().map(|mut e| {
                    e.note = sk.note();
                    e
                });
                out.push((sk, NeedsDetailEpic { epic, rows: vec![row] }));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    (out.into_iter().map(|(_, e)| e).collect(), rows.len())
}

/// The session's four stages, in meeting order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Stage {
    Roadmap,
    NoEpic,
    Ongoing,
    Other,
}

impl Stage {
    /// The rail's stage header.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Roadmap => "Roadmap",
            Stage::NoEpic => "No epic",
            Stage::Ongoing => "Ongoing",
            Stage::Other => "Other epics",
        }
    }
}

/// What staging the epics needs: the store's epics in rank order, the roadmap rules, and the two sprint names.
pub struct StageContext<'a> {
    pub epics: Vec<&'a Issue>,
    pub initiative_label: &'a str,
    pub roadmap: &'a config::Roadmap,
    pub active_sprint: Option<&'a str>,
    pub next_sprint: Option<&'a str>,
}

/// An epic's place in the session: its stage, then (roadmap epics) horizon and priority, then its position in the
/// Epics list, then its key. Sorting by it gives the rail's order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StageKey {
    pub stage: Stage,
    pub horizon: Option<Horizon>,
    pub priority: Option<String>,
    pub rank_pos: usize,
    pub key: Option<String>,
}

impl StageKey {
    fn priority_rank(&self) -> u8 {
        match self.priority.as_deref() {
            Some("P0") => 0,
            Some("P1") => 1,
            _ => 2,
        }
    }

    /// The report heading's note: "roadmap (Now, P0)", "roadmap (Later)", "ongoing", "other"; none for no epic.
    pub fn note(&self) -> Option<String> {
        match self.stage {
            Stage::Roadmap => {
                let h = match self.horizon {
                    Some(Horizon::Now) => "Now",
                    Some(Horizon::Next) => "Next",
                    Some(Horizon::Done) => "Done",
                    Some(Horizon::Later) | None => "Later",
                };
                Some(match &self.priority {
                    Some(p) => format!("roadmap ({h}, {p})"),
                    None => format!("roadmap ({h})"),
                })
            }
            Stage::NoEpic => None,
            Stage::Ongoing => Some("ongoing".into()),
            Stage::Other => Some("other".into()),
        }
    }
}

impl Ord for StageKey {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        (self.stage, self.horizon, self.priority_rank(), self.rank_pos, &self.key).cmp(&(o.stage, o.horizon, o.priority_rank(), o.rank_pos, &o.key))
    }
}
impl PartialOrd for StageKey {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

fn is_ongoing(summary: &str) -> bool {
    summary.to_lowercase().contains("(ongoing)")
}

/// The stage of a ticket's parent. An epic the store has not loaded is staged by the ticket's `parent_summary`
/// (ongoing or other) and sorts after the loaded ones.
pub fn stage_of(parent: Option<&str>, parent_summary: Option<&str>, ctx: &StageContext) -> StageKey {
    let Some(key) = parent else {
        return StageKey { stage: Stage::NoEpic, horizon: None, priority: None, rank_pos: 0, key: None };
    };
    let any_labelled = ctx.epics.iter().any(|e| e.labels.iter().any(|l| l == ctx.initiative_label));
    let listed = ctx.epics.iter().position(|e| e.key == key);
    let (stage, horizon, priority) = match listed.map(|i| ctx.epics[i]) {
        Some(e) if roadmap::is_initiative(e, ctx.initiative_label, any_labelled) => {
            (Stage::Roadmap, Some(roadmap::horizon_of(e, ctx.roadmap)), roadmap::priority_of(e))
        }
        Some(e) if is_ongoing(&e.summary) => (Stage::Ongoing, None, None),
        Some(_) => (Stage::Other, None, None),
        None if parent_summary.is_some_and(is_ongoing) => (Stage::Ongoing, None, None),
        None => (Stage::Other, None, None),
    };
    StageKey { stage, horizon, priority, rank_pos: listed.unwrap_or(usize::MAX), key: Some(key.to_string()) }
}

/// (active, next): the board's active sprint and its first future sprint, by name.
pub fn sprint_names(sprints: &[Sprint]) -> (Option<&str>, Option<&str>) {
    let pick = |state: &str| sprints.iter().find(|s| s.state == state).map(|s| s.name.as_str());
    (pick("active"), pick("future"))
}

/// One epic's tickets for the session: the two sprint blocks (context), then the Ready queue, the Proposed
/// (`Planning`) candidates and the `New` candidates, each in rank order.
pub struct EpicQueue<'a> {
    pub epic: Option<String>,
    pub summary: String,
    pub stage: StageKey,
    pub sprint: Vec<&'a Issue>,
    pub next: Vec<&'a Issue>,
    pub ready: Vec<&'a Issue>,
    pub planning: Vec<&'a Issue>,
    pub new: Vec<&'a Issue>,
}

impl<'a> EpicQueue<'a> {
    /// What the meeting decides on: Proposed, then New.
    pub fn candidates(&self) -> Vec<&'a Issue> {
        self.planning.iter().chain(&self.new).copied().collect()
    }

    /// Every row the cursor walks: Ready, then the candidates.
    pub fn rows(&self) -> Vec<&'a Issue> {
        self.ready.iter().chain(&self.planning).chain(&self.new).copied().collect()
    }

    /// The cursor index of the first candidate (just past the Ready rows).
    pub fn first_candidate(&self) -> usize {
        self.ready.len()
    }
}

/// Groups the team's tickets by epic in stage order (`StageKey`). A ticket in the active or next sprint goes in
/// that block whatever its status; the rest go by status. An epic is named by its summary, else its key; epics
/// with nothing in Ready, Proposed or New are left out.
pub fn epic_queues<'a>(issues: &[&'a Issue], team_label: &str, g: &config::Grooming, ctx: &StageContext) -> Vec<EpicQueue<'a>> {
    let mut by_epic: BTreeMap<Option<String>, EpicQueue<'a>> = BTreeMap::new();
    for i in issues.iter().filter(|i| i.labels.iter().any(|l| l == team_label)) {
        let q = by_epic.entry(i.parent_key.clone()).or_insert_with(|| EpicQueue {
            epic: i.parent_key.clone(),
            summary: match &i.parent_key {
                Some(k) => i.parent_summary.clone().unwrap_or_else(|| k.clone()),
                None => "(no epic)".to_string(),
            },
            stage: stage_of(i.parent_key.as_deref(), i.parent_summary.as_deref(), ctx),
            sprint: vec![],
            next: vec![],
            ready: vec![],
            planning: vec![],
            new: vec![],
        });
        if i.sprint.is_some() && i.sprint.as_deref() == ctx.active_sprint {
            q.sprint.push(i);
        } else if i.sprint.is_some() && i.sprint.as_deref() == ctx.next_sprint {
            q.next.push(i);
        } else {
            match i.status.name.as_str() {
                s if s == g.status_ready => q.ready.push(i),
                s if s == g.status_planning => q.planning.push(i),
                s if s == g.status_new => q.new.push(i),
                _ => {}
            }
        }
    }
    let mut out: Vec<_> = by_epic.into_values().filter(|q| !q.ready.is_empty() || !q.planning.is_empty() || !q.new.is_empty()).collect();
    out.sort_by(|a, b| a.stage.cmp(&b.stage));
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

/// The id of the transition in `ts` that leads to `target` (status names compare case-insensitively).
pub fn pick_transition(ts: &[crate::jira::Transition], target: &str) -> Option<String> {
    ts.iter().find(|t| t.to.eq_ignore_ascii_case(target)).map(|t| t.id.clone())
}

/// The per-ticket timer: "0:07", "4:59", "12:03".
pub fn fmt_elapsed(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
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
    use crate::roadmap::Horizon;
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
            [("CI-1", 1, "new (1d)"), ("CI-4", 2, "older")],
            "positions count the team's New tickets only"
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
        let rm = crate::config::Roadmap::default();
        let c = ctx(&[], &rm);
        let i1 = issue("CI-1", "Planning", Some("E-2"), "", &["PLAT"]);
        let i2 = issue("CI-2", "Ready for Work", Some("E-1"), "", &["PLAT"]);
        let i3 = issue("CI-3", "Planning", Some("E-1"), "", &["PLAT"]);
        let i4 = issue("CI-4", "Planning", Some("E-1"), "", &["PLAT"]);
        let i5 = issue("CI-5", "New", Some("E-3"), "", &["PLAT"]);
        let q = epic_queues(&[&i1, &i2, &i3, &i4, &i5], "PLAT", &g, &c);
        assert_eq!(q.iter().map(|e| e.epic.clone().unwrap()).collect::<Vec<_>>(), ["E-1", "E-2", "E-3"], "unlisted epics: by key");
        assert_eq!(q[0].ready.len(), 1);
        assert_eq!(q[0].planning.len(), 2);
        assert_eq!(q[2].new.len(), 1);
    }

    #[test]
    fn epic_without_parent_summary_falls_back_to_its_key() {
        let g = crate::config::Grooming::default();
        let rm = crate::config::Roadmap::default();
        let c = ctx(&[], &rm);
        let mut i = issue("CI-1", "Planning", Some("E-7"), "", &["PLAT"]);
        i.parent_summary = None;
        let n = issue("CI-2", "Planning", None, "", &["PLAT"]);
        let q = epic_queues(&[&i, &n], "PLAT", &g, &c);
        assert_eq!(q[0].summary, "(no epic)", "no epic is stage 2, before other epics");
        assert_eq!(q[1].summary, "E-7", "an epic's key, not \"(no epic)\"");
    }

    #[test]
    fn epics_with_nothing_to_groom_are_left_out() {
        let g = crate::config::Grooming::default();
        let rm = crate::config::Roadmap::default();
        let c = ctx(&[], &rm);
        let a = issue("CI-1", "In Progress", Some("E-1"), "", &["PLAT"]);
        let b = issue("CI-2", "New", Some("E-2"), "", &["PLAT"]);
        let q = epic_queues(&[&a, &b], "PLAT", &g, &c);
        assert_eq!(q.iter().map(|e| e.epic.clone().unwrap()).collect::<Vec<_>>(), ["E-2"]);
    }

    fn epic(key: &str, summary: &str, status: &str, labels: &[&str]) -> Issue {
        Issue {
            key: key.into(),
            summary: summary.into(),
            issue_type: "Epic".into(),
            status: Status { name: status.into(), category: if status == "Work in Progress" { "indeterminate".into() } else { "new".into() } },
            labels: labels.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }

    fn ctx<'a>(epics: &[&'a Issue], roadmap: &'a config::Roadmap) -> StageContext<'a> {
        StageContext { epics: epics.to_vec(), initiative_label: "roadmap", roadmap, active_sprint: Some("Sprint 92"), next_sprint: Some("Sprint 93") }
    }

    #[test]
    fn stages_order_roadmap_then_no_epic_then_ongoing_then_other() {
        let g = config::Grooming::default();
        let rm = config::Roadmap::default();
        // Rank order in the Epics list: E-X first, then the roadmap epics, then the ongoing one.
        let ex = epic("E-X", "Cloud Role Access", "New", &[]);
        let r_later = epic("E-R1", "Metrics Retention", "New", &["roadmap"]);
        let r_now = epic("E-R2", "Cost Insights", "Work in Progress", &["roadmap", "P0"]);
        let r_now_p1 = epic("E-R3", "Service Manifest", "Work in Progress", &["roadmap"]);
        let og = epic("E-O", "Node Autoscaling (ongoing)", "New", &[]);
        let epics = [&ex, &r_later, &r_now, &r_now_p1, &og];
        let c = ctx(&epics, &rm);
        let t = |k: &str, parent: Option<&str>| issue(k, "New", parent, "", &["PLAT"]);
        let tickets =
            [t("CI-1", Some("E-O")), t("CI-2", Some("E-X")), t("CI-3", None), t("CI-4", Some("E-R1")), t("CI-5", Some("E-R2")), t("CI-6", Some("E-R3"))];
        let refs: Vec<&Issue> = tickets.iter().collect();
        let q = epic_queues(&refs, "PLAT", &g, &c);
        let order: Vec<Option<&str>> = q.iter().map(|e| e.epic.as_deref()).collect();
        assert_eq!(order, [Some("E-R2"), Some("E-R3"), Some("E-R1"), None, Some("E-O"), Some("E-X")], "Now P0, Now, Later; no epic; ongoing; other");
        assert_eq!(q[0].stage.stage, Stage::Roadmap);
        assert_eq!(q[0].stage.horizon, Some(Horizon::Now));
        assert_eq!(q[0].stage.priority.as_deref(), Some("P0"));
        assert_eq!(q[3].stage.stage, Stage::NoEpic);
        assert_eq!(q[4].stage.stage, Stage::Ongoing);
        assert_eq!(q[5].stage.stage, Stage::Other);
        assert_eq!(q[0].stage.note().as_deref(), Some("roadmap (Now, P0)"));
        assert_eq!(q[2].stage.note().as_deref(), Some("roadmap (Later)"));
        assert_eq!(q[3].stage.note(), None);
        assert_eq!(q[4].stage.note().as_deref(), Some("ongoing"));
        assert_eq!(q[5].stage.note().as_deref(), Some("other"));
    }

    #[test]
    fn sprint_blocks_take_tickets_out_of_the_status_queues() {
        let g = config::Grooming::default();
        let rm = config::Roadmap::default();
        let c = ctx(&[], &rm);
        let mut in_sprint = issue("CI-1", "Ready for Work", Some("E-1"), "", &["PLAT"]);
        in_sprint.sprint = Some("Sprint 92".into());
        let mut in_next = issue("CI-2", "New", Some("E-1"), "", &["PLAT"]);
        in_next.sprint = Some("Sprint 93".into());
        let mut old_sprint = issue("CI-3", "Ready for Work", Some("E-1"), "", &["PLAT"]);
        old_sprint.sprint = Some("Sprint 90".into());
        let plain = issue("CI-4", "Planning", Some("E-1"), "", &["PLAT"]);
        let q = epic_queues(&[&in_sprint, &in_next, &old_sprint, &plain], "PLAT", &g, &c);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].sprint.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-1"]);
        assert_eq!(q[0].next.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-2"]);
        assert_eq!(q[0].ready.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-3"], "a closed sprint's name is not the active one");
        assert_eq!(q[0].planning.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-4"]);
        assert!(q[0].new.is_empty(), "the New ticket sits in the next sprint, not the New queue");
    }

    #[test]
    fn candidates_are_proposed_then_new_and_rows_start_with_ready() {
        let g = config::Grooming::default();
        let rm = config::Roadmap::default();
        let c = ctx(&[], &rm);
        let r = issue("CI-1", "Ready for Work", Some("E-1"), "", &["PLAT"]);
        let n = issue("CI-2", "New", Some("E-1"), "", &["PLAT"]);
        let p = issue("CI-3", "Planning", Some("E-1"), "", &["PLAT"]);
        let q = epic_queues(&[&r, &n, &p], "PLAT", &g, &c);
        assert_eq!(q[0].candidates().iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-3", "CI-2"]);
        assert_eq!(q[0].rows().iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-3", "CI-2"]);
        assert_eq!(q[0].first_candidate(), 1);
    }

    #[test]
    fn an_epic_missing_from_the_epics_list_is_staged_by_its_parent_summary() {
        let g = config::Grooming::default();
        let rm = config::Roadmap::default();
        let c = ctx(&[], &rm);
        let mut og = issue("CI-1", "New", Some("E-9"), "", &["PLAT"]);
        og.parent_summary = Some("Deployer (Ongoing)".into());
        let other = issue("CI-2", "New", Some("E-8"), "", &["PLAT"]);
        let q = epic_queues(&[&og, &other], "PLAT", &g, &c);
        assert_eq!(q.iter().map(|e| (e.epic.as_deref(), e.stage.stage)).collect::<Vec<_>>(), [(Some("E-9"), Stage::Ongoing), (Some("E-8"), Stage::Other)]);
        assert_eq!(q[1].stage.rank_pos, usize::MAX, "unlisted epics sort after listed ones");
    }

    #[test]
    fn sprint_names_pick_the_active_and_the_first_future() {
        let s = |name: &str, state: &str| crate::jira::Sprint { id: 1, name: name.into(), state: state.into() };
        let sprints = [s("Sprint 92", "active"), s("Sprint 93", "future"), s("Sprint 94", "future")];
        assert_eq!(sprint_names(&sprints), (Some("Sprint 92"), Some("Sprint 93")));
        assert_eq!(sprint_names(&[s("Sprint 93", "future")]), (None, Some("Sprint 93")));
        assert_eq!(sprint_names(&[]), (None, None));
    }

    #[test]
    fn stage_labels() {
        assert_eq!(Stage::Roadmap.label(), "Roadmap");
        assert_eq!(Stage::NoEpic.label(), "No epic");
        assert_eq!(Stage::Ongoing.label(), "Ongoing");
        assert_eq!(Stage::Other.label(), "Other epics");
        assert!(Stage::Roadmap < Stage::NoEpic && Stage::NoEpic < Stage::Ongoing && Stage::Ongoing < Stage::Other);
    }

    #[test]
    fn transitions_are_picked_by_target_status() {
        let t = |id: &str, to: &str| crate::jira::Transition { id: id.into(), name: format!("to {to}"), to: to.into() };
        let ts = [t("11", "Backlog"), t("31", "Ready for Work"), t("41", "New")];
        assert_eq!(pick_transition(&ts, "Ready for Work"), Some("31".to_string()));
        assert_eq!(pick_transition(&ts, "ready FOR work"), Some("31".to_string()), "case-insensitive");
        assert_eq!(pick_transition(&ts, "Done"), None);
        assert_eq!(pick_transition(&[], "New"), None);
    }

    #[test]
    fn elapsed_reads_as_minutes_and_seconds() {
        assert_eq!(fmt_elapsed(7), "0:07");
        assert_eq!(fmt_elapsed(299), "4:59");
        assert_eq!(fmt_elapsed(723), "12:03");
        assert_eq!(fmt_elapsed(0), "0:00");
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

    #[test]
    fn code_keys_toggle() {
        use Code::*;
        assert_eq!(toggle_code(None, UnclearWhen), Verdict::NeedsDetail(vec![UnclearWhen]));
        assert_eq!(toggle_code(Some(&Verdict::Ready), Empty), Verdict::NeedsDetail(vec![Empty]));
        let one = Verdict::NeedsDetail(vec![UnclearWhat]);
        assert_eq!(toggle_code(Some(&one), UnclearWhen), Verdict::NeedsDetail(vec![UnclearWhat, UnclearWhen]), "codes add up");
        assert_eq!(toggle_code(Some(&one), UnclearWhat), Verdict::Ready, "removing the last code leaves it ready");
        let two = Verdict::NeedsDetail(vec![UnclearWhat, Empty]);
        assert_eq!(toggle_code(Some(&two), UnclearWhat), Verdict::NeedsDetail(vec![Empty]));
    }

    #[test]
    fn promote_order_follows_rank_not_the_map() {
        let mut v = BTreeMap::new();
        for k in ["CI-10", "CI-2", "CI-3", "CI-99"] {
            v.insert(k.to_string(), Verdict::Ready);
        }
        v.insert("CI-5".to_string(), Verdict::NeedsDetail(vec![Code::Empty]));
        // CI-99 has a verdict (from an old report) but is no longer a candidate; CI-7 is unjudged.
        let rank: Vec<String> = ["CI-3", "CI-10", "CI-5", "CI-7", "CI-2"].map(String::from).to_vec();
        let p = promote_order(&v, &rank, 2);
        assert_eq!(p.promoted, ["CI-3", "CI-10"]);
        assert_eq!(p.held_back, ["CI-2"]);
        assert_eq!(p.needs_detail, [("CI-5".to_string(), vec![Code::Empty])]);
        assert_eq!(unjudged(&v, &rank), 1);
    }

    fn run_row(key: &str, epic: Option<&str>, rank: u32) -> RunRow {
        RunRow {
            key: key.into(),
            summary: format!("S {key}"),
            rank,
            age: "older".into(),
            created: "2026-09-01".into(),
            epic: epic.map(|e| crate::report::EpicRef { key: e.into(), summary: format!("Epic {e}"), numeric_id: Some("1".into()), status: None, note: None }),
        }
    }

    #[test]
    fn report_epics_follow_the_stage_order_with_ready_siblings() {
        let rm = config::Roadmap::default();
        let r_now = epic("E-2", "Cost", "Work in Progress", &["roadmap"]);
        let og = epic("E-1", "Node Autoscaling (ongoing)", "New", &[]);
        let epics = [&og, &r_now];
        let c = ctx(&epics, &rm);
        let rows = [run_row("CI-1", Some("E-1"), 1), run_row("CI-2", Some("E-1"), 2), run_row("CI-3", None, 3), run_row("CI-4", Some("E-2"), 4)];
        let ready = |k: &str, e: &str| Issue { key: k.into(), summary: format!("R {k}"), parent_key: Some(e.into()), ..Default::default() };
        let sib = [ready("CI-50", "E-2"), ready("CI-51", "E-1"), ready("CI-52", "E-2")];
        let refs: Vec<&RunRow> = rows.iter().collect();
        let out = promoted_epics(&refs, &sib, &c);
        let keys: Vec<Option<&str>> = out.iter().map(|e| e.epic.as_ref().map(|e| e.key.as_str())).collect();
        assert_eq!(keys, [Some("E-2"), None, Some("E-1")], "roadmap, then no epic, then ongoing, whatever the counts");
        assert_eq!(out[0].epic.as_ref().unwrap().note.as_deref(), Some("roadmap (Now)"));
        assert_eq!(out[0].existing_ready, [("CI-50".to_string(), "R CI-50".to_string(), 1), ("CI-52".to_string(), "R CI-52".to_string(), 2)]);
        assert_eq!(out[2].candidates.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-2"]);
        assert_eq!(out[2].epic.as_ref().unwrap().note.as_deref(), Some("ongoing"));
    }

    #[test]
    fn needs_detail_is_capped_and_in_stage_order() {
        let rm = config::Roadmap::default();
        let c = ctx(&[], &rm);
        let rows: Vec<RunRow> = (1..=5).map(|i| run_row(&format!("CI-{i}"), if i % 2 == 0 { Some("E-1") } else { None }, i)).collect();
        let nd: Vec<(&RunRow, Vec<Code>)> = rows.iter().map(|r| (r, vec![Code::UnclearWhen])).collect();
        let (epics, total) = needs_detail_epics(&nd, 3, &c);
        assert_eq!(total, 5);
        assert_eq!(epics.iter().map(|e| e.rows.len()).sum::<usize>(), 3);
        assert_eq!(epics[0].epic, None, "no epic (stage 2) comes before an other epic (stage 4)");
        assert_eq!(epics[0].rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-3"], "rows keep rank order");
        assert_eq!(epics[1].epic.as_ref().unwrap().note.as_deref(), Some("other"));
    }

    #[test]
    fn pre_check_and_sibling_queries() {
        assert_eq!(pre_check_jql("CI", "PLAT", "New"), "project = CI AND labels = PLAT AND status = New ORDER BY rank ASC");
        assert_eq!(pre_check_jql("CI", "Platform Team", "New"), "project = CI AND labels = \"Platform Team\" AND status = New ORDER BY rank ASC");
        assert_eq!(
            siblings_jql("CI", "Ready for Work", &["CI-79".into(), "CI-9".into()]),
            "project = \"CI\" AND status = \"Ready for Work\" AND parent in (CI-79, CI-9) ORDER BY Rank ASC"
        );
    }
}
