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

Roadmap settings name custom fields by id, such as `customfield_12345`. To find them, open `<site>/rest/api/3/field` in a browser while signed in to Jira and look for the field names.

## Signing in

The app looks for an API token in this order:

1. The `JIRA_API_TOKEN` environment variable. `JIRA_EMAIL` overrides the config's email.
2. `api_token` in the config. It can be a plain value, `op://vault/item/field` (the app runs `op read`), or `$(command)` (the app runs the command). It is resolved on every launch and never written to disk.
3. A token saved earlier from the paste prompt, at `~/Library/Application Support/jira-tickets/token` on macOS or `~/.config/jira-tickets/token` on Linux (mode 0600).
4. The paste prompt, where you paste an Atlassian API token.

Only a token you paste is saved. Signing out deletes that file.

## Keyboard

| Key | Action |
| --- | --- |
| `⌘1`–`⌘4` | Issues, Grooming, Team, Roadmap |
| `⌘R` | Refresh |
| `⌘K` | Go to an issue by key, number, or link |
| `⌘F` or `/` | Focus search |
| `j` / `k` (or arrows) | Next / previous issue |
| `Enter` | Confirm the open picker |
| `t` | Transition |
| `a` | Assign |
| `s` | Sprint |
| `e` | Edit |
| `J` / `K` | Move the issue down / up in rank |
| `c` | Copy the issue link |
| `⌘O` | Open in the browser |
| `⌘W` | Close the window |

Only `⌘1`–`⌘4`, `⌘R`, and `⌘W` work on every screen. The rest are bound on the Issues screen only. Plain letter keys do nothing while a text field has focus. While a picker is open, only `Esc`, the arrow keys (and `j` / `k`, except in the Assign picker), and `Enter` apply. Selecting a row shows its detail.

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
| `src/filter.rs` | Search and quick-filter logic |
| `src/adf.rs` | Atlassian document format parsing |
| `src/theme.rs` | Colors and styling |
| `src/views/mod.rs` | Screen shell, sign-in, toast, shared widgets |
| `src/views/issues.rs` | Issue list |
| `src/views/detail.rs` | Issue detail pane |
| `src/views/adf_view.rs` | Renders rich text |
| `src/views/pickers.rs` | Transition, assign, sprint, and edit pickers |
| `src/snapshot.rs` | Offscreen screenshots (`snapshot` feature) |

Grooming, Team, and Roadmap are placeholders until plans 2 and 3.
