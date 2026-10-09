//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{outline_pill, outline_pill_selected, pill_outlined, small_link, toggle_chip};
use egui::{CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::collections::BTreeSet;

/// When the epic hue slots are given (`track_epics`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotPhase {
    /// The sprint scope is still resolving (the first rows are the all-sprints flash): slots are worked out
    /// afresh each frame and nothing is kept.
    Provisional,
    /// The scope has just resolved: drop the provisional slots and give them from these rows.
    Reset,
    /// Kept for the session; only new epics take slots.
    Keep,
}

/// The slot phase: provisional until the board's sprints answer (or there is no client to ask), then one
/// reset, then kept.
pub fn slot_phase(scope_resolved: bool, already_final: bool) -> SlotPhase {
    match (scope_resolved, already_final) {
        (false, _) => SlotPhase::Provisional,
        (true, false) => SlotPhase::Reset,
        (true, true) => SlotPhase::Keep,
    }
}

/// Gives hue slots to the epics on screen first (row order, then the selected issue's parent), then to the
/// rest of the loaded epic list (see `theme::assign_epic_slots`), once the sprint scope has resolved
/// (`slot_phase`).
pub fn track_epics(app: &mut App) {
    match slot_phase(app.sprints_answered || app.client.is_none(), app.epic_slots_final) {
        SlotPhase::Provisional => app.epic_slots.clear(),
        SlotPhase::Reset => {
            app.epic_slots.clear();
            app.epic_slots_final = true;
        }
        SlotPhase::Keep => {}
    }
    let selected = app.selected.as_ref().and_then(|k| app.store.get(k)).and_then(|i| i.parent_key.clone());
    let visible: Vec<String> = current_rows(app).iter().filter_map(|i| i.parent_key.clone()).chain(selected).collect();
    let rest: Vec<String> = app.store.list(ListId::Epics).iter().map(|e| e.key.clone()).collect();
    theme::assign_epic_slots(&mut app.epic_slots, visible.iter().map(String::as_str), rest.iter().map(String::as_str));
}

/// Keys of the loaded epics labelled as roadmap initiatives (cheap: a few dozen epics).
pub fn roadmap_epics(app: &App) -> BTreeSet<String> {
    filter::roadmap_epic_keys(&app.store.list(ListId::Epics), &app.cfg.roadmap.initiative_label)
}

/// How many rows the list would show without the local text filter (for "3 of 20 issues").
pub fn rows_without_text_filter(app: &App) -> usize {
    let src = app.rows_list();
    let all = filter::action_items(&app.store.list(src));
    let me = app.me.as_ref().map(|m| m.display_name.as_str());
    let roadmap_epics = if app.filter.roadmap { roadmap_epics(app) } else { Default::default() };
    let f = filter::IssueFilter { text: String::new(), roadmap_epics, ..app.filter.clone() };
    filter::visible(&all, &f, src == ListId::Search, me).len()
}

/// The rows on screen are the server text search for exactly what the box holds.
pub fn text_rows_shown(app: &App) -> bool {
    text_rows_match(app.rows_list() == ListId::Search, app.text_shown.as_deref(), &app.filter.text)
}

/// The shown Search rows answer a text search for `text` (trimmed): the server matched the term over summary,
/// description and comments, so the local text filter (key, summary, labels, assignee) must not narrow them.
/// Rows for an older term still get the local filter until the new term's result lands.
pub fn text_rows_match(showing_search: bool, text_shown: Option<&str>, text: &str) -> bool {
    showing_search && text_shown.is_some_and(|t| t == text.trim())
}

pub fn current_rows(app: &App) -> Vec<&Issue> {
    let src = app.rows_list();
    let searched = src == ListId::Search;
    let all = filter::action_items(&app.store.list(src));
    let me = app.me.as_ref().map(|m| m.display_name.as_str());
    let text = if text_rows_shown(app) { String::new() } else { app.filter.text.clone() };
    let roadmap_epics = if app.filter.roadmap { roadmap_epics(app) } else { Default::default() };
    let f = filter::IssueFilter { text, roadmap_epics, ..app.filter.clone() };
    filter::visible(&all, &f, searched, me)
}

/// How a row pill is drawn: the filled status tint, an epic's outline in its hue, or a neutral outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PillLook {
    Tint(egui::Color32, egui::Color32),
    /// Outline only: (text, edge), the hue's text and stripe colours; no fill, so only status is filled.
    Epic(egui::Color32, egui::Color32),
    Outline,
}

/// The row's team pill, neutral like the labels it sits with: under All teams, its team labels; under one
/// team, "no <label> label" when it is the team's by membership only.
pub fn team_pill(f: &filter::IssueFilter, i: &Issue) -> Option<String> {
    match &f.team {
        filter::TeamScope::All => {
            let teams: Vec<&str> = i.labels.iter().filter(|l| f.team_labels.contains(l)).map(String::as_str).collect();
            (!teams.is_empty()).then(|| teams.join(" · "))
        }
        filter::TeamScope::Label(l) => {
            let members = f.team_members.get(l.as_str()).map(Vec::as_slice).unwrap_or_default();
            filter::missing_team_label(i, l, members).then(|| format!("no {l} label"))
        }
        filter::TeamScope::None => None,
    }
}

/// The labels pill's text, without the label the team filter already names (every row would repeat it).
pub fn row_labels(labels: &[String], team_filter: Option<&str>) -> String {
    labels.iter().filter(|l| Some(l.as_str()) != team_filter).cloned().collect::<Vec<_>>().join(", ")
}

/// What a row pill is, for its place in the row and for deciding what gives way when the row is narrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillKind {
    Status,
    Epic,
    /// Labels, team, type, priority: neutral values.
    Other,
    Assignee,
}

/// One order for every row: status, epic, labels and other neutral values, assignee (a stable sort keeps
/// the configured column order within a kind).
pub fn pill_rank(k: PillKind) -> u8 {
    match k {
        PillKind::Status => 0,
        PillKind::Epic => 1,
        PillKind::Other => 2,
        PillKind::Assignee => 3,
    }
}

/// An epic pill narrower than this is dropped rather than cut to a stub.
const EPIC_PILL_MIN: f32 = 64.0;

/// The assignee's short form when the row is tight: the first name.
pub fn first_name(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// Fits a row's pills on one line: `pills` are (kind, full width, short width; the short form only differs
/// for the assignee, its first name); each gets Some(width) or None (dropped). What gives way, in order: the
/// epic is cut to the room left, down to EPIC_PILL_MIN; then the assignee shows its first name; then it is
/// dropped; only then the epic. At each step the epic gets whatever room there is.
pub fn fit_pills(pills: &[(PillKind, f32, f32)], available: f32, spacing: f32) -> Vec<Option<f32>> {
    let total = |ws: &[Option<f32>]| {
        let n = ws.iter().flatten().count();
        ws.iter().flatten().sum::<f32>() + spacing * n.saturating_sub(1) as f32
    };
    let epic = pills.iter().position(|(k, _, _)| *k == PillKind::Epic);
    let mut out = Vec::new();
    for stage in 0..3 {
        out = pills
            .iter()
            .map(|&(k, full, short)| match (k, stage) {
                (PillKind::Assignee, 1) => Some(short),
                (PillKind::Assignee, 2) => None,
                _ => Some(full),
            })
            .collect();
        if total(&out) <= available {
            return out;
        }
        if let Some(e) = epic {
            out[e] = None;
            let room = available - total(&out) - if out.iter().flatten().count() > 0 { spacing } else { 0.0 };
            if room >= EPIC_PILL_MIN {
                out[e] = Some(room.min(pills[e].1).floor());
                return out;
            }
        }
    }
    out
}

/// An epic name shortened to `max_chars` (counting the "…"): whole when it fits; else without a trailing
/// parenthetical ("Cluster (on-prem)" → "Cluster"); else cut at a word boundary with "…"; a single word
/// too long for the room is cut mid-word only as a last resort.
pub fn shorten_epic(name: &str, max_chars: usize) -> String {
    let len = |t: &str| t.chars().count();
    if len(name) <= max_chars {
        return name.to_string();
    }
    let base = match name.trim_end().strip_suffix(')').and_then(|t| t.rfind(" (").map(|i| &name[..i])) {
        Some(b) if !b.trim().is_empty() => b.trim_end(),
        _ => name,
    };
    if len(base) <= max_chars {
        return base.to_string();
    }
    let room = max_chars.saturating_sub(1);
    let mut out = String::new();
    for w in base.split_whitespace() {
        let next = if out.is_empty() { w.to_string() } else { format!("{out} {w}") };
        if len(&next) > room {
            break;
        }
        out = next;
    }
    // Never end on a connector or punctuation ("Cost Insights &…").
    while let Some(last) = out.split_whitespace().last() {
        if last.chars().all(|c| c.is_alphanumeric()) && !["and", "or", "of", "for", "the", "to", "a"].contains(&last.to_lowercase().as_str()) {
            break;
        }
        out = out[..out.len() - last.len()].trim_end().to_string();
    }
    if out.is_empty() {
        out = base.chars().take(room).collect();
    }
    format!("{}…", out.trim_end())
}

/// The longest `shorten_epic` form that fits `max_w` by `measure`.
pub fn fit_epic(name: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    let n = name.chars().count();
    (1..=n).rev().map(|c| shorten_epic(name, c)).find(|t| measure(t) <= max_w).unwrap_or_else(|| "…".to_string())
}

/// The key column's width: the widest key in the list, so every summary starts on one edge.
pub fn key_column_width(widths: impl IntoIterator<Item = f32>) -> f32 {
    widths.into_iter().fold(0.0, f32::max).ceil()
}

/// An epic pill's tooltip: its key and name, so two epics with one name stay apart.
pub fn epic_tip(key: &str, name: &str) -> String {
    if name.is_empty() { key.to_string() } else { format!("{key} · {name}") }
}

/// The neutral "no <label> label" pill; true when it was shown.
pub fn missing_label_pill(ui: &mut Ui, f: &filter::IssueFilter, i: &Issue, label: &str, p: &Palette) -> bool {
    let members = f.team_members.get(label).map(Vec::as_slice).unwrap_or_default();
    let missing = filter::missing_team_label(i, label, members);
    if missing {
        outline_pill(ui, &format!("no {label} label"), p, crate::views::PILL_MARGIN);
    }
    missing
}

/// The hint bar's height; the toast anchors above it.
pub const HINT_BAR_H: f32 = 24.0;
/// The hint bar's inner margin: the first token is placed by its ink, so this is the ink inset.
const HINT_INSET: f32 = 16.0;

/// The hint bar's fill: `surface_2` in both themes (a step off the white light surface, so the undimmed bar
/// under a modal does not glow next to the grey backdrop).
pub fn hint_bar_fill(p: &Palette) -> egui::Color32 {
    p.surface_2
}

/// The hint bar, on every signed-in screen. Drawn before the screen's panels, so the strip spans the full
/// width under them.
pub fn hints_panel(app: &App, ui: &mut Ui, p: &Palette) {
    let placeholder = app.screen != crate::app::Screen::Issues;
    let edit = app.picker.as_ref().map(|pk| match pk {
        crate::app::Picker::Edit { .. } => crate::keys::PickerHint::Edit,
        crate::app::Picker::Assign { .. } => crate::keys::PickerHint::Assign,
        _ => crate::keys::PickerHint::Other,
    });
    let focus = text_focus(ui, app);
    let c = crate::keys::hint_context(crate::keys::HintState {
        help_open: app.help_open,
        goto_open: app.goto.is_some(),
        picker: edit,
        has_selection: app.selected.is_some(),
        focus,
        search_shown: esc_clears_search(app.search_active, app.typed_search, app.text_search.is_some()),
        read_only: super::detail::composer_read_only(),
        placeholder,
    });
    hint_strip(ui, crate::keys::hints(c), p);
}

/// The hint bar on the sign-in form: "⏎ sign in".
pub fn sign_in_hints(ui: &mut Ui, p: &Palette) {
    hint_strip(ui, crate::keys::hints(crate::keys::HintContext::SignIn), p);
}

/// The bottom strip itself: `surface_2`, a 1 px top rule, the hints placed by their ink.
fn hint_strip(ui: &mut Ui, hints: &[crate::keys::Hint], p: &Palette) {
    let bar = egui::Panel::bottom("hints")
        .exact_size(HINT_BAR_H)
        .frame(Frame::new().fill(hint_bar_fill(p)).inner_margin(Margin::symmetric(HINT_INSET as i8, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| hint_bar(ui, hints, p));
        });
    // A top rule only, like the header's bottom rule.
    let r = bar.response.rect;
    ui.painter().hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, p.border));
}

/// Both panes' top inset under the header rule.
pub const PANE_TOP: f32 = 12.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    track_epics(app);
    let (list_w, list_range) = list_pane_width(ui.available_width());
    egui::Panel::left("issue-list")
        .resizable(true)
        .default_size(list_w)
        .size_range(list_range)
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            // The filter block's gaps are explicit (all V_GAP), so no implicit item spacing is added on top.
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.scope(|ui| filters(app, ui, p));
            ui.add_space(SEARCH_TO_COUNT);
            list(app, ui, p);
        });
    // The toast centres on the whole pane as laid out, not on the panel's response (which grows past the
    // window when content overflows it).
    let pane = ui.available_rect_before_wrap();
    app.detail_x = Some((pane.left(), pane.right()));
    egui::CentralPanel::default()
        // The same 12 top inset as the list pane, so the detail buttons' row is level with the combo row.
        // No right margin: the detail pane's scrollbar sits in it, and `detail::show` insets the content by
        // DETAIL_RIGHT (≥ the bar's width + 4), so the bar never covers content.
        .frame(Frame::new().fill(p.surface).inner_margin(Margin { left: 16, right: 0, top: PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| super::detail::show(app, ui));
    goto_box(app, ui.ctx(), p);
}

/// Which text field holds the keyboard, for the hint bar.
fn text_focus(ui: &Ui, app: &App) -> crate::keys::Focus {
    use crate::keys::Focus;
    let has = |id: egui::Id| ui.memory(|m| m.has_focus(id));
    if has(egui::Id::new("issue-search")) {
        let t = app.filter.text.trim();
        Focus::Search {
            clear: search_esc(!app.filter.text.is_empty(), app.search_active) == SearchEsc::Clear,
            plain: !t.is_empty() && !filter::looks_like_jql(t),
        }
    } else if app.selected.as_deref().is_some_and(|k| has(super::detail::composer_id(k))) {
        Focus::Composer
    } else if ui.ctx().text_edit_focused() {
        Focus::OtherText
    } else {
        Focus::None
    }
}

/// Below this window width the list pane takes a 45% share, held back so the description keeps 400 px.
const NARROW: f32 = 1200.0;
/// What the detail pane needs beside the list below NARROW: its 16 + 16 margins, a 400 px description
/// column, the 16 px gutter and the 140 px fields column (`detail::detail_columns`).
const NARROW_DETAIL_MIN: f32 = 32.0 + 400.0 + 16.0 + 140.0;

/// The list pane's default width and the range it may be dragged in: 42% of the window (360–720) when
/// wide; below NARROW 45% of the window, but never so wide that the description column drops under 400.
pub fn list_pane_width(window_w: f32) -> (f32, std::ops::RangeInclusive<f32>) {
    if window_w < NARROW {
        let w = (window_w * 0.45).min(window_w - NARROW_DETAIL_MIN).max(320.0).floor();
        (w, 320.0..=w)
    } else {
        let w = (window_w * 0.42).clamp(360.0, 720.0);
        (w, 320.0..=w)
    }
}

/// Gap between filter controls in a row.
const GAP: f32 = 8.0;
/// Vertical gap between the filter block's rows (combos, chips, search, count).
const V_GAP: f32 = 12.0;
/// One text inset for combos, chips (CHIP_PAD) and the search box's placeholder (`input_field`).
const INNER_X: f32 = 12.0;
/// Text → chevron in a combo.
const CHEVRON_GAP: f32 = 8.0;
/// egui's chevron slot.
const CHEVRON_W: f32 = 14.0;
/// Room a combo needs beside its text: the inset either side, the chevron and its gap (egui truncates the
/// text at that width), plus 2 px so rounding never cuts the last glyph.
const COMBO_CHROME: f32 = 2.0 * INNER_X + CHEVRON_GAP + CHEVRON_W + 2.0;

/// Narrowest the epic combo gets before the team combo gives way: "All epics" whole with the combo chrome.
const EPIC_MIN: f32 = 108.0;
/// Narrowest the team combo is cut to when even its short labels do not fit: "PLAT" whole with the chrome.
const TEAM_MIN: f32 = 72.0;
/// Sizes the sprint combo before the board's sprints load, so its width does not change when they land.
const SPRINT_SAMPLE: &str = "Sprint 0 (active)";
/// The sprint combo's label until the board's sprints answer (muted): the scope that will apply.
pub const SPRINT_PENDING: &str = "Active sprint";
/// Sizes the epic combo: a typical key and a 28-character epic name. Longer names are cut with "…" (the
/// popup shows them whole), so the combo is as wide before the epics load as after.
const EPIC_SAMPLE: &str = "CI-0000 · Observability and dashboards";

/// The filter combos whose width is fixed by samples, never by loaded data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ComboKind {
    Sprint,
    Epic,
}

/// The texts a combo's natural width is measured from: constants only, so nothing the board or the issues
/// load can move the row (round-6 D1).
pub fn combo_samples(kind: ComboKind) -> &'static [&'static str] {
    match kind {
        ComboKind::Sprint => &[SPRINT_SAMPLE, "All sprints", SPRINT_PENDING, "Backlog"],
        ComboKind::Epic => &["All epics", EPIC_SAMPLE],
    }
}

/// A combo's natural width (text + chrome) from its samples alone, so it is identical before and after load.
pub fn combo_width(kind: ComboKind, measure: impl Fn(&str) -> f32) -> f32 {
    (combo_samples(kind).iter().map(|t| measure(t)).fold(0.0, f32::max) + COMBO_CHROME).ceil()
}

/// The sprint combo's text and whether it is muted: until the board's sprints answer, the default scope is
/// still unknown, so the combo reads a muted "Active sprint" instead of "All sprints" flipping to the sprint.
pub fn sprint_combo_text(answered: bool, label: &str) -> (String, bool) {
    if answered { (label.to_string(), false) } else { (SPRINT_PENDING.to_string(), true) }
}

/// The combo widths, in row order team, sprint, epic, and whether the team combo uses its short labels.
/// One rule for all three: each is its widest option (`*_nat`, text + chevron), never the current label, so
/// nothing moves when a selection changes; only the search box stretches. The one exception: in its short form
/// the team combo is its current short label's width (`team_short_nat`). When the row is short, sprint is
/// never cut; epic gives way first, down to EPIC_MIN; then team falls back to its short labels, then is cut.
pub fn combo_widths(team_full_nat: f32, team_short_nat: f32, sprint_nat: f32, epic_nat: f32, available: f32) -> ([f32; 3], bool) {
    let room = (available - 2.0 * GAP).max(0.0);
    let sprint = sprint_nat.min(room).ceil();
    let epic_nat = epic_nat.ceil();
    for (team, short) in [(team_full_nat.ceil(), false), (team_short_nat.ceil(), true)] {
        let left = room - sprint - team;
        if left >= EPIC_MIN.min(epic_nat) {
            // Cut to the room left, exactly, so the row ends flush with the search box.
            return ([team, sprint, epic_nat.min(left)], short);
        }
    }
    let team = (room - sprint - EPIC_MIN).max(TEAM_MIN).floor();
    let epic = (room - sprint - team).max(0.0).min(epic_nat);
    ([team, sprint, epic], true)
}

/// A combo popup's outer width, one rule for all three: the combo's width, or the widest option plus its
/// row padding either side (`row_pad`), the popup frame and 2 px of rounding room when that is wider;
/// never past `cap`, the list pane's width.
pub fn popup_width(combo_w: f32, widest_text: f32, row_pad: f32, cap: f32) -> f32 {
    (widest_text + 2.0 * row_pad + POPUP_FRAME + 2.0).ceil().max(combo_w).min(cap.max(combo_w))
}

/// A popup's left edge: under its combo, shifted left only as far as it must to end at the pane's right
/// edge, and never past the pane's left edge.
pub fn popup_left(combo_left: f32, popup_w: f32, pane_left: f32, pane_right: f32) -> f32 {
    combo_left.min(pane_right - popup_w).max(pane_left)
}

/// The epic popup's left edge: its right edge on the combo's right edge, growing leftward only as far as its
/// content needs (`popup_width`), never past the pane's left edge.
pub fn popup_left_end(combo_right: f32, popup_w: f32, pane_left: f32) -> f32 {
    (combo_right - popup_w).max(pane_left)
}

/// Which edge of its combo a popup lines up with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupEdge {
    Start,
    End,
}

/// A popup frame's side margin, so a selected row's fill sits 4 in from the popup edge (margin 3 + stroke 1).
const POPUP_MARGIN: i8 = 3;
/// A popup frame's top and bottom margin: 4 inside the stroke (round 8 measured 3).
const POPUP_MARGIN_Y: i8 = 4;
/// A popup frame's margin and stroke, both sides.
const POPUP_FRAME: f32 = 2.0 * (POPUP_MARGIN as f32 + 1.0);
/// The popup opens this far below its combo, so its top edge never sits on the combo's bottom edge.
const POPUP_GAP: f32 = 4.0;
/// One popup row: the combo's own height.
const POPUP_ROW_H: f32 = crate::views::BUTTON_H;
/// Rows a popup shows before it scrolls.
const POPUP_MAX_ROWS: usize = 8;

/// A popup list's scroll height and whether its scrollbar shows: whole rows only (never a row cut mid-text),
/// at most POPUP_MAX_ROWS; when there are more, the bar is shown for as long as the popup is open.
pub fn popup_scroll(n_rows: usize) -> (f32, bool) {
    (n_rows.clamp(1, POPUP_MAX_ROWS) as f32 * POPUP_ROW_H, n_rows > POPUP_MAX_ROWS)
}
/// A popup row's text inset: with the frame's 4 it starts at INNER_X, where the combo's own text does.
const POPUP_ROW_PAD: f32 = INNER_X - POPUP_FRAME / 2.0;

/// The team combo's label: the full "PLAT · Platform" when it fits, else just "PLAT".
pub fn team_combo_label(full: &str, fits: bool) -> &str {
    if fits { full } else { full.split(" · ").next().unwrap_or(full) }
}

/// A combo exactly `w` wide and BUTTON_H tall: its text at INNER_X, truncated with "…", a muted chevron at
/// the right. Its popup follows `popup_width` from the options' widest text (`widest`, paired with `w`) and `popup_left`
/// within the pane (`pane`, left and right); `contents` gets the room a row's text has.
#[allow(clippy::too_many_arguments)]
fn combo(
    ui: &mut Ui,
    salt: &str,
    (text, muted): (String, bool),
    (w, widest): (f32, f32),
    n_rows: usize,
    pane: (f32, f32),
    edge: PopupEdge,
    p: &Palette,
    contents: impl FnOnce(&mut Ui, f32),
) {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, crate::views::BUTTON_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, &text));
    let popup_id = egui::Id::new(("combo-popup", salt));
    if ui.is_rect_visible(rect) {
        let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let v = if open { ui.visuals().widgets.open } else { *ui.style().interact(&resp) };
        ui.painter().rect(rect, f32::from(theme::RADIUS), v.weak_bg_fill, v.bg_stroke, egui::StrokeKind::Inside);
        let font = theme::body(14.0);
        let painter = ui.painter().clone();
        let measure = |t: &str| painter.layout_no_wrap(t.to_string(), font.clone(), p.text).size().x;
        let shown = crate::views::truncate_text(&text, w - COMBO_CHROME, measure);
        let color = if muted { p.muted } else { p.text };
        let g = painter.layout_no_wrap(shown, font.clone(), color);
        painter.galley(egui::pos2(rect.left() + INNER_X, (rect.center().y - g.size().y / 2.0).round()), g, color);
        let c = egui::pos2(rect.right() - INNER_X - CHEVRON_W / 2.0, rect.center().y);
        let pts = vec![c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)];
        painter.add(egui::Shape::line(pts, Stroke::new(1.5, p.muted)));
    }
    let pop_w = popup_width(w, widest, POPUP_ROW_PAD, pane.1 - pane.0);
    let left = match edge {
        PopupEdge::Start => popup_left(rect.left(), pop_w, pane.0, pane.1),
        PopupEdge::End => popup_left_end(rect.right(), pop_w, pane.0),
    };
    let anchor = Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(left + pop_w, rect.bottom()));
    let inner = pop_w - POPUP_FRAME;
    let (scroll_h, bar) = popup_scroll(n_rows);
    egui::Popup::menu(&resp)
        .id(popup_id)
        .frame(Frame::menu(ui.style()).inner_margin(Margin::symmetric(POPUP_MARGIN, POPUP_MARGIN_Y)))
        .gap(POPUP_GAP)
        .anchor(anchor)
        .align(egui::RectAlign::BOTTOM_START)
        .align_alternatives(&[egui::RectAlign::TOP_START])
        .width(inner)
        .show(|ui| {
            ui.set_min_width(inner);
            ui.set_max_width(inner);
            ui.spacing_mut().button_padding.x = POPUP_ROW_PAD;
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.spacing_mut().interact_size.y = POPUP_ROW_H;
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            let visibility = if bar {
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                // The solid thumb paints in the widgets' `bg_fill` (rows use `weak_bg_fill`, so they keep theirs).
                crate::views::set_thumb_fills(ui, p);
                egui::scroll_area::ScrollBarVisibility::AlwaysVisible
            } else {
                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
            };
            let row_w = inner - 2.0 * POPUP_ROW_PAD - if bar { ui.spacing().scroll.allocated_width() } else { 0.0 };
            egui::ScrollArea::vertical().max_height(scroll_h).scroll_bar_visibility(visibility).show(ui, |ui| contents(ui, row_w));
        });
}

/// The search box's placeholder: it filters, searches, and runs JQL on Enter.
pub const SEARCH_HINT: &str = "Filter or search, Enter for JQL (⌘F)";

/// What Enter in the search box runs as typed JQL: whatever it holds, plain text included (the hint says
/// "Enter run as JQL"); the text stays in the box.
pub fn enter_jql(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// What Esc does in the search box: first it clears the text and any search, then it blurs.
#[derive(Debug, PartialEq, Eq)]
pub enum SearchEsc {
    Clear,
    Blur,
}

pub fn search_esc(has_text: bool, search_active: bool) -> SearchEsc {
    if has_text || search_active { SearchEsc::Clear } else { SearchEsc::Blur }
}

/// With the search box unfocused, Esc still clears a server search (typed JQL or a text search) on screen;
/// a quick filter's search is not the box's and stays.
pub fn esc_clears_search(search_active: bool, typed: bool, text: bool) -> bool {
    search_active && (typed || text)
}

/// The text search is waiting out its debounce: the box holds a term the active text search is not for yet
/// (and that Enter has not already run as JQL, see `typed_for`).
pub fn text_debouncing(term: Option<&str>, text_search: Option<&str>, typed: bool) -> bool {
    !typed && term.is_some_and(|t| text_search != Some(t))
}

/// The active typed search is for exactly `term` (Enter ran the box's plain text as JQL), so no text search
/// replaces it.
pub fn typed_for(typed_search: bool, last_jql: Option<&str>, term: &str) -> bool {
    typed_search && last_jql == Some(term)
}

fn filters(app: &mut App, ui: &mut Ui, p: &Palette) {
    ui.spacing_mut().button_padding = vec2(INNER_X, 4.0);
    ui.spacing_mut().icon_spacing = CHEVRON_GAP;
    ui.spacing_mut().icon_width = CHEVRON_W;
    ui.spacing_mut().interact_size.y = crate::views::BUTTON_H;
    let pane_w = ui.available_width();
    let pane = (ui.max_rect().left(), ui.max_rect().right());
    let text_w = |ui: &Ui, t: &str| ui.painter().layout_no_wrap(t.to_string(), theme::body(14.0), p.text).size().x;
    // Team scope: All, each configured sub-team, or tickets with no team label.
    let team_options = filter::team_options(&app.cfg.scope.teams);
    let team_label = team_options.iter().find(|(t, _)| *t == app.filter.team).map_or_else(
        || match &app.filter.team {
            filter::TeamScope::Label(l) => l.clone(),
            _ => "All teams".to_string(),
        },
        |(_, l)| l.clone(),
    );
    // Epic picker: parents of the scoped team's issues (other filters ignored), by summary.
    let src = app.rows_list();
    let mut epics: Vec<(String, String)> = filter::in_team(&app.store.list(src), &app.filter, src == ListId::Search)
        .iter()
        .filter_map(|i| Some((i.parent_key.clone()?, i.parent_summary.clone().unwrap_or_default())))
        .collect();
    epics.sort();
    epics.dedup();
    let on_roadmap = roadmap_epics(app);
    let entry = |k: &str, s: &str| format!("{k} · {s}");
    let suffix = |k: &str| if on_roadmap.contains(k) { " · roadmap" } else { "" };
    let epic_label = app.filter.epic.as_ref().and_then(|k| epics.iter().find(|(key, _)| key == k)).map(|(k, s)| entry(k, s)).unwrap_or("All epics".into());
    // Sprint scope: All, the board's active and future sprints, or Backlog.
    let sprint_options = crate::filter::sprint_options(&app.sprints);
    let sprint_label = sprint_options.iter().find(|(s, _)| *s == app.filter.sprint).map_or("All sprints".to_string(), |(_, l)| l.clone());

    let widest = |ui: &Ui, rows: &mut dyn Iterator<Item = String>| rows.map(|r| text_w(ui, &r)).fold(0.0, f32::max);
    let team_widest = widest(ui, &mut team_options.iter().map(|(_, l)| l.clone()));
    // Once the team combo falls back to its short label it is sized to that label ("PLAT"), not to the widest
    // short option, so a narrow row does not carry ~60 px of slack (B8).
    let team_short_nat = text_w(ui, team_combo_label(&team_label, false)) + COMBO_CHROME;
    let sprint_widest = widest(ui, &mut sprint_options.iter().map(|(_, l)| l.clone()));
    let epic_widest = widest(ui, &mut epics.iter().map(|(k, s)| format!("{}{}", entry(k, s), suffix(k))).chain(["All epics".to_string()]));
    // The combos' own widths come from constant samples, so nothing moves when sprints or issues land; the
    // popups still size to the loaded options.
    let sprint_nat = combo_width(ComboKind::Sprint, |t| text_w(ui, t));
    let epic_nat = combo_width(ComboKind::Epic, |t| text_w(ui, t));
    let ([team_w, sprint_w, epic_w], team_short) = combo_widths(team_widest + COMBO_CHROME, team_short_nat, sprint_nat, epic_nat, pane_w);
    let team_text = team_combo_label(&team_label, !team_short).to_string();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        combo(ui, "team", (team_text, false), (team_w, team_widest), team_options.len(), pane, PopupEdge::Start, p, |ui, _| {
            for (scope, l) in &team_options {
                if ui.selectable_label(app.filter.team == *scope, l).clicked() {
                    app.actions.push(Action::SetTeam(scope.clone()));
                }
            }
        });
        combo(
            ui,
            "sprint",
            sprint_combo_text(app.sprints_answered, &sprint_label),
            (sprint_w, sprint_widest),
            sprint_options.len(),
            pane,
            PopupEdge::Start,
            p,
            |ui, _| {
                for (scope, l) in &sprint_options {
                    if ui.selectable_label(app.filter.sprint == *scope, l).clicked() {
                        app.actions.push(Action::SetSprint(scope.clone()));
                    }
                }
            },
        );
        combo(ui, "epic", (epic_label, false), (epic_w, epic_widest), epics.len() + 1, pane, PopupEdge::End, p, |ui, row_text_w| {
            if ui.selectable_label(app.filter.epic.is_none(), "All epics").clicked() {
                app.actions.push(Action::SetEpic(None));
            }
            for (k, s) in &epics {
                // The roadmap suffix is muted: it qualifies the epic, it is not part of its name.
                let mut job = egui::text::LayoutJob::default();
                job.append(&entry(k, s), 0.0, egui::TextFormat { font_id: theme::body(14.0), color: p.text, ..Default::default() });
                job.append(suffix(k), 0.0, egui::TextFormat { font_id: theme::body(14.0), color: p.muted, ..Default::default() });
                job.wrap = egui::text::TextWrapping::truncate_at_width(row_text_w);
                let full = format!("{}{}", entry(k, s), suffix(k));
                if ui.selectable_label(app.filter.epic.as_deref() == Some(k), job).on_hover_text(full).clicked() {
                    app.actions.push(Action::SetEpic(Some(k.clone())));
                }
            }
        });
    });
    ui.add_space(V_GAP);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        let quick = app.cfg.issues.filters.clone();
        for (i, f) in quick.iter().enumerate() {
            let active = app.filter.quick == Some(i);
            if toggle_chip(ui, &crate::views::sentence_case(&f.name), active, p).clicked() {
                app.actions.push(Action::SetQuick(if active { None } else { Some(i) }));
            }
        }
        if toggle_chip(ui, "Mine", app.filter.mine, p).clicked() {
            app.actions.push(Action::ToggleMine);
        }
        if toggle_chip(ui, "Roadmap", app.filter.roadmap, p).on_hover_text("m").clicked() {
            app.actions.push(Action::ToggleRoadmap);
        }
    });
    ui.add_space(V_GAP);
    let id = egui::Id::new("issue-search");
    if ui.memory(|m| m.has_focus(id))
        && search_esc(!app.filter.text.is_empty(), app.search_active) == SearchEsc::Clear
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        app.filter.text.clear();
        if app.search_active {
            app.actions.push(Action::RunSearch(String::new()));
        }
    }
    let r = ui.add(crate::views::input_field(&mut app.filter.text, SEARCH_HINT, theme::body(14.0), p).id(id));
    if r.changed() {
        app.text_edited = Some(std::time::Instant::now());
    }
    if app.focus_search {
        r.request_focus();
        app.focus_search = false;
    }
    if r.lost_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter))
        && let Some(jql) = enter_jql(&app.filter.text)
    {
        app.actions.push(Action::RunTypedSearch(jql));
    }
}

/// The count row's text: a plain count, "3 of 20 issues" under a local text filter (`total` is the
/// count before it), "· JQL" for a typed search's rows, "· text" for a text search's, or (nothing to show
/// yet) a loading line.
pub fn count_label(shown: usize, total: Option<usize>, loading_first: bool, suffix: CountSuffix) -> String {
    if shown == 0 && loading_first {
        return "Loading issues…".to_string();
    }
    let noun = |n: usize| if n == 1 { "issue" } else { "issues" };
    let base = match total {
        Some(t) if t != shown => format!("{shown} of {t} {}", noun(t)),
        _ => format!("{shown} {}", noun(shown)),
    };
    match suffix {
        CountSuffix::None => base,
        CountSuffix::Jql => format!("{base} · JQL"),
        CountSuffix::Text => format!("{base} · text match"),
    }
}

/// What the count row says the rows are, after the count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CountSuffix {
    None,
    Jql,
    Text,
}

/// "· JQL" only when the rows on screen are an accepted typed-JQL result, "· text" when they are an accepted
/// text search's. In flight the count keeps the label of the rows it counts, verbatim, so a pending search
/// over plain rows adds no suffix and a pending quick filter over JQL or text rows keeps it until its own
/// result lands.
pub fn count_suffix(typed_shown: bool, text_shown: bool, showing_search_result: bool) -> CountSuffix {
    match (showing_search_result, typed_shown, text_shown) {
        (false, _, _) => CountSuffix::None,
        (true, true, _) => CountSuffix::Jql,
        (true, false, true) => CountSuffix::Text,
        (true, false, false) => CountSuffix::None,
    }
}

/// Nothing on screen yet while the list (or a search) loads: the count row shows "⟳ Loading issues…".
pub fn first_load(app: &App) -> bool {
    let pending = app.search_pending.is_some();
    let loading = app.lists.get(&app.rows_list()).is_some_and(|l| l.loading);
    (pending || loading) && current_rows(app).is_empty()
}

/// The count's colour: `text` when it is current, `muted` while a request will replace it.
pub fn count_color(updating: bool, p: &Palette) -> egui::Color32 {
    if updating { p.muted } else { p.text }
}

/// The clear control shows whenever a row filter is on: a quick filter, Mine, Roadmap, or anything in the
/// search box (typed JQL or a local text filter).
pub fn clear_visible(quick: bool, mine: bool, roadmap: bool, box_text: bool) -> bool {
    quick || mine || roadmap || box_text
}

/// "Clear search" while the search box holds text (JQL or a local filter); "Clear filters" otherwise.
pub fn clear_label(box_text: bool) -> &'static str {
    if box_text { "Clear search" } else { "Clear filters" }
}

fn list(app: &mut App, ui: &mut Ui, p: &Palette) {
    let rows: Vec<Issue> = current_rows(app).into_iter().cloned().collect();
    let src = app.rows_list();
    let loaded = app.lists.get(&src);
    if let Some(e) = loaded.and_then(|l| l.error.clone())
        && rows.is_empty()
        && super::error_banner(ui, p, "Couldn't load issues.", &e)
    {
        let retry = match (&app.last_search_jql, app.search_active) {
            (Some(jql), true) if app.typed_search => Action::RunTypedSearch(jql.clone()),
            (Some(_), true) if app.text_search.is_some() => Action::RunTextSearch(app.text_search.clone().unwrap_or_default()),
            (Some(jql), true) => Action::RunSearch(jql.clone()),
            _ => Action::Refresh,
        };
        app.actions.push(retry);
    }
    // One fixed height in every state, so the list below never moves.
    let pending = app.search_pending.is_some();
    let updating = pending || loaded.is_some_and(|l| l.loading);
    let loading_first = updating && rows.is_empty();
    // One spinner per operation (`spinner_slots`): the header for a refresh, the count row for a first load or
    // a search started here.
    let syncing = crate::views::header_syncing(&app.lists);
    // From the first keystroke of a term, not only once its debounced search is sent.
    let term = filter::text_search_term(&app.filter.text);
    let typed = term.is_some_and(|t| typed_for(app.typed_search, app.last_search_jql.as_deref(), t));
    let debouncing = text_debouncing(term, app.text_search.as_deref(), typed);
    let row_spinner = crate::views::spinner_slots(syncing, pending, app.search_from_refresh, loading_first).1 || debouncing;
    let text_filter = !app.filter.text.trim().is_empty() && !filter::looks_like_jql(&app.filter.text) && !text_rows_shown(app);
    let total = text_filter.then(|| rows_without_text_filter(app));
    let showing_search = src == ListId::Search && app.store.lists.contains_key(&ListId::Search);
    let label = count_label(rows.len(), total, loading_first, count_suffix(app.typed_shown, app.text_shown.is_some(), showing_search));
    let box_text = !app.filter.text.trim().is_empty() || (app.typed_search && app.search_active);
    let show_clear = clear_visible(app.filter.quick.is_some(), app.filter.mine, app.filter.roadmap, box_text);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), COUNT_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(COUNT_H);
        // 7 in code reads as 8 ink to ink: the 12 px text's side bearing adds about 1.
        ui.spacing_mut().item_spacing.x = COUNT_GAP;
        // One pattern, muted 12, the spinner before the verb: "[count] ⟳ Updating…", or "⟳ Loading issues…"
        // when there is nothing to count yet.
        // Muted only beside the inline "Updating…": a header sync (⌘R) leaves the count at full `text`.
        let color = if loading_first { p.muted } else { count_color(row_spinner, p) };
        if loading_first {
            if row_spinner {
                crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            }
            ui.label(RichText::new(label).color(color).size(12.0));
        } else {
            ui.label(RichText::new(label).color(color).size(12.0));
            if row_spinner {
                crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                ui.label(RichText::new("Updating…").color(p.muted).size(12.0));
            }
        }
        if show_clear {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_link(ui, clear_label(box_text), p).clicked() {
                    app.actions.push(Action::ClearFilters);
                }
            });
        }
    });
    ui.add_space(COUNT_TO_ROWS);
    let columns = app.cfg.issues.columns.clone();
    let cfg = app.cfg.clone();
    let (status_colors, epic_colors) = (&cfg.issues.status_colors, &cfg.issues.epic_colors);
    let selected = app.selected.clone();
    let slots = app.epic_slots.clone();
    let team_filter = match &app.filter.team {
        filter::TeamScope::Label(l) => Some(l.clone()),
        _ => None,
    };
    // Keyboard moves change the cursor; bring its row into view once per change.
    let scroll_cursor = app.cursor != app.last_cursor;
    app.last_cursor = app.cursor;
    let key_w = {
        let painter = ui.painter();
        key_column_width(rows.iter().map(|i| painter.layout_no_wrap(i.key.clone(), theme::mono(12.5), p.muted).size().x))
    };
    // egui's own fade dims whatever sits near an edge; the list fades only a row that is really cut (`fade_strip`).
    ui.spacing_mut().scroll.fade = egui::style::ScrollFadeStyle { strength: 0.0, size: 0.0 };
    let mut row_spans: Vec<(f32, f32)> = Vec::with_capacity(rows.len());
    let mut selected_rect: Option<(Rect, usize)> = None;
    let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        for (idx, i) in rows.iter().enumerate() {
            let is_sel = selected.as_deref() == Some(&i.key);
            let epic = i.parent_key.as_deref().map(|k| theme::epic_tint_in(k, &slots, p, epic_colors));
            // The fill goes under the content once we know whether the pointer is over the row.
            let bg = ui.painter().add(egui::Shape::Noop);
            let corners = row_corners(idx, rows.len());
            // Tooltips for parts of the row, shown through the row's own response (it sits on top of them).
            let mut tips: Vec<(Rect, String)> = Vec::new();
            let r = Frame::new()
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(corners)
                .inner_margin(Margin { left: 12, right: 12, top: ROW_PAD_TOP, bottom: ROW_PAD_BOTTOM })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add_space(ROW_PAD_TOP_EXTRA);
                    ui.spacing_mut().item_spacing.y = ROW_LINE_GAP;
                    ui.horizontal(|ui| {
                        let key_color = row_key_color(is_sel, p);
                        let g = ui.painter().layout_no_wrap(i.key.clone(), theme::mono(12.5), key_color);
                        let (kr, _) = ui.allocate_exact_size(vec2(key_w, g.size().y), Sense::hover());
                        ui.painter().galley(kr.min, g, key_color);
                        let painter = ui.painter().clone();
                        let title = crate::views::truncate_title(&i.summary, ui.available_width(), |t| {
                            painter.layout_no_wrap(t.to_string(), theme::bold(13.5), p.text).size().x
                        });
                        let r = ui.add(egui::Label::new(RichText::new(&title).font(theme::bold(13.5)).color(p.text)).extend().selectable(false));
                        if title != i.summary {
                            tips.push((r.rect, i.summary.clone()));
                        }
                    });
                    // One line of pills, never wrapped: see `fit_pills`.
                    let mut pills: Vec<(PillKind, String, PillLook)> = Vec::new();
                    if let Some(t) = team_pill(&app.filter, i) {
                        pills.push((PillKind::Other, t, PillLook::Outline));
                    }
                    for c in &columns {
                        let v = match c.as_str() {
                            "labels" => row_labels(&i.labels, team_filter.as_deref()),
                            _ => filter::column_value(i, c),
                        };
                        if v.is_empty() {
                            continue;
                        }
                        let (kind, look) = match (c.as_str(), epic) {
                            ("status", _) => {
                                let t = theme::status_tint(&i.status.name, &i.status.category, p, status_colors);
                                (PillKind::Status, PillLook::Tint(status_fill(&t, p), t.fg))
                            }
                            ("parent.summary" | "parent.key", Some(t)) => (PillKind::Epic, PillLook::Epic(t.fg, t.stripe)),
                            ("parent.summary" | "parent.key", None) => (PillKind::Epic, PillLook::Outline),
                            ("assignee", _) => (PillKind::Assignee, PillLook::Outline),
                            _ => (PillKind::Other, PillLook::Outline),
                        };
                        pills.push((kind, v, look));
                    }
                    pills.sort_by_key(|(k, _, _)| pill_rank(*k));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(PILL_GAP, 4.0);
                        // The pill row starts on the key's ink, not its side bearing.
                        ui.add_space(KEY_INK_INSET);
                        let painter = ui.painter().clone();
                        let measure = |t: &str| painter.layout_no_wrap(t.to_string(), theme::body(12.0), p.text).size().x;
                        let pad = 2.0 * crate::views::ROW_PILL_MARGIN.x;
                        let short = |k: PillKind, t: &str| if k == PillKind::Assignee { first_name(t).to_string() } else { t.to_string() };
                        let widths: Vec<(PillKind, f32, f32)> = pills.iter().map(|(k, t, _)| (*k, measure(t) + pad, measure(&short(*k, t)) + pad)).collect();
                        let fit = fit_pills(&widths, ui.available_width(), PILL_GAP);
                        for (((kind, full, look), w), (_, full_w, _)) in pills.iter().zip(fit).zip(&widths) {
                            let Some(w) = w else { continue };
                            let source = if *kind == PillKind::Assignee && w < *full_w { short(*kind, full) } else { full.clone() };
                            let text = if *kind == PillKind::Epic {
                                fit_epic(&source, w - pad, measure)
                            } else {
                                crate::views::truncate_text(&source, w - pad, measure)
                            };
                            let r = match look {
                                PillLook::Tint(bg, fg) => pill_outlined(ui, &text, *bg, *fg),
                                PillLook::Epic(fg, edge) => {
                                    crate::views::pill_sized(ui, &text, egui::Color32::TRANSPARENT, *fg, Stroke::new(1.0, *edge), crate::views::ROW_PILL_MARGIN)
                                }
                                PillLook::Outline if is_sel => outline_pill_selected(ui, &text, p, crate::views::ROW_PILL_MARGIN),
                                PillLook::Outline => outline_pill(ui, &text, p, crate::views::ROW_PILL_MARGIN),
                            };
                            if let (PillKind::Epic, Some(k)) = (kind, &i.parent_key) {
                                tips.push((r.rect, epic_tip(k, i.parent_summary.as_deref().unwrap_or_default())));
                            } else if text != *full {
                                tips.push((r.rect, full.clone()));
                            }
                        }
                    });
                    ui.add_space(ROW_PAD_BOTTOM_EXTRA);
                });
            let rect = r.response.rect;
            row_spans.push((rect.top(), rect.bottom()));
            let mut r = ui.interact(rect, row_id(&i.key), Sense::click());
            if let Some(tip) = r.hover_pos().and_then(|at| tips.iter().find(|(t, _)| t.contains(at))).map(|(_, t)| t.clone()) {
                r = r.on_hover_text(tip);
            }
            let fill = row_fill(is_sel, r.hovered(), p);
            ui.painter().set(bg, egui::epaint::RectShape::filled(rect, corners, fill));
            if let Some(t) = epic {
                // Over the card's left border, on whole physical pixels, so no sliver of the page shows between
                // the stripe and the fill.
                let stripe = CornerRadius { nw: corners.nw, sw: corners.sw, ne: 0, se: 0 };
                ui.painter().rect_filled(stripe_rect(rect, ui.ctx().pixels_per_point()), stripe, t.stripe);
            }
            if is_sel {
                selected_rect = Some((rect, idx));
            }
            if r.clicked() {
                app.cursor = idx;
                app.last_cursor = idx;
                app.actions.push(Action::Select(i.key.clone()));
            }
            if r.double_clicked() {
                app.actions.push(Action::OpenUrl(app.cfg.browse_url(&i.key)));
            }
            if scroll_cursor && is_sel {
                r.scroll_to_me(None);
            }
        }
        // The dividers next to the selected row, over both fills (the next row's fill covers the lower one) in
        // the selected-row edge, so they never sink into the selection.
        if let Some((rect, idx)) = selected_rect {
            let edge = crate::views::selected_edge(p);
            for (y, inset) in selected_dividers(rect, idx, rows.len()) {
                ui.painter().hline(rect.x_range().shrink(inset), y, Stroke::new(1.0, edge));
            }
        }
        // Room under the last row, so a toast (anchored above the hint bar) never covers it.
        ui.add_space(LIST_BOTTOM_PAD);
    });
    let view = out.inner_rect;
    // More rows below the viewport: a 1 px rule at its bottom edge says so, without dimming any row.
    if more_below(&row_spans, view.bottom()) {
        ui.painter().hline(view.x_range(), view.bottom() - 0.5, Stroke::new(1.0, p.border));
    }
    let fade = p.bg.gamma_multiply(FADE_STRENGTH);
    for (strip, dir, colors) in [
        (fade_strip(&row_spans, view.bottom(), true), egui::epaint::Direction::BottomUp, [fade, egui::Color32::TRANSPARENT]),
        (fade_strip(&row_spans, view.top(), false), egui::epaint::Direction::TopDown, [fade, egui::Color32::TRANSPARENT]),
    ] {
        if let Some((top, bottom)) = strip {
            let r = Rect::from_x_y_ranges(view.x_range(), top..=bottom);
            ui.painter().add(egui::Shape::gradient_rect(r, dir, colors));
        }
    }
}

/// A row cut by more than this at a viewport edge fades; less, and it reads as whole.
const CLIP_MIN: f32 = 8.0;
/// The fade's height at most: a short gradient over the cut row's visible strip only.
const FADE_MAX: f32 = 24.0;
/// The fade's strongest alpha, the same in both themes.
const FADE_STRENGTH: f32 = 0.5;

/// The strip to fade at a viewport edge (`edge`, the bottom when `bottom`, else the top), as (top, bottom):
/// the visible part of the row the edge cuts, at most FADE_MAX tall at the edge, and only when the row is cut
/// by more than CLIP_MIN. `rows` are the rows' (top, bottom) in screen space.
pub fn fade_strip(rows: &[(f32, f32)], edge: f32, bottom: bool) -> Option<(f32, f32)> {
    let &(top, bot) = rows.iter().find(|(t, b)| *t < edge && edge < *b)?;
    if bottom { (bot - edge > CLIP_MIN).then(|| (top.max(edge - FADE_MAX), edge)) } else { (edge - top > CLIP_MIN).then(|| (edge, bot.min(edge + FADE_MAX))) }
}

/// Whether any row reaches past the viewport's bottom edge (by more than half a pixel).
pub fn more_below(rows: &[(f32, f32)], edge: f32) -> bool {
    rows.last().is_some_and(|(_, b)| *b > edge + 0.5)
}

/// Between the count row's parts.
const COUNT_GAP: f32 = 7.0;
/// The count row: its 12 px line box (the spinner and link fit), so the gaps around it are what they say:
/// 12 from the search box to the count's ink and 8 from its ink to the first card.
const COUNT_H: f32 = 16.0;
/// The count's 12 px text sits ~4 inside its 16 px line box at the top and the bottom (measured 16 / 12 with
/// 12 / 8 in code), so the code gaps are 8 and 4.
const COUNT_INK_INSET: f32 = 4.0;
/// Search box → count row (12 to the ink).
const SEARCH_TO_COUNT: f32 = V_GAP - COUNT_INK_INSET;
/// Count row → first card (8 from the ink).
const COUNT_TO_ROWS: f32 = 8.0 - COUNT_INK_INSET;
/// Space between a row's pills.
const PILL_GAP: f32 = 8.0;
/// The mono key's left side bearing at 12.5: the pill row moves in by it so pills and key share an ink edge.
const KEY_INK_INSET: f32 = 1.0;
/// Clearance under the last row: a toast's height (~34) plus its 16 px gap, rounded up.
const LIST_BOTTOM_PAD: f32 = 56.0;
/// Rows stack as one list: −1 overlaps neighbouring borders, so a single 1 px divider separates rows, and
/// only the list's outer corners are rounded (no doubled lines or notches at the seams).
const ROW_GAP: f32 = -1.0;
/// Row inner vertical padding, measured edge (border included) to ink: 6 / title / 4 / pills / 6 (round 8; it
/// measured 5.5 / 5 / 7.5 with pads 0 / 1 / 6). The title's line box carries ~4.5 of leading above the caps,
/// so the top pad is 0.5; the bottom read 1.5 over its pad, so it is 4.5. Margins are whole points, so the
/// half points are spaces inside the frame.
const ROW_PAD_TOP: i8 = 0;
const ROW_PAD_TOP_EXTRA: f32 = 0.5;
const ROW_PAD_BOTTOM: i8 = 4;
const ROW_PAD_BOTTOM_EXTRA: f32 = 0.5;
const ROW_LINE_GAP: f32 = 0.0;
/// One row's pitch (rows overlap by ROW_GAP's 1 px), for the density check: 44 − 2 from round 8's padding.
pub const ROW_PITCH: f32 = 42.0;

/// The 3 px epic stripe over a row card's left edge, snapped to whole physical pixels.
pub fn stripe_rect(row: Rect, ppp: f32) -> Rect {
    let snap = |v: f32| (v * ppp).round() / ppp;
    Rect::from_min_max(egui::pos2(snap(row.left()), row.top()), egui::pos2(snap(row.left() + 3.0), row.bottom()))
}

/// The selected row's two rules, as (centre y, inset from each end): its top and bottom dividers in the
/// selected-row edge, on the list frame too at the outer edges (inset by the corner radius).
pub fn selected_dividers(rect: Rect, idx: usize, n: usize) -> Vec<(f32, f32)> {
    // An outer edge (the first row's top, the last row's bottom) is the list frame's rounded border: the rule
    // goes over it too, stopping where the corners start.
    let inset = |outer: bool| if outer { f32::from(theme::RADIUS) } else { 0.0 };
    vec![(rect.top() + 0.5, inset(idx == 0)), (rect.bottom() - 0.5, inset(idx + 1 == n))]
}

/// A status pill's fill: its wash resolved over the plain row, so the pill is one opaque colour on every row
/// (a wash composited over the selection shifted its colour, round 8).
pub fn status_fill(t: &theme::Tint, p: &Palette) -> egui::Color32 {
    theme::over(t.bg, p.surface)
}

/// The list's outer corners are rounded (6); the seams between rows are square.
pub fn row_corners(idx: usize, n: usize) -> CornerRadius {
    let r = theme::RADIUS;
    let (top, bottom) = (if idx == 0 { r } else { 0 }, if idx + 1 == n { r } else { 0 });
    CornerRadius { nw: top, ne: top, sw: bottom, se: bottom }
}

fn row_id(key: &str) -> egui::Id {
    egui::Id::new(("issue-row", key))
}

/// One selection state: the selected row is filled; any other row fills only under the pointer.
/// The key's colour: `muted` on plain and hovered rows, `text` on the selected row, where muted only reaches
/// ~4.6:1 on the selection fill (round-4 T2, restored in round 6).
pub fn row_key_color(selected: bool, p: &Palette) -> egui::Color32 {
    if selected { p.text } else { p.muted }
}

pub fn row_fill(selected: bool, hovered: bool, p: &Palette) -> egui::Color32 {
    match (selected, hovered) {
        (true, _) => p.selected_row,
        (false, true) => p.hover_row,
        (false, false) => p.surface,
    }
}

/// Keys in mono 12.5 `text`, words in 12 `muted`: 4 px from key to word, 12 px either side of the "·"
/// between hints; elided from the right with "…".
fn hint_bar(ui: &mut Ui, hs: &[crate::keys::Hint], p: &Palette) {
    use crate::keys::Tok;
    // Each token is its own galley, placed by its ink so the gaps are what they say: KEY_TO_WORD between a
    // key and its word, SEP_SPACE either side of "·", whatever the mono key's side bearings.
    let galley = |t: &Tok| match t {
        Tok::K(s) => ui.painter().layout_no_wrap(s.to_string(), theme::key_font(), p.text),
        Tok::W(s) => ui.painter().layout_no_wrap(s.to_string(), theme::body(12.0), p.muted),
    };
    let word = |s: &str| ui.painter().layout_no_wrap(s.to_string(), theme::body(12.0), p.muted);
    let ink = |g: &std::sync::Arc<egui::Galley>| (g.mesh_bounds.left(), g.mesh_bounds.right());
    let ink_w = |g: &std::sync::Arc<egui::Galley>| g.mesh_bounds.width();
    let hints: Vec<Vec<std::sync::Arc<egui::Galley>>> = hs.iter().map(|h| h.iter().map(galley).collect()).collect();
    let widths: Vec<f32> = hints
        .iter()
        .map(|gs| ink_origins(&gs.iter().enumerate().map(|(j, g)| (ink(g).0, ink(g).1, if j > 0 { KEY_TO_WORD } else { 0.0 })).collect::<Vec<_>>()).1)
        .collect();
    let dot = word("·");
    let ell = word("…");
    let sep = ink_w(&dot) + 2.0 * SEP_SPACE;
    let (n, elided) = crate::keys::fit_hints(&widths, sep, ink_w(&ell), ui.available_width());
    let mut items: Vec<std::sync::Arc<egui::Galley>> = Vec::new();
    let mut gaps: Vec<f32> = Vec::new();
    for (i, gs) in hints.into_iter().take(n).enumerate() {
        if i > 0 {
            items.push(dot.clone());
            gaps.push(SEP_SPACE);
        }
        for (j, g) in gs.into_iter().enumerate() {
            gaps.push(match (i, j) {
                (_, j) if j > 0 => KEY_TO_WORD,
                (0, 0) => 0.0,
                _ => SEP_SPACE,
            });
            items.push(g);
        }
    }
    if elided {
        if n > 0 {
            items.push(dot.clone());
            gaps.push(SEP_SPACE);
        }
        items.push(ell.clone());
        gaps.push(if n > 0 { SEP_SPACE } else { 0.0 });
    }
    let spec: Vec<(f32, f32, f32)> = items.iter().zip(&gaps).map(|(g, gap)| (ink(g).0, ink(g).1, *gap)).collect();
    let (xs, total) = ink_origins(&spec);
    let h = ui.available_height();
    let (rect, _) = ui.allocate_exact_size(vec2(total.ceil(), h), Sense::hover());
    // One baseline for keys and words: a 12 px word's line box centred in the bar.
    let line = (rect.center().y - dot.size().y / 2.0).round() + crate::views::galley_baseline(&dot);
    for (g, x) in items.into_iter().zip(xs) {
        let top = line - crate::views::galley_baseline(&g);
        // Each galley carries its own colour (keys `text`, words `muted`).
        ui.painter().galley(egui::pos2(rect.left() + x, top), g, p.muted);
    }
}

/// Places items left to right by their ink: each is (ink left, ink right, both from its own origin, and the
/// ink gap before it). Returns each item's origin x and the run's ink width, from the first ink.
pub fn ink_origins(items: &[(f32, f32, f32)]) -> (Vec<f32>, f32) {
    let mut cursor = 0.0;
    let mut out = Vec::with_capacity(items.len());
    for (i, &(l, r, gap)) in items.iter().enumerate() {
        let x = cursor + if i > 0 { gap } else { 0.0 } - l;
        out.push(x);
        cursor = x + r;
    }
    (out, cursor)
}

/// A key → its word, ink to ink.
const KEY_TO_WORD: f32 = 4.0;

/// Space either side of the "·" between hints.
const SEP_SPACE: f32 = 12.0;

fn goto_box(app: &mut App, ctx: &egui::Context, p: &Palette) {
    if app.goto.is_none() {
        return;
    }
    // The standard modal backdrop: the box is an overlay, not a widget floating over the filters.
    if crate::views::backdrop(ctx, "goto-backdrop", egui::Order::Middle, crate::views::hint_bar_height(app), p).clicked() {
        app.goto = None;
        return;
    }
    let Some(text) = app.goto.as_mut() else { return };
    let mut go = false;
    let x = ((ctx.content_rect().width() - GOTO_W) / 2.0).round();
    egui::Area::new(egui::Id::new("goto")).order(egui::Order::Foreground).fixed_pos(egui::pos2(x, goto_top())).show(ctx, |ui| {
        Frame::new()
            .fill(crate::views::dialog_fill(p))
            .stroke(crate::views::dialog_stroke(p))
            .corner_radius(theme::RADIUS)
            .inner_margin(Margin::same(GOTO_PAD as i8))
            .show(ui, |ui| {
                crate::views::dialog_fields(ui, p);
                ui.set_width(GOTO_W - 2.0 * GOTO_PAD);
                ui.label(RichText::new("Go to issue").font(theme::bold(13.5)).color(p.text));
                ui.add_space(4.0);
                // Proportional 14 like the search box: mono is for rendered ids, not input.
                let r = ui.add(crate::views::input_field(text, "9001, CI-9001, or a browse link", theme::body(14.0), p));
                // The box keeps focus while open, so Enter counts whether or not the field just gave it up.
                if (r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                } else {
                    r.request_focus();
                }
            });
    });
    if go {
        app.actions.push(Action::GoTo);
    }
}

/// The go-to box's outer width and padding.
const GOTO_W: f32 = 444.0;
const GOTO_PAD: f32 = 12.0;

/// The go-to box's top edge: 12 px below the header rule.
pub fn goto_top() -> f32 {
    crate::views::HEADER_H + 12.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_selected_or_pointed_row_is_filled() {
        let p = &theme::LIGHT;
        assert_eq!(row_fill(true, false, p), p.selected_row);
        assert_eq!(row_fill(true, true, p), p.selected_row, "selection wins over hover");
        assert_eq!(row_fill(false, true, p), p.hover_row);
        assert_eq!(row_fill(false, false, p), p.surface, "no sticky cursor fill");
    }

    #[test]
    fn count_row_label_follows_state() {
        assert_eq!(count_label(0, None, true, CountSuffix::None), "Loading issues…", "nothing yet: no count");
        assert_eq!(count_label(0, None, true, CountSuffix::Jql), "Loading issues…");
        assert_eq!(count_label(0, None, false, CountSuffix::None), "0 issues");
        assert_eq!(count_label(1, None, false, CountSuffix::None), "1 issue");
        assert_eq!(count_label(20, None, true, CountSuffix::None), "20 issues", "previous rows stay with their count while updating");
        assert_eq!(count_label(2, None, false, CountSuffix::Jql), "2 issues · JQL");
        assert_eq!(count_label(3, Some(20), false, CountSuffix::None), "3 of 20 issues", "local text filter");
        assert_eq!(count_label(20, Some(20), false, CountSuffix::None), "20 issues", "a filter that hides nothing");
        assert_eq!(count_label(0, Some(1), false, CountSuffix::None), "0 of 1 issue");
        assert_eq!(count_label(3, Some(20), false, CountSuffix::Jql), "3 of 20 issues · JQL");
        assert_eq!(count_label(14, None, false, CountSuffix::Text), "14 issues · text match", "a text search's rows");
    }

    #[test]
    fn count_is_muted_while_it_will_change() {
        assert_eq!(count_color(true, &theme::LIGHT), theme::LIGHT.muted);
        assert_eq!(count_color(false, &theme::DARK), theme::DARK.text);
    }

    #[test]
    fn count_suffix_waits_for_the_result() {
        use CountSuffix::*;
        assert_eq!(count_suffix(false, false, false), None, "a typed search pending over plain rows: the old label, verbatim");
        assert_eq!(count_suffix(true, false, true), Jql, "a pending quick filter over JQL rows: still JQL rows, and landed");
        assert_eq!(count_suffix(false, false, true), None, "a quick filter's result");
        assert_eq!(count_suffix(true, false, false), None, "the search failed and its rows are gone");
        assert_eq!(count_suffix(false, true, true), Text, "a text search's result");
        assert_eq!(count_suffix(false, true, false), None, "a text search pending over the team list");
    }

    #[test]
    fn the_local_filter_steps_aside_only_for_its_own_terms_rows() {
        assert!(text_rows_match(true, Some("f1"), " f1 "));
        assert!(!text_rows_match(true, Some("f1"), "f12"), "f12 pending over f1's rows: filter them locally");
        assert!(!text_rows_match(false, Some("f1"), "f1"), "the team list while the first search runs");
        assert!(!text_rows_match(true, None, "f1"), "a quick filter's rows");
    }

    #[test]
    fn clear_shows_for_any_row_filter_and_names_it() {
        assert!(!clear_visible(false, false, false, false));
        for (q, m, r, t) in [(true, false, false, false), (false, true, false, false), (false, false, true, false), (false, false, false, true)] {
            assert!(clear_visible(q, m, r, t));
        }
        assert_eq!(clear_label(true), "Clear search");
        assert_eq!(clear_label(false), "Clear filters");
    }

    #[test]
    fn light_hint_bar_is_not_pure_white() {
        assert_ne!(hint_bar_fill(&theme::LIGHT), egui::Color32::WHITE);
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(hint_bar_fill(p), p.surface_2, "one step in both themes");
        }
    }

    #[test]
    fn epic_tooltip_names_key_and_epic() {
        assert_eq!(epic_tip("CI-1860", "Cluster Operations (ongoing)"), "CI-1860 · Cluster Operations (ongoing)");
        assert_eq!(epic_tip("CI-1860", ""), "CI-1860");
    }

    #[test]
    fn goto_box_sits_just_under_the_header() {
        assert_eq!(goto_top() - crate::views::HEADER_H, 12.0, "12 below the rule's bottom (round 7 measured 11)");
    }

    #[test]
    fn unfocused_esc_clears_only_the_boxs_searches() {
        assert!(esc_clears_search(true, true, false), "typed JQL");
        assert!(esc_clears_search(true, false, true), "text search");
        assert!(!esc_clears_search(true, false, false), "a quick filter's search");
        assert!(!esc_clears_search(false, true, false));
    }

    #[test]
    fn updating_shows_from_the_first_keystroke() {
        assert!(text_debouncing(Some("alpha"), None, false), "typed, debounce running");
        assert!(text_debouncing(Some("alpha"), Some("alp"), false), "a newer term");
        assert!(!text_debouncing(Some("alpha"), Some("alpha"), false), "sent: the search spinner takes over");
        assert!(!text_debouncing(None, None, false), "JQL or one letter: no text search");
        assert!(!text_debouncing(Some("alpha"), None, true), "Enter ran it as JQL");
        assert!(typed_for(true, Some("alpha"), "alpha"));
        assert!(!typed_for(true, Some("alpha"), "alphas"), "edited after Enter: text search again");
        assert!(!typed_for(false, Some("alpha"), "alpha"));
    }

    #[test]
    fn enter_runs_the_box_as_jql() {
        assert_eq!(enter_jql(" status = Blocked "), Some("status = Blocked".into()));
        assert_eq!(enter_jql("alpha"), Some("alpha".into()), "plain text too, as the hint says");
        assert_eq!(enter_jql("  "), None);
        assert_eq!(SEARCH_HINT, "Filter or search, Enter for JQL (⌘F)");
    }

    #[test]
    fn esc_clears_then_blurs() {
        assert_eq!(search_esc(true, false), SearchEsc::Clear);
        assert_eq!(search_esc(false, true), SearchEsc::Clear);
        assert_eq!(search_esc(false, false), SearchEsc::Blur);
    }

    #[test]
    fn combos_size_from_their_options_and_epic_gives_way_first() {
        let fill = |w: [f32; 3], avail: f32| (w.iter().sum::<f32>() + 2.0 * GAP - avail).abs() < 1.01;
        // Wide: every combo at its widest option; the epic does not stretch to fill the row.
        let (w, short) = combo_widths(187.0, 124.0, 140.0, 150.0, 572.0);
        assert_eq!((w, short), ([187.0, 140.0, 150.0], false));
        // The width depends only on the options, not on which is selected: same inputs, same widths.
        assert_eq!(combo_widths(187.0, 124.0, 140.0, 150.0, 572.0).0, w);
        // A long epic list: the epic takes what is left, no more.
        let (w, _) = combo_widths(187.0, 124.0, 140.0, 900.0, 572.0);
        assert!(fill(w, 572.0), "{w:?}");
        // Narrower: epic shrinks to its minimum before team goes short.
        let (w, short) = combo_widths(187.0, 124.0, 140.0, 300.0, 451.0);
        assert_eq!((w[0], w[1], short), (187.0, 140.0, false));
        assert!(w[2] >= EPIC_MIN && fill(w, 451.0), "{w:?}");
        // Then team takes its short labels.
        let (w, short) = combo_widths(187.0, 124.0, 140.0, 300.0, 400.0);
        assert_eq!((w[0], w[1], short), (124.0, 140.0, true));
        // A narrow row (338 px): sprint whole, epic at its minimum, team cut to what is left.
        let (w, short) = combo_widths(187.0, 124.0, 140.0, 300.0, 338.0);
        assert_eq!((w[1], w[2], short), (140.0, EPIC_MIN, true));
        assert!(fill(w, 338.0), "{w:?}");
        assert_eq!(w[0], 74.0);
        // A short epic list never takes more than its widest option.
        assert_eq!(combo_widths(187.0, 124.0, 140.0, 80.0, 572.0).0[2], 80.0);
    }

    #[test]
    fn one_popup_width_rule() {
        // Rows pad 12 either side; the frame is 8; 2 px rounding room.
        assert_eq!(popup_width(190.0, 100.0, 12.0, 500.0), 190.0, "the combo's width when the options fit");
        assert_eq!(popup_width(120.0, 100.0, 12.0, 500.0), 134.0, "the widest option is never cut");
        assert_eq!(popup_width(190.0, 300.0, 12.0, 500.0), 334.0, "grows for a wide option");
        assert_eq!(popup_width(190.0, 900.0, 12.0, 500.0), 500.0, "capped at the pane's right edge");
        assert_eq!(popup_width(190.0, 900.0, 12.0, 100.0), 190.0, "never narrower than the combo");
        assert_eq!(POPUP_ROW_PAD + POPUP_FRAME / 2.0, INNER_X, "popup text starts where the combo's does");
    }

    #[test]
    fn hint_tokens_sit_by_their_ink() {
        // A mono key with 1.5 of bearing each side, then a word with 0.5: the inks sit KEY_TO_WORD apart.
        let (xs, w) = ink_origins(&[(1.5, 6.0, 0.0), (0.5, 20.0, KEY_TO_WORD)]);
        assert_eq!(xs[0], -1.5, "the first ink starts at the bar's inset");
        assert_eq!(xs[1] + 0.5 - (xs[0] + 6.0), KEY_TO_WORD);
        assert_eq!(w, xs[1] + 20.0);
        // Either side of a separator: SEP_SPACE ink to ink.
        let (xs, _) = ink_origins(&[(0.0, 10.0, 0.0), (1.0, 3.0, SEP_SPACE), (2.0, 8.0, SEP_SPACE)]);
        assert_eq!(xs[1] + 1.0 - 10.0, SEP_SPACE);
        assert_eq!(xs[2] + 2.0 - (xs[1] + 3.0), SEP_SPACE);
        assert_eq!(HINT_BAR_H, 24.0);
    }

    #[test]
    fn popups_show_whole_rows_and_a_bar_when_they_scroll() {
        assert_eq!(popup_scroll(3), (3.0 * POPUP_ROW_H, false));
        assert_eq!(popup_scroll(POPUP_MAX_ROWS), (POPUP_MAX_ROWS as f32 * POPUP_ROW_H, false));
        let (h, bar) = popup_scroll(35);
        assert!(bar, "35 epics: the bar shows while open");
        assert_eq!(h % POPUP_ROW_H, 0.0, "never a row cut mid-text");
        assert_eq!(POPUP_FRAME / 2.0, 4.0, "selected-row fill inset 4");
    }

    #[test]
    fn popups_stay_in_the_pane() {
        assert_eq!(popup_left(300.0, 200.0, 16.0, 588.0), 300.0, "fits under its combo");
        assert_eq!(popup_left(300.0, 400.0, 16.0, 588.0), 188.0, "shifts left to end at the pane edge");
        assert_eq!(popup_left(300.0, 572.0, 16.0, 588.0), 16.0, "as wide as the pane: from its left edge");
        assert_eq!(popup_left_end(588.0, 200.0, 16.0), 388.0, "epic: right edge on the combo's, grows leftward");
        assert_eq!(popup_left_end(588.0, 700.0, 16.0), 16.0, "bounded by the pane");
        assert_eq!((SEARCH_TO_COUNT, COUNT_TO_ROWS), (8.0, 4.0), "12 / 8 to the count's ink");
    }

    #[test]
    fn combo_widths_do_not_depend_on_loaded_data() {
        // A stand-in measure: 7 px a character.
        let m = |t: &str| t.chars().count() as f32 * 7.0;
        let before = combo_width(ComboKind::Epic, m);
        // Data arriving (epics, sprints) never enters the combo's own width: it is the samples' widest.
        let loaded_epics = ["CI-4321 · Cluster (on-prem migration and cluster upgrades)", "CI-79 · Deployer"];
        assert!(loaded_epics.iter().all(|e| !combo_samples(ComboKind::Epic).contains(e)));
        assert_eq!(combo_width(ComboKind::Epic, m), before, "identical before and after load");
        assert_eq!(before, (m(EPIC_SAMPLE) + COMBO_CHROME).ceil());
        assert!(before >= m("All epics") + COMBO_CHROME);
        assert_eq!(EPIC_SAMPLE.split(" · ").nth(1).unwrap().chars().count(), 28, "a 28-character epic name");
        let sprint = combo_width(ComboKind::Sprint, m);
        assert!(sprint >= m(SPRINT_PENDING) + COMBO_CHROME && sprint >= m("All sprints") + COMBO_CHROME);
        assert_eq!(combo_width(ComboKind::Sprint, m), sprint);
    }

    #[test]
    fn sprint_combo_reads_pending_until_sprints_answer() {
        assert_eq!(sprint_combo_text(false, "All sprints"), ("Active sprint".to_string(), true));
        assert_eq!(sprint_combo_text(true, "Sprint 91 (active)"), ("Sprint 91 (active)".to_string(), false));
        assert_eq!(sprint_combo_text(true, "All sprints"), ("All sprints".to_string(), false), "a board with no active sprint");
    }

    #[test]
    fn epic_names_shorten_at_word_boundaries() {
        assert_eq!(shorten_epic("Cluster (on-prem)", 40), "Cluster (on-prem)", "fits whole");
        assert_eq!(shorten_epic("Cluster (on-prem)", 14), "Cluster", "the parenthetical goes first");
        assert_eq!(shorten_epic("Cost Insights & Analytics", 20), "Cost Insights…", "a word boundary, no dangling &");
        assert_eq!(shorten_epic("Cost Insights & Analytics", 12), "Cost…");
        assert_eq!(shorten_epic("Observability and dashboards", 20), "Observability…", "no dangling connector");
        assert_eq!(shorten_epic("Supercalifragilistic", 8), "Superca…", "one long word: cut as a last resort");
        assert!(shorten_epic("Cluster (on-prem migration) upgrades", 12).ends_with('…'));
        let m = |t: &str| t.chars().count() as f32 * 7.0;
        assert_eq!(fit_epic("Cluster (on-prem)", 100.0, m), "Cluster");
        assert_eq!(fit_epic("Cluster (on-prem)", 200.0, m), "Cluster (on-prem)");
    }

    #[test]
    fn selected_row_key_reads_in_text() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(row_key_color(true, p), p.text);
            assert_eq!(row_key_color(false, p), p.muted);
            assert!(theme::contrast(row_key_color(true, p), p.selected_row) >= 7.0, "dark={}", p.dark);
            assert!(theme::contrast(row_key_color(false, p), p.hover_row) >= 4.5, "dark={}", p.dark);
        }
    }

    #[test]
    fn pills_fit_on_one_line() {
        use PillKind::*;
        let row = [(Status, 100.0, 100.0), (Epic, 150.0, 150.0), (Other, 40.0, 40.0), (Assignee, 110.0, 50.0)];
        // Everything fits: 400 + 3 gaps of 6.
        assert_eq!(fit_pills(&row, 418.0, 6.0), vec![Some(100.0), Some(150.0), Some(40.0), Some(110.0)]);
        // The epic is cut first (417 − 250 − 3 gaps), keeping the full assignee.
        assert_eq!(fit_pills(&row, 417.0, 6.0), vec![Some(100.0), Some(149.0), Some(40.0), Some(110.0)]);
        assert_eq!(fit_pills(&row, 332.0, 6.0), vec![Some(100.0), Some(64.0), Some(40.0), Some(110.0)]);
        // Below the epic minimum the assignee shows its first name, and the epic gets the room back.
        assert_eq!(fit_pills(&row, 300.0, 6.0), vec![Some(100.0), Some(92.0), Some(40.0), Some(50.0)]);
        assert_eq!(fit_pills(&row, 272.0, 6.0), vec![Some(100.0), Some(64.0), Some(40.0), Some(50.0)]);
        // Then the assignee goes.
        assert_eq!(fit_pills(&row, 250.0, 6.0), vec![Some(100.0), Some(98.0), Some(40.0), None]);
        // Only then the epic.
        assert_eq!(fit_pills(&row, 200.0, 6.0), vec![Some(100.0), None, Some(40.0), None]);
        assert!(fit_pills(&[], 10.0, 6.0).is_empty());
        // No epic: the first name before dropping.
        let plain = [(Status, 100.0, 100.0), (Assignee, 110.0, 50.0)];
        assert_eq!(fit_pills(&plain, 160.0, 6.0), vec![Some(100.0), Some(50.0)]);
        assert_eq!(fit_pills(&plain, 150.0, 6.0), vec![Some(100.0), None]);
    }

    #[test]
    fn row_pills_keep_one_order() {
        use PillKind::*;
        let mut kinds = vec![Assignee, Other, Epic, Status];
        kinds.sort_by_key(|k| pill_rank(*k));
        assert_eq!(kinds, vec![Status, Epic, Other, Assignee], "status, epic, labels, assignee");
        assert_eq!(first_name("Shawn Sorichetti"), "Shawn");
        assert_eq!(first_name("Cher"), "Cher");
    }

    #[test]
    fn summaries_share_one_left_edge() {
        assert_eq!(key_column_width([52.0, 60.4, 45.0]), 61.0);
        assert_eq!(key_column_width([]), 0.0);
    }

    #[test]
    fn only_the_list_ends_are_rounded() {
        let r = |nw, ne, sw, se| CornerRadius { nw, ne, sw, se };
        assert_eq!(row_corners(0, 1), r(6, 6, 6, 6));
        assert_eq!(row_corners(0, 3), r(6, 6, 0, 0));
        assert_eq!(row_corners(1, 3), r(0, 0, 0, 0));
        assert_eq!(row_corners(2, 3), r(0, 0, 6, 6));
    }

    #[test]
    fn labels_pill_skips_the_filtered_team() {
        let l = ["PLAT".to_string(), "infra".to_string()];
        assert_eq!(row_labels(&l, Some("PLAT")), "infra");
        assert_eq!(row_labels(&l, None), "PLAT, infra");
        assert_eq!(row_labels(&l[..1], Some("PLAT")), "", "nothing left: no pill");
    }

    #[test]
    fn only_a_really_cut_row_fades() {
        let rows = [(0.0, 44.0), (43.0, 87.0), (86.0, 130.0)];
        assert_eq!(fade_strip(&rows, 125.0, true), None, "cut by 5: reads whole, no fade");
        assert_eq!(fade_strip(&rows, 100.0, true), Some((86.0, 100.0)), "cut by 30: its 14 px strip fades");
        assert_eq!(fade_strip(&rows, 120.0, true), Some((96.0, 120.0)), "a short gradient, at most 24");
        assert_eq!(fade_strip(&rows, 200.0, true), None, "nothing at the edge");
        assert_eq!(fade_strip(&rows, 20.0, false), Some((20.0, 44.0)), "the same rule at the top");
        assert_eq!(fade_strip(&rows, 5.0, false), None);
    }

    #[test]
    fn a_rule_marks_rows_below() {
        let rows = [(0.0, 44.0), (43.0, 87.0), (86.0, 130.0)];
        assert!(more_below(&rows, 87.0), "the last row is out of view");
        assert!(!more_below(&rows, 130.0), "every row is in view");
        assert!(!more_below(&[], 10.0));
    }

    #[test]
    fn fifteen_rows_fit_at_1440_by_900() {
        // 900 less the header (48), the filter block (12 + 28 + 12 + 28 + 12 + 28 + 12, the count row, 8) and the hint bar.
        let list_h = 900.0
            - crate::views::HEADER_H
            - (12.0 + 28.0 + 12.0 + 28.0 + 12.0 + crate::views::INPUT_H + SEARCH_TO_COUNT + COUNT_H + COUNT_TO_ROWS)
            - HINT_BAR_H;
        assert!((list_h / ROW_PITCH).floor() >= 15.0, "{}", list_h / ROW_PITCH);
    }

    #[test]
    fn narrow_windows_share_the_list_pane() {
        assert_eq!(list_pane_width(1000.0), (412.0, 320.0..=412.0), "45% held back so the description keeps 400");
        assert_eq!(list_pane_width(1199.0).0, (1199.0f32 * 0.45).floor());
        assert!(list_pane_width(700.0).0 >= 320.0);
        let (w, r) = list_pane_width(1440.0);
        assert_eq!(w, (1440.0f32 * 0.42).clamp(360.0, 720.0));
        assert_eq!(r, 320.0..=w);
    }

    #[test]
    fn team_label_shortens_when_tight() {
        assert_eq!(team_combo_label("PLAT · Platform", true), "PLAT · Platform");
        assert_eq!(team_combo_label("PLAT · Platform", false), "PLAT");
        assert_eq!(team_combo_label("All teams", false), "All teams");
    }

    #[test]
    fn epic_slots_wait_for_the_sprint_scope() {
        // Round 7: slots given during the all-sprints flash were kept, so neighbouring hues met on screen.
        assert_eq!(slot_phase(false, false), SlotPhase::Provisional);
        assert_eq!(slot_phase(true, false), SlotPhase::Reset, "once, when the scope resolves");
        assert_eq!(slot_phase(true, true), SlotPhase::Keep);
    }

    #[test]
    fn stripe_covers_the_left_border_on_whole_pixels() {
        let row = Rect::from_min_max(egui::pos2(16.3, 100.0), egui::pos2(400.0, 144.0));
        let s = stripe_rect(row, 2.0);
        assert_eq!((s.left(), s.right(), s.top(), s.bottom()), (16.5, 19.5, 100.0, 144.0));
        assert_eq!(stripe_rect(row, 1.0).left(), 16.0);
    }

    #[test]
    fn every_hint_key_is_drawn_mono() {
        use crate::keys::{HintContext::*, Tok, hints};
        let all = [
            Help,
            GoTo,
            Search,
            SearchText,
            SearchEmpty,
            Composer,
            EditPicker,
            AssignPicker,
            Picker,
            Selected,
            Browse,
            Placeholder,
            SignIn,
            SelectedSearch,
            BrowseSearch,
        ];
        crate::testutil::with_ui(|ui| {
            ui.fonts_mut(|f| {
                let mut adv = |c: char| f.glyph_width(&theme::key_font(), c);
                let m = adv('m');
                for c in all {
                    for h in hints(c) {
                        for t in h.iter() {
                            if let Tok::K(k) = t {
                                // ASCII in a key is the mono face's: one advance for every character.
                                for ch in k.chars().filter(char::is_ascii) {
                                    let w = adv(ch);
                                    assert!((w - m).abs() < 0.01, "{c:?} key {k:?}: {ch:?} is {w:.2}, m {m:.2}");
                                }
                            }
                        }
                    }
                }
            })
        });
    }

    #[test]
    fn status_pills_are_one_opaque_colour_on_every_row() {
        let statuses = ["New", "Planning", "Ready for Work", "In Progress", "In Review", "Blocked", "Done", "Waiting for support", "Work in Progress"];
        for p in [&theme::LIGHT, &theme::DARK] {
            for s in statuses {
                let t = theme::status_tint(s, "indeterminate", p, &Default::default());
                let fill = status_fill(&t, p);
                assert_eq!(fill.a(), 255);
                assert!(theme::contrast(t.fg, fill) >= 4.5, "dark={} {s}: {:.2}", p.dark, theme::contrast(t.fg, fill));
            }
        }
    }

    #[test]
    fn row_padding_is_six_four_six() {
        // Edge to ink as round 8 measured it: above, the border 1 and the title's ~4.5 of leading over the caps
        // (5.5 with a 0 pad); below, 1.5 more than the pad (7.5 with 6).
        assert_eq!(1.0 + 4.5 + f32::from(ROW_PAD_TOP) + ROW_PAD_TOP_EXTRA, 6.0);
        assert_eq!(f32::from(ROW_PAD_BOTTOM) + ROW_PAD_BOTTOM_EXTRA + 1.5, 6.0);
        assert_eq!(ROW_LINE_GAP, 0.0, "baseline to pill top 5 → 4");
    }

    #[test]
    fn selected_row_keeps_its_dividers() {
        let r = Rect::from_min_max(egui::pos2(0.0, 100.0), egui::pos2(10.0, 144.0));
        assert_eq!(selected_dividers(r, 3, 10), [(100.5, 0.0), (143.5, 0.0)]);
        assert_eq!(selected_dividers(r, 0, 10), [(100.5, 6.0), (143.5, 0.0)], "the first row's top goes over the frame, inside its corners");
        assert_eq!(selected_dividers(r, 9, 10), [(100.5, 0.0), (143.5, 6.0)]);
        assert_eq!(selected_dividers(r, 0, 1), [(100.5, 6.0), (143.5, 6.0)]);
        for p in [&theme::LIGHT, &theme::DARK] {
            let edge = theme::over(crate::views::selected_edge(p), p.selected_row);
            assert!(theme::contrast(edge, p.selected_row) > theme::contrast(p.border, p.selected_row), "dark={}", p.dark);
        }
    }

    #[test]
    fn selected_row_pills_stay_secondary() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let t = crate::views::selected_pill_text(p);
            assert_ne!(t, p.text, "only the key switches to text (dark={})", p.dark);
            assert!(theme::contrast(t, p.selected_row) >= 4.5, "dark={}: {:.2}", p.dark, theme::contrast(t, p.selected_row));
        }
    }
}
