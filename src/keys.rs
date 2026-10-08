//! Every keyboard binding in one list: the help overlay, the hint bar, and the
//! README table are all drawn from it.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Global,
    Issues,
    Pickers,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Global => "Global",
            Group::Issues => "Issues",
            Group::Pickers => "Pickers",
        }
    }
}

/// `keys` lists alternatives separated by ", " (each one is a key chip).
pub struct KeyBinding {
    pub group: Group,
    pub keys: &'static str,
    pub what: &'static str,
}

const fn b(group: Group, keys: &'static str, what: &'static str) -> KeyBinding {
    KeyBinding { group, keys, what }
}

pub const BINDINGS: &[KeyBinding] = &[
    b(Group::Global, "⌘1–⌘4", "Issues, Grooming, Team, Roadmap"),
    b(Group::Global, "⌘R", "Refresh"),
    b(Group::Global, "⌘W", "Close the window"),
    b(Group::Global, "?", "Show or hide this help"),
    b(Group::Issues, "⌘K", "Go to an issue by key, number, or link"),
    b(Group::Issues, "Esc", "Close the go-to box"),
    b(Group::Issues, "⌘F, /", "Focus search"),
    b(Group::Issues, "j, ↓", "Next issue"),
    b(Group::Issues, "k, ↑", "Previous issue"),
    b(Group::Issues, "t", "Transition"),
    b(Group::Issues, "a", "Assign"),
    b(Group::Issues, "s", "Sprint"),
    b(Group::Issues, "e", "Edit labels, priority, points"),
    b(Group::Issues, "l", "Add the picked team's label"),
    b(Group::Issues, "J, K", "Move down (J) or up (K) in rank"),
    b(Group::Issues, "c", "Copy the issue link"),
    b(Group::Issues, "⌘O", "Open in Jira"),
    b(Group::Issues, "⌘Enter", "Post the comment (composer focused)"),
    b(Group::Pickers, "↑, ↓", "Move"),
    b(Group::Pickers, "j, k", "Move (not in Assign)"),
    b(Group::Pickers, "Enter", "Confirm"),
    b(Group::Pickers, "Esc", "Cancel"),
];

/// The key chips of one binding.
pub fn chips(keys: &str) -> impl Iterator<Item = &str> {
    keys.split(", ")
}

/// The README keyboard table, in `BINDINGS` order, with a bold row per group.
pub fn readme_table() -> String {
    let mut out = String::from("| Keys | Action |\n| --- | --- |\n");
    let mut group = None;
    for k in BINDINGS {
        if group != Some(k.group) {
            group = Some(k.group);
            out.push_str(&format!("| **{}** | |\n", k.group.name()));
        }
        let keys = chips(k.keys).map(|c| format!("`{c}`")).collect::<Vec<_>>().join(" or ");
        out.push_str(&format!("| {keys} | {} |\n", k.what));
    }
    out
}

/// The hint bar text for what the keyboard does right now on the Issues screen.
pub fn hints(picker_open: bool, has_selection: bool) -> &'static str {
    if picker_open {
        "↑/↓ move · Enter confirm · Esc cancel"
    } else if has_selection {
        "t transition · a assign · s sprint · e edit · l label · J/K rank · c copy · ⌘O open · ? help"
    } else {
        "j/k move · ⌘K go to · / search · ? help"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_matches_bindings() {
        let readme = include_str!("../README.md");
        let start = readme.find("<!-- keys:start -->").expect("start marker") + "<!-- keys:start -->".len();
        let end = readme.find("<!-- keys:end -->").expect("end marker");
        assert_eq!(readme[start..end].trim(), readme_table().trim());
    }

    #[test]
    fn readme_table_groups_and_quotes_keys() {
        let t = readme_table();
        assert!(t.starts_with("| Keys | Action |\n| --- | --- |\n| **Global** | |\n| `⌘1–⌘4` | Issues, Grooming, Team, Roadmap |\n"));
        assert!(t.contains("| `⌘F` or `/` | Focus search |\n"));
        assert_eq!(t.matches("| **").count(), 3);
    }

    #[test]
    fn hints_follow_context() {
        assert_eq!(hints(true, true), "↑/↓ move · Enter confirm · Esc cancel");
        assert_eq!(hints(true, false), "↑/↓ move · Enter confirm · Esc cancel");
        assert_eq!(hints(false, true), "t transition · a assign · s sprint · e edit · l label · J/K rank · c copy · ⌘O open · ? help");
        assert_eq!(hints(false, false), "j/k move · ⌘K go to · / search · ? help");
    }
}
