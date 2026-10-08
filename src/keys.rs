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

/// `keys` lists alternatives separated by ", " (each one is a key chip); " to " joins the ends of a range
/// ("⌘1 to ⌘4"), drawn as two chips with a muted "to".
pub struct KeyBinding {
    pub group: Group,
    pub keys: &'static str,
    pub what: &'static str,
}

const fn b(group: Group, keys: &'static str, what: &'static str) -> KeyBinding {
    KeyBinding { group, keys, what }
}

pub const BINDINGS: &[KeyBinding] = &[
    b(Group::Global, "⌘1 to ⌘4", "Issues, Grooming, Team, Roadmap"),
    b(Group::Global, "⌘R", "Refresh"),
    b(Group::Global, "⌘W", "Close the window"),
    b(Group::Global, "?", "Show or hide this help"),
    b(Group::Issues, "⌘K", "Go to an issue by key, number, or link"),
    b(Group::Issues, "Esc", "Close the go-to box"),
    b(Group::Issues, "⌘F, /", "Focus search (Esc clears, then leaves it)"),
    b(Group::Issues, "j/k, ↑/↓", "Next or previous issue"),
    b(Group::Issues, "t", "Transition"),
    b(Group::Issues, "a", "Assign"),
    b(Group::Issues, "s", "Sprint"),
    b(Group::Issues, "e", "Edit labels, priority, points"),
    b(Group::Issues, "l", "Add the picked team's label"),
    b(Group::Issues, "m", "Toggle Roadmap filter"),
    b(Group::Issues, "J/K", "Move down or up in rank"),
    b(Group::Issues, "c", "Copy the issue link"),
    b(Group::Issues, "⌘O", "Open in Jira"),
    b(Group::Issues, "⌘Enter", "Post the comment (composer focused)"),
    b(Group::Pickers, "↑/↓", "Move"),
    b(Group::Pickers, "j/k", "Move (not in Assign)"),
    b(Group::Pickers, "Tab", "Next field (Edit)"),
    b(Group::Pickers, "Enter", "Confirm"),
    b(Group::Pickers, "Esc", "Cancel"),
];

/// The alternatives of one binding; each is one chip, or a range (see `range`).
pub fn chips(keys: &str) -> impl Iterator<Item = &str> {
    keys.split(", ")
}

/// The chips of one alternative: ["⌘1", "⌘4"] for the range "⌘1 to ⌘4", else just the key.
pub fn range(alt: &str) -> Vec<&str> {
    alt.split(" to ").collect()
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
        let keys = chips(k.keys).map(|c| range(c).iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(" to ")).collect::<Vec<_>>().join(" or ");
        out.push_str(&format!("| {keys} | {} |\n", k.what));
    }
    out
}

/// One token of a hint: a key (drawn mono) or a word (drawn small and muted).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    K(&'static str),
    W(&'static str),
}
use Tok::{K, W};

/// One hint, e.g. `[K("t"), W("transition")]`; the bar joins hints with " · ".
pub type Hint = &'static [Tok];

/// What the hint bar describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HintContext {
    Help,
    GoTo,
    /// The search box has focus and Esc would clear it (text, or a search showing).
    Search,
    /// The search box has focus with nothing to clear, so Esc leaves it.
    SearchEmpty,
    /// The comment composer has focus.
    Composer,
    EditPicker,
    /// The Assign picker: its filter box has focus.
    AssignPicker,
    Picker,
    Selected,
    Browse,
}

/// Which text field has keyboard focus, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    None,
    /// The search box; true when Esc would clear it rather than leave.
    Search(bool),
    Composer,
    /// Some other field: plain letters would type into it.
    OtherText,
}

/// Which kind of picker is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerHint {
    Edit,
    Assign,
    Other,
}

pub fn hint_context(help_open: bool, goto_open: bool, picker: Option<PickerHint>, has_selection: bool, focus: Focus) -> HintContext {
    match (help_open, goto_open, picker) {
        (true, _, _) => HintContext::Help,
        (_, _, Some(PickerHint::Edit)) => HintContext::EditPicker,
        (_, _, Some(PickerHint::Assign)) => HintContext::AssignPicker,
        (_, _, Some(PickerHint::Other)) => HintContext::Picker,
        (_, true, None) => HintContext::GoTo,
        _ => match focus {
            Focus::Search(true) => HintContext::Search,
            Focus::Search(false) => HintContext::SearchEmpty,
            Focus::Composer => HintContext::Composer,
            Focus::None if has_selection => HintContext::Selected,
            _ => HintContext::Browse,
        },
    }
}

/// The hints for what the keyboard does right now on the Issues screen.
pub fn hints(c: HintContext) -> &'static [Hint] {
    match c {
        // Alternatives are separate chips; a slash only joins an up/down pair.
        HintContext::Help => &[&[K("?"), W("or"), K("Esc"), W("close")]],
        HintContext::GoTo => &[&[K("Enter"), W("open")], &[K("Esc"), W("close")]],
        HintContext::Search => &[&[K("Enter"), W("search JQL")], &[K("Esc"), W("clear")]],
        HintContext::SearchEmpty => &[&[K("Enter"), W("search JQL")], &[K("Esc"), W("done")]],
        HintContext::Composer => &[&[K("⌘Enter"), W("post")], &[K("Esc"), W("done")]],
        HintContext::EditPicker => &[&[K("Tab/Shift+Tab"), W("field")], &[K("Enter"), W("confirm")], &[K("Esc"), W("cancel")]],
        HintContext::AssignPicker => &[&[W("Type to filter")], &[K("↑/↓"), W("move")], &[K("Enter"), W("confirm")], &[K("Esc"), W("cancel")]],
        HintContext::Picker => &[&[K("j/k"), W("or"), K("↑/↓"), W("move")], &[K("Enter"), W("confirm")], &[K("Esc"), W("cancel")]],
        HintContext::Selected => &[
            &[K("j/k"), W("move")],
            &[K("t"), W("transition")],
            &[K("a"), W("assign")],
            &[K("s"), W("sprint")],
            &[K("e"), W("edit")],
            &[K("l"), W("label")],
            &[K("J/K"), W("rank")],
            &[K("c"), W("copy")],
            &[K("⌘O"), W("open")],
            &[K("?"), W("help")],
        ],
        HintContext::Browse => &[&[K("j/k"), W("move")], &[K("⌘K"), W("go to")], &[K("⌘F"), W("search")], &[K("?"), W("help")]],
    }
}

/// Plain text of a hint line, for tests and tooltips.
pub fn hint_text(hs: &[Hint]) -> String {
    hs.iter()
        .map(|h| {
            h.iter()
                .map(|t| match t {
                    K(s) | W(s) => *s,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// How many leading hints fit in `available`, given each hint's width, the separator's, and the
/// trailing "…"'s. Returns (count, elided): when not all fit, room is kept for the ellipsis.
pub fn fit_hints(widths: &[f32], sep: f32, ellipsis: f32, available: f32) -> (usize, bool) {
    let total: f32 = widths.iter().sum::<f32>() + sep * widths.len().saturating_sub(1) as f32;
    if total <= available {
        return (widths.len(), false);
    }
    let mut used = 0.0;
    let mut n = 0;
    for (i, w) in widths.iter().enumerate() {
        let next = used + if i > 0 { sep } else { 0.0 } + w;
        if next + sep + ellipsis > available {
            break;
        }
        used = next;
        n += 1;
    }
    (n, true)
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
        assert!(t.starts_with("| Keys | Action |\n| --- | --- |\n| **Global** | |\n| `⌘1` to `⌘4` | Issues, Grooming, Team, Roadmap |\n"));
        assert_eq!(range("⌘1 to ⌘4"), ["⌘1", "⌘4"]);
        assert_eq!(range("j/k"), ["j/k"]);
        assert!(t.contains("| `⌘F` or `/` | Focus search (Esc clears, then leaves it) |\n"));
        assert!(t.contains("| `j/k` or `↑/↓` | Next or previous issue |\n"));
        assert_eq!(t.matches("| **").count(), 3);
    }

    #[test]
    fn hints_follow_context() {
        use HintContext::*;
        assert_eq!(hint_context(true, true, Some(PickerHint::Other), true, Focus::OtherText), Help, "help wins");
        assert_eq!(hint_context(false, false, Some(PickerHint::Edit), true, Focus::OtherText), EditPicker);
        assert_eq!(hint_context(false, false, Some(PickerHint::Other), false, Focus::OtherText), Picker, "a picker's own text field still means picker keys");
        assert_eq!(hint_context(false, false, Some(PickerHint::Assign), true, Focus::OtherText), AssignPicker);
        assert_eq!(hint_text(hints(AssignPicker)), "Type to filter · ↑/↓ move · Enter confirm · Esc cancel");
        assert_eq!(hint_context(false, true, None, true, Focus::OtherText), GoTo);
        assert_eq!(hint_context(false, false, None, true, Focus::Search(true)), Search);
        assert_eq!(hint_context(false, false, None, true, Focus::Search(false)), SearchEmpty);
        assert_eq!(hint_text(hints(SearchEmpty)), "Enter search JQL · Esc done");
        assert_eq!(hint_context(false, false, None, true, Focus::Composer), Composer, "the bar follows focus into the composer");
        assert_eq!(hint_context(false, false, None, true, Focus::None), Selected);
        assert_eq!(hint_context(false, false, None, true, Focus::OtherText), Browse, "letters type into the field, so no selection hints");
        assert_eq!(hint_context(false, false, None, false, Focus::None), Browse);
        assert_eq!(hint_text(hints(Composer)), "⌘Enter post · Esc done");
        assert_eq!(hint_text(hints(Help)), "? or Esc close");
        assert_eq!(hint_text(hints(GoTo)), "Enter open · Esc close");
        assert_eq!(hint_text(hints(Search)), "Enter search JQL · Esc clear");
        assert_eq!(hint_text(hints(EditPicker)), "Tab/Shift+Tab field · Enter confirm · Esc cancel");
        assert_eq!(hint_text(hints(Picker)), "j/k or ↑/↓ move · Enter confirm · Esc cancel");
        assert_eq!(hint_text(hints(Browse)), "j/k move · ⌘K go to · ⌘F search · ? help");
        assert_eq!(hint_text(hints(Selected)), "j/k move · t transition · a assign · s sprint · e edit · l label · J/K rank · c copy · ⌘O open · ? help");
    }

    #[test]
    fn hint_bar_elides_from_the_right() {
        assert_eq!(fit_hints(&[10.0, 10.0, 10.0], 5.0, 4.0, 40.0), (3, false), "exactly fits");
        assert_eq!(fit_hints(&[10.0, 10.0, 10.0], 5.0, 4.0, 39.0), (2, true), "two plus separator and ellipsis");
        assert_eq!(fit_hints(&[10.0, 10.0, 10.0], 5.0, 4.0, 18.0), (0, true));
        assert_eq!(fit_hints(&[], 5.0, 4.0, 0.0), (0, false));
    }

    #[test]
    fn notation_is_slash_pairs() {
        assert!(!BINDINGS.iter().any(|k| k.keys.contains("↓/↑")), "up before down everywhere");
        for k in BINDINGS {
            assert!(!k.keys.contains("j, k") && !k.keys.contains("J, K") && !k.keys.contains("↑, ↓"), "{}", k.keys);
        }
    }
}
