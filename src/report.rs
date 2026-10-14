//! Pre-check reports: parse the files the Claude-driven `team-pre-check` writes
//! (and this app's own), and write this app's reports in the plugin template.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EpicRef {
    pub key: String,
    pub summary: String,
    pub numeric_id: Option<String>,
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
    Some(EpicRef { key, summary, numeric_id })
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
}
