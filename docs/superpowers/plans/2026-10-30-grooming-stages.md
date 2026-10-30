# Jira Tickets, plan 4: Grooming session by stage

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Read `CLAUDE.md` at the repo root first. Plans 2 and 3 must be complete (this plan relies on `src/grooming.rs`, `src/app/groom.rs`, `src/roadmap.rs` and `App::for_test`).

**Goal:** The Grooming screen's Session mode walks the team's epics in four stages (roadmap epics in Roadmap-screen order, tickets with no epic, ongoing epics, other epics), shows each epic's active-sprint and next-sprint tickets as context over its Ready / Proposed / New queues, accepts `New` tickets straight into Ready for Work, and re-ranks the Ready queue with `J`/`K`; the pre-check report orders its epics the same way.

**Architecture:** `grooming.rs` stays the pure model: a `Stage` per epic and a `StageContext` (the store's epics, the roadmap config, the active and next sprint names) turn the team's open tickets into ordered `EpicQueue`s with five blocks. `app/groom.rs` keeps the cursor over one combined list (Ready rows, then Proposed, then New) and runs the writes through `spawn_write` as today. `views/grooming.rs` only draws. No new network calls: sprint membership comes from the `sprint` field already parsed on every issue and the board's sprint list already loaded for the Issues picker.

**Tech Stack:** Rust 2024, eframe/egui 0.36.2. Toolchain via `nix develop` only.

**Spec:** `docs/superpowers/specs/2026-10-07-jira-tickets-design.md`, section 7 "Session mode" (rewritten 2026-10-30) and section 9 for the horizon derivation the stage order reuses.

## Global Constraints

- Every cargo command runs as `nix develop --command cargo <args>` (alias `ndc`). No system cargo.
- Before each commit: `ndc cargo fmt`, `ndc cargo clippy --all-targets -- -D warnings`, the same with `--features snapshot`, `ndc cargo test`. Conventional commits, no co-author or attribution trailers. If `git commit` fails on 1Password signing ("failed to fill whole buffer", "agent returned an error", or a hang), stop and report BLOCKED with the exact error; never investigate or change signing config.
- Architecture invariants from `CLAUDE.md`: every write goes through `App::spawn_write` and `jira::check_writable()`; `App::spawn` no-ops without a client so every spawning action checks `client.is_some()` first; views push `Action`s, never mutate; new keys go into `keys::BINDINGS` first (the README table between `<!-- keys:start -->` and `<!-- keys:end -->` must match it; test `readme_matches_bindings`).
- Stage order (spec §7): 1 Roadmap (epics that are initiatives by `roadmap::is_initiative` with `cfg.roadmap.initiative_label`), ordered by horizon Now < Next < Later < Done (`roadmap::horizon_of`), then priority `P0` < `P1` < none (`roadmap::priority_of`), then the epic's position in the store's Epics list (Jira rank), then key; 2 No epic; 3 Ongoing (summary contains "(ongoing)", case-insensitive, and not an initiative), by rank then key; 4 Other, by rank then key.
- Sprint blocks take precedence: a ticket whose `sprint` equals the active sprint's name goes in the sprint block, one whose `sprint` equals the first future sprint's name goes in the next block, and neither appears in the Ready, Proposed or New queues. Active sprint = the first `Sprint` in `app.sprints` with `state == "active"`; next = the first with `state == "future"`.
- Candidates are Proposed (`status_planning`) then `New` (`status_new`) rows, in rank order within each. An epic with no candidates is dimmed and skipped by `]`/`[` without Shift. An epic with nothing in Ready, Proposed or New is left out of the rail.
- The cursor is one index over `ready ++ planning ++ new`. Entering an epic (mode switch, `]`/`[`, click) puts it on the first candidate (index `ready.len()`). `y`/`b`/`p` act only when the cursor is on a candidate; `J`/`K` act only when it is on a Ready row.
- Copy rules (docs/design/criteria.md): sentence case, `muted` for context text, no new colours, spacing on the 4/8/12/16/24 scale.
- Status names come from `cfg.grooming` (`status_new`, `status_planning`, `status_ready`, `status_backlog`); never hard-code "New" or "Planning" outside tests. The word "Proposed" is UI copy for the `status_planning` queue.

Shorthand: `alias ndc='nix develop --command'`.

## File structure (plan 4)

| File | Responsibility |
|---|---|
| `src/grooming.rs` | `Stage`, `StageContext`, `StageKey`, `sprint_names`, the staged `epic_queues`, `EpicQueue::{candidates, rows, first_candidate}`, staged `promoted_epics` / `needs_detail_epics` |
| `src/roadmap.rs` | `horizon_of` and `priority_of` become `pub` |
| `src/app/groom.rs` | `App::stage_context`, combined cursor, accept from New, bounce on New, `rank_ready` |
| `src/app/{actions,shortcuts,mod}.rs` | `Action::RankReady`, `J`/`K` keys |
| `src/keys.rs`, `README.md` | Bindings and hints |
| `src/views/grooming.rs` | Rail with stage headers and five counts; centre with five blocks; selection on Ready rows |
| `src/views/grooming_guide.rs` | Session wording |
| `src/report.rs` | `EpicRef.note`, heading suffix, parser tolerance |
| `docs/superpowers/specs/…`, `README.md`, `CLAUDE.md` | Deviations (plan 4), Session docs, plan status |

---

### Task 1: Stage model and the combined cursor

**Files:**
- Modify: `src/roadmap.rs:150-170` (make `horizon_of` and `priority_of` `pub`)
- Modify: `src/grooming.rs` (`EpicQueue`, `epic_queues`, new types)
- Modify: `src/app/groom.rs` (`session_queues`, `groom_mode`, `groom_move`, `groom_epic`, `session_keep`, `session_focus`, `session_ready_keys`)
- Modify: `src/views/grooming.rs:513-555` (only what must change to compile: `q.new_count` → `q.new.len()`, the rail skip rule uses candidates)
- Test: `src/grooming.rs` tests, `src/app/groom.rs` tests

**Interfaces:**
- Consumes: `roadmap::{is_initiative, horizon_of, priority_of, Horizon}`, `jira::Sprint { name, state }`, `Issue.sprint: Option<String>`, `App.sprints: Vec<Sprint>`, `store.list(ListId::Epics)`.
- Produces (later tasks rely on these exact names):
  ```rust
  pub enum Stage { Roadmap, NoEpic, Ongoing, Other }          // Ord in this order
  impl Stage { pub fn label(self) -> &'static str }            // "Roadmap", "No epic", "Ongoing", "Other epics"
  pub struct StageContext<'a> { pub epics: Vec<&'a Issue>, pub initiative_label: &'a str, pub roadmap: &'a config::Roadmap, pub active_sprint: Option<&'a str>, pub next_sprint: Option<&'a str> }
  pub struct StageKey { pub stage: Stage, pub horizon: Option<Horizon>, pub priority: Option<String>, pub rank_pos: usize, pub key: Option<String> }  // Ord
  impl StageKey { pub fn note(&self) -> Option<String> }       // "roadmap (Now, P0)", "roadmap (Later)", "ongoing", "other", None for no epic
  pub fn stage_of(parent: Option<&str>, parent_summary: Option<&str>, ctx: &StageContext) -> StageKey
  pub fn sprint_names(sprints: &[Sprint]) -> (Option<&str>, Option<&str>)
  pub struct EpicQueue<'a> { pub epic: Option<String>, pub summary: String, pub stage: StageKey, pub sprint: Vec<&'a Issue>, pub next: Vec<&'a Issue>, pub ready: Vec<&'a Issue>, pub planning: Vec<&'a Issue>, pub new: Vec<&'a Issue> }
  impl<'a> EpicQueue<'a> { pub fn candidates(&self) -> Vec<&'a Issue>; pub fn rows(&self) -> Vec<&'a Issue>; pub fn first_candidate(&self) -> usize }
  pub fn epic_queues<'a>(issues: &[&'a Issue], team_label: &str, g: &config::Grooming, ctx: &StageContext) -> Vec<EpicQueue<'a>>
  // app/groom.rs
  impl App { pub fn stage_context(&self) -> StageContext<'_>; pub fn session_queues(&self) -> Vec<EpicQueue<'_>>; pub fn session_focus(&self) -> Option<&Issue> /* candidate under the cursor only */; pub fn session_ready_focus(&self) -> Option<(usize, String)> /* (index in ready, key) when the cursor is on a Ready row */ }
  ```

- [ ] **Step 1: Make the roadmap helpers public**

In `src/roadmap.rs` change `fn horizon_of(` to `pub fn horizon_of(` and `fn priority_of(` to `pub fn priority_of(`. Add a doc line to each: `/// The epic's horizon: a `horizon:` label, else `horizon_by_status`, else its status category.` and `/// `P0`/`P1` from labels, else from Jira priority (Highest → P0, High → P1).`

- [ ] **Step 2: Write the failing model tests**

Append to `src/grooming.rs`'s `mod tests` (reuse the existing `issue(...)` helper; add an `epic` helper):

```rust
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
        let tickets = [t("CI-1", Some("E-O")), t("CI-2", Some("E-X")), t("CI-3", None), t("CI-4", Some("E-R1")), t("CI-5", Some("E-R2")), t("CI-6", Some("E-R3"))];
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
```

Update the three existing `epic_queues` tests to the new signature and order. They have no Epics list, so every epic is `Other` and sorts by key; `(no epic)` comes before them (stage 2 precedes stage 4):

```rust
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
```

Add `use crate::roadmap::Horizon;` to the test module's imports.

- [ ] **Step 3: Run the tests to see them fail**

Run: `ndc cargo test grooming::tests 2>&1 | tail -20`
Expected: compile errors naming `Stage`, `StageContext`, `epic_queues` arity.

- [ ] **Step 4: Implement the model**

In `src/grooming.rs`, add `use crate::jira::Sprint; use crate::roadmap::{self, Horizon};` and replace the `EpicQueue` struct and `epic_queues` function with:

```rust
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
```

- [ ] **Step 5: Wire the app: context, queues and the combined cursor**

In `src/app/groom.rs`:

```rust
use crate::grooming::{self, Candidate, Code, EpicQueue, Promotion, RunRow, StageContext, Verdict};
```

Replace `session_queues` and add `stage_context`:

```rust
    /// What `grooming::epic_queues` needs from the app: the loaded epics, the roadmap rules, the sprint names.
    pub fn stage_context(&self) -> StageContext<'_> {
        let (active, next) = grooming::sprint_names(&self.sprints);
        StageContext {
            epics: self.store.list(ListId::Epics),
            initiative_label: &self.cfg.roadmap.initiative_label,
            roadmap: &self.cfg.roadmap,
            active_sprint: active,
            next_sprint: next,
        }
    }

    /// The session rail: the team's action items (no epics) grouped by epic, in stage order.
    pub fn session_queues(&self) -> Vec<EpicQueue<'_>> {
        let ctx = self.stage_context();
        grooming::epic_queues(&crate::filter::action_items(&self.store.list(ListId::TeamOpen)), &self.cfg.scope.team_label, &self.cfg.grooming, &ctx)
    }

    /// The cursor index of the selected epic's first candidate, where a freshly entered epic starts.
    fn session_start_cursor(&self) -> usize {
        self.session_queues().get(self.grooming.epic_index).map_or(0, |q| q.first_candidate())
    }
```

`groom_mode`: after `g.mode = mode;` set the cursor with `self.grooming.cursor = if mode == GroomMode::Session { self.session_start_cursor() } else { 0 };` (compute before taking `let g = &mut self.grooming`).

`groom_move`: the Session arm becomes `self.session_queues().get(self.grooming.epic_index).map_or(0, |q| q.rows().len().saturating_sub(1))`.

`groom_epic`: `let counts: Vec<usize> = self.session_queues().iter().map(|q| q.candidates().len()).collect();` then `step_epic(&counts, …)`; after `g.epic_index = to;` set the cursor to the new epic's first candidate (compute `let start = self.session_queues().get(to).map_or(0, |q| q.first_candidate());` before the mutable borrow and assign `g.cursor = start`). Update `step_epic`'s doc comment to say "candidates" instead of "Planning tickets".

`session_focus` (the candidate a decision acts on):

```rust
    /// Session: the candidate under the cursor. A Ready row under the cursor is not a candidate.
    pub fn session_focus(&self) -> Option<&Issue> {
        let queues = self.session_queues();
        let q = queues.get(self.grooming.epic_index)?;
        let at = self.grooming.cursor.checked_sub(q.first_candidate())?;
        q.candidates().get(at).copied()
    }

    /// Session: the Ready row under the cursor, as (index in the Ready queue, key).
    pub fn session_ready_focus(&self) -> Option<(usize, String)> {
        let queues = self.session_queues();
        let q = queues.get(self.grooming.epic_index)?;
        let at = self.grooming.cursor;
        q.ready.get(at).map(|i| (at, i.key.clone()))
    }
```

(`session_focus` returned `Option<&Issue>` borrowed from `self` before; keep that by returning the issue through the store: `q.candidates().get(at).map(|i| self.store.issues.get(&i.key)).flatten()` if the borrow checker rejects the direct form. The store's issues are keyed by `key`; `Store.issues` is a `BTreeMap<String, Issue>`, pub within the crate.)

`session_keep`: `let n = queues.get(at).map_or(0, |q| q.rows().len());` and keep the rest.

In `src/views/grooming.rs::session_body`, make it compile only: `new: q.new.len()` for the rail row, `q.planning.len()` where it was, and `let cursor = app.grooming.cursor.min(…)` against `ready.len() + planning.len()` for now. Task 3 redraws this function; do not redesign it here.

- [ ] **Step 6: Cursor tests in `app/groom.rs`**

Append to `src/app/groom.rs` tests (the `session_app` helper has CI-5 and CI-6 Ready, CI-1 and CI-2 Planning under CI-79):

```rust
    #[test]
    fn entering_the_session_focuses_the_first_candidate() {
        let (base, _rx) = crate::jira::tests::stub_server(vec![]);
        let mut app = session_app(&base);
        app.grooming.mode = GroomMode::PreCheck;
        app.groom_mode(GroomMode::Session);
        assert_eq!(app.grooming.cursor, 2, "past the two Ready rows");
        assert_eq!(app.session_focus().map(|i| i.key.as_str()), Some("CI-1"));
        assert_eq!(app.session_ready_focus(), None);
    }

    #[test]
    fn the_cursor_walks_ready_rows_then_candidates() {
        let (base, _rx) = crate::jira::tests::stub_server(vec![]);
        let mut app = session_app(&base);
        app.grooming.cursor = 2;
        app.groom_move(-1);
        assert_eq!(app.session_ready_focus(), Some((1, "CI-6".to_string())));
        assert_eq!(app.session_focus(), None, "a Ready row is not a candidate");
        app.groom_move(-5);
        assert_eq!(app.grooming.cursor, 0);
        app.groom_move(9);
        assert_eq!(app.grooming.cursor, 3, "the last candidate");
        assert_eq!(app.session_focus().map(|i| i.key.as_str()), Some("CI-2"));
    }

    #[test]
    fn stepping_epics_lands_on_the_first_candidate_and_skips_epics_without_one() {
        let (base, _rx) = crate::jira::tests::stub_server(vec![]);
        let mut app = session_app(&base);
        let mut only_ready = session_issue("CI-20", "Ready for Work");
        only_ready.parent_key = Some("CI-80".into());
        only_ready.parent_summary = Some("Epic CI-80".into());
        let mut with_new = session_issue("CI-30", "New");
        with_new.parent_key = Some("CI-81".into());
        with_new.parent_summary = Some("Epic CI-81".into());
        let mut ready_81 = session_issue("CI-31", "Ready for Work");
        ready_81.parent_key = Some("CI-81".into());
        ready_81.parent_summary = Some("Epic CI-81".into());
        // `absorb` replaces the list, so the session's four tickets go in again with the new ones.
        app.store.absorb(
            ListId::TeamOpen,
            vec![
                session_issue("CI-5", "Ready for Work"),
                session_issue("CI-1", "Planning"),
                session_issue("CI-6", "Ready for Work"),
                session_issue("CI-2", "Planning"),
                only_ready,
                with_new,
                ready_81,
            ],
        );
        // Unlisted epics sort by key: CI-79, CI-80, CI-81.
        app.grooming.epic_index = 0;
        app.groom_epic(1, true);
        assert_eq!(app.grooming.epic_index, 2, "CI-80 has no candidate and is skipped");
        assert_eq!(app.grooming.cursor, 1, "past CI-81's one Ready row");
        assert_eq!(app.session_focus().map(|i| i.key.as_str()), Some("CI-30"), "a New ticket is a candidate");
        app.groom_epic(-1, false);
        assert_eq!(app.grooming.epic_index, 1, "Shift lands on the empty epic");
        assert_eq!(app.session_focus(), None);
    }
```

`stub_server` takes `Vec<(method, path, json)>` and returns `(base_url, Receiver<Recorded>)`; an empty route list is fine for tests that never write.

- [ ] **Step 7: Run the full checks**

Run: `ndc cargo fmt && ndc cargo clippy --all-targets -- -D warnings && ndc cargo clippy --all-targets --features snapshot -- -D warnings && ndc cargo test 2>&1 | tail -5`
Expected: all green. The `session_draws_rail_queue_ghost_and_bounce_box` view test asserts `"CI-1860 Progressive Delivery · P 1 · R 1 · N 0"` and `both_modes_draw_headless` asserts `"(no epic) · P 0 · R 1 · N 1"`; the rail wording is unchanged in this task so they still pass. If a view test fails only because the cursor now starts past the Ready rows, set `app.grooming.cursor` explicitly in that test.

- [ ] **Step 8: Commit**

```bash
git add src/grooming.rs src/roadmap.rs src/app/groom.rs src/views/grooming.rs
git commit -m "feat(grooming): stage the session's epics and walk ready rows with the candidates"
```

---

### Task 2: Session actions: accept from New, bounce on New, re-rank Ready rows

**Files:**
- Modify: `src/app/groom.rs` (`bounce`, new `rank_ready`)
- Modify: `src/app/actions.rs` (`Action::RankReady(i32)`)
- Modify: `src/app/shortcuts.rs:160-235` (`J`/`K` in Session)
- Modify: `src/keys.rs` (`BINDINGS`, `HintContext::GroomSession`)
- Modify: `README.md` (the generated keys table)
- Test: `src/app/groom.rs` tests, `src/keys.rs` tests

**Interfaces:**
- Consumes: `App::session_focus`, `App::session_ready_focus` (Task 1), `jira::Client::rank(key, before, after)`, `App.pending_ranks: Vec<(ListId, String, String)>` and the `"Rank"` arm of `Msg::Done` in `src/app/msg.rs` (undoes the swap on error).
- Produces: `Action::RankReady(i32)`; `App::rank_ready(delta: i32)`.

- [ ] **Step 1: Failing tests**

Append to `src/app/groom.rs` tests:

```rust
    #[test]
    fn accept_moves_a_new_ticket_straight_to_ready() {
        let mut new_ticket = session_issue("CI-7", "New");
        new_ticket.status.category = "new".into();
        let (base, rx) = crate::jira::tests::stub_server(vec![
            ("GET", "/rest/api/3/issue/CI-7/transitions", transitions_json()),
            ("POST", "/rest/api/3/issue/CI-7/transitions", serde_json::json!({})),
            ("PUT", "/rest/agile/1.0/issue/rank", serde_json::json!({})),
        ]);
        let mut app = session_app(&base);
        // `absorb` replaces the list: the session's four tickets plus the New one.
        app.store.absorb(
            ListId::TeamOpen,
            vec![session_issue("CI-5", "Ready for Work"), session_issue("CI-1", "Planning"), session_issue("CI-6", "Ready for Work"), session_issue("CI-2", "Planning"), new_ticket],
        );
        // Rows: CI-5, CI-6 (Ready), CI-1, CI-2 (Planning), CI-7 (New).
        app.grooming.cursor = 4;
        assert_eq!(app.session_focus().map(|i| i.key.as_str()), Some("CI-7"));
        app.start_accept();
        app.grooming.placement = Some(0);
        app.accept();
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(w[0].1, "/rest/api/3/issue/CI-7/transitions");
        assert_eq!(w[0].2["transition"]["id"], "31", "the transition to Ready for Work, found by name");
        assert_eq!(w[1].2["rankBeforeIssue"], "CI-5");
    }

    #[test]
    fn bounce_on_a_new_ticket_posts_the_comment_only() {
        let mut new_ticket = session_issue("CI-7", "New");
        new_ticket.status.category = "new".into();
        let (base, rx) = crate::jira::tests::stub_server(vec![("POST", "/rest/api/3/issue/CI-7/comment", serde_json::json!({"id": "1"}))]);
        let mut app = session_app(&base);
        app.store.absorb(
            ListId::TeamOpen,
            vec![session_issue("CI-5", "Ready for Work"), session_issue("CI-1", "Planning"), session_issue("CI-6", "Ready for Work"), session_issue("CI-2", "Planning"), new_ticket],
        );
        app.grooming.cursor = 4;
        app.grooming.bounce_text = Some("Which clusters?".into());
        app.bounce();
        drain_until(&mut app, |a| a.grooming.deciding.is_none());
        let w = writes(&rx);
        assert_eq!(w.len(), 1, "no transition for a ticket already in New: {w:?}");
        assert_eq!(w[0].1, "/rest/api/3/issue/CI-7/comment");
        assert!(app.grooming.bounce_text.is_none(), "the box closes");
    }

    #[test]
    fn shift_j_ranks_a_ready_row_after_the_next_one() {
        let (base, rx) = crate::jira::tests::stub_server(vec![("PUT", "/rest/agile/1.0/issue/rank", serde_json::json!({}))]);
        let mut app = session_app(&base);
        app.grooming.cursor = 0; // CI-5
        app.rank_ready(1);
        assert_eq!(app.grooming.cursor, 1, "the cursor follows the ticket");
        let ready: Vec<String> = app.session_queues()[0].ready.iter().map(|i| i.key.clone()).collect();
        assert_eq!(ready, ["CI-6", "CI-5"], "swapped at once");
        drain_until(&mut app, |a| a.pending_ranks.is_empty());
        let w = writes(&rx);
        assert_eq!(w[0].2["issues"][0], "CI-5");
        assert_eq!(w[0].2["rankAfterIssue"], "CI-6");
    }

    #[test]
    fn shift_k_at_the_top_and_on_a_candidate_do_nothing() {
        let (base, rx) = crate::jira::tests::stub_server(vec![]);
        let mut app = session_app(&base);
        app.grooming.cursor = 0;
        app.rank_ready(-1);
        app.grooming.cursor = 2; // CI-1, a candidate
        app.rank_ready(1);
        assert!(writes(&rx).is_empty());
        assert!(app.pending_ranks.is_empty());
    }

    #[test]
    fn a_failed_rank_swaps_the_ready_rows_back() {
        let (base, _rx) = crate::jira::tests::stub_server(vec![("PUT", "/rest/agile/1.0/issue/rank", serde_json::json!({"errorMessages": ["no"]}))]);
        let mut app = session_app(&base);
        app.grooming.cursor = 0;
        app.rank_ready(1);
        drain_until(&mut app, |a| a.pending_ranks.is_empty());
        let ready: Vec<String> = app.session_queues()[0].ready.iter().map(|i| i.key.clone()).collect();
        assert_eq!(ready, ["CI-5", "CI-6"]);
    }
```

Every test that writes starts with `let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());` as `accept_transitions_to_ready_then_ranks_at_the_ghost` (`src/app/groom.rs:877`) does; add that line to the first, second, third and fifth tests above. For the failing rank, copy the error reply `an_accept_whose_rank_fails_still_moves_the_ticket` uses. `pending_ranks` is a private field of `App` in `src/app/mod.rs`; tests in `src/app/groom.rs` are inside the `app` module tree so they can read it.

In `src/keys.rs` tests, extend `readme_table_groups_and_quotes_keys` (or add a sibling) with:

```rust
        assert!(t.contains("| `J/K` | Move a Ready for Work ticket down or up in rank |\n"));
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `ndc cargo test app::groom 2>&1 | tail -20`
Expected: `rank_ready` not found; the New accept test fails because `session_app`'s cursor logic is new.

- [ ] **Step 3: Implement**

`src/app/groom.rs`, `bounce`: find whether the focused candidate is already `New` and skip the transition:

```rust
    pub(super) fn bounce(&mut self) {
        let Some(text) = self.grooming.bounce_text.clone().filter(|t| grooming::bounce_comment_ok(t)) else { return };
        let Some(key) = self.decidable() else { return };
        let status = self.cfg.grooming.status_new.clone();
        // A New ticket gets the comment only; there is nothing to move it back to.
        let already_new = self.session_focus().is_some_and(|i| i.status.name == status);
        self.grooming.deciding = Some(key.clone());
        self.spawn_write(move |c, tx| {
            let (what, result) = match jira::check_writable().and_then(|_| c.add_comment(&key, text.trim())) {
                Err(e) => ("Bounce", Err(e)),
                Ok(()) if already_new => ("Bounce", Ok(())),
                Ok(()) => match transition_to(c, &key, &status) {
                    Ok(()) => ("Bounce", Ok(())),
                    Err(e) => ("Bounce move", Err(jira::Error::Other(bounce_move_failed(&key, &status, &e)))),
                },
            };
            let _ = tx.send(Msg::Done { key, what, result });
        });
    }
```

Check `session_done` (`src/app/groom.rs:488`): on a successful "Bounce" it sets the ticket's status to `status_new` in the store, which is already true for a New ticket; make sure it also closes the bounce box (`self.grooming.bounce_text = None`) on success, as it does today.

Add `rank_ready` next to `park`:

```rust
    /// `J`/`K`: moves the Ready row under the cursor one place down or up with one rank call. The row swaps at
    /// once and the cursor follows it; a failed rank swaps it back when its Done lands (`pending_ranks`).
    pub(super) fn rank_ready(&mut self, delta: i32) {
        if self.grooming.mode != GroomMode::Session || self.client.is_none() || self.grooming.placement.is_some() || self.grooming.bounce_text.is_some() {
            return;
        }
        let Some((at, key)) = self.session_ready_focus() else { return };
        let ready: Vec<String> = self.session_ready_keys();
        let to = at as i64 + delta as i64;
        if to < 0 || to >= ready.len() as i64 {
            return;
        }
        let neighbour = ready[to as usize].clone();
        if let Err(e) = jira::check_writable() {
            let (text, kind) = done_toast("Rank", &Err(e));
            self.toast_kind(text, kind);
            return;
        }
        if let Some(list) = self.store.lists.get_mut(&ListId::TeamOpen)
            && let (Some(a), Some(b)) = (list.iter().position(|k| *k == key), list.iter().position(|k| *k == neighbour))
        {
            list.swap(a, b);
            self.pending_ranks.push((ListId::TeamOpen, key.clone(), neighbour.clone()));
            self.grooming.cursor = to as usize;
        }
        let before = delta < 0;
        self.spawn_write(move |c, tx| {
            let (b, a) = if before { (Some(neighbour.as_str()), None) } else { (None, Some(neighbour.as_str())) };
            let _ = tx.send(Msg::Done { key: key.clone(), what: "Rank", result: c.rank(&key, b, a) });
        });
    }
```

`pending_ranks` is private to `src/app/mod.rs`; make it `pub(super)` (or `pub(crate)`) so `groom.rs` can push to it. `done_toast` is already imported in `groom.rs`.

`src/app/actions.rs`: add the variant after `Park`:

```rust
    /// Session `J`/`K`: moves the focused Ready for Work row one place down or up in rank.
    RankReady(i32),
```

and the arm `Action::RankReady(d) => self.rank_ready(d),` next to `Action::Park`.

`src/app/shortcuts.rs`, in `grooming_shortcuts`: the shared `j`/`k` block runs for both modes, so take Shift+J/K first. Replace

```rust
        if key(Key::J) | key(Key::ArrowDown) {
            self.actions.push(Action::GroomMove(1));
        }
        if key(Key::K) | key(Key::ArrowUp) {
            self.actions.push(Action::GroomMove(-1));
        }
```

with

```rust
        // Shift+J/K re-rank a Ready row in the session; checked before plain j/k, which consume_shortcut(NONE, J)
        // would otherwise match too.
        let shifted = |k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::SHIFT, k)));
        if self.grooming.mode == GroomMode::Session {
            if shifted(Key::J) {
                self.actions.push(Action::RankReady(1));
            }
            if shifted(Key::K) {
                self.actions.push(Action::RankReady(-1));
            }
        }
        if key(Key::J) | key(Key::ArrowDown) {
            self.actions.push(Action::GroomMove(1));
        }
        if key(Key::K) | key(Key::ArrowUp) {
            self.actions.push(Action::GroomMove(-1));
        }
```

and reuse that `shifted` closure in the Session arm below instead of defining it again.

`src/keys.rs`: in `BINDINGS`, change the two session lines and add one:

```rust
    b(Group::Grooming, "y", "Accept into Ready for Work from Planning or New: ↑/↓ to place, Enter to commit"),
    b(Group::Grooming, "b", "Bounce with a comment (⌘Enter posts); a Planning ticket goes back to New"),
    b(Group::Grooming, "p", "Park in Backlog"),
    b(Group::Grooming, "J/K", "Move a Ready for Work ticket down or up in rank"),
```

In `hints`, `HintContext::GroomSession` gains `&[K("J/K"), W("rank")]` after the `p` entry.

`README.md`: edit the three rows in the keys table between `<!-- keys:start -->` and `<!-- keys:end -->` to match (format: ``| `J/K` | Move a Ready for Work ticket down or up in rank |``), inserting the `J/K` row right after the `p` row. Run `ndc cargo test readme_matches_bindings`; its assertion message prints both tables when they differ.

- [ ] **Step 4: Full checks**

Run: `ndc cargo fmt && ndc cargo clippy --all-targets -- -D warnings && ndc cargo clippy --all-targets --features snapshot -- -D warnings && ndc cargo test 2>&1 | tail -5`
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add src/app/groom.rs src/app/actions.rs src/app/shortcuts.rs src/app/mod.rs src/keys.rs README.md
git commit -m "feat(grooming): accept New tickets, comment-only bounce on New, and J/K re-ranking"
```

---

### Task 3: The session view: stage rail and five blocks

**Files:**
- Modify: `src/views/grooming.rs:495-710` (`RailRow`, `session_body`, `rail_line`, `rail_counts`, `epic_rail`, `epic_queue`)
- Modify: `src/views/grooming_guide.rs` (Session wording)
- Test: `src/views/grooming.rs` tests, `src/views/grooming_guide.rs` tests

**Interfaces:**
- Consumes: `EpicQueue { stage: StageKey, sprint, next, ready, planning, new }`, `Stage::label`, `App::session_focus`, `App::session_ready_focus`, `grooming::sprint_names` (Task 1).
- Produces: `rail_line(epic, summary, counts: &RailCounts) -> String`, `pub struct RailCounts { sprint, next, ready, planning, new: usize }`, `guide::nothing_to_groom(status_new: &str) -> String` (replaces `nothing_in_planning`), new `guide::purpose` Session text, a `("J/K", …)` rubric line.

- [ ] **Step 1: Guidance tests**

In `src/views/grooming_guide.rs` tests, change the Session expectations:

```rust
        assert_eq!(
            purpose(GroomMode::Session, "New", "Planning", "Ready for Work"),
            "In the meeting. Walk the roadmap epics first, then tickets with no epic, ongoing epics and the rest; accept each Proposed or New ticket into the epic's Ready for Work queue in rank."
        );
        assert_eq!(
            rubric_lines(GroomMode::Session, "New", "Ready for Work"),
            vec![
                ("y", "Ready for Work: accept and place it in rank.".to_string()),
                ("b", "Bounce with a one-line comment on what's missing; Planning goes back to New.".to_string()),
                ("p", "Backlog: real work, lower priority.".to_string()),
                ("J/K", "Move a Ready for Work ticket down or up in rank.".to_string()),
                ("", "Three to five minutes per ticket. Past that, bounce.".to_string()),
            ]
        );
        assert_eq!(nothing_to_groom("New"), "Nothing Proposed or New here. Run a pre-check first, or pick another epic with ].");
```

Replace the `nothing_in_planning` test and function accordingly:

```rust
/// The session's empty state for an epic with no Proposed and no New ticket.
pub fn nothing_to_groom(status_new: &str) -> String {
    format!("Nothing Proposed or {status_new} here. Run a pre-check first, or pick another epic with ].")
}
```

and the `purpose` Session arm:

```rust
        GroomMode::Session => format!(
            "In the meeting. Walk the roadmap epics first, then tickets with no epic, ongoing epics and the rest; accept each Proposed or {status_new} ticket into the epic's {status_ready} queue in rank."
        ),
```

(`status_planning` is then unused in the Session arm; it is still used by PreCheck, so the signature stays.) The `b` rubric line becomes `("b", format!("Bounce with a one-line comment on what's missing; {status_planning} goes back to {status_new}."))`: `rubric_lines` must therefore take `status_planning` too. New signature: `pub fn rubric_lines(mode: GroomMode, status_new: &str, status_planning: &str, status_ready: &str) -> Vec<(&'static str, String)>`; update its one caller (`rubric_card` in `views/grooming.rs`) and its tests. Add `("J/K", format!("Move a {status_ready} ticket down or up in rank."))` before the closing line.

- [ ] **Step 2: View tests**

Replace `session_draws_rail_queue_ghost_and_bounce_box` and the session part of `both_modes_draw_headless` in `src/views/grooming.rs`:

```rust
    #[test]
    fn session_draws_stages_blocks_ghost_and_bounce_box() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        let issue = |k: &str, st: &str, epic: Option<&str>, sprint: Option<&str>| crate::jira::Issue {
            key: k.into(),
            summary: format!("Summary {k}"),
            labels: vec!["PLAT".into()],
            status: crate::jira::Status { name: st.into(), category: "new".into() },
            parent_key: epic.map(String::from),
            parent_summary: epic.map(|_| "Blue Green Ops".to_string()),
            sprint: sprint.map(String::from),
            ..Default::default()
        };
        app.sprints = vec![
            crate::jira::Sprint { id: 1, name: "Sprint 92".into(), state: "active".into() },
            crate::jira::Sprint { id: 2, name: "Sprint 93".into(), state: "future".into() },
        ];
        app.store.absorb(
            crate::store::ListId::Epics,
            vec![crate::jira::Issue { key: "CI-1860".into(), summary: "Blue Green Ops".into(), issue_type: "Epic".into(), labels: vec!["roadmap".into(), "P0".into()], ..Default::default() }],
        );
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![
                issue("CI-5", "Ready for Work", Some("CI-1860"), None),
                issue("CI-1", "Planning", Some("CI-1860"), None),
                issue("CI-3", "New", Some("CI-1860"), None),
                issue("CI-4", "Work in Progress", Some("CI-1860"), Some("Sprint 92")),
                issue("CI-6", "Ready for Work", Some("CI-1860"), Some("Sprint 93")),
                issue("CI-9", "New", None, None),
            ],
        );
        app.grooming.mode = GroomMode::Session;
        app.grooming.cursor = 1; // CI-1, the first candidate
        app.grooming.focused_since = Some(std::time::Instant::now());
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1400.0, 900.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let t = texts(&mut app);
        for want in ["Decide", "Ready for Work: accept and place it in rank.", "Move a Ready for Work ticket down or up in rank.", "Three to five minutes per ticket. Past that, bounce."] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        for want in [
            "Roadmap",
            "CI-1860 Blue Green Ops · Sp 1 · Nx 1 · R 1 · P 1 · N 1",
            "No epic",
            "(no epic) · R 0 · P 0 · N 1",
            "Sprint 92 · 1",
            "Sprint 93 · 1",
            "Ready for Work · 1",
            "Proposed · 1",
            "New · 1",
            "0:00",
            "Unassigned",
            "CI-5",
            "CI-4",
            "CI-6",
        ] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Ongoing" || x == "Other epics"), "stage headers only for stages with epics: {t:?}");
        app.grooming.placement = Some(0);
        let t = texts(&mut app);
        assert!(t.iter().any(|x| x == "Lands here · Enter to place"), "{t:?}");
        app.grooming.placement = None;
        app.grooming.bounce_text = Some(String::new());
        let t = texts(&mut app);
        assert!(t.iter().any(|x| x == "A one- or two-line comment is required"), "{t:?}");
        app.grooming.bounce_text = None;
        app.grooming.cursor = 0;
        let t = texts(&mut app);
        assert!(!t.iter().any(|x| x == "0:00"), "no timer on a Ready row: {t:?}");
    }

    #[test]
    fn session_wording() {
        let counts = |s, n, r, p, new| RailCounts { sprint: s, next: n, ready: r, planning: p, new };
        assert_eq!(rail_line(Some("CI-1860"), "Progressive Delivery", &counts(2, 1, 7, 3, 12)), "CI-1860 Progressive Delivery · Sp 2 · Nx 1 · R 7 · P 3 · N 12");
        assert_eq!(rail_line(Some("CI-1860"), "CI-1860", &counts(0, 0, 1, 0, 2)), "CI-1860 · R 1 · P 0 · N 2", "a key-only epic shows the key once; zero sprint counts are left out");
        assert_eq!(rail_line(None, "(no epic)", &counts(0, 3, 0, 1, 0)), "(no epic) · Nx 3 · R 0 · P 1 · N 0");
        assert_eq!(bounce_note("Which clusters?", "New"), ("⌘Enter posts the comment, then moves the ticket to New".to_string(), false));
        assert_eq!(bounce_note(" ", "New").0, "A one- or two-line comment is required");
        assert_eq!(bounce_note("a\nb\nc", "New"), ("Two lines at most".to_string(), true));
        let p = &theme::LIGHT;
        assert_eq!(timer_color(Level::Ok, p), p.text);
        assert_eq!(timer_color(Level::Amber, p), p.at_risk);
        assert_eq!(timer_color(Level::Red, p), p.off_track);
    }
```

In `both_modes_draw_headless`, the session assertions become:

```rust
        app.grooming.mode = GroomMode::Session;
        let session = texts(&mut app);
        assert!(session.iter().any(|t| t == "(no epic) · R 1 · P 0 · N 1"), "{session:?}");
        let session_purpose = guide::purpose(GroomMode::Session, "New", "Planning", "Ready for Work");
        for want in [session_purpose.as_str(), "No epic", "Ready for Work · 1", "Proposed · 0", "New · 1"] {
            assert!(session.iter().any(|t| t == want), "{want} in {session:?}");
        }
```

and, for the empty state, add a sibling test:

```rust
    #[test]
    fn an_epic_with_nothing_to_groom_says_so() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![crate::jira::Issue {
                key: "CI-2".into(),
                labels: vec!["PLAT".into()],
                status: crate::jira::Status { name: "Ready for Work".into(), category: "new".into() },
                parent_key: Some("CI-1".into()),
                parent_summary: Some("Metrics Retention".into()),
                ..Default::default()
            }],
        );
        app.grooming.mode = GroomMode::Session;
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let t: Vec<String> = crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect();
        assert!(t.iter().any(|x| x == "Nothing Proposed or New here. Run a pre-check first, or pick another epic with ]."), "{t:?}");
    }
```

Note the bounce-box "A one- or two-line comment is required" text only draws when the cursor is on a candidate; keep `cursor = 1` for that part.

- [ ] **Step 3: Run the view tests to see them fail**

Run: `ndc cargo test views::grooming 2>&1 | tail -20`
Expected: `RailCounts` missing; wording mismatches.

- [ ] **Step 4: Draw it**

In `src/views/grooming.rs`:

```rust
/// The rail row's counts: tickets in the active sprint, the next sprint, Ready for Work, Proposed and New.
pub struct RailCounts {
    pub sprint: usize,
    pub next: usize,
    pub ready: usize,
    pub planning: usize,
    pub new: usize,
}

/// One epic on the session rail, taken from `grooming::epic_queues`.
struct RailRow {
    epic: Option<String>,
    summary: String,
    stage: grooming::Stage,
    counts: RailCounts,
    has_candidates: bool,
}

/// "CI-1860 Progressive Delivery · Sp 2 · Nx 1 · R 7 · P 3 · N 12": the epic, then its counts. The sprint counts
/// (`Sp`, `Nx`) are left out when zero; R, P and N always show. A key-only epic shows the key once.
pub fn rail_line(epic: Option<&str>, summary: &str, c: &RailCounts) -> String {
    let name = match epic {
        Some(k) if k != summary => format!("{k} {summary}"),
        _ => summary.to_string(),
    };
    format!("{name}{}", rail_counts(c))
}

/// The rail row's tail: " · Sp 2 · Nx 1 · R 7 · P 3 · N 12".
fn rail_counts(c: &RailCounts) -> String {
    let mut s = String::new();
    if c.sprint > 0 {
        s.push_str(&format!(" · Sp {}", c.sprint));
    }
    if c.next > 0 {
        s.push_str(&format!(" · Nx {}", c.next));
    }
    s.push_str(&format!(" · R {} · P {} · N {}", c.ready, c.planning, c.new));
    s
}

/// The rail row's tooltip: the counts spelt out.
fn rail_tip(c: &RailCounts, active: Option<&str>, next: Option<&str>, status_ready: &str, status_new: &str) -> String {
    let mut parts = Vec::new();
    if let Some(a) = active {
        parts.push(format!("{} in {a}", c.sprint));
    }
    if let Some(n) = next {
        parts.push(format!("{} in {n}", c.next));
    }
    parts.push(format!("{} {status_ready}", c.ready));
    parts.push(format!("{} Proposed", c.planning));
    parts.push(format!("{} {status_new}", c.new));
    parts.join(" · ")
}
```

`session_body`:

```rust
/// What the centre column draws for the selected epic, cloned out of the store so the app can be borrowed mutably.
struct QueueView {
    active: Option<String>,
    next: Option<String>,
    sprint: Vec<jira::Issue>,
    next_sprint: Vec<jira::Issue>,
    ready: Vec<jira::Issue>,
    planning: Vec<jira::Issue>,
    new: Vec<jira::Issue>,
}

fn session_body(app: &mut App, ui: &mut Ui, p: &Palette) {
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    let (active, next) = grooming::sprint_names(&app.sprints);
    let (active, next) = (active.map(String::from), next.map(String::from));
    let queues = app.session_queues();
    let rail: Vec<RailRow> = queues
        .iter()
        .map(|q| RailRow {
            epic: q.epic.clone(),
            summary: q.summary.clone(),
            stage: q.stage.stage,
            counts: RailCounts { sprint: q.sprint.len(), next: q.next.len(), ready: q.ready.len(), planning: q.planning.len(), new: q.new.len() },
            has_candidates: !q.planning.is_empty() || !q.new.is_empty(),
        })
        .collect();
    let at = app.grooming.epic_index.min(rail.len().saturating_sub(1));
    let clone_all = |v: &[&jira::Issue]| v.iter().map(|i| (*i).clone()).collect::<Vec<_>>();
    let view = queues.get(at).map(|q| QueueView {
        active: active.clone(),
        next: next.clone(),
        sprint: clone_all(&q.sprint),
        next_sprint: clone_all(&q.next),
        ready: clone_all(&q.ready),
        planning: clone_all(&q.planning),
        new: clone_all(&q.new),
    });
    drop(queues);
    if rail.is_empty() {
        let g = &app.cfg.grooming;
        empty_state(ui, p, &format!("Nothing in {}, {} or {}", g.status_planning, g.status_ready, g.status_new));
        return;
    }
    let view = view.expect("a non-empty rail has a selected epic");
    let rows = view.ready.len() + view.planning.len() + view.new.len();
    let cursor = app.grooming.cursor.min(rows.saturating_sub(1));
    let focused: Option<jira::Issue> = view.ready.iter().chain(&view.planning).chain(&view.new).nth(cursor).cloned();
    let is_candidate = cursor >= view.ready.len();
    let list_w = (ui.available_width() * 0.36).clamp(LIST_MIN, LIST_MAX).min(ui.available_width());
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = PANE_GAP;
        let h = ui.available_height();
        ui.allocate_ui_with_layout(vec2(RAIL_W, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(RAIL_W);
            epic_rail(app, ui, p, &rail, at, active.as_deref(), next.as_deref());
        });
        ui.allocate_ui_with_layout(vec2(list_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(list_w);
            epic_queue(app, ui, p, &view, cursor);
        });
        let right_w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(right_w);
            match (focused, is_candidate) {
                (Some(issue), _) => focused_ticket(app, ui, p, issue),
                (None, _) => card(ui, p, |ui| {
                    ui.add(egui::Label::new(RichText::new(guide::nothing_to_groom(&app.cfg.grooming.status_new)).color(p.muted).size(14.0)).wrap());
                }),
            }
        });
    });
}
```

(The right pane shows the detail of a focused Ready row too; `focused_ticket` fetches detail on focus for any issue. The empty state draws when the epic has rows but no candidate and the cursor is clamped onto nothing, i.e. when `rows == 0` for the selected epic, which cannot happen since such epics are left out; so the empty state is reached when the epic has only Ready rows and the user has not moved onto them yet: handle this by showing it whenever `!is_candidate && view.planning.is_empty() && view.new.is_empty()` in place of the focused Ready row's detail. Implement that condition in the match: `(Some(_), false) if view.planning.is_empty() && view.new.is_empty() => card(... nothing_to_groom ...)` as the first arm.)

`epic_rail` gains `active: Option<&str>, next: Option<&str>` parameters, draws a stage header whenever the stage changes, and dims rows without candidates:

```rust
fn epic_rail(app: &mut App, ui: &mut Ui, p: &Palette, rail: &[RailRow], at: usize, active: Option<&str>, next: Option<&str>) {
    let mut clicked = None;
    let g = app.cfg.grooming.clone();
    egui::ScrollArea::vertical().id_salt("groom-rail").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        let mut last_stage = None;
        for (idx, r) in rail.iter().enumerate() {
            if last_stage != Some(r.stage) {
                if last_stage.is_some() {
                    ui.add_space(GROUP_GAP);
                }
                ui.label(RichText::new(r.stage.label()).font(theme::bold(12.0)).color(p.muted));
                ui.add_space(4.0);
                last_stage = Some(r.stage);
            }
            let sel = idx == at;
            let color = match &r.epic {
                _ if !r.has_candidates => p.muted,
                Some(k) => theme::epic_tint_in(k, &app.epic_slots, p, &app.cfg.issues.epic_colors).fg,
                None => p.text,
            };
            let full = rail_line(r.epic.as_deref(), &r.summary, &r.counts);
            let resp = row_frame(ui, p, "groom-rail", sel, idx, rail.len(), |ui| {
                let font = theme::body(13.0);
                let painter = ui.painter().clone();
                let measure = |t: &str| painter.layout_no_wrap(t.to_string(), font.clone(), color).size().x;
                let tail = rail_counts(&r.counts);
                let head = full.strip_suffix(&tail).unwrap_or(&full);
                let head = crate::views::truncate_title(head, ui.available_width() - measure(&tail), measure);
                ui.label(RichText::new(format!("{head}{tail}")).font(font.clone()).color(color));
            });
            let resp = crate::views::tooltip(resp, format!("{full}\n{}", rail_tip(&r.counts, active, next, &g.status_ready, &g.status_new)));
            if resp.clicked() {
                clicked = Some(idx);
            }
        }
    });
    if let Some(i) = clicked {
        app.actions.push(Action::GroomEpic { delta: i as i32 - at as i32, skip_empty: false });
    }
}
```

`row_frame`'s `idx`/`n` give the rounded corners of the first and last rows; with stage headers between groups the corners still follow the whole list, which is acceptable (criteria.md has no rule on it).

`epic_queue(app, ui, p, view: &QueueView, cursor)`: five blocks. Sprint blocks draw only when the board has that sprint (`view.active.is_some()` / `view.next.is_some()`), labelled with the sprint's name; their rows are the muted key + title rows the Ready queue uses today (factor that row into `fn context_row(ui, p, salt, i, idx, n)`), with "None" in small muted text when empty. Then the Ready block with `sel = cursor == at` on each row (so `row_frame(ui, p, "groom-ready", sel, …)`; titles in `p.text` when selected, `p.muted` otherwise) and the ghost logic unchanged, then `queue_label(ui, p, "Proposed", view.planning.len())` with the candidate rows (index offset `view.ready.len()`), then `queue_label(ui, p, &app.cfg.grooming.status_new, view.new.len())` with the New rows (offset `view.ready.len() + view.planning.len()`), the same row drawing as Proposed (badge if the report has one, age pill, assignee pill, timer on the selected row, bounce box under the selected row when open). Factor the candidate row into `fn candidate_row(app, ui, p, i, idx_in_list, n, sel, report, today, elapsed) -> Response` so Proposed and New share it. Clicking any row pushes `Action::GroomMove(target - cursor)` with `target` the row's index in the combined list; the sprint blocks are not clickable.

`rubric_card` passes `&app.cfg.grooming.status_planning` to the new `rubric_lines` signature.

- [ ] **Step 5: Full checks and a hands-off launch**

Run: `ndc cargo fmt && ndc cargo clippy --all-targets -- -D warnings && ndc cargo clippy --all-targets --features snapshot -- -D warnings && ndc cargo test 2>&1 | tail -5`
Expected: green.

Then the launch check from `CLAUDE.md` (15 s, `JIRA_TICKETS_READONLY=1`, no panic).

- [ ] **Step 6: Commit**

```bash
git add src/views/grooming.rs src/views/grooming_guide.rs
git commit -m "feat(grooming): stage rail with sprint context blocks and Proposed and New queues"
```

---

### Task 4: The report orders epics by stage

**Files:**
- Modify: `src/report.rs` (`EpicRef.note`, heading render and parse)
- Modify: `src/grooming.rs` (`promoted_epics`, `needs_detail_epics` take a `StageContext`)
- Modify: `src/app/groom.rs` (the two call sites pass `&self.stage_context()`)
- Modify: `tests/fixtures/report-golden.md` (the golden `render_matches_golden` compares against)
- Test: `src/report.rs`, `src/grooming.rs`

**Interfaces:**
- Consumes: `StageContext`, `stage_of`, `StageKey::note` (Task 1).
- Produces: `EpicRef { key, summary, numeric_id, status, note: Option<String> }`; `promoted_epics(promoted: &[&RunRow], siblings: &[Issue], ctx: &StageContext) -> Vec<PromotedEpic>`; `needs_detail_epics(rows: &[(&RunRow, Vec<Code>)], cap: usize, ctx: &StageContext) -> (Vec<NeedsDetailEpic>, usize)`.

- [ ] **Step 1: Failing tests**

`src/report.rs` tests:

```rust
    #[test]
    fn headings_carry_a_stage_note_that_is_not_a_status() {
        let text = "# PLAT pre-grooming — 2026-10-30\n\n## Planning — by epic\n\n### Epic [CI-1569](https://x/boards/101/backlog?customFilter=201&issueParent=1) · Cluster Upgrade · New · roadmap (Later, P0)\n\nPlanning candidates:\n\n- [CI-1](https://x/browse/CI-1) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### Epic [CI-79](https://x/boards/101/backlog?customFilter=201&issueParent=2) · Cluster Operations (ongoing) · ongoing\n\n- [CI-2](https://x/browse/CI-2) — b · rank 2 · created 2026-09-30 · missing: empty\n";
        let r = parse(text);
        let e = r.promoted[0].epic.as_ref().unwrap();
        assert_eq!(e.status.as_deref(), Some("New"));
        assert_eq!(e.note.as_deref(), Some("roadmap (Later, P0)"));
        let n = r.needs_detail[0].epic.as_ref().unwrap();
        assert_eq!(n.status, None, "a note is not a status");
        assert_eq!(n.note.as_deref(), Some("ongoing"));
    }
```

(Adjust the field paths to how `Report` exposes promoted and needs-detail rows; the existing `parses_title_date_and_epics` test shows them.)

Render: in `render_matches_golden`, give the golden's epics notes (`note: Some("roadmap (Now, P0)".into())` on the Planning epic, `note: Some("other".into())` on the Needs-detail one) and update the golden file's two headings to end in ` · roadmap (Now, P0)` and ` · other`.

`src/grooming.rs` tests: replace `report_epics_order_by_count_then_key_with_ready_siblings`, `needs_detail_epics_order_by_count_then_key` and `no_epic_is_last_in_planning_even_when_largest` with:

```rust
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
```

Update `run_row` in the tests to build `EpicRef { …, note: None }`, and the `needs_detail_is_capped_and_counted` test's call to pass `&c`.

- [ ] **Step 2: Run to see them fail**

Run: `ndc cargo test report 2>&1 | tail -20` and `ndc cargo test grooming::tests 2>&1 | tail -20`
Expected: `note` field missing; arity errors.

- [ ] **Step 3: Implement**

`src/report.rs`: add `pub note: Option<String>` to `EpicRef` (every literal in the crate gets `note: None`; `rg -n 'EpicRef \{' src` lists them). In `parse_epic_heading`, after computing `after`, split the trailing segments and classify:

```rust
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
```

with

```rust
/// A heading segment the writer adds for the stage: "roadmap (…)", "ongoing", "other".
fn is_stage_note(seg: &str) -> bool {
    seg.starts_with("roadmap") || seg == "ongoing" || seg == "other"
}
```

Keep the existing `(key, summary)` derivation. In `render`'s `epic_heading`, after the status append: `if let Some(n) = &e.note { h.push_str(&format!(" · {n}")); }` (for both sections).

`src/grooming.rs`: `promoted_epics(promoted, siblings, ctx)` and `needs_detail_epics(rows, cap, ctx)` compute `let sk = stage_of(key, r.epic.as_ref().map(|e| e.summary.as_str()), ctx);` when inserting a new epic, store `epic: r.epic.clone().map(|mut e| { e.note = sk.note(); e })`, keep `sk` beside the entry (a `Vec<(StageKey, PromotedEpic)>` while building), and sort by the `StageKey` instead of the count; then strip the keys. The "(no epic)" entry has `StageKey { stage: NoEpic, … }` so it lands second by itself; remove the explicit `epic.is_none()` sort term.

`src/app/groom.rs`: the two call sites pass `&self.stage_context()` (compute `let ctx = self.stage_context();` before any mutable borrow in those methods).

- [ ] **Step 4: Full checks**

Run: `ndc cargo fmt && ndc cargo clippy --all-targets -- -D warnings && ndc cargo clippy --all-targets --features snapshot -- -D warnings && ndc cargo test 2>&1 | tail -5`
Expected: green, including `render_matches_golden` against the updated golden.

- [ ] **Step 5: Commit**

```bash
git add src/report.rs src/grooming.rs src/app/groom.rs tests/fixtures
git commit -m "feat(grooming): order the report's epics by stage and note the stage in each heading"
```

---

### Task 5: Docs, deviations, captures

**Files:**
- Modify: `README.md` (Session and "How to groom" sections, lines 114-128)
- Modify: `docs/superpowers/specs/2026-10-07-jira-tickets-design.md` (add "### Deviations (plan 4)" after "### Deviations (plan 2)"; code map row for `src/grooming.rs`)
- Modify: `CLAUDE.md:5` ("Plans 1 …, 3 (Team, Roadmap) and 4 (Grooming stages) are done.")
- Create: `captures/plan4/session-light.png`, `captures/plan4/session-dark.png` (git-ignored)

- [ ] **Step 1: README**

Replace the "### Session" section with:

```markdown
### Session

Session mode walks epics in four stages, so the meeting spends its time on the roadmap first: roadmap epics in the Roadmap screen's order (Now, Next, Later; P0 first; then rank), then the team's tickets with no epic, then ongoing epics, then the rest. The left rail shows a header per stage and each epic's counts: `Sp` in the active sprint and `Nx` in the next (left out when zero), `R` Ready for Work, `P` Proposed (Planning) and `N` New. Epics with nothing Proposed and nothing New are dimmed; `]` and `[` skip them (with Shift they land on them too).

The centre shows the selected epic in five blocks: the active sprint's tickets and the next sprint's (context only), the Ready for Work queue in rank order, Proposed and New. `j`/`k` walk the Ready, Proposed and New rows as one list; entering an epic starts on its first Proposed or New ticket. For a Proposed or New ticket:

- `y` accepts it: `↑`/`↓` move a ghost row through the Ready queue and `Enter` commits the move to Ready for Work at that place, from Planning or straight from New. `Esc` cancels.
- `b` bounces it: type a one- or two-line comment and press `⌘Enter`. A Planning ticket goes back to New after the comment posts; a New ticket gets the comment only.
- `p` parks it in Backlog.

For a Ready for Work row, `J` and `K` move it one place down or up in rank with a single rank call (the row swaps at once and swaps back if the call fails).

A timer runs per candidate and turns amber at 3 minutes and red at 5. The Ready ceiling pill in the header reads "Ready for Work N · soft S · hard H", counts the team's (PLAT-labelled) Ready for Work tickets, and turns amber past the soft ceiling (10) and red past the hard one (15). Both limits are configurable under `[grooming]`, and warnings never block anything.
```

In "### How to groom", replace the Session paragraph with the new `guide::purpose` sentence followed by the rubric lines in prose: "Session: in the meeting. Walk the roadmap epics first, then tickets with no epic, ongoing epics and the rest; accept each Proposed or New ticket into the epic's Ready for Work queue in rank. Decide each ticket: `y` Ready for Work, accept and place it in rank; `b` bounce with a one-line comment on what's missing, Planning goes back to New; `p` Backlog, real work, lower priority. `J`/`K` move a Ready for Work ticket down or up in rank. Three to five minutes per ticket. Past that, bounce."

Also add to the Pre-check section, after the report paragraph: "The report lists epics in the session's stage order (roadmap first, then no epic, ongoing, other), and each epic heading carries its stage, for example `· roadmap (Now, P0)`."

- [ ] **Step 2: Spec deviations and code map**

After "### Deviations (plan 2)" add:

```markdown
### Deviations (plan 4)

- Sprint blocks take precedence: a ticket in the active or next sprint appears only there, whatever its status, so a `New` ticket someone already pulled into a sprint is not a session candidate.
- An epic's stage is known once the Epics list has landed; until then it is "Other epics" and the rail reorders when the list arrives. An epic absent from the list is staged by the ticket's parent summary (ongoing, else other) and sorts after the listed ones.
- Rail counts leave out `Sp` and `Nx` when zero; `R`, `P`, `N` always show.
- The pre-check summary dialog keeps its rank-ordered list; only the written report is staged.
- `J`/`K` reuse the Issues view's optimistic swap: the Ready rows swap at once and swap back when the rank call fails.
- The focused Ready row's detail shows in the right pane; the "nothing to groom" card shows only for an epic with no Proposed and no New ticket.
```

In the code map table, change the `src/grooming.rs` row (or add it if absent) to: `| `src/grooming.rs` | Pure grooming model: pre-check verdicts and promotion, session stages (`Stage`, `StageKey`, `epic_queues`), report grouping |`.

- [ ] **Step 3: CLAUDE.md**

Line 5: `Plans 1 (foundation + Issues view), 2 (Grooming), 3 (Team, Roadmap) and 4 (Grooming session by stage) are done.`

- [ ] **Step 4: Tests still green, then captures**

Run: `ndc cargo test 2>&1 | tail -3` (the README keys table is covered by `readme_matches_bindings`; prose changes do not affect it).

Build and capture both themes, headless, never bringing the window forward:

```bash
nix develop --command cargo build --release --features snapshot
mkdir -p captures/plan4
for theme in light dark; do
  JIRA_API_TOKEN="$(op read --account my.1password.com "op://Private/jira-cli JIRA API Credentials/credential")" \
  JIRA_TICKETS_READONLY=1 JIRA_TICKETS_THEME=$theme \
  JIRA_TICKETS_SCRIPT="key:cmd+2;wait:4000;key:tab;wait:2500;shot:$PWD/captures/plan4/session-$theme.png" \
  target/release/jira-tickets
done
```

Never print or save the token. If `op read` fails with "authorization timeout", report BLOCKED (the user is away) rather than retrying in a loop. Look at both PNGs: stage headers present, five blocks, no overlap or clipping in the 300 pt rail; fix anything wrong before committing.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/superpowers/specs/2026-10-07-jira-tickets-design.md CLAUDE.md
git commit -m "docs: describe the staged grooming session and plan 4 deviations"
```

---

## Hand-off

After the final whole-branch review and its fix wave: `scripts/bundle-macos.sh ~/Applications`, relaunch the app, then `superpowers:finishing-a-development-branch`. The branch `feat/grooming-stages` is based on `feat/team-and-roadmap` (plan 3, unmerged at the time of writing); both merge to `main` together or in order.
