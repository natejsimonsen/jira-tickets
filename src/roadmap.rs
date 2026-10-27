//! Pure derivation of roadmap initiatives from epics and their children.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde_json::Value;

use crate::config;
use crate::jira::Issue;

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Horizon {
    Now,
    Next,
    Later,
    Done,
}

/// Ordered worst first so a plain sort puts the problems on top.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Health {
    OffTrack,
    AtRisk,
    OnTrack,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Quarter {
    pub year: i32,
    pub q: u8,
}

impl Quarter {
    pub fn of(date: NaiveDate) -> Quarter {
        Quarter { year: date.year(), q: (date.month0() / 3 + 1) as u8 }
    }

    /// Accepts `2026-Q4`, `2026-q4`, `Q4 2026`, `Q4-2026`, or a `YYYY-MM-DD` date (a trailing time is ignored).
    pub fn parse(s: &str) -> Option<Quarter> {
        let s = s.trim();
        if let Some(d) = s.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
            return Some(Quarter::of(d));
        }
        let parts: Vec<&str> = s.split(|c: char| c == '-' || c.is_whitespace()).filter(|p| !p.is_empty()).collect();
        let [a, b] = parts[..] else { return None };
        let (year, qs) = if a.len() == 4 && a.chars().all(|c| c.is_ascii_digit()) { (a, b) } else { (b, a) };
        let year: i32 = if year.len() == 4 { year.parse().ok()? } else { return None };
        let q = qs.strip_prefix(['Q', 'q'])?.parse::<u8>().ok().filter(|q| (1..=4).contains(q))?;
        Some(Quarter { year, q })
    }

    pub fn label(&self) -> String {
        format!("Q{} {}", self.q, self.year)
    }
}

fn has_label(e: &Issue, label: &str) -> bool {
    e.labels.iter().any(|l| l == label)
}

pub fn is_initiative(epic: &Issue, label: &str, any_labelled: bool) -> bool {
    if any_labelled { has_label(epic, label) } else { !epic.summary.to_lowercase().contains("(ongoing)") }
}

/// (initiatives, ongoing). Ongoing is everything that is not an initiative.
pub fn select<'a>(epics: &[&'a Issue], label: &str) -> (Vec<&'a Issue>, Vec<&'a Issue>) {
    let any = epics.iter().any(|e| has_label(e, label));
    let inits = epics.iter().copied().filter(|e| is_initiative(e, label, any)).collect();
    let ongoing = epics.iter().copied().filter(|e| !is_initiative(e, label, any)).collect();
    (inits, ongoing)
}

pub fn quarter_from_labels(labels: &[String], prefix: &str) -> Option<Quarter> {
    labels.iter().find_map(|l| l.strip_prefix(prefix).and_then(Quarter::parse))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub todo: usize,
    pub doing: usize,
    pub done: usize,
}

#[derive(Clone, Debug)]
pub struct Initiative {
    pub key: String,
    pub name: String,
    pub horizon: Horizon,
    pub health: Health,
    pub priority: Option<String>,
    pub lane: String,
    pub start: Option<Quarter>,
    pub target: Option<Quarter>,
    pub owner: Option<String>,
    /// (author, short date, first paragraph)
    pub latest_update: Option<(String, String, String)>,
    pub confidence: Option<String>,
    pub next_milestone: Option<String>,
    pub definition_of_done: Option<String>,
    pub metrics: Vec<String>,
    pub progress: Progress,
    pub jira_key: String,
}

const NO_LANE: &str = "No lane";

/// A custom field value as plain text: strings, `{"value": ..}` options, and ADF documents.
fn field_text(v: &Value) -> Option<String> {
    let t = match v {
        Value::String(s) => s.trim().to_string(),
        Value::Object(o) if o.get("type").and_then(Value::as_str) == Some("doc") => crate::adf::to_plain_text(v).trim().to_string(),
        Value::Object(o) => o.get("value").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
        _ => String::new(),
    };
    (!t.is_empty()).then_some(t)
}

fn custom_text(e: &Issue, field: &Option<String>) -> Option<String> {
    field.as_ref().and_then(|f| e.custom.get(f)).and_then(field_text)
}

fn horizon_word(s: &str) -> Option<Horizon> {
    match s.trim().to_lowercase().as_str() {
        "now" => Some(Horizon::Now),
        "next" => Some(Horizon::Next),
        "later" => Some(Horizon::Later),
        "done" => Some(Horizon::Done),
        _ => None,
    }
}

fn health_word(s: &str) -> Health {
    let norm: String = s.to_lowercase().chars().map(|c| if c == '-' || c == '_' { ' ' } else { c }).collect();
    match norm.split_whitespace().collect::<Vec<_>>().join(" ").as_str() {
        "on track" => Health::OnTrack,
        "at risk" => Health::AtRisk,
        "off track" => Health::OffTrack,
        _ => Health::None,
    }
}

fn horizon_of(epic: &Issue, cfg: &config::Roadmap) -> Horizon {
    if let Some(h) = epic.labels.iter().find_map(|l| l.strip_prefix("horizon:").and_then(horizon_word)) {
        return h;
    }
    if let Some(h) = cfg.horizon_by_status.get(&epic.status.name).and_then(|w| horizon_word(w)) {
        return h;
    }
    match epic.status.category.as_str() {
        "done" => Horizon::Done,
        "indeterminate" => Horizon::Now,
        _ => Horizon::Later,
    }
}

fn health_of(epic: &Issue, cfg: &config::Roadmap) -> Health {
    if let Some(l) = epic.labels.iter().find_map(|l| l.strip_prefix("health:")) {
        return health_word(l);
    }
    custom_text(epic, &cfg.health_field).map(|v| health_word(&v)).unwrap_or(Health::None)
}

fn priority_of(epic: &Issue) -> Option<String> {
    ["P0", "P1"].iter().find(|p| has_label(epic, p)).map(|p| p.to_string()).or_else(|| match epic.priority.as_str() {
        "Highest" => Some("P0".into()),
        "High" => Some("P1".into()),
        _ => None,
    })
}

/// Quarter from the configured field (`duedate` reads `due_date`), else from the label prefix.
fn quarter_of(epic: &Issue, field: &Option<String>, prefix: &str) -> Option<Quarter> {
    match field {
        Some(f) if f == "duedate" => epic.due_date.as_deref().and_then(Quarter::parse),
        Some(_) => custom_text(epic, field).and_then(|v| Quarter::parse(&v)),
        None => quarter_from_labels(&epic.labels, prefix),
    }
}

fn latest_update(epic: &Issue) -> Option<(String, String, String)> {
    let c = epic.comments.iter().max_by(|a, b| a.created.cmp(&b.created))?;
    let text = crate::adf::to_plain_text(&c.body);
    let first = text.split("\n\n").next().unwrap_or("").lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    Some((c.author.clone(), crate::views::detail::short_date(&c.created), first))
}

pub fn derive(epic: &Issue, children: &[&Issue], cfg: &config::Roadmap) -> Initiative {
    let lane = epic.labels.iter().find_map(|l| cfg.lanes.get(l)).cloned().unwrap_or_else(|| NO_LANE.to_string());
    let mut progress = Progress::default();
    for c in children {
        match c.status.category.as_str() {
            "done" => progress.done += 1,
            "indeterminate" => progress.doing += 1,
            _ => progress.todo += 1,
        }
    }
    Initiative {
        key: epic.key.clone(),
        name: epic.summary.clone(),
        horizon: horizon_of(epic, cfg),
        health: health_of(epic, cfg),
        priority: priority_of(epic),
        lane,
        start: quarter_of(epic, &cfg.start_field, "start:"),
        target: quarter_of(epic, &cfg.target_field, "target:"),
        owner: epic.assignee.as_ref().map(|a| a.display_name.clone()),
        latest_update: latest_update(epic),
        confidence: custom_text(epic, &cfg.confidence_field),
        next_milestone: custom_text(epic, &cfg.next_milestone_field),
        definition_of_done: custom_text(epic, &cfg.definition_of_done_field),
        metrics: epic.labels.iter().filter_map(|l| l.strip_prefix("metric:")).map(|m| m.trim().to_string()).collect(),
        progress,
        jira_key: epic.key.clone(),
    }
}

fn urgency(i: &Initiative) -> (bool, Health) {
    (i.priority.as_deref() != Some("P0"), i.health)
}

/// Now, Next, Later columns: P0 first, then worst health.
pub fn now_next_later(items: &[Initiative]) -> [Vec<&Initiative>; 3] {
    [Horizon::Now, Horizon::Next, Horizon::Later].map(|h| {
        let mut v: Vec<&Initiative> = items.iter().filter(|i| i.horizon == h).collect();
        v.sort_by_key(|i| urgency(i));
        v
    })
}

pub fn shipped(items: &[Initiative]) -> Vec<&Initiative> {
    items.iter().filter(|i| i.horizon == Horizon::Done).collect()
}

/// Lanes (alphabetical by lane name because no config is passed, "No lane" last, empty lanes omitted) holding items targeted in `year`, ordered by
/// start quarter (target when no start); plus the tray of non-Done items with no target.
pub fn timeline(items: &[Initiative], year: i32) -> (Vec<(String, Vec<&Initiative>)>, Vec<&Initiative>) {
    let mut by_lane: BTreeMap<(bool, &str), Vec<&Initiative>> = BTreeMap::new();
    for i in items.iter().filter(|i| i.target.is_some_and(|t| t.year == year)) {
        by_lane.entry((i.lane == NO_LANE, i.lane.as_str())).or_default().push(i);
    }
    let lanes = by_lane
        .into_iter()
        .map(|((_, lane), mut v)| {
            v.sort_by_key(|i| i.start.or(i.target));
            (lane.to_string(), v)
        })
        .collect();
    let tray = items.iter().filter(|i| i.target.is_none() && i.horizon != Horizon::Done).collect();
    (lanes, tray)
}

/// Now items, worst health first.
pub fn status_rows(items: &[Initiative]) -> Vec<&Initiative> {
    let mut v: Vec<&Initiative> = items.iter().filter(|i| i.horizon == Horizon::Now).collect();
    v.sort_by_key(|i| i.health);
    v
}

/// Counts in `[OffTrack, AtRisk, OnTrack, None]` order.
pub fn health_counts(rows: &[&Initiative]) -> [usize; 4] {
    let mut c = [0; 4];
    for r in rows {
        c[r.health as usize] += 1;
    }
    c
}

/// (rows with an owner, total rows), for the "owners set n / total" tile.
pub fn owners_set(rows: &[&Initiative]) -> (usize, usize) {
    (rows.iter().filter(|r| r.owner.is_some()).count(), rows.len())
}

/// metric -> (active Now items, other non-Done items), sorted by metric name.
pub fn outcomes(items: &[Initiative]) -> Vec<(String, Vec<&Initiative>, Vec<&Initiative>)> {
    let mut by: BTreeMap<&str, (Vec<&Initiative>, Vec<&Initiative>)> = BTreeMap::new();
    for i in items.iter().filter(|i| i.horizon != Horizon::Done) {
        for m in &i.metrics {
            let e = by.entry(m.as_str()).or_default();
            if i.horizon == Horizon::Now { e.0.push(i) } else { e.1.push(i) }
        }
    }
    by.into_iter().map(|(m, (a, b))| (m.to_string(), a, b)).collect()
}

/// How far through `today`'s year we are, 0.0 to 1.0.
pub fn today_fraction(today: NaiveDate) -> f32 {
    let days = NaiveDate::from_ymd_opt(today.year(), 12, 31).map_or(365, |d| d.ordinal());
    today.ordinal0() as f32 / days as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Comment, Issue, Status, User};
    use chrono::NaiveDate;

    fn cfg() -> crate::config::Roadmap {
        crate::config::Roadmap::default()
    }
    fn epic(key: &str, status: &str, cat: &str, labels: &[&str]) -> Issue {
        Issue {
            key: key.into(),
            summary: format!("Epic {key}"),
            status: Status { name: status.into(), category: cat.into() },
            labels: labels.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }
    fn child(cat: &str) -> Issue {
        Issue { status: Status { name: "x".into(), category: cat.into() }, ..Default::default() }
    }

    #[test]
    fn selects_initiatives_by_label_or_ongoing_rule() {
        let a = epic("CI-1", "Canary Deployments", "indeterminate", &["roadmap"]);
        let mut b = epic("CI-2", "In Progress", "indeterminate", &[]);
        b.summary = "Deployer (ongoing)".into();
        let c = epic("CI-3", "Cluster Migration", "new", &[]);
        let (inits, ongoing) = select(&[&a, &b, &c], "roadmap");
        assert_eq!(inits.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["CI-1"], "label present on some epic: only labelled ones count");
        assert!(ongoing.iter().any(|e| e.key == "CI-2"));
        let (inits2, ongoing2) = select(&[&b, &c], "roadmap");
        assert_eq!(inits2.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["CI-3"], "no label anywhere: everything not (ongoing)");
        assert_eq!(ongoing2.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["CI-2"]);
    }

    #[test]
    fn quarters_and_today_fraction() {
        assert_eq!(Quarter::parse("2026-Q4"), Some(Quarter { year: 2026, q: 4 }));
        assert_eq!(Quarter::parse("Q1 2027"), Some(Quarter { year: 2027, q: 1 }));
        assert_eq!(Quarter::parse("2026-05-01"), Some(Quarter { year: 2026, q: 2 }));
        assert_eq!(Quarter::parse("2026-Q5"), None);
        assert_eq!(quarter_from_labels(&["PLAT".into(), "target:2026-Q3".into()], "target:"), Some(Quarter { year: 2026, q: 3 }));
        assert_eq!(Quarter::of(NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()), Quarter { year: 2026, q: 4 });
        assert_eq!(Quarter { year: 2026, q: 1 }.label(), "Q1 2026");
        let f = today_fraction(NaiveDate::from_ymd_opt(2026, 10, 6).unwrap());
        assert!((f - (278.0 / 365.0)).abs() < 0.01, "{f}");
    }

    #[test]
    fn derive_uses_status_labels_and_children() {
        let mut e =
            epic("CI-1860", "Work in Progress", "indeterminate", &["PLAT", "health:at-risk", "P0", "metric:Deployment Frequency", "target:2026-Q4", "roadmap"]);
        e.due_date = Some("2026-06-30".into()); // must be ignored: quarters come from labels
        e.assignee = Some(User { account_id: "a".into(), display_name: "Ada".into() });
        e.comments = vec![Comment {
            id: "1".into(),
            author: "Grace".into(),
            created: "2026-10-01T09:00:00.000-0700".into(),
            body: crate::adf::text_doc("Bar 1 missed.\nSecond para"),
        }];
        let kids = [child("done"), child("indeterminate"), child("new"), child("new")];
        let i = derive(&e, &kids.iter().collect::<Vec<_>>(), &cfg());
        assert_eq!(i.horizon, Horizon::Now);
        assert_eq!(i.health, Health::AtRisk);
        assert_eq!(i.priority.as_deref(), Some("P0"));
        assert_eq!(i.lane, "Platform");
        assert_eq!(i.target, Some(Quarter { year: 2026, q: 4 }));
        assert_eq!(i.start, None);
        assert_eq!(i.owner.as_deref(), Some("Ada"));
        assert_eq!(i.metrics, vec!["Deployment Frequency"]);
        assert_eq!(i.latest_update.as_ref().map(|u| u.2.as_str()), Some("Bar 1 missed."));
        assert_eq!((i.progress.todo, i.progress.doing, i.progress.done), (2, 1, 1));
        assert!(i.confidence.is_none() && i.next_milestone.is_none() && i.definition_of_done.is_none());
    }

    #[test]
    fn derive_defaults_and_overrides() {
        let i = derive(&epic("CI-1", "New", "new", &["dashboards"]), &[], &cfg());
        assert_eq!(i.horizon, Horizon::Later);
        assert_eq!(i.health, Health::None);
        assert_eq!(i.lane, "No lane");
        assert_eq!(i.target, None);
        let i2 = derive(&epic("CI-2", "Weird", "indeterminate", &["horizon:later"]), &[], &cfg());
        assert_eq!(i2.horizon, Horizon::Later, "label overrides status");
        let i3 = derive(&epic("CI-3", "Closed", "done", &[]), &[], &cfg());
        assert_eq!(i3.horizon, Horizon::Done);
        let mut c = cfg();
        c.health_field = Some("customfield_1".into());
        let mut e = epic("CI-4", "New", "new", &[]);
        e.custom.insert("customfield_1".into(), serde_json::json!({"value": "Off track"}));
        assert_eq!(derive(&e, &[], &c).health, Health::OffTrack);
    }

    #[test]
    fn projections_sort_and_group() {
        let mk = |k: &str, h: Horizon, health: Health, p: Option<&str>, lane: &str, target: Option<u8>, metrics: &[&str]| Initiative {
            key: k.into(),
            name: k.into(),
            horizon: h,
            health,
            priority: p.map(String::from),
            lane: lane.into(),
            start: None,
            target: target.map(|q| Quarter { year: 2026, q }),
            owner: None,
            latest_update: None,
            confidence: None,
            next_milestone: None,
            definition_of_done: None,
            metrics: metrics.iter().map(|m| m.to_string()).collect(),
            progress: Progress { todo: 0, doing: 0, done: 0 },
            jira_key: k.into(),
        };
        let items = vec![
            mk("A", Horizon::Now, Health::OnTrack, None, "Platform", Some(4), &["Cost"]),
            mk("B", Horizon::Now, Health::OffTrack, Some("P0"), "Platform", Some(3), &["Cost", "Toil"]),
            mk("C", Horizon::Next, Health::None, None, "No lane", None, &["Toil"]),
            mk("D", Horizon::Done, Health::None, None, "Platform", Some(2), &[]),
        ];
        let [now, next, later] = now_next_later(&items);
        assert_eq!(now.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["B", "A"]);
        assert_eq!(next.len(), 1);
        assert!(later.is_empty());
        assert_eq!(shipped(&items)[0].key, "D");
        let (lanes, tray) = timeline(&items, 2026);
        assert_eq!(lanes[0].0, "Platform");
        assert_eq!(lanes[0].1.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["D", "B", "A"]);
        assert_eq!(tray.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["C"]);
        let rows = status_rows(&items);
        assert_eq!(rows.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["B", "A"]);
        assert_eq!(health_counts(&rows), [1, 0, 1, 0]);
        assert_eq!(owners_set(&rows), (0, 2));
        let mut owned = items[0].clone();
        owned.owner = Some("Ada".into());
        assert_eq!(owners_set(&[&owned, &items[1]]), (1, 2));
        let out = outcomes(&items);
        assert_eq!(out[0].0, "Cost");
        assert_eq!(out[0].1.len(), 2);
        assert_eq!(out[1].0, "Toil");
        assert_eq!(out[1].1.len(), 1);
        assert_eq!(out[1].2.len(), 1);
    }
}
