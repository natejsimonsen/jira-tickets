# Jira Tickets

A fast native desktop app for working through Jira tickets, built with Rust and egui. It keeps a local cache so it opens instantly, searches and filters the team's issues from the keyboard, and shows each issue's description and comments with Jira's rich text rendered. Grooming, Team, and Roadmap screens are placeholders for now; plans 2 and 3 fill them in.

## Install from source

This project is built through its nix dev shell, so run cargo via `nix develop`.

    nix develop --command cargo run --release

To build a macOS app bundle and install it (default `/Applications`, or pass a folder):

    scripts/bundle-macos.sh
    scripts/bundle-macos.sh ~/Applications

The bundle is ad-hoc signed and has no icon yet.

## Config

The config lives at `~/.config/jira-tickets/config.toml` (override with `JIRA_TICKETS_CONFIG`). On first run the app writes an example there and exits; edit it, then launch again. Set `site` and `email`, and adjust the scope (projects, team label, board or filter), team members, issue columns, and quick filters.

The Issues view loads every the parent team sub-team listed under `[[scope.teams]]` (each with a `label`, `name`, and `board_id`; the defaults are PLAT · Platform and TOOLS · Developer Tools), plus `parent_board_id` for the parent board. `team_label` stays your own team and `board_id` the sprint board. A ticket counts as a team's when it carries the team's label or is assigned to one of the team's `members` (Jira display names or account ids, matched case-insensitively). Your own team's `members` default to `[team].members`; other teams have none unless you list them.

Status pills are coloured by status name (falling back to the status category) and each epic gets a stable colour derived from its key, shown on its pills and as the left edge of each list row. Override either with `[issues.status_colors]` (status name, case-insensitive) or `[issues.epic_colors]` (epic key), mapping to `"#rrggbb"`.

Roadmap settings name custom fields by id, such as `customfield_12345`. To find them, open `<site>/rest/api/3/field` in a browser while signed in to Jira and look for the field names.

## Signing in

The app looks for an API token in this order:

1. The `JIRA_API_TOKEN` environment variable. `JIRA_EMAIL` overrides the config's email.
2. `api_token` in the config. It can be a plain value, `op://vault/item/field` (the app runs `op read`), or `$(command)` (the app runs the command). It is resolved on every launch and never written to disk. If `op` knows more than one 1Password account (it fails with "multiple accounts found"), set `op_account` in the config to the account's sign-in address or email.
3. A token saved earlier from the paste prompt, at `~/Library/Application Support/jira-tickets/token` on macOS or `~/.config/jira-tickets/token` on Linux (mode 0600).
4. The paste prompt, where you paste an Atlassian API token.

Only a token you paste is saved. Signing out deletes that file.

## Keyboard

<!-- keys:start -->
| Keys | Action |
| --- | --- |
| **Global** | |
| `⌘1` to `⌘4` | Issues, Grooming, Team, Roadmap |
| `⌘R` | Refresh |
| `⌘W` | Close the window |
| `?` or `Esc` | Toggle this help (Esc closes) |
| **Issues** | |
| `⌘K` | Go to an issue by key, number, or link |
| `⌘F` or `/` | Focus search; Esc clears, then exits |
| `j/k` or `↑/↓` | Next or previous issue |
| `t` | Transition |
| `a` | Assign |
| `s` | Sprint |
| `e` | Edit labels, priority, story points |
| `l` | Add the picked team's label |
| `m` | Toggle Roadmap filter |
| `J/K` | Move down or up in rank |
| `c` | Copy the issue link |
| `⌘O` | Open in Jira |
| `⌘Enter` | Post the comment; Esc keeps the draft |
| **Pickers** | |
| `↑/↓` | Move |
| `j/k` | Move, except in Assign |
| `Tab/⇧Tab` | Next or previous field in Edit |
| `Enter` | Confirm |
| `Esc` | Cancel |
<!-- keys:end -->

Only `⌘1`–`⌘4`, `⌘R`, `⌘W`, and `?` work on every screen. The rest are bound on the Issues screen only. Plain keys (letters, `/`, `?`) do nothing while a text field has focus. While a picker is open, only `Esc`, the arrow keys (and `j` / `k`, except in the Assign picker), and `Enter` apply (in the Edit dialog, `Tab` and `⇧Tab` move between fields and `Enter` confirms); `?` also works while a picker without a text field is open. Selecting a row shows its detail. `?` (or the "?" button in the header) opens this list in the app, and a bar at the bottom of every screen shows the keys that apply right now. While the help is open only the global keys apply; `?` or `Esc` closes it.

Above the Issues list are a team picker, an epic picker, a sprint picker, the config's quick filters, a Mine toggle, a Roadmap toggle, and the search box. The Roadmap toggle (`m`) shows only tickets under epics labelled `roadmap`; epics themselves are containers and never appear in the list. The sprint picker offers All sprints, the board's active and future sprints, and Backlog (issues in no active or future sprint, including ones only ever in closed sprints, as on the Jira board). It starts on the active sprint, or All sprints if the board has none.

The search box filters the rows on screen as you type. Pause on two or more characters and it also asks Jira, like the board's text box: `text ~ "<term>*"` within the current team, quick filter, sprint, and epic, which matches descriptions and comments too; the count then reads "N issues · text match". Press Enter to run whatever the box holds as JQL instead (the count then reads "· JQL"); JQL (anything with `=`, ` in `, `~`, or `!`) is never text-searched. Esc clears the box.

The team picker offers All teams, each configured team, and No team label (tickets that match no team by label or membership). It starts on your own team. No team label asks Jira for those tickets directly (no team label, and not assigned to any team's members), so it also finds tickets outside the loaded team lists; picking a team again goes back to the team list. Under All teams each row shows its team label; the epic picker lists only the epics of the picked team's tickets. When a ticket is in the picked team only through its assignee, its row and detail show an amber "no PLAT label" pill (named for the picked team), and `l` adds that label, keeping the ticket's other labels. The label is never added automatically; `l` on a ticket that already has it just says "Already labelled", and under All teams or No team label it asks you to pick a team. Quick filters follow the team picker: under one team they search that team's label or members, under All teams every team's labels and members, and under No team label only tickets no team claims. Changing team re-runs an active quick filter. If Jira rejects a configured member (a name or account id it does not know), the team queries are re-run on labels alone and stay that way until the app restarts.

Known limitation: the TOOLS board also includes unlabelled tickets assigned to specific people. Those show under TOOLS only once the account ids are listed as TOOLS's `members` (the example config has them commented out); otherwise a label-based filter does not see them.

## Developer switches

- `JIRA_TICKETS_READONLY` blocks every write to Jira.
- `JIRA_TICKETS_PERF` logs request timings.
- `JIRA_TICKETS_CONFIG` points at another config file.
- `JIRA_TICKETS_TOKEN_FILE` points at another saved-token file.
- `JIRA_TICKETS_CACHE_DIR` points at another cache folder.
- `JIRA_TICKETS_THEME` is `light` or `dark`; the default follows the system.

Screenshots need the `snapshot` feature:

    nix develop --command cargo build --release --features snapshot
    JIRA_TICKETS_SCREENSHOT=out.png JIRA_TICKETS_VIEW=issues JIRA_TICKETS_ROW=2 ./target/release/jira-tickets

`JIRA_TICKETS_SCREENSHOT` is the output path and switches the app to an offscreen render at 1440x900, 2x. `JIRA_TICKETS_VIEW` picks `issues`, `grooming`, `team`, or `roadmap`. `JIRA_TICKETS_ROW=<n>` moves the selection by n rows, and `JIRA_TICKETS_THEME` picks `light` or `dark`; unset means light here, not the system theme. Writes are always blocked in this mode. If nothing has loaded after 20 seconds (for example, no token), the picture shows whatever is on screen.

## Code map

| File | Role |
| --- | --- |
| `src/main.rs` | Startup, config load, window options |
| `src/app.rs` | App state, actions, keyboard shortcuts, background work |
| `src/auth.rs` | Token lookup, saving, and sign-out |
| `src/config.rs` | Config file, example, secret references |
| `src/jira.rs` | Jira REST client, read-only guard |
| `src/store.rs` | Issue store and list ids |
| `src/cache.rs` | On-disk cache |
| `src/filter.rs` | Search, quick-filter, and sprint-scope logic |
| `src/adf.rs` | Atlassian document format parsing |
| `src/theme.rs` | Colors and styling |
| `src/views/mod.rs` | Screen shell, sign-in, toast, shared widgets |
| `src/views/issues.rs` | Issue list |
| `src/views/detail.rs` | Issue detail pane |
| `src/views/adf_view.rs` | Renders rich text |
| `src/views/pickers.rs` | Transition, assign, sprint, and edit pickers |
| `src/snapshot.rs` | Offscreen screenshots (`snapshot` feature) |

Grooming, Team, and Roadmap are placeholders until plans 2 and 3.
