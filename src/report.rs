//! Pre-check reports: parse the files the Claude-driven `team-pre-check` writes
//! (and this app's own), and render and write this app's reports in the plugin template.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EpicRef {
    pub key: String,
    pub summary: String,
    pub numeric_id: Option<String>,
    pub status: Option<String>,
    /// The stage note the writer adds to the heading: "roadmap (Now, P0)", "ongoing", "other".
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanningRow {
    pub key: String,
    pub summary: String,
    pub rank: Option<u32>,
    pub age: Option<String>,
    pub epic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NeedsDetailRow {
    pub key: String,
    pub summary: String,
    pub rank: Option<u32>,
    pub created: Option<String>,
    pub codes: Vec<String>,
    pub epic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub date: String,
    pub promoted: Vec<PlanningRow>,
    pub needs_detail: Vec<NeedsDetailRow>,
    pub epics: BTreeMap<String, EpicRef>,
    pub struck: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Badge {
    Promoted,
    NeedsDetail(Vec<String>),
    Struck,
}

#[derive(PartialEq)]
enum Section {
    Other,
    Planning,
    NeedsDetail,
}

pub fn parse(text: &str) -> Report {
    let mut r = Report::default();
    let mut section = Section::Other;
    let mut epic: Option<String> = None;
    let mut in_existing = false;
    for line in text.lines() {
        let t = line.trim_end();
        if let Some(title) = t.strip_prefix("# ") {
            // Only the first title carries the report date; later `# ` headings (appendices) must not override it.
            if r.date.is_empty()
                && let Some((_, d)) = title.rsplit_once("— ")
            {
                r.date = d.trim().to_string();
            }
            continue;
        }
        if let Some(h) = t.strip_prefix("## ") {
            section = if h.starts_with("Planning") {
                Section::Planning
            } else if h.starts_with("Needs detail") {
                Section::NeedsDetail
            } else {
                Section::Other
            };
            epic = None;
            in_existing = false;
            continue;
        }
        if section == Section::Other {
            continue;
        }
        if let Some(h) = t.strip_prefix("### ") {
            in_existing = false;
            epic = parse_epic_heading(h).map(|e| {
                let k = e.key.clone();
                r.epics.insert(k.clone(), e);
                k
            });
            continue;
        }
        if t.starts_with("Existing Ready for Work") {
            in_existing = true;
            continue;
        }
        if t.starts_with("Planning candidates") {
            // The plugin template puts the existing-Ready line before the candidates in the same epic block.
            in_existing = false;
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if !t.starts_with("- ") {
            continue;
        }
        for s in struck_keys(t) {
            if !r.struck.contains(&s) {
                r.struck.push(s);
            }
        }
        if in_existing {
            continue;
        }
        let Some((key, summary, notes)) = parse_row(t) else { continue };
        match section {
            Section::Planning => r.promoted.push(PlanningRow {
                key,
                summary,
                rank: note_u32(&notes, "rank "),
                age: notes.iter().find(|n| *n == "older" || n.starts_with("new (")).cloned(),
                epic: epic.clone(),
            }),
            Section::NeedsDetail => r.needs_detail.push(NeedsDetailRow {
                key,
                summary,
                rank: note_u32(&notes, "rank "),
                created: note_str(&notes, "created "),
                codes: note_str(&notes, "missing: ")
                    .map(|c| c.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                    .unwrap_or_default(),
                epic: epic.clone(),
            }),
            Section::Other => {}
        }
    }
    r
}

/// `[CI-1860 Progressive Delivery](url?issueParent=10001)` or `Epic [CI-79](url) · Cluster Operations (ongoing) · In Progress` or `(no epic)`.
fn parse_epic_heading(h: &str) -> Option<EpicRef> {
    let h = h.trim();
    if h.starts_with("(no epic)") {
        return None;
    }
    let h = h.strip_prefix("Epic ").unwrap_or(h);
    let open = h.find('[')?;
    let close = h[open..].find("](")? + open;
    let inner = &h[open + 1..close];
    let rest = &h[close + 2..];
    let url_end = rest.find(')').unwrap_or(rest.len());
    let url = &rest[..url_end];
    let after = rest.get(url_end + 1..).unwrap_or("");
    let numeric_id = url.split("issueParent=").nth(1).map(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>()).filter(|s| !s.is_empty());
    let (key, summary) = match inner.split_once(' ') {
        Some((k, s)) => (k.to_string(), s.trim().to_string()),
        None => (inner.to_string(), after.split('·').nth(1).map(|s| s.trim().to_string()).unwrap_or_default()),
    };
    let mut status = None;
    let mut note = None;
    for seg in after.split(" · ").skip(2).map(str::trim).filter(|s| !s.is_empty()) {
        if is_stage_note(seg) {
            note = Some(seg.to_string());
        } else if status.is_none() {
            status = Some(seg.to_string());
        }
    }
    Some(EpicRef { key, summary, numeric_id, status, note })
}

/// A heading segment the writer adds for the stage: "roadmap (…)", "ongoing", "other".
fn is_stage_note(seg: &str) -> bool {
    seg.starts_with("roadmap") || seg == "ongoing" || seg == "other"
}

/// `- [CI-1](url) — summary · note · note` → (key, summary, notes). Struck rows (`- ~~CI-1~~ — …`) return None.
fn parse_row(t: &str) -> Option<(String, String, Vec<String>)> {
    let body = t.strip_prefix("- ")?;
    let body = body.strip_prefix(|c: char| c.is_ascii_digit()).unwrap_or(body);
    let open = body.find('[')?;
    if body[..open].contains("~~") {
        return None;
    }
    let close = body[open..].find("](")? + open;
    let key = body[open + 1..close].to_string();
    let after = &body[close + 2..];
    let paren = after.find(')')?;
    let rest = after[paren + 1..].trim_start();
    let rest = rest.strip_prefix("— ").or_else(|| rest.strip_prefix("- ")).unwrap_or(rest);
    let mut parts = rest.split(" · ").map(|s| s.trim().to_string());
    let summary = parts.next().unwrap_or_default();
    Some((key, summary, parts.collect()))
}

fn struck_keys(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = t;
    while let Some(i) = rest.find("~~") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("~~") else { break };
        let k = after[..j].trim();
        if k.contains('-') && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            out.push(k.to_string());
        }
        rest = &after[j + 2..];
    }
    out
}

fn note_str(notes: &[String], prefix: &str) -> Option<String> {
    notes.iter().find_map(|n| n.strip_prefix(prefix).map(|s| s.trim().to_string()))
}

fn note_u32(notes: &[String], prefix: &str) -> Option<u32> {
    note_str(notes, prefix).and_then(|s| s.split_whitespace().next().and_then(|n| n.parse().ok()))
}

impl Report {
    pub fn badge_for(&self, key: &str) -> Option<Badge> {
        if self.struck.iter().any(|k| k == key) {
            return Some(Badge::Struck);
        }
        if self.promoted.iter().any(|p| p.key == key) {
            return Some(Badge::Promoted);
        }
        self.needs_detail.iter().find(|n| n.key == key).map(|n| Badge::NeedsDetail(n.codes.clone()))
    }
}

/// Newest `YYYY-MM-DD.md` in `dir`; other names only when no plain-dated file exists.
pub fn latest(dir: &Path) -> Option<(PathBuf, Report)> {
    let mut plain: Vec<PathBuf> = Vec::new();
    let mut other: Vec<PathBuf> = Vec::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.ends_with(".md") {
            continue;
        }
        let stem = &name[..name.len() - 3];
        let dated = stem.len() == 10 && stem.chars().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() });
        if dated { plain.push(p) } else { other.push(p) }
    }
    let pick = if plain.is_empty() { other } else { plain };
    let p = pick.into_iter().max()?;
    let text = std::fs::read_to_string(&p).ok()?;
    Some((p, parse(&text)))
}

pub struct WriterInput<'a> {
    pub date: &'a str,
    pub jql: &'a str,
    pub site: &'a str,
    pub board_id: u64,
    pub filter_id: u64,
    pub parent_board_id: u64,
    pub planning_cap: usize,
    pub needs_detail_cap: usize,
    pub promoted: &'a [PromotedEpic],
    pub needs_detail: &'a [NeedsDetailEpic],
    /// Needs-detail rows found before the cap was applied.
    pub needs_detail_total: usize,
    pub failed: &'a [(String, String, String)],
    pub scanned: usize,
    pub held_back: usize,
}

impl WriterInput<'_> {
    fn needs_detail_truncated(&self) -> usize {
        let shown: usize = self.needs_detail.iter().map(|e| e.rows.len()).sum();
        self.needs_detail_total.saturating_sub(shown)
    }
}

pub struct PromotedEpic {
    pub epic: Option<EpicRef>,
    /// (key, summary, rank position) of tickets already in Ready for Work.
    pub existing_ready: Vec<(String, String, u32)>,
    pub candidates: Vec<PlanningRow>,
}

pub struct NeedsDetailEpic {
    pub epic: Option<EpicRef>,
    pub rows: Vec<NeedsDetailRow>,
}

#[derive(Debug, PartialEq)]
pub enum WriteError {
    Exists(PathBuf),
    Io(String),
}

pub fn render(i: &WriterInput) -> String {
    let mut s = String::new();
    s.push_str(&format!("# PLAT pre-grooming — {}\n\n", i.date));
    s.push_str(&format!("JQL: `{}`\n", i.jql));
    s.push_str(&format!(
        "Boards: [Parent {pb}]({site}/jira/software/c/projects/CI/boards/{pb}/backlog) · [PLAT {b}]({site}/jira/software/c/projects/CI/boards/{b}/backlog?customFilter={f})\n",
        pb = i.parent_board_id,
        b = i.board_id,
        f = i.filter_id,
        site = i.site
    ));
    let epic_heading = |e: &Option<EpicRef>, with_status: bool| -> String {
        match e {
            None => "### (no epic)".to_string(),
            Some(e) => {
                let url = format!(
                    "{}/jira/software/c/projects/CI/boards/{}/backlog?customFilter={}&issueParent={}",
                    i.site,
                    i.board_id,
                    i.filter_id,
                    e.numeric_id.clone().unwrap_or_default()
                );
                let mut h = format!("### Epic [{}]({url}) · {}", e.key, e.summary);
                if with_status && let Some(st) = &e.status {
                    h.push_str(&format!(" · {st}"));
                }
                if let Some(n) = &e.note {
                    h.push_str(&format!(" · {n}"));
                }
                h
            }
        }
    };
    let browse = |k: &str| format!("{}/browse/{k}", i.site);
    if !i.promoted.is_empty() {
        s.push_str("\n## Planning — by epic\n");
        for pe in i.promoted {
            s.push_str(&format!("\n{}\n", epic_heading(&pe.epic, true)));
            if !pe.existing_ready.is_empty() {
                let list: Vec<String> = pe.existing_ready.iter().map(|(k, _, pos)| format!("{k} rank {pos}")).collect();
                s.push_str(&format!("\nExisting Ready for Work in this epic: {} ({})\n", pe.existing_ready.len(), list.join(", ")));
            }
            s.push_str("\nPlanning candidates:\n\n");
            for c in &pe.candidates {
                s.push_str(&format!(
                    "- [{k}]({u}) — {sum} · rank {r} · {age}\n",
                    k = c.key,
                    u = browse(&c.key),
                    sum = c.summary,
                    r = c.rank.unwrap_or(0),
                    age = c.age.clone().unwrap_or_else(|| "older".into())
                ));
            }
        }
    }
    let nd_rows: usize = i.needs_detail.iter().map(|e| e.rows.len()).sum();
    if nd_rows > 0 {
        s.push_str("\n## Needs detail (still in `New`)\n");
        for ne in i.needs_detail {
            s.push_str(&format!("\n{}\n\n", epic_heading(&ne.epic, false)));
            for r in &ne.rows {
                s.push_str(&format!(
                    "- [{k}]({u}) — {sum} · rank {rk} · created {c} · missing: {codes}\n",
                    k = r.key,
                    u = browse(&r.key),
                    sum = r.summary,
                    rk = r.rank.unwrap_or(0),
                    c = r.created.clone().unwrap_or_default(),
                    codes = r.codes.join(", ")
                ));
            }
        }
    }
    if !i.failed.is_empty() {
        s.push_str("\n## Failed transitions (manual triage required)\n\n");
        for (k, sum, err) in i.failed {
            s.push_str(&format!("- [{k}]({u}) — {sum} · `{err}`\n", u = browse(k)));
        }
    }
    let promoted_n: usize = i.promoted.iter().map(|e| e.candidates.len()).sum();
    let epics_n = i.promoted.iter().filter(|e| !e.candidates.is_empty()).count();
    s.push_str(&format!(
        "\n---\n\n**Run summary:** {} `New` tickets scanned · {} promoted to Planning · {} ready but held back (cap {}) · {} still need detail · {} Planning tickets shown across {} epics · {} rows truncated from Needs detail (cap {}) · {} failed transitions.\n",
        i.scanned,
        promoted_n,
        i.held_back,
        i.planning_cap,
        nd_rows,
        promoted_n,
        epics_n,
        i.needs_detail_truncated(),
        i.needs_detail_cap,
        i.failed.len()
    ));
    s
}

/// Writes `<dir>/<date>.md` via a temp file in the same directory, then renames it into place.
/// Refuses to replace an existing report unless `overwrite`.
pub fn write(dir: &Path, date: &str, text: &str, overwrite: bool) -> Result<PathBuf, WriteError> {
    let io = |e: std::io::Error| WriteError::Io(e.to_string());
    std::fs::create_dir_all(dir).map_err(io)?;
    let path = dir.join(format!("{date}.md"));
    if path.exists() && !overwrite {
        return Err(WriteError::Exists(path));
    }
    let tmp = dir.join(format!(".{date}.md.tmp"));
    std::fs::write(&tmp, text).map_err(io)?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real() -> Report {
        parse(&std::fs::read_to_string("tests/fixtures/report-2026-09-04.md").unwrap())
    }

    #[test]
    fn parses_title_date_and_epics() {
        let r = real();
        assert_eq!(r.date, "2026-09-04");
        let e = &r.epics["CI-1860"];
        assert_eq!(e.summary, "Progressive Delivery");
        assert_eq!(e.numeric_id.as_deref(), Some("10001"));
    }

    #[test]
    fn headings_carry_a_stage_note_that_is_not_a_status() {
        let text = "# PLAT pre-grooming — 2026-10-30\n\n## Planning — by epic\n\n### Epic [CI-1569](https://x/boards/101/backlog?customFilter=201&issueParent=1) · Cluster Upgrade · New · roadmap (Later, P0)\n\nPlanning candidates:\n\n- [CI-1](https://x/browse/CI-1) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### Epic [CI-79](https://x/boards/101/backlog?customFilter=201&issueParent=2) · Cluster Operations (ongoing) · ongoing\n\n- [CI-2](https://x/browse/CI-2) — b · rank 2 · created 2026-09-30 · missing: empty\n";
        let r = parse(text);
        let e = &r.epics["CI-1569"];
        assert_eq!(e.status.as_deref(), Some("New"));
        assert_eq!(e.note.as_deref(), Some("roadmap (Later, P0)"));
        let n = &r.epics["CI-79"];
        assert_eq!(n.status, None, "a note is not a status");
        assert_eq!(n.note.as_deref(), Some("ongoing"));
    }

    #[test]
    fn parses_promoted_rows_and_skips_existing_ready() {
        let r = real();
        let row = r.promoted.iter().find(|p| p.key == "CI-3727").expect("CI-3727 promoted");
        assert_eq!(row.rank, Some(175));
        assert_eq!(row.age.as_deref(), Some("older"));
        assert_eq!(row.epic.as_deref(), Some("CI-1860"));
        assert!(r.promoted.iter().any(|p| p.key == "CI-9156" && p.age.as_deref() == Some("new (14d)")));
        assert!(!r.promoted.iter().any(|p| p.key == "CI-3835"), "existing Ready for Work rows are not promoted");
    }

    #[test]
    fn parses_needs_detail_codes() {
        let r = real();
        let row = r.needs_detail.iter().find(|n| n.key == "CI-2950").expect("CI-2950");
        assert_eq!(row.codes, vec!["unclear-when", "unclear-what"]);
        assert_eq!(row.created.as_deref(), Some("2025-09-23"));
        assert_eq!(row.epic.as_deref(), Some("CI-65"));
    }

    #[test]
    fn struck_keys_and_badges() {
        let r = real();
        assert!(r.struck.contains(&"CI-4102".to_string()));
        assert!(matches!(r.badge_for("CI-3727"), Some(Badge::Promoted)));
        assert!(matches!(r.badge_for("CI-4058"), Some(Badge::NeedsDetail(c)) if c == vec!["unclear-when"]));
        assert!(matches!(r.badge_for("CI-4102"), Some(Badge::Struck)));
        assert!(r.badge_for("CI-0000").is_none());
    }

    #[test]
    fn parses_plugin_template_form_too() {
        let text = "# PLAT pre-grooming — 2026-10-01\n\n## Planning — by epic\n\n### Epic [CI-79](https://x/boards/101/backlog?customFilter=201&issueParent=10002) · Cluster Operations (ongoing) · In Progress\n\nPlanning candidates:\n\n- [CI-1](https://x/browse/CI-1) — Remove the old autoscaler · rank 3 · new (2d)\n\n## Needs detail (still in `New`)\n\n### (no epic)\n\n- [CI-2](https://x/browse/CI-2) — Look into X · rank 9 · created 2026-09-30 · missing: unclear-when\n";
        let r = parse(text);
        assert_eq!(r.epics["CI-79"].summary, "Cluster Operations (ongoing)");
        assert_eq!(r.promoted[0].key, "CI-1");
        assert_eq!(r.needs_detail[0].epic, None);
    }

    #[test]
    fn latest_prefers_plain_dated_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("2026-08-21.md"), "# PLAT pre-grooming — 2026-08-21\n").unwrap();
        std::fs::write(dir.path().join("2026-09-04.md"), "# PLAT pre-grooming — 2026-09-04\n").unwrap();
        std::fs::write(dir.path().join("2026-09-30-simulation.md"), "# x — 2026-09-30\n").unwrap();
        let (p, r) = latest(dir.path()).unwrap();
        assert!(p.ends_with("2026-09-04.md"));
        assert_eq!(r.date, "2026-09-04");
    }

    #[test]
    fn render_matches_golden() {
        let kube = EpicRef {
            key: "CI-79".into(),
            summary: "Cluster Operations (ongoing)".into(),
            numeric_id: Some("10002".into()),
            status: Some("In Progress".into()),
            note: Some("roadmap (Now, P0)".into()),
        };
        let input = WriterInput {
            date: "2026-10-08",
            jql: "project = CI AND labels = PLAT AND status = New ORDER BY rank ASC",
            site: "https://example.atlassian.net",
            board_id: 101,
            filter_id: 201,
            parent_board_id: 100,
            planning_cap: 10,
            needs_detail_cap: 10,
            promoted: &[
                PromotedEpic {
                    epic: Some(kube.clone()),
                    existing_ready: vec![("CI-100".into(), "Old one".into(), 1), ("CI-101".into(), "Older one".into(), 4)],
                    candidates: vec![PlanningRow {
                        key: "CI-1".into(),
                        summary: "Remove the old autoscaler".into(),
                        rank: Some(3),
                        age: Some("new (2d)".into()),
                        epic: Some("CI-79".into()),
                    }],
                },
                PromotedEpic {
                    epic: None,
                    existing_ready: vec![],
                    candidates: vec![PlanningRow {
                        key: "CI-2".into(),
                        summary: "Delete deploy.toml".into(),
                        rank: Some(9),
                        age: Some("older".into()),
                        epic: None,
                    }],
                },
            ],
            needs_detail: &[NeedsDetailEpic {
                epic: Some(EpicRef { status: None, note: Some("other".into()), ..kube.clone() }),
                rows: vec![NeedsDetailRow {
                    key: "CI-3".into(),
                    summary: "Look into X".into(),
                    rank: Some(12),
                    created: Some("2026-09-30".into()),
                    codes: vec!["unclear-when".into()],
                    epic: Some("CI-79".into()),
                }],
            }],
            needs_detail_total: 1,
            failed: &[("CI-4".into(), "Broken one".into(), "transition failed (status 400)".into())],
            scanned: 12,
            held_back: 0,
        };
        let out = render(&input);
        let golden = std::fs::read_to_string("tests/fixtures/report-golden.md").unwrap();
        assert_eq!(out, golden);
        // The writer's output must parse back with the same promoted and needs-detail keys.
        let back = parse(&out);
        assert_eq!(back.promoted.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["CI-1", "CI-2"]);
        assert_eq!(back.needs_detail[0].codes, vec!["unclear-when"]);
        assert_eq!(back.epics["CI-79"].numeric_id.as_deref(), Some("10002"));
    }

    #[test]
    fn write_refuses_to_overwrite_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(dir.path(), "2026-10-08", "a", false).unwrap();
        assert!(p.ends_with("2026-10-08.md"));
        assert!(matches!(write(dir.path(), "2026-10-08", "b", false), Err(WriteError::Exists(_))));
        write(dir.path(), "2026-10-08", "b", true).unwrap();
        assert_eq!(std::fs::read_to_string(p).unwrap(), "b");
    }
}
