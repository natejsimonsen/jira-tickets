# Jira Tickets: design

Status: approved in brainstorm, 2026-10-07. Next step: implementation plan.

## Purpose

A fast native desktop app for Jira, built the way `github-rs` is built for
GitHub pull requests. It replaces the `jira-cli` terminal UI, turns the
Platform grooming runbook into a screen, shows what each team member
is working on, and renders the roadmap layouts from the 2026-10-06 mockups
from Jira epics.

Reference material:

- `github-rs` (adalovelace): structure, conventions, look.
- `jira-cli` (Go, Bubble Tea): the Jira behaviour to port.
- `the team's grooming runbook`: the
  grooming runbook this app implements.
- `~/.claude/plugins/cache/example/parent-team/*/commands/team-pre-check.md`:
  the Claude-driven pre-check whose report format this app reads and writes.
- `~/Documents/designs/2026-10-06-roadmap-layout-mockups.html`: the four
  roadmap views.

## Decisions made in brainstorm

| Question | Decision |
|---|---|
| Form factor | Rust + egui desktop app, a sibling of `github-rs`. |
| Structure | Copy `github-rs`'s layout and conventions; back every view with one cached issue store so Team and Roadmap are projections, not separate fetch paths. |
| Roadmap data | Jira epics only. No Notion. |
| Readiness assessment | Human triage with one-key verdicts. If the Claude pre-check already ran, its report is parsed and its verdicts shown and pre-filled. |
| Team roster | A list in the config file, seeded from the developer portal `platform-team` members. |

## 1. Scope and naming

- Repo `jira-rs` at `/Volumes/example/non-zip/jira-rs`.
- Binary and app name `jira-tickets` ("Jira Tickets").
- Rust 2024 edition. Dependencies: eframe/egui 0.36 (wgpu, accesskit,
  persistence), egui_extras, ureq 3 (json, platform-verifier), serde,
  serde_json, chrono, dirs, toml, webbrowser (macOS). No
  tree-sitter, no diff or image code.
- One window, four top-level views switched by `⌘1`–`⌘4`: Issues, Grooming,
  Team, Roadmap.
- Light and dark themes following the system, using the mockup's colour
  tokens (`--on-track`, `--at-risk`, `--off-track`, `--no-health`, lane
  tints, `--today`).

## 2. Config

TOML at `~/.config/jira-tickets/config.toml`. Generated with comments on
first run, then the app exits with instructions, as `jira-cli` does.

```toml
site = "https://example.atlassian.net"
email = "you@example.com"
# Optional. Accepts a plain token, "op://vault/item/field", or "$(command)".
# Resolved on every launch, before the saved token file; never written to disk.
api_token = "op://Private/Jira API Token/credential"

[scope]
projects = ["CI", "OPS"]      # projects searched for team members' work
project = "CI"                 # project the team label lives in
team_label = "PLAT"             # the user's own sub-team (grooming scope, default filter)
board_id = 101                # board whose sprints drive the sprint scope
filter_id = 201
parent_board_id = 100         # parent board
[[scope.teams]]                # sub-teams whose tickets the Issues view loads
label = "PLAT"
name = "Platform"
board_id = 101
[[scope.teams]]
label = "TOOLS"
name = "Developer Tools"
board_id = 102
# members = ["<account id>", ...]  # team members; a ticket assigned to one counts as team work even without the label

[grooming]
status_new = "New"
status_planning = "Planning"
status_ready = "Ready for Work"
status_backlog = "Backlog"
planning_transition_id = "11"
planning_cap = 10
needs_detail_cap = 10
ready_soft_ceiling = 10        # amber past this
ready_hard_ceiling = 15        # red past this
report_dir = "~/Documents/grooming-reports"

[team]
members = ["barbara", "edsger.dijkstra", "ada.lovelace", "grace", "sam", "alan"]
# Jira display names or account ids. Order is column order. The generated
# file seeds SSO usernames from the team roster; replace each with the
# name Jira shows, or the Team view marks it "no tickets found".

[issues]
columns = ["status", "labels", "parent.summary", "assignee"]
[[issues.filters]]
name = "Only Opened"
jql = "status not in (Closed)"
[[issues.filters]]
name = "To Do"
jql = 'statusCategory = "To Do"'
# Optional colour overrides ("#rrggbb"); status names match case-insensitively.
# [issues.status_colors]
# "Ready for Work" = "#0f9d9a"
# [issues.epic_colors]
# "CI-1860" = "#8250df"

[roadmap]
# epics = ["CI-1860", "CI-1569"]   # optional allowlist; default is every epic a team-label ticket points at
lanes = { PLAT = "Platform" }  # label -> lane name
horizon_by_status = { "Work in Progress" = "now", "In Progress" = "now", "Ready for Work" = "next", "Planning" = "next", "New" = "later", "Backlog" = "later" }
initiative_label = "roadmap"          # epics with this label are initiatives; else all epics not named "(ongoing)"
# Quarters come from labels target:YYYY-Qn / start:YYYY-Qn by default.
# health_field = "customfield_XXXXX"
# target_field = "customfield_XXXXX"   # optional: a date or YYYY-Qn field instead of the label
# start_field = "customfield_XXXXX"
# confidence_field = "customfield_XXXXX"
# next_milestone_field = "customfield_XXXXX"
# definition_of_done_field = "customfield_XXXXX"
```

## 3. Auth

Token lookup order:

1. `JIRA_API_TOKEN` environment variable (with `JIRA_EMAIL` overriding the
   config email if set).
2. `api_token` in config. `op://` runs `op read`; `$(…)` runs `sh -c`.
   These are the only subprocesses the app ever starts; when `op` is not on
   the app's PATH (a Finder launch), it is retried through the user's login
   shell (`$SHELL -lc`). The value is resolved on every launch and is never
   written to disk: 1Password stays the only store for that secret. A failed
   resolution is shown on the sign-in panel.
3. Saved token file `~/Library/Application Support/jira-tickets/token`
   (`~/.config/jira-tickets/token` on Linux), mode 0600. Only a pasted
   token is ever saved here.
4. A paste prompt in the window.

A token pasted in step 4 is saved to the file. Every request uses HTTP
basic auth with email and token. A 401 shows the `jira-cli` diagnostic
(site, email, masked token, what to check) in a panel with a "sign out"
action that deletes the file.

## 4. Data layer

`src/jira.rs`: a blocking client over Jira Cloud REST v3 and Agile 1.0,
called only from background threads that post `Msg` values to the UI
thread via a channel, exactly as `github-rs` does. Each request logs its
duration when `JIRA_TICKETS_PERF=1`.

Reads:

- `search`: POST `/rest/api/3/search/jql` with JQL, field list, and
  `nextPageToken` paging until exhausted.
- `issue`: GET `/rest/api/3/issue/{key}` with description, comments,
  `statuscategorychangedate`, and configured custom fields.
- `transitions`, `resolutions`, `assignable users` for a project,
  `board sprints`, `board configuration` and `filter` (for the board's JQL).

Writes (ported from `jira-cli`):

- `transition` with optional resolution; on a "resolution" error, fetch
  resolutions and retry via a picker.
- `rank before` / `rank after`: PUT `/rest/agile/1.0/issue/rank`.
- `assign` (account id, or `null` to unassign).
- `move to sprint` / `move to backlog`.
- `update fields`: labels, priority, story points.
- `add comment` (ADF body built from plain text paragraphs).

`src/adf.rs` renders Atlassian Document Format JSON to egui rich text:
paragraph, heading, bulletList, orderedList, listItem, codeBlock, blockquote,
rule, panel, table (as plain rows), hardBreak, text with marks (strong, em,
code, link, strike), mention, emoji (shortname text), inlineCard (URL).
Unknown nodes fall back to their concatenated text. A `to_plain_text`
function feeds the pre-check verdict screen and the report.

`JIRA_TICKETS_READONLY=1` makes every write return an error before any
request is sent.

## 5. Issue store

`src/store.rs`: `Store { issues: HashMap<String, Issue>, lists: HashMap<ListId, Vec<String>>, fetched_at }`.

Three queries populate it on launch and `⌘R`:

| List | JQL | Fields |
|---|---|---|
| `team_open` | `project = {project} AND (labels in ({team labels}) OR assignee in ({all teams' members})) AND statusCategory != Done ORDER BY Rank ASC` | summary, status, assignee, reporter, parent, labels, priority, issuetype, created, updated, duedate, rank (customfield_10019), statuscategorychangedate, sprint, story points |
| `epics` | `project = {project} AND issuetype = Epic AND (labels in ({lane labels}) OR labels = {initiative_label} OR key in ({parents of team_open}))` plus the optional allowlist | summary, status, assignee, labels, priority, duedate, created, updated, description, comments (last 5), configured roadmap custom fields |
| `team_in_progress` | `project in ({projects}) AND assignee in ({team}) AND statusCategory = "In Progress" ORDER BY updated DESC` | same as `team_open` |

Views are projections:

- Issues: `team_open` filtered by epic, quick filter, Mine, and search JQL
  (search runs its own query and stores a `search` list).
- Grooming: `team_open` partitioned by status into New, Planning, Ready for
  Work, grouped by parent.
- Team: `team_in_progress` ∪ `team_open` grouped by assignee.
- Roadmap: `epics` joined to `team_open` children for progress counts.

Detail (full description, all comments) is fetched per issue when opened and
merged into the same record. The store serialises to JSON under
`~/Library/Caches/jira-tickets/` so launch shows the last state in under
0.1 s and refreshes quietly. Every mutation refetches the affected issue
and then refreshes the team lists, since a transition can move an issue
out of them. Writes run on one ordered worker thread.

## 6. Issues view

Layout from `github-rs`: collapsible list panel on the left (`⌘B`), the
selected issue on the right.

List rows: key, summary, configured columns. Status pills are coloured by
status name (New grey, Planning blue, Ready for Work teal, In Progress
accent, In Review purple, Blocked red, Done green; unmapped names fall
back to the status category). Status is the only filled pill. Each epic
gets a hue from a 10-colour perceptual palette (OKLCH hues 36° apart, every pair
≥ ΔE2000 12 in each theme) in order of first appearance:
the loaded epic list in Jira rank order, then any other parent seen on
screen, keyed by epic key and kept for the session, so one epic has one hue
in every list and two epics with the same name never share one by name. The
epic pill is outline-only in its hue (1 px stroke in the hue's stripe
colour, text in the hue's text colour, no fill), so status and epic differ in
shape, not just hue; the detail view's parent line uses the hue's text
colour, and each row has a 3 px left edge in the stripe colour. Hovering an
epic pill shows "KEY · name". Labels stay neutral. Both maps can be overridden in config under
`[issues.status_colors]` and `[issues.epic_colors]` (`"#rrggbb"`). Filters above the list: a team picker (All teams, each
configured the parent team sub-team label such as PLAT or TOOLS, or "No team
label"; defaults to the user's own team), an epic picker, a sprint scope (All, the
board's active sprint, each future sprint, or Backlog; defaults to the
active sprint, as jira-cli did), quick filters from config, a "Mine"
toggle, a "Roadmap" toggle (`m`) that keeps only tickets whose parent epic
carries `initiative_label`, and a search box (`/` or `⌘F`).

The search box works three ways. Plain text filters the rows on screen
locally at once (key, summary, labels, assignee; substring), and the
count reads "3 of 20 issues". Once the text is at least 2 characters and
has not changed for 350 ms, it also runs a server search the way Jira's
board text box does: the current scope's query (the picked team's open
work, or the quick filter's or No team label's query), narrowed by the
sprint scope and epic, `AND text ~ "<term>*"` (summary, description,
comments, environment; `"` and `\` escaped). It goes through the same
sequenced search path as every other search, so a newer term or scope drops
an older reply, and the previous rows stay on screen with a muted count and
"⟳ Updating…" while it runs. When it lands the rows are the server's set in
Rank order (Mine and Roadmap still apply locally) and the count reads
"N issues · text match". Changing team, quick filter, sprint or epic reruns it in
the new scope. Clearing the box (or Esc) returns to the scope's rows; the
team list needs no refetch, a quick filter's search is rerun. Text with a
JQL operator (`=`, ` in `, `~`, `!`) is never text-searched. Enter runs
whatever the box holds, plain text included, as a JQL search (the hint bar
reads "Enter run as JQL" over plain text) and the count reads "· JQL".
Epics never appear in the Issues list: they are containers, not action
items, and stay in the store for the epic picker, grooming, and the
roadmap. `⌘K` jumps to a key or pasted browse link. A ticket assigned to a team member counts as that
team's work even without the team label; such rows carry an amber "no
PLAT label" pill and `l` applies the label (a write, never automatic,
sent as an atomic Jira `labels add` so concurrent edits are kept). "No
team label" runs a server query (`labels is EMPTY OR labels not in (...)`
and the same for assignees) because the base list never holds such
tickets. If a configured member fails to resolve, the open-issues query
is retried without the assignee clause and a toast says so. The header's
refresh control is a painted sync icon that becomes a spinner while
loading. `?` opens a keyboard help overlay on every screen, generated from
one binding table that also produces the README's keyboard table (a test
keeps them equal); the header shows a "? help" pill and the Issues screen
a bottom hint bar listing the keys that apply right now. The sprint list comes from board 101's active and
future sprints, refreshed with the lists; filtering is local on each
issue's sprint, which is its last `active` or `future` sprint. Backlog means
what Jira's board backlog means: not in an active or future sprint, so an
issue whose sprints are all closed (carried out of Sprint 88–90, say) is in the
backlog. Where the scope goes to the server (the text search below) it is
`(sprint is EMPTY OR sprint not in (openSprints(), futureSprints()))`; the
`is EMPTY` is needed because `not in` alone never matches an issue with no
sprint.

Keys: `j`/`k` move, `Enter` opens detail, `t` transition, `a` assign, `s`
sprint move, `e` edit labels/priority/points, `J`/`K` rank the selected
issue past its neighbour, `c` copies the browse link, `⌘O` opens in the
browser, `⌘R` refresh, `Esc` back.

Detail: summary, status bar, rendered description, a fields column
(assignee, reporter, priority, labels, epic, sprint, created, updated,
story points), comments rendered from ADF, and a comment box with `⌘Enter`
to post. In read-only mode (`JIRA_TICKETS_READONLY=1`) there is no box and no
button: one muted line, "Read-only, comments can't be posted", sits under the
Comments label, and the hint bar shows no composer keys. Snapshot builds alone
honour `JIRA_TICKETS_SNAPSHOT_COMPOSER=1`, which draws the writable composer
for captures while every write stays blocked.

Every mutation runs on a background thread, shows a toast, and refetches
that issue.

## 7. Grooming view

The runbook as a screen, in two modes.

### Pre-check mode

Walks the `New` column in rank order. Each ticket shows title, rendered
description, epic, age tag (`new (Nd)` when created within 14 days, else
`older`), and rank position.

Verdict keys: `r` ready, `w` unclear-what, `n` unclear-when, `e` empty, `u`
undo, `j`/`k` move without judging. A ticket may carry several failure
codes. The summary screen lists the ready set in rank order, marks those
beyond `planning_cap` as held back, and offers one "Promote" button that
performs the `New → Planning` transitions with no fields, no resolution, no
comment, and nothing else. Transitions that fail are collected and shown;
the rest continue.

After promotion the app fetches existing `Ready for Work` siblings for the
promoted epics and writes `report_dir/YYYY-MM-DD.md` in exactly the format
the Claude-driven command produces: header with JQL and board links,
"Planning — by epic" with the board URL per epic using the numeric parent
id, "Needs detail (still in `New`)" capped at `needs_detail_cap`, "Failed
transitions", and the run summary line. Epic order: newly promoted count
descending, then key ascending. If a report for today exists, the app asks
before overwriting.

### Report overlay

At launch and on refresh the app reads the newest `report_dir/*.md`. The
parser extracts, from "Planning — by epic", each candidate key marked as
promoted, and from "Needs detail", each key with its `missing:` codes. The
Grooming view overlays badges: `New` tickets show "needs detail:
unclear-when (report 2026-10-07)"; `Planning` tickets show "promoted by
pre-check". Unmentioned tickets show no badge. Pre-check mode starts with
those verdicts pre-filled so only unlabelled or disputed tickets need
attention. The report format is the contract; the command is not modified.

### Session mode

Epic-by-epic walk. Left rail: epics with Planning / Ready for Work / New
counts; epics with nothing in Planning are dimmed and skipped by `]` and
`[`. Centre: the selected epic's `Ready for Work` queue in rank order with
its `Planning` candidates beneath, each with the report badge if any.

For the focused candidate:

- `y` accept: enters placement, where `↑`/`↓` move a ghost row through the
  Ready queue and `Enter` commits one transition to `Ready for Work` plus
  one rank-before or rank-after call. `Esc` cancels.
- `b` bounce: an inline box requires a one- or two-line comment; `⌘Enter`
  posts the comment then transitions to `New`.
- `p` park: transitions to `Backlog`.

A per-ticket timer starts on focus, amber at 3 minutes, red at 5. The header
shows total `Ready for Work` against the ceilings, amber past the soft
ceiling and red past the hard one. Warnings never block.

### Guidance

Static text in the grooming runbook's words, from `views::grooming_guide`:

- Purpose line: one `muted` line under the mode chips saying what the mode is for and when it happens.
- Rubric card: over the focused ticket, the readiness bar (pre-check) or the three decisions (session), keys drawn as in the hint bar.
- All-judged cue: on the summary row, "N of M judged" (or "All M judged") and that Enter opens the summary.
- Promote explainer: at the top of the Promote dialog, what Promote moves, the cap when it holds tickets back, the report path, and "Paste the report into the team channel".
- Empty Planning: an epic with nothing in Planning says to run a pre-check first or pick another epic with `]`.

### Deviations (plan 2)

- Prefill from a report happens once per report file, not on every refresh.
- The session rail omits epics with 0 Planning, 0 Ready and 0 New.
- Accept into an empty Ready queue makes one transition and no rank call.
- A bounce whose transition fails keeps the posted comment and closes the box with a toast.

## 8. Team view

One column per member in config order, plus "Unassigned" last. Each column
shows the member's In Progress tickets first, then their assigned `Ready for
Work` tickets, as cards with key, summary, epic chip, status, and days in
the current status (from `statuscategorychangedate`). A collapsed "done in
the last 7 days" footer per column, populated by a small per-member query
run on demand when expanded. The Unassigned column shows the top of the
`Ready for Work` queue. A member with nothing in progress shows a "nothing
in progress" marker. Clicking a card opens the issue in the Issues view.

Members are matched by display name or account id; a configured member
that matches no assignee in any list shows "no tickets found" so a typo is
visible.

## 9. Roadmap view

Four tabs from the mockup, over `epics` joined to their children. Which
epics count as initiatives: those carrying `initiative_label` (default
`roadmap`); when no epic carries it, every epic whose summary does not
contain "(ongoing)". Ongoing epics are listed in a KTLO footnote under the
Now column, as in the mockup. Each initiative shows a progress bar of
children done / in progress / to do. Empty fields render as red
"missing" / "not set" flags. Roadmap time is quarters, not dates: an
initiative's target and start come from labels `target:YYYY-Qn` and
`start:YYYY-Qn` (for example `target:2026-Q4`). Epics with no target
quarter go to the Unscheduled tray.

Field mapping:

| Concept | Default source | Override |
|---|---|---|
| Horizon | `horizon_by_status` (the live team's in-progress status is literally `Work in Progress`); Done status category → Done | Label `horizon:now`, `horizon:next`, `horizon:later` |
| Health | Labels `health:on-track`, `health:at-risk`, `health:off-track`; absent → no health | `health_field` |
| Priority chip | Priority Highest → P0, High → P1 | Labels `P0`, `P1` |
| Target quarter | Label `target:YYYY-Qn` | `target_field` (a date or `YYYY-Qn` custom field; a date maps to its quarter) |
| Start quarter | Label `start:YYYY-Qn`; unset: bar spans the target quarter only | `start_field` |
| Initiative set | Label `initiative_label` (default `roadmap`); fallback: summary without "(ongoing)" | `initiative_label` |
| Lane | `lanes` label → name; epics with no lane label go to a "No lane" lane | Add entries |
| Latest update | Newest epic comment: author, date, first paragraph | None |
| Next milestone, confidence, definition of done | Missing | Custom field ids |
| Metrics | Labels prefixed `metric:` | None |

Tabs:

1. Now / Next / Later: three columns of cards sorted P0 first, then worst
   health; health dot only in Now. A "Shipped in {year}" list of Done epics
   with their last quarter.
2. Timeline: lane rows by configured label, quarter columns for the current
   year, bars from start quarter to target quarter (quarters from the
   `start:`/`target:` labels), a today line, done bars muted, P0 bars with
   a red left edge, and the Unscheduled tray.
3. Status review: health summary tiles (counts per health, owners set / total)
   and a table of Now epics sorted worst health first: initiative with lane,
   key, owner; health; confidence; latest update; next milestone; definition
   of done; progress.
4. Outcomes: groups by `metric:` label; each card lists active and later
   initiatives and shows "Nothing in progress moves this metric" when
   applicable. With no metric labels anywhere, the tab says so.

### Deviations (plan 3)

- Timeline lanes are alphabetical by lane name, with "No lane" last.
- An initiative is on the timeline when its start..=target quarter range overlaps the year; its bar is clipped to the year's columns.
- Done bars show "✓" instead of a health dot.
- The Shipped list also includes Done initiatives with no target, flagged "target not set".
- Progress counts every child of an initiative, Done ones included, via the `EpicChildren` list.
- The KTLO footnote lists only non-initiative epics whose summary contains "(ongoing)".

## 10. Error handling

- Network failure: cached data stays on screen under an "offline, data from
  HH:MM" banner; refresh retries.
- 401: auth panel with diagnostic.
- 429: one retry after `Retry-After`; then a toast.
- Failed mutation: Jira's error text in a toast, store untouched, issue
  refetched. Failed rank snaps the ghost row back.
- Report write failure: toast with path and error; transitions already made
  are not rolled back, and the summary stays on screen for copy.

## 11. Testing

Unit tests with fixtures, no live Jira:

- JQL builders for the three store lists and the search filters.
- ADF renderer against captured JSON from real tickets, checking plain-text
  output and the sequence of styled spans.
- Config parsing, defaults, and the generated example round-trip.
- Horizon, health, priority, and quarter derivations.
- Report writer against a golden file in today's format.
- Report parser against a real report copied from `~/Documents/grooming-reports`.
- Jira client against recorded JSON responses served by a local stub.

An optional `snapshot` cargo feature with egui_kittest renders each view
offscreen to PNG for UI review (`JIRA_TICKETS_SCREENSHOT=out.png`,
`JIRA_TICKETS_VIEW=issues|grooming|team|roadmap`, `JIRA_TICKETS_THEME`).
Snapshot runs block every write.

## 12. Build and distribution

- Cargo, with `github-rs`'s dev profile: `opt-level = 1` for the crate,
  `opt-level = 3` for dependencies; release with thin LTO and strip.
- `scripts/bundle-macos.sh` builds `Jira Tickets.app` into `/Applications`.
- `flake.nix` and `nix/` for Nix and Flox installs.
- MIT licence. Conventional commits.
- Out of scope for v1: CI workflows, Homebrew tap, Windows and Linux
  packaging, modifying the `team-pre-check` command, Notion integration.

## Code map

| File | Role |
|---|---|
| `src/main.rs` | Opens the window |
| `src/app.rs` | App state, `Msg` enum, background loading, keyboard |
| `src/config.rs` | TOML config, defaults, example generation, secret resolution |
| `src/auth.rs` | Token lookup and saving |
| `src/jira.rs` | REST v3 and Agile 1.0 client, data types |
| `src/adf.rs` | ADF to egui rich text and plain text |
| `src/store.rs` | Issue store, list ids, projections, disk cache |
| `src/report.rs` | Pre-check report writer and parser |
| `src/roadmap.rs` | Epic → initiative derivation (horizon, health, quarters, metrics) |
| `src/theme.rs` | Colours (light and dark) and fonts |
| `src/views/mod.rs` | View switcher, panels, toasts |
| `src/views/issues.rs` | List and detail |
| `src/views/grooming.rs` | Pre-check and session modes |
| `src/views/team.rs` | Member columns |
| `src/views/roadmap.rs` | Four roadmap tabs |
| `src/snapshot.rs` | Offscreen screenshots (feature `snapshot`) |
