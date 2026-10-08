# Jira Tickets, plan 3 of 3: Team and Roadmap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Read `CLAUDE.md` at the repo root first. Plan 2 (`2026-10-08-grooming.md`) must be complete: this plan relies on the `src/app/` module split and `App::for_test`.

**Goal:** Two screens over the existing store: Team (one column per member showing what they are working on) and Roadmap (the four mockup tabs over epics: Now/Next/Later, Timeline, Status review, Outcomes), with a pure derivation layer that maps Jira fields to the mockup's concepts and flags what is missing.

**Architecture:** `roadmap.rs` derives an `Initiative` per epic from status, labels, due date, comments, and configured custom fields; `teamview.rs` derives member columns from the store's assignee projection. Both are pure and unit-tested; the views only draw. One new load: a per-member "done in the last 7 days" search run on demand.

**Tech Stack:** Rust 2024, eframe/egui 0.36.2, `chrono` for quarters and dates. Toolchain via `nix develop` only.

**Spec:** `docs/superpowers/specs/2026-10-07-jira-tickets-design.md`, sections 8 (Team view) and 9 (Roadmap view), and the mockup `~/Documents/designs/2026-10-06-roadmap-layout-mockups.html` for layout and colour tokens (already in `theme::Palette`: `on_track`, `at_risk`, `off_track`, `no_health`, `done`, `today`).

## Global Constraints

- Every cargo command runs as `nix develop --command cargo <args>` (alias `ndc`). No system cargo.
- Before each commit: fmt, clippy `-D warnings` with and without `--features snapshot`, full `cargo test`. Conventional commits, no trailers; BLOCKED on a 1Password signing failure.
- Team members come from `cfg.team.members` (Jira account ids or display names; the live config holds account ids). Match an issue to a member by `assignee.account_id` or `assignee.display_name`, case-insensitive. Column order = config order. A member matching no issue in any list shows "no tickets found".
- Team columns show, per member: In Progress status category first, then assigned `Ready for Work`, each as a card with key, summary, epic chip (epic tint), status pill (status tint), and "N d in status" from `status_changed`. "Unassigned" column last shows the top of the `Ready for Work` queue (team-label scoped). A collapsed "done in the last 7 days" footer per column loads on expand with `project in ({projects}) AND assignee = "{member}" AND statusCategory = Done AND resolved >= -7d ORDER BY resolved DESC`.
- Roadmap field mapping (spec §9 table) with these defaults: horizon from `cfg.roadmap.horizon_by_status` plus Done category → Done (the live config must include `"Work in Progress" = "now"`; add it to `config::Roadmap::default()` and `EXAMPLE`); health from labels `health:on-track|at-risk|off-track` or `health_field`; priority chip from `priority` Highest → P0, High → P1 or labels `P0`/`P1`; target from `duedate` or `target_field`; start from `start_field`; lane from the first label present in `cfg.roadmap.lanes` else "No lane"; latest update from the newest epic comment (author, date, first paragraph via `adf::to_plain_text`); confidence / next milestone / definition of done from the configured custom fields, else missing; metrics from labels prefixed `metric:`.
- Quarters: current calendar year, Q1–Q4; a date maps to its quarter; bars span start quarter → target quarter (target only → one quarter); no target → Unscheduled tray. Today line at the current date's fraction of the year.
- Progress per initiative = children (`store.children_of(epic)`) counted by status category (`new` → to do, `indeterminate` → in progress, `done` → done).
- Missing values render as red "missing"/"not set" text (`p.off_track`), never as blanks.
- New keys go into `keys::BINDINGS` first (README table test). Team: `j`/`k` move within a column, `h`/`l` switch column, Enter opens the issue in the Issues screen (select + `SetScreen(Issues)`), `d` toggles the Done footer. Roadmap: `1`–`4` switch tabs, `j`/`k` move, Enter opens the epic in Issues.

Shorthand: `alias ndc='nix develop --command'`.

## File structure (plan 3)

| File | Responsibility |
|---|---|
| `src/teamview.rs` | Pure member-column derivation: `Member`, `Column`, `columns(...)`, days-in-status, done footer JQL |
| `src/views/team.rs` | Team screen |
| `src/roadmap.rs` | Pure derivation: `Initiative`, `Horizon`, `Health`, `Quarter`, `derive(...)`, tab projections |
| `src/views/roadmap.rs` | Roadmap screen with four tabs |
| `src/app/loads.rs` | `load_done_recent(member)` |
| `src/app/{msg,actions,mod,shortcuts}.rs` | `Msg::DoneRecent`, `TeamState`, `RoadmapState`, actions and keys |

---

### Task 1: Team derivation (pure)

**Files:**
- Create: `src/teamview.rs`
- Modify: `src/main.rs` (`mod teamview;`)

**Interfaces:**
- `pub struct Member { pub id: String, pub display: String }` (`id` is the configured string; `display` the assignee display name seen on any matching issue, else the configured string)
- `pub struct Card<'a> { pub issue: &'a Issue, pub days_in_status: Option<i64> }`
- `pub struct Column<'a> { pub member: Option<Member>, pub in_progress: Vec<Card<'a>>, pub ready: Vec<Card<'a>>, pub none_found: bool }` (`member == None` is the Unassigned column)
- `pub fn matches(issue: &Issue, member_id: &str) -> bool` (account id or display name, case-insensitive)
- `pub fn columns<'a>(members: &[String], in_progress: &[&'a Issue], team_open: &[&'a Issue], status_ready: &str, today: chrono::DateTime<chrono::Utc>) -> Vec<Column<'a>>`: per member, `in_progress` filtered by `matches` and status category `indeterminate` (deduplicated by key, in list order), then `team_open` items with status `status_ready` and `matches`; the Unassigned column takes `team_open` items with status `status_ready` and no assignee, in rank order (capped at 10). `none_found` when both lists are empty for that member AND no issue in either input matches the member at all.
- `pub fn days_in_status(status_changed: Option<&str>, today: DateTime<Utc>) -> Option<i64>` parsing Jira's `2026-10-01T10:00:00.000-0700` with `chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.3f%z")`.
- `pub fn jql_done_recent(projects: &[String], member_id: &str) -> String` → `project in ("CI", "OPS") AND assignee = "<id>" AND statusCategory = Done AND resolved >= -7d ORDER BY resolved DESC` (quote with `store::jql_quote`).

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status, User};
    use chrono::{TimeZone, Utc};

    fn issue(key: &str, status: &str, cat: &str, who: Option<(&str, &str)>, changed: Option<&str>) -> Issue {
        Issue { key: key.into(), status: Status { name: status.into(), category: cat.into() },
            assignee: who.map(|(id, n)| User { account_id: id.into(), display_name: n.into() }), status_changed: changed.map(String::from), ..Default::default() }
    }
    fn today() -> chrono::DateTime<Utc> { Utc.with_ymd_and_hms(2026, 10, 8, 12, 0, 0).unwrap() }

    #[test]
    fn matches_by_id_or_name_case_insensitive() {
        let i = issue("CI-1", "Work in Progress", "indeterminate", Some(("abc", "Grace Hopper")), None);
        assert!(matches(&i, "abc")); assert!(matches(&i, "grace hopper")); assert!(!matches(&i, "nate"));
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
        assert_eq!(cols[0].in_progress[0].issue.key, "CI-1"); assert_eq!(cols[0].in_progress[0].days_in_status, Some(7));
        assert_eq!(cols[0].ready[0].issue.key, "CI-2");
        assert!(cols[1].none_found && cols[1].member.as_ref().unwrap().display == "zzz");
        assert!(cols[2].member.is_none()); assert_eq!(cols[2].ready.iter().map(|c| c.issue.key.as_str()).collect::<Vec<_>>(), ["CI-3"]);
    }

    #[test]
    fn done_recent_jql() {
        assert_eq!(jql_done_recent(&["CI".into(), "OPS".into()], "abc"), r#"project in ("CI", "OPS") AND assignee = "abc" AND statusCategory = Done AND resolved >= -7d ORDER BY resolved DESC"#);
    }
}
```

- [ ] **Step 2: Run to verify failure** → compile errors. **Step 3: Implement.** **Step 4:** `ndc cargo test teamview` → 4 passed. **Step 5: Commit** `feat(team): pure member-column derivation`.

---

### Task 2: Team screen

**Files:**
- Create: `src/views/team.rs`
- Modify: `src/views/mod.rs` (dispatch `Screen::Team`), `src/app/mod.rs` (`pub team: TeamState { pub col: usize, pub row: usize, pub done_open: HashSet<String>, pub done: HashMap<String, Vec<Issue>>, pub done_loading: HashSet<String> }`), `src/app/loads.rs` (`load_done_recent(member_id)`), `src/app/msg.rs` (`Msg::DoneRecent { member: String, result: jira::Result<Vec<Issue>> }`), `src/app/actions.rs` (`TeamMove(i32)`, `TeamCol(i32)`, `TeamOpen`, `TeamToggleDone`), `src/app/shortcuts.rs`, `src/keys.rs`, `README.md`.

**Interfaces:**
- `views::team::show(app, ui)`: horizontal `ScrollArea` of equal-width columns (min 260 px). Column header: display name (bold) plus counts "3 in progress · 2 ready"; "nothing in progress" marker in muted italics when `in_progress` is empty; "no tickets found" in `p.off_track` when `none_found`. Cards: key (mono, muted), summary (bold 13.5), pills: status (`status_tint`), epic (`epic_tint`, parent key + summary), "N d" days-in-status in muted. Focused card highlighted with `p.selected_row`. Footer "Done in the last 7 days ▸" toggles (`d` or click) and lists `team.done[member]` keys + summaries, or a spinner while loading.
- Keys (Team screen, no text focus): `h`/`l` (or ←/→) change column, `j`/`k` move within the column (in_progress then ready), Enter → `Select(key)` + `SetScreen(Issues)`, `d` toggles the focused column's footer (loads on first open).
- `load_done_recent(member)`: `should_load`-guarded on `team.done_loading`; uses `teamview::jql_done_recent(&cfg.scope.projects, member)` with `jira::LIST_FIELDS`; result stored in `team.done`.

- [ ] **Step 1: Failing headless tests** in `src/app/mod.rs`: `team_move_clamps_and_open_selects` (absorb two issues for one member, `TeamMove(1)` then `TeamOpen` selects the second key and switches the screen) and `done_recent_result_is_stored` (handle `Msg::DoneRecent`).
- [ ] **Step 2: Run to verify failure. Step 3: Implement.** Pure helper in `teamview.rs`: `pub fn clamp_cursor(col: usize, row: usize, cols: &[Column]) -> (usize, usize)` with a test. **Step 4:** build, test, fmt, clippy, hands-off launch. **Step 5: Commit** `feat(team): member columns with in-progress, ready, and recent-done footer`.

---

### Task 3: Roadmap derivation (pure)

**Files:**
- Create: `src/roadmap.rs`
- Modify: `src/main.rs` (`mod roadmap;`), `src/config.rs` (`Roadmap::default()` and `EXAMPLE` gain `"Work in Progress" = "now"`; update `minimal_config_gets_defaults`)

**Interfaces:**
- `#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)] pub enum Horizon { Now, Next, Later, Done }`
- `#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)] pub enum Health { OffTrack, AtRisk, OnTrack, None }` (ordered worst first for sorting)
- `pub struct Quarter { pub year: i32, pub q: u8 }` with `pub fn of(date: chrono::NaiveDate) -> Quarter` and `pub fn label(&self) -> String` ("Q4 2026")
- `pub struct Progress { pub todo: usize, pub doing: usize, pub done: usize }`
- `pub struct Initiative { pub key: String, pub name: String, pub horizon: Horizon, pub health: Health, pub priority: Option<String>, pub lane: String, pub start: Option<Quarter>, pub target: Option<Quarter>, pub owner: Option<String>, pub latest_update: Option<(String, String, String)>, pub confidence: Option<String>, pub next_milestone: Option<String>, pub definition_of_done: Option<String>, pub metrics: Vec<String>, pub progress: Progress, pub jira_key: String }`
- `pub fn derive(epic: &Issue, children: &[&Issue], cfg: &config::Roadmap) -> Initiative` implementing the spec §9 table: horizon from `cfg.horizon_by_status[status.name]` else by status category (`done` → Done, `indeterminate` → Now, else Later), overridable by labels `horizon:now|next|later`; health from labels or `health_field` (string value or object `.value`); priority Highest → "P0", High → "P1", labels `P0`/`P1` win; target from `target_field` (custom string `YYYY-MM-DD`) else `due_date`; start from `start_field`; lane = first label in `cfg.lanes` (map value) else "No lane"; owner = assignee display name; latest update = newest comment `(author, short date, first paragraph of to_plain_text)`; confidence/next milestone/DoD from custom fields as plain strings (ADF objects via `adf::to_plain_text`); metrics = labels with `metric:` prefix stripped; progress from children by status category.
- Tab projections: `pub fn now_next_later(items: &[Initiative]) -> [Vec<&Initiative>; 3]` sorted P0 first then worst health; `pub fn shipped(items: &[Initiative]) -> Vec<&Initiative>`; `pub fn timeline(items: &[Initiative], year: i32) -> (Vec<(String, Vec<&Initiative>)>, Vec<&Initiative>)` lanes in `cfg.lanes` order then "No lane", items with a target in `year` sorted by start quarter, plus the unscheduled tray; `pub fn status_rows(items: &[Initiative]) -> Vec<&Initiative>` (Now only, worst health first); `pub fn health_counts(rows: &[&Initiative]) -> [usize; 4]`; `pub fn owners_set(rows: &[&Initiative]) -> (usize, usize)`; `pub fn outcomes(items: &[Initiative]) -> Vec<(String, Vec<&Initiative>, Vec<&Initiative>)>` (metric → active Now items, other non-Done items), sorted by metric name; `pub fn today_fraction(today: NaiveDate) -> f32` (0.0–1.0 through the year).

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Comment, Issue, Status, User};
    use chrono::NaiveDate;

    fn cfg() -> crate::config::Roadmap { crate::config::Roadmap::default() }
    fn epic(key: &str, status: &str, cat: &str, labels: &[&str]) -> Issue {
        Issue { key: key.into(), summary: format!("Epic {key}"), status: Status { name: status.into(), category: cat.into() }, labels: labels.iter().map(|l| l.to_string()).collect(), ..Default::default() }
    }
    fn child(cat: &str) -> Issue { Issue { status: Status { name: "x".into(), category: cat.into() }, ..Default::default() } }

    #[test]
    fn quarters_and_today_fraction() {
        assert_eq!(Quarter::of(NaiveDate::from_ymd_opt(2026, 10, 6).unwrap()), Quarter { year: 2026, q: 4 });
        assert_eq!(Quarter { year: 2026, q: 1 }.label(), "Q1 2026");
        let f = today_fraction(NaiveDate::from_ymd_opt(2026, 10, 6).unwrap());
        assert!((f - (278.0 / 365.0)).abs() < 0.01, "{f}");
    }

    #[test]
    fn derive_uses_status_labels_and_children() {
        let mut e = epic("CI-1860", "Work in Progress", "indeterminate", &["PLAT", "health:at-risk", "P0", "metric:Deployment Frequency"]);
        e.due_date = Some("2026-12-31".into());
        e.assignee = Some(User { account_id: "a".into(), display_name: "Ada".into() });
        e.comments = vec![Comment { id: "1".into(), author: "Grace".into(), created: "2026-10-01T09:00:00.000-0700".into(), body: crate::adf::text_doc("Bar 1 missed.\nSecond para") }];
        let kids = [child("done"), child("indeterminate"), child("new"), child("new")];
        let i = derive(&e, &kids.iter().collect::<Vec<_>>(), &cfg());
        assert_eq!(i.horizon, Horizon::Now); assert_eq!(i.health, Health::AtRisk); assert_eq!(i.priority.as_deref(), Some("P0"));
        assert_eq!(i.lane, "Platform"); assert_eq!(i.target, Some(Quarter { year: 2026, q: 4 })); assert_eq!(i.start, None);
        assert_eq!(i.owner.as_deref(), Some("Ada")); assert_eq!(i.metrics, vec!["Deployment Frequency"]);
        assert_eq!(i.latest_update.as_ref().map(|u| u.2.as_str()), Some("Bar 1 missed."));
        assert_eq!((i.progress.todo, i.progress.doing, i.progress.done), (2, 1, 1));
        assert!(i.confidence.is_none() && i.next_milestone.is_none() && i.definition_of_done.is_none());
    }

    #[test]
    fn derive_defaults_and_overrides() {
        let i = derive(&epic("CI-1", "New", "new", &["dashboards"]), &[], &cfg());
        assert_eq!(i.horizon, Horizon::Later); assert_eq!(i.health, Health::None); assert_eq!(i.lane, "No lane"); assert_eq!(i.target, None);
        let i2 = derive(&epic("CI-2", "Weird", "indeterminate", &["horizon:later"]), &[], &cfg());
        assert_eq!(i2.horizon, Horizon::Later, "label overrides status");
        let i3 = derive(&epic("CI-3", "Closed", "done", &[]), &[], &cfg());
        assert_eq!(i3.horizon, Horizon::Done);
        let mut c = cfg(); c.health_field = Some("customfield_1".into());
        let mut e = epic("CI-4", "New", "new", &[]); e.custom.insert("customfield_1".into(), serde_json::json!({"value": "Off track"}));
        assert_eq!(derive(&e, &[], &c).health, Health::OffTrack);
    }

    #[test]
    fn projections_sort_and_group() {
        let mk = |k: &str, h: Horizon, health: Health, p: Option<&str>, lane: &str, target: Option<u8>, metrics: &[&str]| Initiative {
            key: k.into(), name: k.into(), horizon: h, health, priority: p.map(String::from), lane: lane.into(), start: None,
            target: target.map(|q| Quarter { year: 2026, q }), owner: None, latest_update: None, confidence: None, next_milestone: None,
            definition_of_done: None, metrics: metrics.iter().map(|m| m.to_string()).collect(), progress: Progress { todo: 0, doing: 0, done: 0 }, jira_key: k.into() };
        let items = vec![
            mk("A", Horizon::Now, Health::OnTrack, None, "Platform", Some(4), &["Cost"]),
            mk("B", Horizon::Now, Health::OffTrack, Some("P0"), "Platform", Some(3), &["Cost", "Toil"]),
            mk("C", Horizon::Next, Health::None, None, "No lane", None, &["Toil"]),
            mk("D", Horizon::Done, Health::None, None, "Platform", Some(2), &[]),
        ];
        let [now, next, later] = now_next_later(&items);
        assert_eq!(now.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["B", "A"]);
        assert_eq!(next.len(), 1); assert!(later.is_empty());
        assert_eq!(shipped(&items)[0].key, "D");
        let (lanes, tray) = timeline(&items, 2026);
        assert_eq!(lanes[0].0, "Platform"); assert_eq!(lanes[0].1.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["D", "B", "A"]);
        assert_eq!(tray.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["C"]);
        let rows = status_rows(&items);
        assert_eq!(rows.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["B", "A"]);
        assert_eq!(health_counts(&rows), [1, 0, 1, 0]);
        let out = outcomes(&items);
        assert_eq!(out[0].0, "Cost"); assert_eq!(out[0].1.len(), 2);
        assert_eq!(out[1].0, "Toil"); assert_eq!(out[1].1.len(), 1); assert_eq!(out[1].2.len(), 1);
    }
}
```

- [ ] **Step 2: Run to verify failure. Step 3: Implement.** Use `chrono::NaiveDate::ordinal0` for `today_fraction` (`ordinal0 as f32 / days_in_year`). Children categories: `"done"` → done, `"indeterminate"` → doing, else todo. **Step 4:** `ndc cargo test roadmap` → 4 passed, plus the config default test updated. **Step 5: Commit** `feat(roadmap): derive initiatives from epics with the mockup's field mapping`.

---

### Task 4: Roadmap screen, tabs 1 and 3 (Now/Next/Later, Status review)

**Files:**
- Create: `src/views/roadmap.rs`
- Modify: `src/views/mod.rs` (dispatch `Screen::Roadmap`), `src/app/mod.rs` (`pub roadmap: RoadmapState { pub tab: u8, pub cursor: usize }`), `src/app/actions.rs` (`RoadmapTab(u8)`, `RoadmapMove(i32)`, `RoadmapOpen`), `src/app/shortcuts.rs`, `src/keys.rs`, `README.md`

**Interfaces:**
- `views::roadmap::show(app, ui)`: derive `Vec<Initiative>` each frame from `store.list(ListId::Epics)` and `store.children_of` (cheap at ~35 epics; cache by `store.fetched_at` if a profiler says otherwise), a tab strip ("1 Now / Next / Later", "2 Timeline", "3 Status review", "4 Outcomes") styled like the mockup's tabs (underline in `p.accent`), then the tab body. Shared pieces: `fn health_dot(ui, health, p)` using `views::health_dot` with `on_track`/`at_risk`/`off_track`/`no_health` (hollow ring for None), `fn missing(ui, p, text)` red small text, `fn progress_bar(ui, p, &Progress)` a 6 px three-segment bar (done `on_track`, doing `accent`, todo `surface_2`) with "d / n" text.
- Tab 1: three columns (`col` frames on `surface_2`), column heads with counts and the mockup's subtitles ("In progress this quarter", "Committed, starts when capacity frees up", "Ideas and parked work, not committed"); cards: health dot (Now only) + name (bold), chips: P0 (`off_track` outline), lane, up to two metrics; the card body shows the latest update text in Now, otherwise the name only; progress bar at the bottom. Below: "Shipped in {year}" list of Done initiatives with their target quarter.
- Tab 3: health summary tiles (counts per health + "owners set n / total") then a table (`egui::Grid`, striped) of `status_rows`: Initiative (name + P0 chip, sub-line lane · jira key · owner or red "missing"), Health, Confidence (or "not set"), Latest update, Next milestone (or "not set"), Definition of done (or "not defined"), Progress bar.
- Keys: `1`–`4` tab, `j`/`k` move the cursor through the current tab's rows (tab 1: all three columns top to bottom; tab 3: table rows), Enter → `Select(jira_key)` + `SetScreen(Issues)`; cursor row highlighted.

- [ ] **Step 1: Failing headless tests**: `roadmap_tab_and_move_clamp` (set tab 3 with two Now epics, `RoadmapMove(5)` clamps to 1, `RoadmapOpen` selects that epic key and switches screen).
- [ ] **Step 2: Run to verify failure. Step 3: Implement.** **Step 4:** build, test, fmt, clippy, hands-off launch. **Step 5: Commit** `feat(roadmap): Now/Next/Later and Status review tabs`.

---

### Task 5: Roadmap tabs 2 and 4 (Timeline, Outcomes)

**Files:**
- Modify: `src/views/roadmap.rs`

**Interfaces:**
- Tab 2: a grid with a 170 px lane-label column and four quarter columns (`Q1 {year}` …). For each lane from `roadmap::timeline`: a tinted lane background (cycle `theme::EPIC_HUES` at low alpha per lane index via `tint_from_hue`), bars positioned from start quarter to target quarter (`egui::Rect` math over the quarter column widths), bar fill `p.accent`, Done bars `p.done` with "✓ " prefix, P0 bars with a 3 px `off_track` left edge, health dot + name inside; quarter separator lines; a 2 px `p.today` vertical line at `today_fraction` with the date label. Below: "Unscheduled" tray of chips for items with no target. Hover on a bar shows the name as a tooltip. No keyboard cursor on this tab beyond `1`–`4`.
- Tab 4: groups from `roadmap::outcomes` as cards (`metric` heading with "n active"), list of active items with health dots then later items muted with their horizon in parentheses, and the amber "Nothing in progress moves this metric." gap note when no active items. When no initiative has any `metric:` label: a single muted sentence "No `metric:` labels on epics yet; add them to see outcomes."

- [ ] **Step 1:** Pure, tested helper in `roadmap.rs`: `pub fn bar_span(start: Option<Quarter>, target: Option<Quarter>, year: i32) -> Option<(u8, u8)>` (1-based inclusive quarter indices, clamped to 1..=4; None when no target or target in another year; start defaults to target). Tests: `(None, Some(Q4)) → (4,4)`, `(Some(Q2), Some(Q4)) → (2,4)`, `(Some(Q4), Some(Q2)) → (2,2)`, other year → None.
- [ ] **Step 2–5:** implement, build, test, launch; commit `feat(roadmap): Timeline and Outcomes tabs`.

---

### Task 6: Docs, screenshot, and close-out

**Files:**
- Modify: `README.md` (Team and Roadmap sections: what each column/tab shows, the field mapping table from spec §9 with the config keys, the `metric:` / `health:` / `horizon:` label conventions), `CLAUDE.md` (plans complete; remove the "temporary" `#![allow(dead_code)]` note once the attribute is removed), `src/main.rs` (remove `#![allow(dead_code)]` and fix any resulting warnings).

- [ ] **Step 1:** Remove `#![allow(dead_code)]`; fix or delete whatever is unused (clippy `-D warnings` will list it).
- [ ] **Step 2:** README sections; remove placeholder texts for Team and Roadmap.
- [ ] **Step 3:** If `op` is unlocked, take offscreen screenshots with `JIRA_TICKETS_VIEW=team` and `=roadmap` and look at them (`Read` the PNGs) for contrast and layout; fix obvious issues. Otherwise record "unverified visually".
- [ ] **Step 4:** Full check set; commit `docs: describe Team and Roadmap screens and finish plan 3`.

## Known gaps to state in the README

- Epics in the live project currently have no due dates, priorities, or roadmap custom fields, and only three have comments, so the Roadmap will show mostly "missing" flags until the team fills those fields or sets `health:`/`horizon:`/`metric:` labels. That is by design (spec §9): the red flags are the to-do list.
- The TOOLS board includes unlabelled tickets by assignee; team columns only cover members listed in `[team].members`.
