//! The modal pickers' state: opening each one, moving its cursor, and confirming the choice.

use super::App;
use super::msg::Msg;
use crate::jira::{self, Resolution, Sprint, Transition, User};

/// The open modal picker. `cursor` indexes the rows the picker draws; Assign
/// and Sprint put a synthetic first row ("Unassigned", "Backlog") at 0.
pub enum Picker {
    Transition {
        key: String,
        items: Vec<Transition>,
        cursor: usize,
    },
    Resolution {
        key: String,
        transition_id: String,
        items: Vec<Resolution>,
        cursor: usize,
        /// Opened by the session's `c`: titled "Close KEY as", and its write is a "Close" the session applies.
        close: bool,
    },
    Assign {
        key: String,
        items: Vec<User>,
        /// The issue's assignee when the picker opened; pinned right after "Unassigned".
        current: Option<User>,
        cursor: usize,
        filter: String,
    },
    Sprint {
        key: String,
        items: Vec<Sprint>,
        cursor: usize,
    },
    /// The `orig_*` fields hold the values the dialog opened with; only changed fields are sent.
    Edit {
        key: String,
        labels: String,
        priority: String,
        points: String,
        orig_labels: String,
        orig_priority: String,
        orig_points: String,
    },
}

impl Picker {
    /// The write this picker sends, as its done toast names it ("Close", "Sprint move").
    pub fn write_name(&self) -> &'static str {
        match self {
            Picker::Transition { .. } | Picker::Resolution { close: false, .. } => "Transition",
            Picker::Resolution { close: true, .. } => "Close",
            Picker::Assign { .. } => "Assign",
            Picker::Sprint { .. } => "Sprint move",
            Picker::Edit { .. } => "Edit",
        }
    }

    /// The Edit dialog's priority choices: Jira's five default priorities, highest first.
    pub const PRIORITIES: [&'static str; 5] = ["Highest", "High", "Medium", "Low", "Lowest"];

    /// The Assign filter matched nobody: no row shows, nothing is selected and Confirm is off, so Enter never
    /// unassigns by accident. `shown` is the user rows `assign_rows` left.
    pub fn assign_empty(filter: &str, shown: usize) -> bool {
        !filter.trim().is_empty() && shown == 0
    }

    /// The Edit dialog's selected priority among PRIORITIES (case-insensitive), if it is one of them.
    pub fn priority_index(priority: &str) -> Option<usize> {
        Self::PRIORITIES.iter().position(|p| p.eq_ignore_ascii_case(priority.trim()))
    }

    /// Whether Confirm would change anything, so it is on: off while the cursor sits on the issue's current
    /// value (its `status` or `sprint`), on an Assign filter that matched nobody, or while no Edit field differs
    /// from what it opened with. A Resolution has no current value, so it is always a change.
    pub fn changes(&self, status: &str, sprint: Option<&str>) -> bool {
        match self {
            Picker::Transition { items, cursor, .. } => items.get(*cursor).is_some_and(|t| !t.to.eq_ignore_ascii_case(status)),
            Picker::Resolution { items, cursor, .. } => *cursor < items.len(),
            Picker::Assign { items, current, cursor, filter, .. } => {
                let rows = assign_rows(items, filter, current.as_ref()).0;
                assign_choice(&rows, filter, *cursor).is_some() && assign_current_row(&rows, current.as_ref()) != Some(*cursor)
            }
            Picker::Sprint { items, cursor, .. } => *cursor <= items.len() && sprint_current_row(items, sprint) != Some(*cursor),
            Picker::Edit { labels, priority, points, orig_labels, orig_priority, orig_points, .. } => {
                edit_fields(labels, priority, points, orig_labels, orig_priority, orig_points).is_some()
            }
        }
    }

    /// The issue the picker acts on.
    pub fn key(&self) -> &str {
        match self {
            Picker::Transition { key, .. }
            | Picker::Resolution { key, .. }
            | Picker::Assign { key, .. }
            | Picker::Sprint { key, .. }
            | Picker::Edit { key, .. } => key,
        }
    }
}

/// What confirming the Assign picker does: None when nothing is selected (the filter matched nobody, or the
/// cursor is past the rows), Some(None) to unassign (row 0), Some(Some(account)) to assign.
fn assign_choice(rows: &[User], filter: &str, cursor: usize) -> Option<Option<String>> {
    if Picker::assign_empty(filter, rows.len()) {
        return None;
    }
    if cursor == 0 {
        return Some(None);
    }
    rows.get(cursor - 1).map(|u| Some(u.account_id.clone()))
}

impl App {
    // spawn is a no-op without a client, so every open_* checks first
    // or `picker_loading` would never clear.
    pub(super) fn open_transition(&mut self) {
        let Some(key) = self.selected.clone() else { return };
        if self.client.is_none() || self.picker_loading {
            return;
        }
        self.picker_loading = true;
        self.spawn(move |c, tx| {
            let _ = tx.send(Msg::Transitions { key: key.clone(), result: c.transitions(&key) });
        });
    }

    pub(super) fn open_assign(&mut self) {
        let Some(key) = self.action_target() else { return };
        if self.client.is_none() || self.picker_loading {
            return;
        }
        self.picker_loading = true;
        let project = self.cfg.scope.project.clone();
        self.spawn(move |c, tx| {
            let _ = tx.send(Msg::Users { key, result: c.assignable_users(&project) });
        });
    }

    pub(super) fn open_sprint(&mut self) {
        let Some(key) = self.action_target() else { return };
        if self.client.is_none() || self.picker_loading {
            return;
        }
        self.picker_loading = true;
        let board = self.cfg.scope.board_id;
        self.spawn(move |c, tx| {
            let _ = tx.send(Msg::Sprints { key, result: c.board_sprints(board, "active,future") });
        });
    }

    pub(super) fn open_edit(&mut self) {
        // A late picker response would replace the Edit dialog.
        if self.client.is_none() || self.picker_loading {
            return;
        }
        let Some(i) = self.selected.as_ref().and_then(|k| self.store.get(k)) else { return };
        let (labels, priority, points) = (i.labels.join(", "), i.priority.clone(), i.story_points.map(|n| n.to_string()).unwrap_or_default());
        self.picker = Some(Picker::Edit {
            key: i.key.clone(),
            labels: labels.clone(),
            priority: priority.clone(),
            points: points.clone(),
            orig_labels: labels,
            orig_priority: priority,
            orig_points: points,
        });
    }

    pub(super) fn picker_move(&mut self, d: i32) {
        if let Some(p) = &mut self.picker {
            let (cur, len) = match p {
                Picker::Transition { cursor, items, .. } => (cursor, items.len()),
                Picker::Resolution { cursor, items, .. } => (cursor, items.len()),
                Picker::Assign { cursor, items, filter, current, .. } => {
                    let shown = assign_rows(items, filter, current.as_ref()).0.len();
                    (cursor, if Picker::assign_empty(filter, shown) { 0 } else { shown + 1 })
                }
                Picker::Sprint { cursor, items, .. } => (cursor, items.len() + 1),
                Picker::Edit { .. } => return,
            };
            *cur = wrap_cursor(*cur, d, len);
        }
    }

    pub(super) fn confirm_picker(&mut self) {
        // Without a client spawn does nothing; keep the picker open rather than drop the choice silently.
        if self.client.is_none() {
            return;
        }
        // Read-only: Confirm is off, so Enter keeps the dialog open, sends nothing, and the toast repeats why.
        if let Err(e) = jira::check_writable() {
            if let Some(open) = &self.picker {
                let (text, kind) = super::msg::done_toast(open.write_name(), &Err(e));
                self.toast_kind(text, kind);
            }
            return;
        }
        // Confirm is off while the choice is the issue's current value (or nothing is chosen): Enter, or a click
        // on the current row, keeps the dialog open and sends nothing.
        let Some(open) = &self.picker else { return };
        let issue = self.store.get(open.key());
        let (status, sprint) = (issue.map(|i| i.status.name.clone()).unwrap_or_default(), issue.and_then(|i| i.sprint.clone()));
        if !open.changes(&status, sprint.as_deref()) {
            return;
        }
        let Some(p) = self.picker.take() else { return };
        match p {
            Picker::Transition { key, items, cursor } => {
                let Some(t) = items.get(cursor).cloned() else { return };
                // Remember the transition so a resolution error can retry it.
                self.pending_transition = Some((key.clone(), t.id.clone()));
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Transition", result: c.transition(&key, &t.id, None) });
                });
            }
            Picker::Resolution { key, transition_id, items, cursor, close } => {
                let Some(r) = items.get(cursor).cloned() else { return };
                let what = if close { "Close" } else { "Transition" };
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what, result: c.transition(&key, &transition_id, Some(&r.id)) });
                });
            }
            Picker::Assign { key, items, current, cursor, filter } => {
                let shown = assign_rows(&items, &filter, current.as_ref()).0;
                // `changes` already kept the dialog open when nothing is selected.
                let Some(account) = assign_choice(&shown, &filter, cursor) else { return };
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Assign", result: c.assign(&key, account.as_deref()) });
                });
            }
            Picker::Sprint { key, items, cursor } => {
                let sprint = if cursor == 0 { None } else { items.get(cursor - 1).map(|s| s.id) };
                if cursor != 0 && sprint.is_none() {
                    return;
                }
                if self.screen == super::Screen::Grooming && self.grooming.mode == super::GroomMode::Session {
                    let name = sprint.and_then(|id| items.iter().find(|s| s.id == id)).map(|s| s.name.clone());
                    self.grooming.pending_sprint_move = Some((key.clone(), name));
                }
                self.spawn_write(move |c, tx| {
                    let r = match sprint {
                        Some(id) => c.move_to_sprint(id, &[&key]),
                        None => c.move_to_backlog(&[&key]),
                    };
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Sprint move", result: r });
                });
            }
            Picker::Edit { key, labels, priority, points, orig_labels, orig_priority, orig_points } => {
                let Some(fields) = edit_fields(&labels, &priority, &points, &orig_labels, &orig_priority, &orig_points) else { return };
                self.spawn_write(move |c, tx| {
                    let _ = tx.send(Msg::Done { key: key.clone(), what: "Edit", result: c.update_fields(&key, fields) });
                });
            }
        }
    }
}

/// The Transition picker opens on the first transition into the current status, else the first row.
pub fn transition_cursor(items: &[Transition], status: &str) -> usize {
    items.iter().position(|t| t.to.eq_ignore_ascii_case(status)).unwrap_or(0)
}

/// Users the Assign picker lists after "Unassigned", at most this many, so the dialog never scrolls.
pub const ASSIGN_USERS: usize = 9;

/// The Assign picker's user rows (row 0, "Unassigned", is not included): the current assignee pinned
/// first (whether or not the fetched list holds them) while they match the filter, then the filter's other
/// matches, capped at ASSIGN_USERS. A current assignee the filter does not match is not shown. The bool
/// says matches were cut (the dialog says "9 of N · type to see more").
pub fn assign_rows(items: &[User], filter: &str, current: Option<&User>) -> (Vec<User>, bool) {
    let others: Vec<User> = filtered_users(items, filter).into_iter().filter(|u| current.is_none_or(|c| c.account_id != u.account_id)).collect();
    let pinned = current.filter(|c| !filtered_users(std::slice::from_ref(*c), filter).is_empty());
    let mut rows: Vec<User> = pinned.cloned().into_iter().collect();
    let room = ASSIGN_USERS - rows.len();
    let cut = others.len() > room;
    rows.extend(others.into_iter().take(room));
    (rows, cut)
}

/// The Sprint picker opens on the issue's sprint (row 0 is "Backlog").
pub fn sprint_cursor(items: &[Sprint], current: Option<&str>) -> usize {
    current.and_then(|n| items.iter().position(|s| s.name == n)).map_or(0, |i| i + 1)
}

/// A transition row: just the name when it is named after its target status.
pub fn transition_row(t: &Transition) -> String {
    if t.name.eq_ignore_ascii_case(&t.to) { t.name.clone() } else { format!("{}  →  {}", t.name, t.to) }
}

/// Users whose display name contains `filter`, case-insensitively.
pub fn filtered_users(items: &[User], filter: &str) -> Vec<User> {
    let f = filter.trim().to_lowercase();
    items.iter().filter(|u| f.is_empty() || u.display_name.to_lowercase().contains(&f)).cloned().collect()
}

/// The Assign cursor for `rows` (from `assign_rows`; screen row = index + 1): with no filter, the pinned
/// current assignee (else "Unassigned"); with one, the first match (every row matches it), so
/// type-then-Enter assigns rather than unassigns; no match: "Unassigned".
pub fn assign_cursor_after_filter(rows: &[User], filter: &str, pinned: bool) -> usize {
    if filter.trim().is_empty() {
        return usize::from(pinned && !rows.is_empty());
    }
    usize::from(!rows.is_empty())
}

/// The screen row (0 is "Unassigned") holding the issue's current assignee, if it is shown.
pub fn assign_current_row(rows: &[User], current: Option<&User>) -> Option<usize> {
    match current {
        None => Some(0),
        Some(c) => rows.iter().position(|u| u.account_id == c.account_id).map(|i| i + 1),
    }
}

/// How many users match the Assign filter: the listed matches plus a pinned current assignee who matches.
pub fn assign_matches(items: &[User], filter: &str, current: Option<&User>) -> usize {
    let others = filtered_users(items, filter).into_iter().filter(|u| current.is_none_or(|c| c.account_id != u.account_id)).count();
    others + usize::from(current.is_some_and(|c| !filtered_users(std::slice::from_ref(c), filter).is_empty()))
}

/// The line under the Assign list, leading with the count like the filtered "N matches": "9 of N · type to
/// see more" when matches were cut, else "N matches" while a filter is typed (so a short list does not end in
/// blank space), else nothing.
pub fn assign_note(filter: &str, shown: usize, total: usize, cut: bool) -> Option<String> {
    let matches = shown;
    if cut {
        Some(format!("{shown} of {total} · type to see more"))
    } else if !filter.trim().is_empty() {
        Some(format!("{matches} {}", if matches == 1 { "match" } else { "matches" }))
    } else {
        None
    }
}

/// The Sprint picker's row (0 is "Backlog") for the issue's current sprint; None for a sprint not listed.
pub fn sprint_current_row(items: &[Sprint], current: Option<&str>) -> Option<usize> {
    match current {
        None => Some(0),
        Some(n) => items.iter().position(|s| s.name == n).map(|i| i + 1),
    }
}

/// The Edit fields' placeholders: what each accepts, in plain words. The fields open filled with the
/// current value, so the placeholder shows only once cleared (which leaves the value unchanged).
pub const PRIORITY_HINT: &str = "Highest, High, Medium, Low or Lowest";
pub const POINTS_HINT: &str = "Number, e.g. 3";

/// A sprint's row in the Sprint picker: its name, and its state as a secondary tag only when that adds something
/// ("future"), drawn like the "current" tag beside the issue's own sprint; the active sprint has none.
pub fn sprint_row_label(name: &str, state: &str) -> (String, Option<String>) {
    let tag = (!state.eq_ignore_ascii_case("active") && !state.trim().is_empty()).then(|| state.trim().to_lowercase());
    (name.to_string(), tag)
}

/// Moves a picker cursor by `d`, wrapping at both ends.
fn wrap_cursor(cur: usize, d: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (cur as i32 + d).rem_euclid(len as i32) as usize
}

/// The `fields` body for the Edit picker, holding only what changed from the
/// values it opened with, so a save never overwrites someone else's edit to
/// another field: labels when the list differs (comma split, trimmed, empties
/// dropped), priority when it differs and is named, points when they differ
/// and parse. None when nothing changed.
fn edit_fields(labels: &str, priority: &str, points: &str, orig_labels: &str, orig_priority: &str, orig_points: &str) -> Option<serde_json::Value> {
    let split = |s: &str| s.split(',').map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect::<Vec<_>>();
    let mut fields = serde_json::Map::new();
    if split(labels) != split(orig_labels) {
        fields.insert("labels".into(), serde_json::json!(split(labels)));
    }
    if !priority.trim().is_empty() && priority.trim() != orig_priority.trim() {
        fields.insert("priority".into(), serde_json::json!({ "name": priority.trim() }));
    }
    if let Ok(n) = points.trim().parse::<f64>()
        && orig_points.trim().parse::<f64>().ok() != Some(n)
    {
        fields.insert(jira::POINTS_FIELD.into(), serde_json::json!(n));
    }
    (!fields.is_empty()).then_some(serde_json::Value::Object(fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_cursor_wraps() {
        assert_eq!(wrap_cursor(0, -1, 3), 2);
        assert_eq!(wrap_cursor(2, 1, 3), 0);
        assert_eq!(wrap_cursor(1, 1, 3), 2);
        assert_eq!(wrap_cursor(0, 1, 0), 0);
    }

    #[test]
    fn users_filter_by_name_case_insensitively() {
        let u = |n: &str| User { account_id: n.to_lowercase(), display_name: n.into() };
        let items = [u("Ann Lee"), u("Bob Ray"), u("Joanne")];
        let names = |f: &str| filtered_users(&items, f).into_iter().map(|u| u.display_name).collect::<Vec<_>>();
        assert_eq!(names(""), ["Ann Lee", "Bob Ray", "Joanne"]);
        assert_eq!(names(" ANN "), ["Ann Lee", "Joanne"]);
        assert!(names("zed").is_empty());
    }

    #[test]
    fn edit_fields_body() {
        let f = edit_fields(" a, ,b ", "  ", "3.5", "", "Low", "");
        assert_eq!(f, Some(serde_json::json!({ "labels": ["a", "b"], "customfield_10016": 3.5 })));
        let f = edit_fields("", "High", "x", "old", "Low", "2");
        assert_eq!(f, Some(serde_json::json!({ "labels": [], "priority": { "name": "High" } })));
    }

    #[test]
    fn edit_fields_omit_unchanged() {
        // Only priority changed; labels (reformatted) and points (same value) stay out.
        let f = edit_fields("a,b", "High", "3.0", "a, b", "Low", "3");
        assert_eq!(f, Some(serde_json::json!({ "priority": { "name": "High" } })));
        let f = edit_fields("a, b", "Low", "3", "a, b", "Low", "3");
        assert_eq!(f, None, "nothing changed");
        assert_eq!(edit_fields("a, b", "Low", "5", "a, b", "Low", "3"), Some(serde_json::json!({ "customfield_10016": 5.0 })));
    }

    #[test]
    fn assign_filter_moves_cursor_to_first_match() {
        let u = |n: &str| User { account_id: n.to_lowercase(), display_name: n.into() };
        let shawn = u("Shawn");
        let items: Vec<User> = ["Ann", "Grace", "Grace Lee"].map(u).to_vec();
        let (rows, _) = assign_rows(&items, "grace", Some(&shawn));
        assert_eq!(rows.iter().map(|r| r.display_name.as_str()).collect::<Vec<_>>(), ["Grace", "Grace Lee"], "the non-matching assignee is not pinned");
        assert_eq!(assign_cursor_after_filter(&rows, "grace", false), 1, "the first match");
        assert_eq!(assign_current_row(&rows, Some(&shawn)), None);
        let (rows, _) = assign_rows(&items, "sha", Some(&shawn));
        assert_eq!(assign_cursor_after_filter(&rows, "sha", true), 1, "the pinned assignee matches");
        assert_eq!(assign_current_row(&rows, Some(&shawn)), Some(1));
        assert_eq!(assign_current_row(&rows, None), Some(0), "unassigned: the Unassigned row is current");
        let (rows, _) = assign_rows(&items, "zed", None);
        assert_eq!(assign_cursor_after_filter(&rows, "zed", false), 0, "typed, no match");
        let (rows, _) = assign_rows(&items, "", Some(&shawn));
        assert_eq!(assign_cursor_after_filter(&rows, "", true), 1, "opens on the pinned assignee");
        let (rows, _) = assign_rows(&items, "", None);
        assert_eq!(assign_cursor_after_filter(&rows, "", false), 0, "unassigned issue opens on Unassigned");
    }

    #[test]
    fn assign_with_no_match_selects_nothing() {
        let u = |n: &str| User { account_id: n.to_lowercase(), display_name: n.into() };
        let items: Vec<User> = ["Ann", "Grace"].map(u).to_vec();
        let (rows, _) = assign_rows(&items, "vin", None);
        assert!(Picker::assign_empty("vin", rows.len()));
        assert_eq!(assign_choice(&rows, "vin", 0), None, "Enter on 0 matches never unassigns");
        assert!(!Picker::assign_empty("  ", 0), "no filter: Unassigned is a real choice");
        assert_eq!(assign_choice(&[], "", 0), Some(None));
        let (rows, _) = assign_rows(&items, "gra", None);
        assert!(!Picker::assign_empty("gra", rows.len()));
        assert_eq!(assign_choice(&rows, "gra", 1), Some(Some("grace".into())));
        assert_eq!(assign_choice(&rows, "gra", 0), Some(None), "Unassigned stays pickable beside matches");
        assert_eq!(assign_choice(&rows, "gra", 5), None, "past the rows");
    }

    #[test]
    fn priority_choices_match_case_insensitively() {
        assert_eq!(Picker::priority_index("high"), Some(1));
        assert_eq!(Picker::priority_index(" Lowest "), Some(4));
        assert_eq!(Picker::priority_index("P1"), None, "a custom priority selects no segment");
        assert_eq!(Picker::priority_index(""), None);
    }

    #[test]
    fn assign_pins_the_current_assignee() {
        let u = |n: &str| User { account_id: n.to_lowercase(), display_name: n.into() };
        let items: Vec<User> = (0..20).map(|i| u(&format!("User {i:02}"))).chain([u("Shawn")]).collect();
        let shawn = u("Shawn");
        // Capped list: Shawn is past the cap but pinned first, and the cut is reported.
        let (rows, cut) = assign_rows(&items, "", Some(&shawn));
        assert_eq!(rows.len(), ASSIGN_USERS);
        assert_eq!(rows[0].display_name, "Shawn");
        assert!(cut);
        assert_eq!(rows.iter().filter(|r| r.display_name == "Shawn").count(), 1, "not listed twice");
        // Filtered to others: the assignee drops out.
        let (rows, cut) = assign_rows(&items, "user 0", Some(&shawn));
        assert_eq!(rows[0].display_name, "User 00");
        assert!(cut, "User 00..09: 10 matches, room for 9");
        assert_eq!(assign_matches(&items, "user 0", Some(&shawn)), 10);
        assert_eq!(assign_note("user 0", rows.len(), 10, cut).as_deref(), Some("9 of 10 · type to see more"));
        assert_eq!(assign_matches(&items, "", Some(&shawn)), 21, "the pinned assignee counts once");
        assert_eq!(assign_note("", 9, 21, true).as_deref(), Some("9 of 21 · type to see more"), "unfiltered leads with the count");
        assert_eq!(assign_note("grace", 2, 2, false).as_deref(), Some("2 matches"));
        assert_eq!(assign_note("grace", 1, 1, false).as_deref(), Some("1 match"));
        assert_eq!(assign_note("", 9, 9, false), None);
        // Short list: nothing cut.
        let (rows, cut) = assign_rows(&items[..3], "", None);
        assert_eq!(rows.len(), 3);
        assert!(!cut);
    }

    #[test]
    fn pickers_open_on_the_current_value() {
        let t = |n: &str, to: &str| Transition { id: n.into(), name: n.into(), to: to.into() };
        let items = [t("Start", "In Progress"), t("Review", "In Review"), t("In Review", "In Review")];
        assert_eq!(transition_cursor(&items, "in review"), 1);
        assert_eq!(transition_cursor(&items, "Done"), 0);
        let sp = |n: &str| Sprint { id: 1, name: n.into(), state: "active".into() };
        let sprints = [sp("Sprint 91"), sp("Sprint 92")];
        assert_eq!(sprint_cursor(&sprints, Some("Sprint 92")), 2);
        assert_eq!(sprint_cursor(&sprints, None), 0, "backlog");
        assert_eq!(sprint_current_row(&sprints, Some("Sprint 92")), Some(2));
        assert_eq!(sprint_current_row(&sprints, Some("Sprint 80")), None, "a closed sprint is not listed");
        assert_eq!(sprint_current_row(&sprints, None), Some(0));
        assert_eq!(PRIORITY_HINT, "Highest, High, Medium, Low or Lowest");
        assert_eq!(POINTS_HINT, "Number, e.g. 3");
        assert_eq!(sprint_row_label("Sprint 91", "active"), ("Sprint 91".to_string(), None), "no \"active current\"");
        assert_eq!(sprint_row_label("Sprint 92", "future"), ("Sprint 92".to_string(), Some("future".to_string())), "a tag like current");
        assert_eq!(sprint_row_label("Sprint 93", "FUTURE").1.as_deref(), Some("future"));
    }

    #[test]
    fn confirm_is_off_until_the_choice_differs() {
        let t = |n: &str, to: &str| Transition { id: n.into(), name: n.into(), to: to.into() };
        let items = vec![t("Start", "In Progress"), t("Review", "In Review")];
        let tr = |cursor| Picker::Transition { key: "CI-1".into(), items: items.clone(), cursor };
        assert!(!tr(1).changes("in review", None), "the current status is a no-op");
        assert!(tr(0).changes("In Review", None));
        assert!(!tr(5).changes("In Review", None), "past the rows");

        let sp = |n: &str| Sprint { id: 1, name: n.into(), state: "active".into() };
        let sprints = vec![sp("Sprint 91"), sp("Sprint 92")];
        let sprint = |cursor| Picker::Sprint { key: "CI-1".into(), items: sprints.clone(), cursor };
        assert!(!sprint(2).changes("", Some("Sprint 92")));
        assert!(sprint(0).changes("", Some("Sprint 92")), "to the backlog");
        assert!(!sprint(0).changes("", None), "already in the backlog");
        assert!(sprint(1).changes("", Some("Sprint 80")), "a closed sprint is not listed: every row is a change");

        let u = |n: &str| User { account_id: n.to_lowercase(), display_name: n.into() };
        let assign = |cursor, current: Option<User>, filter: &str| Picker::Assign {
            key: "CI-1".into(),
            items: vec![u("Ann"), u("Grace")],
            current,
            cursor,
            filter: filter.into(),
        };
        assert!(!assign(1, Some(u("Ann")), "").changes("", None), "the pinned assignee");
        assert!(assign(0, Some(u("Ann")), "").changes("", None), "unassign");
        assert!(!assign(0, None, "").changes("", None), "already unassigned");
        assert!(!assign(0, None, "zed").changes("", None), "nothing matched");

        let edit = |priority: &str| Picker::Edit {
            key: "CI-1".into(),
            labels: "a".into(),
            priority: priority.into(),
            points: String::new(),
            orig_labels: "a".into(),
            orig_priority: String::new(),
            orig_points: String::new(),
        };
        assert!(!edit("").changes("", None), "nothing edited");
        assert!(edit("High").changes("", None));

        let res = |cursor| Picker::Resolution {
            key: "CI-1".into(),
            transition_id: "1".into(),
            items: vec![Resolution { id: "1".into(), name: "Done".into() }],
            cursor,
            close: false,
        };
        assert!(res(0).changes("", None));
        assert!(!res(1).changes("", None));
    }

    #[test]
    fn read_only_confirm_keeps_the_picker_open() {
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ro = crate::jira::tests::ReadOnlyGuard::new();
        let (base, rx) = crate::jira::tests::stub_server(vec![]);
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let login = crate::auth::Login { email: "a@b.c".into(), token: "t".into(), source: crate::auth::Source::Env };
        let mut app = App::for_test(cfg, Some(std::sync::Arc::new(crate::jira::Client::new(&base, &login))));
        let t = Transition { id: "1".into(), name: "Start".into(), to: "In Progress".into() };
        app.picker = Some(Picker::Transition { key: "CI-1".into(), items: vec![t], cursor: 0 });
        app.confirm_picker();
        assert!(app.picker.is_some(), "the dialog stays open");
        assert!(app.toast.as_ref().is_some_and(|t| t.text.contains("the transition was not sent")), "and the toast says why");
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(rx.try_iter().next().is_none(), "nothing sent");
    }

    #[test]
    fn transition_rows_drop_a_repeated_target() {
        let t = |n: &str, to: &str| Transition { id: "1".into(), name: n.into(), to: to.into() };
        assert_eq!(transition_row(&t("Done", "Done")), "Done");
        assert_eq!(transition_row(&t("Start work", "In Progress")), "Start work  →  In Progress");
    }
}
