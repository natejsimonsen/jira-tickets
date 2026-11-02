//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{outline_pill, outline_pill_selected, pill_outlined, toggle_chip};
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
    match slot_phase(app.sprints_answered || app.sprints_cached || app.client.is_none(), app.epic_slots_final) {
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

/// How many rows the list would show without any local narrowing (Mine, Roadmap, the text filter), for
/// "4 of 18 issues".
pub fn rows_without_narrowing(app: &App) -> usize {
    let src = app.rows_list();
    let all = filter::action_items(&app.store.list(src));
    let me = app.me.as_ref().map(|m| m.display_name.as_str());
    let f = filter::IssueFilter { text: String::new(), mine: false, roadmap: false, roadmap_epics: Default::default(), ..app.filter.clone() };
    filter::visible(&all, &f, src == ListId::Search, me).len()
}

/// Whether the rows on screen are narrowed locally (B1): Mine, Roadmap or the local text filter. Server results
/// alone (a search, a text match) are not: they count as "N issues".
pub fn locally_narrowed(mine: bool, roadmap: bool, text_filter: bool) -> bool {
    mine || roadmap || text_filter
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
        let kept: String = base.chars().take(room).collect();
        out = crate::views::pill_back_off(&kept, &base[kept.len()..]).unwrap_or(&kept).to_string();
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
    use crate::app::Screen;
    let grooming = (app.screen == Screen::Grooming).then(|| crate::views::grooming::hint(&app.grooming));
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
        grooming,
        team: app.screen == Screen::Team,
        roadmap: (app.screen == Screen::Roadmap).then_some(app.roadmap.tab),
    });
    hint_strip(ui, crate::keys::hints(c), p);
}

/// The hint bar on the sign-in form: "Enter sign in".
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
            ui.add_space(count_gaps_in(ui, p).0);
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
/// column, the 24 px gutter and the 132 px fields column (`detail::detail_columns`).
pub const NARROW_DETAIL_MIN: f32 = 32.0 + 400.0 + 24.0 + 132.0;

/// The list pane's narrowest drag.
const LIST_PANE_MIN: f32 = 320.0;

/// The list pane's default width and the range it may be dragged in. The default is 42% of the window
/// (360–720) when wide; below NARROW 45% of the window, but never so wide that the description column drops
/// under 400. The drag has no cap of its own: only the detail pane's minimum (`detail::min_pane_width`) bounds
/// it. egui clamps a saved width to this range every frame, so one saved in a wider window fits this one.
pub fn list_pane_width(window_w: f32) -> (f32, std::ops::RangeInclusive<f32>) {
    let w =
        if window_w < NARROW { (window_w * 0.45).min(window_w - NARROW_DETAIL_MIN).max(LIST_PANE_MIN).floor() } else { (window_w * 0.42).clamp(360.0, 720.0) };
    (w, super::split_range(window_w, LIST_PANE_MIN, super::detail::min_pane_width(window_w)))
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
/// The chevron's half width (to its stroke's centre) and stroke.
const CHEVRON_HALF_W: f32 = 4.0;
const CHEVRON_STROKE: f32 = 1.5;

/// The chevron's centre x in a combo whose right edge is `right`: its ink box (stroke included) ends INNER_X
/// in, like the label's inset on the left (it sat in egui's 14 px slot, 14 in).
pub fn chevron_centre_x(right: f32) -> f32 {
    right - INNER_X - CHEVRON_HALF_W - CHEVRON_STROKE / 2.0
}

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
/// row padding either side (`row_pad`), the scrollbar's room when it shows (`bar_w`; round 13 B1: without it the
/// widest row was cut and the popup ran to the cap), the popup frame and 2 px of rounding room when that is
/// wider; never past `cap`, the list pane's width.
pub fn popup_width(combo_w: f32, widest_text: f32, row_pad: f32, bar_w: f32, cap: f32) -> f32 {
    (widest_text + 2.0 * row_pad + bar_w + POPUP_FRAME + 2.0).ceil().max(combo_w).min(cap.max(combo_w))
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

/// A popup frame's side margin: a selected row's fill sits 4 inside the stroke, as at the top and bottom
/// (POPUP_MARGIN_Y; it measured 3 at the sides, round 11 B3).
const POPUP_MARGIN: i8 = 4;
/// The popup thumb's outer margin: with the frame's 4 it sits 4 in, mirroring the rows' 4 on the left.
const POPUP_THUMB_OUTER: f32 = 0.0;
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
) -> egui::Response {
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
        let px = 1.0 / ui.ctx().pixels_per_point();
        // One device pixel up: centred on whole points the cap's centre sat half a point low (round 15 B1).
        painter.galley(egui::pos2(rect.left() + INNER_X, (rect.center().y - g.size().y / 2.0).round() - px), g, color);
        // The chevron's ink box on the device-pixel grid (round 15 B2: the epic combo's sat on a half pixel).
        let reach = CHEVRON_HALF_W + CHEVRON_STROKE / 2.0;
        let c = egui::pos2(snap_px(chevron_centre_x(rect.right()) - reach, 1.0 / px) + reach, rect.center().y);
        let half = CHEVRON_HALF_W;
        let pts = vec![c + vec2(-half, -2.0), c + vec2(0.0, 2.0), c + vec2(half, -2.0)];
        painter.add(egui::Shape::line(pts, Stroke::new(CHEVRON_STROKE, p.muted)));
    }
    let ppp = ui.ctx().pixels_per_point();
    let (scroll_h, bar) = popup_scroll(n_rows);
    let bar_w = if bar { popup_bar_style().allocated_width() } else { 0.0 };
    let pop_w = snap_px(popup_width(w, widest, POPUP_ROW_PAD, bar_w, pane.1 - pane.0), ppp);
    let left = match edge {
        PopupEdge::Start => popup_left(rect.left(), pop_w, pane.0, pane.1),
        PopupEdge::End => popup_left_end(rect.right(), pop_w, pane.0),
    };
    // On whole physical pixels, so the frame's edges are crisp and its 4 margins measure 4 (round 12 B1).
    let anchor = snap_rect(Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(left + pop_w, rect.bottom())), ppp);
    let inner = pop_w - POPUP_FRAME;
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
            let saved = bar.then(|| {
                // The floating bars' THUMB_W width and round ends (set_thumb_fills), 4 in from the frame.
                ui.spacing_mut().scroll = popup_bar_style();
                // The solid thumb paints in the widgets' `bg_fill` (rows use `weak_bg_fill`, so they keep theirs).
                crate::views::set_thumb_fills(ui, p)
            });
            let visibility =
                if bar { egui::scroll_area::ScrollBarVisibility::AlwaysVisible } else { egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded };
            let row_w = inner - 2.0 * POPUP_ROW_PAD - if bar { ui.spacing().scroll.allocated_width() } else { 0.0 };
            // The track runs the rows' full height: its ends sit 4 inside the frame, like the rows' (round 14 B2:
            // THUMB_TRACK_INSET on top of the margin put it 6 down).
            let track = popup_track(ui.cursor().min, ui.available_width(), scroll_h);
            egui::ScrollArea::vertical().max_height(scroll_h).scroll_bar_visibility(visibility).scroll_bar_rect(track).show(ui, |ui| {
                // The rows keep their own corners (the thumb's THUMB_RADIUS is for the bar only).
                if let Some(saved) = saved {
                    crate::views::restore_fills(ui, saved);
                }
                contents(ui, row_w)
            });
        });
    resp
}

/// A popup's scroll bar rect: from the rows' top (`min`) down their scroll height, the frame's margin its only
/// inset.
pub fn popup_track(min: egui::Pos2, width: f32, scroll_h: f32) -> Rect {
    Rect::from_min_size(min, vec2(width, scroll_h))
}

/// A popup's solid bar: the floating bars' THUMB_W width, POPUP_THUMB_OUTER from the frame's margin.
fn popup_bar_style() -> egui::style::ScrollStyle {
    egui::style::ScrollStyle { bar_outer_margin: POPUP_THUMB_OUTER, bar_width: crate::views::THUMB_W, ..egui::style::ScrollStyle::solid() }
}

/// `v` rounded to a whole physical pixel.
pub fn snap_px(v: f32, ppp: f32) -> f32 {
    (v * ppp).round() / ppp
}

/// `r` with every edge on a whole physical pixel.
pub fn snap_rect(r: Rect, ppp: f32) -> Rect {
    Rect::from_min_max(egui::pos2(snap_px(r.min.x, ppp), snap_px(r.min.y, ppp)), egui::pos2(snap_px(r.max.x, ppp), snap_px(r.max.y, ppp)))
}

/// The search box's placeholder: it filters, searches, and runs JQL on Enter.
pub const SEARCH_HINT: &str = "Filter or search, Enter run as JQL (⌘F)";

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
        let team = combo(ui, "team", (team_text, false), (team_w, team_widest), team_options.len(), pane, PopupEdge::Start, p, |ui, _| {
            for (scope, l) in &team_options {
                if ui.selectable_label(app.filter.team == *scope, l).clicked() {
                    app.actions.push(Action::SetTeam(scope.clone()));
                }
            }
        });
        // The short label ("PLAT") names its team in full on hover (round 13 B3).
        if team_short {
            crate::views::tooltip(team, team_label.clone());
        }
        combo(
            ui,
            "sprint",
            sprint_combo_text(app.sprints_answered || app.sprints_cached, &sprint_label),
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
                if crate::views::tooltip(ui.selectable_label(app.filter.epic.as_deref() == Some(k), job), full.clone()).clicked() {
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
        if crate::views::tooltip(toggle_chip(ui, "Roadmap", app.filter.roadmap, p), "m").clicked() {
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
    // On whole points, so its 1 px border is one crisp row and its height reads 28 (round 13 I2).
    let top = ui.cursor().top();
    ui.add_space(top.round() - top);
    let r = crate::views::add_field(ui, None, &mut app.filter.text, |t| {
        crate::views::input_field_in(t, SEARCH_HINT, theme::body(14.0), p, crate::views::search_margin()).id(id)
    });
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
#[cfg(test)]
pub fn count_label(shown: usize, total: Option<usize>, loading_first: bool, suffix: CountSuffix) -> String {
    let (count, mode) = count_parts(shown, total, loading_first, suffix);
    format!("{count}{mode}")
}

/// `count_label` in its two inks: the count (`text`, or muted while stale) and the mode suffix ("· JQL",
/// "· text match", or empty), always `muted` like the Enter note (round 12 D1).
pub fn count_parts(shown: usize, total: Option<usize>, loading_first: bool, suffix: CountSuffix) -> (String, String) {
    if shown == 0 && loading_first {
        return ("Loading issues…".to_string(), String::new());
    }
    let noun = |n: usize| if n == 1 { "issue" } else { "issues" };
    let base = match total {
        Some(t) if t != shown => format!("{shown} of {t} {}", noun(t)),
        _ => format!("{shown} {}", noun(shown)),
    };
    let mode = match suffix {
        CountSuffix::None => String::new(),
        CountSuffix::Jql => count_note("JQL"),
        CountSuffix::Text => count_note("text match"),
    };
    (base, mode)
}

/// The count row's note while JQL-looking text sits in the box unrun (the local filter is off for it).
pub const JQL_ENTER_NOTE: &str = "Enter run as JQL";

/// The Enter note's key, drawn in the key style.
pub const JQL_ENTER_KEY: &str = "Enter";

/// Appends `other`'s text and sections to `job`.
pub fn append_job(job: &mut egui::text::LayoutJob, other: egui::text::LayoutJob) {
    let base = job.text.len();
    job.text.push_str(&other.text);
    job.sections.extend(other.sections.into_iter().map(|mut s| {
        s.byte_range = egui::text::ByteIndex(s.byte_range.start.0 + base)..egui::text::ByteIndex(s.byte_range.end.0 + base);
        s
    }));
}

/// How much tighter the space after the "·" is before the mono "Enter" than a proportional word (round 14 I2:
/// the dot read 3 / 4.5 to its neighbours' ink there, 3.5 / 3 in "· JQL").
pub const NOTE_DOT_TIGHTEN: f32 = 1.5;

/// The count row's one line: the count in `color`, then in `muted` the mode suffix and, with `note`, the Enter
/// note ("Enter" in the key style). The line box is always the mono key's, so the count keeps one baseline
/// whether or not the note shows (round 14 I2: it dropped 1 device px with it).
pub fn count_job(f: &mut egui::epaint::text::FontsView<'_>, label: &str, color: egui::Color32, mode: &str, note: bool, p: &Palette) -> egui::text::LayoutJob {
    let body = theme::body(12.0);
    let fmt = |c: egui::Color32| egui::TextFormat { font_id: body.clone(), color: c, ..Default::default() };
    let mut job = egui::text::LayoutJob { first_row_min_height: f.row_height(&theme::key_font()), ..Default::default() };
    job.append(label, 0.0, fmt(color));
    job.append(mode, 0.0, fmt(p.muted));
    if note {
        // The key in the key style, like the hint bar's (round 13 I4); the space after the dot NOTE_DOT_TIGHTEN
        // narrower.
        let (key, rest) = JQL_ENTER_NOTE.split_at(JQL_ENTER_KEY.len());
        job.append(COUNT_SEP.trim_end(), 0.0, fmt(p.muted));
        let first = job.sections.len();
        append_job(&mut job, key_job(key, p.muted));
        if let Some(s) = job.sections.get_mut(first) {
            s.leading_space += f.glyph_width(&body, ' ') - NOTE_DOT_TIGHTEN;
        }
        job.append(rest, 0.0, fmt(p.muted));
    }
    job
}

/// `job` with every "·" on a whole device pixel: each dot starts a section of its own whose leading space rounds
/// its x up to the grid, so its raster is never binned between pixels (round 15 D4: "1 issue · JQL" read 7 / 6 px
/// either side).
pub fn snap_dots(f: &mut egui::epaint::text::FontsView<'_>, job: egui::text::LayoutJob, ppp: f32) -> egui::text::LayoutJob {
    let range = |s: &egui::text::LayoutSection| s.byte_range.start.0..s.byte_range.end.0;
    let mut out = egui::text::LayoutJob { sections: Vec::new(), text: String::new(), ..job.clone() };
    for sec in &job.sections {
        let mut lead = sec.leading_space;
        for piece in job.text[range(sec)].split_inclusive('·') {
            let parts = match piece.strip_suffix('·') {
                Some(head) => [head, "·"],
                None => [piece, ""],
            };
            for part in parts.into_iter().filter(|t| !t.is_empty()) {
                out.append(part, lead, sec.format.clone());
                lead = 0.0;
            }
        }
    }
    let dots: Vec<usize> = (0..out.sections.len()).filter(|&i| &out.text[range(&out.sections[i])] == "·").collect();
    for i in dots {
        let g = f.layout_job(out.clone());
        let at = out.text[..out.sections[i].byte_range.start.0].chars().count();
        let Some(x) = g.rows.iter().flat_map(|r| r.row.glyphs.iter().map(move |gl| r.pos.x + gl.pos.x)).nth(at) else { continue };
        out.sections[i].leading_space += ((x * ppp).ceil() - x * ppp) / ppp;
    }
    out
}

/// The separator before anything the count row adds after the count: mode suffixes and the Enter note alike.
pub const COUNT_SEP: &str = " · ";

/// The count row's Enter note as it follows the count in the same line: the suffixes' own separator (B4).
pub fn count_note(note: &str) -> String {
    format!("{COUNT_SEP}{note}")
}

/// Whether the box holds JQL-looking text that has not been run: not the typed search on screen (`typed_shown`
/// with `last_jql`) and nothing in flight.
pub fn jql_awaiting_enter(text: &str, last_jql: Option<&str>, typed_shown: bool, pending: bool) -> bool {
    let t = text.trim();
    !pending && !t.is_empty() && filter::looks_like_jql(t) && !(typed_shown && last_jql == Some(t))
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
    let total = locally_narrowed(app.filter.mine, app.filter.roadmap, text_filter).then(|| rows_without_narrowing(app));
    let showing_search = src == ListId::Search && app.store.lists.contains_key(&ListId::Search);
    let (label, mode) = count_parts(rows.len(), total, loading_first, count_suffix(app.typed_shown, app.text_shown.is_some(), showing_search));
    let box_text = !app.filter.text.trim().is_empty() || (app.typed_search && app.search_active);
    let show_clear = clear_visible(app.filter.quick.is_some(), app.filter.mine, app.filter.roadmap, box_text);
    count_row(ui, |ui| {
        // 7 in code reads as 8 ink to ink: the 12 px text's side bearing adds about 1.
        ui.spacing_mut().item_spacing.x = COUNT_GAP;
        // One pattern, muted 12, the spinner before the verb: "[count] ⟳ Updating…", or "⟳ Loading issues…"
        // when there is nothing to count yet.
        // Muted only beside the inline "Updating…": a header sync (⌘R) leaves the count at full `text`.
        let color = if loading_first { p.muted } else { count_color(row_spinner, p) };
        if loading_first {
            if row_spinner {
                spinner_label(ui, p, &label);
            } else {
                ui.label(RichText::new(label).color(color).size(12.0));
            }
        } else {
            // One line: the count, then in `muted` the mode suffix and (JQL-looking text unrun, so the rows are not
            // filtered by it) what Enter does, each after the same "·".
            let note = !row_spinner && jql_awaiting_enter(&app.filter.text, app.last_search_jql.as_deref(), app.typed_search && app.search_active, pending);
            let ppp = ui.ctx().pixels_per_point();
            let job = ui.ctx().fonts_mut(|f| {
                let job = count_job(f, &label, color, &mode, note, p);
                snap_dots(f, job, ppp)
            });
            ui.label(job);
            if row_spinner {
                spinner_label(ui, p, "Updating…");
            }
        }
        if show_clear {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Right-aligned by the ink: "Clear search" and "Clear filters" end their last glyph on one edge
                // (round 13 B2: their boxes did, 0.5–1 apart in ink).
                if crate::views::small_link_ink_right(ui, clear_label(box_text), p).clicked() {
                    app.actions.push(Action::ClearFilters);
                }
            });
        }
    });
    ui.add_space(count_gaps_in(ui, p).1);
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
    crate::views::no_scroll_fade(ui);
    // A thin, faint, round-ended thumb at rest whenever the list overflows, so "more below" never hides (C4, C1).
    crate::views::floating_thumb(&mut ui.spacing_mut().scroll);
    let saved = crate::views::set_thumb_fills(ui, p);
    let mut row_spans: Vec<(f32, f32)> = Vec::with_capacity(rows.len());
    let mut selected_rect: Option<(Rect, usize)> = None;
    let mut row_x: Option<egui::Rangef> = None;
    let mut stripes: Vec<(usize, Rect, CornerRadius, egui::Color32)> = Vec::new();
    let mut painted_stripes: Vec<(Rect, egui::Color32)> = Vec::new();
    let track = crate::views::thumb_track(ui.available_rect_before_wrap());
    let out = egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_bar_rect(track).show(ui, |ui| {
        crate::views::restore_fills(ui, saved);
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
                .inner_margin(Margin { left: ROW_PAD_LEFT, right: 12, top: ROW_PAD_TOP, bottom: ROW_PAD_BOTTOM })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // The row's outer top edge (the content starts inside the 1 px border and the top margin).
                    let row_top = ui.cursor().top() - ROW_STROKE - f32::from(ROW_PAD_TOP);
                    ui.add_space(ROW_PAD_TOP_EXTRA);
                    ui.spacing_mut().item_spacing.y = ROW_LINE_GAP;
                    ui.horizontal(|ui| {
                        let key_color = row_key_color(is_sel, p);
                        let g = ui.painter().layout_no_wrap(i.key.clone(), theme::mono(12.5), key_color);
                        let (kr, _) = ui.allocate_exact_size(vec2(key_w, g.size().y), Sense::hover());
                        // The key and the summary share one baseline, TITLE_RAISE above the row's slot.
                        ui.painter().galley(kr.min - vec2(0.0, TITLE_RAISE), g, key_color);
                        let painter = ui.painter().clone();
                        let title = crate::views::truncate_title(&i.summary, ui.available_width(), |t| {
                            painter.layout_no_wrap(t.to_string(), theme::bold(13.5), p.text).size().x
                        });
                        let tg = painter.layout_no_wrap(title.clone(), theme::bold(13.5), p.text);
                        let (tr, _) = ui.allocate_exact_size(tg.size(), Sense::hover());
                        ui.painter().galley(tr.min - vec2(0.0, TITLE_RAISE), tg, p.text);
                        if title != i.summary {
                            tips.push((tr, i.summary.clone()));
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
                                (PillKind::Status, PillLook::Tint(status_fill(&t, p, is_sel), t.fg))
                            }
                            ("parent.summary" | "parent.key", Some(t)) => (PillKind::Epic, PillLook::Epic(t.fg, row_stripe(&t, p, is_sel))),
                            ("parent.summary" | "parent.key", None) => (PillKind::Epic, PillLook::Outline),
                            ("assignee", _) => (PillKind::Assignee, PillLook::Outline),
                            _ => (PillKind::Other, PillLook::Outline),
                        };
                        pills.push((kind, v, look));
                    }
                    pills.sort_by_key(|(k, _, _)| pill_rank(*k));
                    // The pill line at one fixed, pixel-rounded y from the row's top (C1), whatever the title row
                    // laid out to (its slot follows the inherited interact height and the galley).
                    let line = pill_line_y(row_top, ui.ctx().pixels_per_point());
                    ui.add_space(line - ui.cursor().top());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(PILL_GAP, 4.0);
                        // Pills sit on the line's top, not centred in an inherited interact height.
                        ui.spacing_mut().interact_size.y = 0.0;
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
                                crate::views::truncate_pill(&source, w - pad, measure)
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
            row_x = Some(rect.x_range());
            let mut r = ui.interact(rect, row_id(&i.key), Sense::click());
            if let Some(tip) = r.hover_pos().and_then(|at| tips.iter().find(|(t, _)| t.contains(at))).map(|(_, t)| t.clone()) {
                r = crate::views::hover_tip(r, tip, app);
            }
            let fill = row_fill(is_sel, r.hovered(), p);
            ui.painter().set(bg, egui::epaint::RectShape::filled(rect, corners, fill));
            if RIGHT_EDGE_OPEN {
                let (x, top, bottom) = right_border_cover(rect);
                ui.painter().vline(x, top..=bottom, Stroke::new(1.0, fill));
            }
            if let Some(t) = epic {
                // Over the card's left border, on whole physical pixels, so no sliver of the page shows between
                // the stripe and the fill. Painted after the selection rules (below), so they never notch it.
                let stripe = CornerRadius { nw: corners.nw, sw: corners.sw, ne: 0, se: 0 };
                stripes.push((idx, stripe_rect(rect, ui.ctx().pixels_per_point()), stripe, row_stripe(&t, p, is_sel)));
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
                ui.painter().hline(egui::Rangef::new(rect.left() + inset, rect.right()), y, Stroke::new(1.0, edge));
            }
            // At the list's outer edge the rule carries on round the card's corner in the same tone, so no plain
            // border shows in the arc (round 15 C2).
            let radius = f32::from(theme::RADIUS);
            for (outer, y) in [(idx == 0, rect.top()), (idx + 1 == rows.len(), rect.bottom() - radius)] {
                if outer {
                    let corner = Rect::from_min_size(egui::pos2(rect.left(), y), vec2(radius, radius));
                    let arc = egui::epaint::RectShape::stroke(rect, row_corners(idx, rows.len()), Stroke::new(1.0, edge), egui::StrokeKind::Inside);
                    ui.painter().with_clip_rect(corner.intersect(ui.clip_rect())).add(arc);
                }
            }
        }
        // Stripes last, in both themes; a row hidden as a bottom sliver shows none (it peeked 1 pt over the seam).
        let hidden = sliver_row(&row_spans, ui.clip_rect().bottom());
        // The selected row's stripe goes last, over the seam it shares with the next row, so its bottom rule
        // never shows the next row's stripe tone (C3).
        stripes.sort_by_key(|(idx, ..)| stripe_paints_last(*idx, selected_rect.map(|(_, i)| i)));
        for (idx, r, corners, color) in stripes {
            if Some(idx) != hidden {
                ui.painter().rect_filled(r, corners, color);
                painted_stripes.push((r, color));
            }
        }
        // Room under the last row, so a toast (anchored above the hint bar) never covers it.
        ui.add_space(LIST_BOTTOM_PAD);
    });
    let view = out.inner_rect;
    // A row showing less than CLIP_MIN at the bottom is hidden, but the card runs on to the hint bar: its fill
    // and left border fill the leftover gap, so it reads as card, not panel (C4).
    let sliver = bottom_sliver(&row_spans, view.bottom());
    if let (Some((top, bottom)), Some(x)) = (sliver, row_x) {
        ui.painter().rect_filled(Rect::from_x_y_ranges(x, top..=bottom), 0.0, row_fill(false, false, p));
        ui.painter().vline(x.min + 0.5, top..=bottom, Stroke::new(1.0, p.border));
    }
    // No rule at the viewport's bottom edge: it landed on the hint bar's border as a second 1 px band (C2); the
    // card fill runs to the bar and the fade says "more".
    // Each fade goes toward the cut row's own fill, so the card's fill never tints and then jumps back on the
    // unfaded last pixel row (round 14 K3: it faded toward `bg`).
    let sel_idx = selected_rect.map(|(_, i)| i);
    let fade_at = |edge: f32| fade_color(cut_row(&row_spans, edge).is_some_and(|i| Some(i) == sel_idx), p);
    let ppp = ui.ctx().pixels_per_point();
    for (strip, dir, colors) in [
        (
            fade_strip(&row_spans, view.bottom(), true).map(|(t, b)| (t, fade_end(b, ppp))),
            egui::epaint::Direction::BottomUp,
            [fade_at(view.bottom()), egui::Color32::TRANSPARENT],
        ),
        (fade_strip(&row_spans, view.top(), false), egui::epaint::Direction::TopDown, [fade_at(view.top()), egui::Color32::TRANSPARENT]),
    ] {
        if let Some((top, bottom)) = strip {
            let r = Rect::from_x_y_ranges(view.x_range(), top..=bottom);
            ui.painter().add(egui::Shape::gradient_rect(r, dir, colors));
            // The bottom fade dims the cut row's content, not its card: the left border and stripe go back on
            // top, so the card's edge runs unbroken to the bar (round 13 K3).
            if dir == egui::epaint::Direction::BottomUp
                && let Some(x) = row_x
            {
                ui.painter().vline(x.min + 0.5, top..=bottom, Stroke::new(1.0, p.border));
                for (s, color) in &painted_stripes {
                    let part = s.intersect(r);
                    if part.is_positive() {
                        ui.painter().rect_filled(part, 0.0, *color);
                    }
                }
            }
        }
    }
}

/// The row a viewport edge cuts through, if any.
pub fn cut_row(rows: &[(f32, f32)], edge: f32) -> Option<usize> {
    rows.iter().position(|(t, b)| *t < edge && edge < *b)
}

/// A list fade's strongest colour: the cut row's fill (selected or plain) at FADE_STRENGTH, so it dims the row's
/// content toward its own card and leaves the fill as it is.
pub fn fade_color(selected: bool, p: &Palette) -> egui::Color32 {
    row_fill(selected, false, p).gamma_multiply(FADE_STRENGTH)
}

/// A row cut by more than this at a viewport edge fades; less, and it reads as whole.
const CLIP_MIN: f32 = 8.0;
/// The fade's height at most: a short gradient over the cut row's visible strip only (24 read as the list
/// fading into the hint bar; round 10: ≤ 8).
const FADE_MAX: f32 = 8.0;
/// The fade's strongest alpha, the same in both themes.
pub const FADE_STRENGTH: f32 = 0.5;

/// A bottom fade's last y: one physical pixel row short of the viewport's edge, so the fade never lands on the
/// hint bar's border row (C2).
pub fn fade_end(edge: f32, ppp: f32) -> f32 {
    edge - 1.0 / ppp
}

/// The detail pane's bottom fade (E1): FADE_MAX over the viewport's bottom, ending a pixel row short of it, while
/// more than half a point of content lies below (`below`); none at the end of the content.
pub fn detail_fade(edge: f32, below: f32, ppp: f32) -> Option<(f32, f32)> {
    (below > 0.5).then(|| (edge - FADE_MAX, fade_end(edge, ppp)))
}

/// The strip to fade at a viewport edge (`edge`, the bottom when `bottom`, else the top), as (top, bottom):
/// the visible part of the row the edge cuts, at most FADE_MAX tall at the edge, and only when the row is cut
/// by more than CLIP_MIN. `rows` are the rows' (top, bottom) in screen space.
pub fn fade_strip(rows: &[(f32, f32)], edge: f32, bottom: bool) -> Option<(f32, f32)> {
    let &(top, bot) = rows.iter().find(|(t, b)| *t < edge && edge < *b)?;
    // The cut row's top border is the seam it shares with the last whole row: the fade starts under it, so that
    // row's separator keeps its tone (round 15 K1).
    if bottom {
        (bot - edge > CLIP_MIN).then(|| ((top - ROW_GAP).max(edge - FADE_MAX), edge))
    } else {
        (edge - top > CLIP_MIN).then(|| (edge, bot.min(edge + FADE_MAX)))
    }
}

/// The strip to hide at the viewport's bottom: a row showing less than CLIP_MIN there, from just under the
/// previous row's border (rows share their 1 px seam) to the edge, as (top, bottom).
pub fn bottom_sliver(rows: &[(f32, f32)], edge: f32) -> Option<(f32, f32)> {
    sliver_row(rows, edge).map(|i| (rows[i].0 + 1.0, edge))
}

/// The index of the row the viewport's bottom `edge` cuts to a sliver (showing less than CLIP_MIN), if any.
pub fn sliver_row(rows: &[(f32, f32)], edge: f32) -> Option<usize> {
    let i = rows.iter().position(|(t, b)| *t < edge && edge < *b)?;
    (edge - rows[i].0 < CLIP_MIN).then_some(i)
}

/// Between the count row's parts.
const COUNT_GAP: f32 = 7.0;

/// The count row's spinner → its verb, ink to ink, one gap for "Updating…" and "Loading issues…" (round 13 D4:
/// COUNT_GAP to the boxes read 8 and 8.5 with the letters' bearings).
pub const SPINNER_TO_LABEL: f32 = 8.0;

/// The space between the spinner's slot and a muted 12 `text` for SPINNER_TO_LABEL between their ink: the ring is
/// ICON_INK across, centred in its slot.
pub fn spinner_label_space(text_left_bearing: f32) -> f32 {
    let ring_inset = (crate::views::icons::INLINE_SPINNER - crate::views::icons::ICON_INK) / 2.0;
    SPINNER_TO_LABEL - ring_inset - text_left_bearing
}

/// The count row's spinner and its muted 12 verb, SPINNER_TO_LABEL apart to the ink.
fn spinner_label(ui: &mut Ui, p: &Palette, text: &str) {
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::body(12.0), p.muted);
    let lb = theme::ink_rect(&g).left().max(0.0);
    let gap = ui.spacing().item_spacing.x;
    crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
    // The layout's own item spacing already follows the spinner.
    ui.add_space(spinner_label_space(lb) - gap);
    ui.add(egui::Label::new(g));
}
/// The count row: its 12 px line box (the spinner and link fit), the line centred in it.
const COUNT_H: f32 = 16.0;
/// Search box → the count's ink, and the count's ink (its baseline) → the first card (round 14 B1: 11.5 / 7.5).
const COUNT_INK_ABOVE: f32 = 12.0;
const COUNT_INK_BELOW: f32 = 8.0;

/// The space before and after the count row for COUNT_INK_ABOVE / COUNT_INK_BELOW to the ink of a count line
/// laid out as `g` (`count_job`), centred in COUNT_H.
pub fn count_gaps(g: &egui::Galley) -> (f32, f32) {
    let ink = theme::ink_rect(g);
    let top = (COUNT_H - g.size().y) / 2.0;
    (COUNT_INK_ABOVE - top - ink.top(), COUNT_INK_BELOW - (COUNT_H - top - ink.bottom()))
}

/// `count_gaps` for this ui's fonts, from a plain count ("18 issues": digits and x-height, no descenders).
pub fn count_gaps_in(ui: &Ui, p: &Palette) -> (f32, f32) {
    let g = ui.ctx().fonts_mut(|f| {
        let job = count_job(f, "18 issues", p.text, "", false, p);
        f.layout_job(job)
    });
    count_gaps(&g)
}

/// The count row's box: COUNT_H tall, left to right, its contents centred.
pub fn count_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), COUNT_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(COUNT_H);
        add(ui)
    })
    .inner
}
/// Space between a row's pills.
pub const PILL_GAP: f32 = 8.0;
/// The mono key's left side bearing at 12.5: the pill row moves in by it so pills and key share an ink edge.
const KEY_INK_INSET: f32 = 1.0;
/// Clearance under the last row: a toast's height (~34) plus its 16 px gap, rounded up.
const LIST_BOTTOM_PAD: f32 = 56.0;
/// Rows stack as one list: −1 overlaps neighbouring borders, so a single 1 px divider separates rows, and
/// only the list's outer corners are rounded (no doubled lines or notches at the seams).
const ROW_GAP: f32 = -1.0;
/// Row inner vertical padding, measured edge (border included) to ink: 6 / title / pills / 6 (round 8; it
/// measured 5.5 / 5 / 7.5 with pads 0 / 1 / 6). The title's line box carries ~4.5 of leading above the caps,
/// so the top pad is 0.5. Round 10 measured the bottom 4.5 with a 4.5 pad (3.0 more than the old 1.5 estimate),
/// so the pill row moves up 1.5 (ROW_LINE_GAP) and the pad below grows 1.5: the row's height is unchanged and
/// the bottom lands on 6. ROW_LINE_GAP is the frame's item spacing, so it also follows the pill row: the bottom
/// space is 3.5 to net 2.0 under the pills. Margins are whole points, so the half points are spaces inside the
/// frame.
/// The row's left margin: with the mono key's 1 of bearing, its ink sits 12 inside the border (round 14 C2: 13).
const ROW_PAD_LEFT: i8 = 11;
const ROW_PAD_TOP: i8 = 0;
const ROW_PAD_TOP_EXTRA: f32 = 0.5;
const ROW_PAD_BOTTOM: i8 = 4;
const ROW_PAD_BOTTOM_EXTRA: f32 = 1.5;
/// Round 15 C1: the pill line 1 lower (−0.5, was −1.5) and 1 less under it, so the row reads 2 / 4 / 5.
const ROW_LINE_GAP: f32 = -0.5;
/// The row frame's border width (egui 0.36 frames draw the stroke inside, ahead of the margin).
const ROW_STROKE: f32 = 1.0;
/// Row top edge → the title's cap ink (the border, the 0.5 pad and the title's ~4.5 leading).
const ROW_INK_TOP: f32 = 6.0;
/// The title line as the row grid counts it: its 18 slot less the 4.5 leading over the caps, the 1.5 the pills
/// moved up in round 10, and the 4 gap.
const TITLE_LINE: f32 = 8.0;
/// Title line → pill line (round 15 C1: 4 read 2 from the descenders to the pills).
const TITLE_TO_PILLS: f32 = 5.0;
/// How far the key and summary are painted above their slot, so the summary's descenders clear the pills (round 13
/// C1: they touched the pill line; round 15 C1: 3 above the caps → 2); the slot and the pill line (PILL_LINE_TOP)
/// do not move.
const TITLE_RAISE: f32 = 3.0;
/// The pill line's top below the row's top edge: 6 + title line + 5 (round 15 C1: row top + 19, 38 px at 2×).
pub const PILL_LINE_TOP: f32 = ROW_INK_TOP + TITLE_LINE + TITLE_TO_PILLS;

/// The pill line's y for a row whose outer top edge is `row_top`: PILL_LINE_TOP below it, on a whole physical
/// pixel, so it never depends on the title row's laid-out height, the row's index, or the scroll offset's fraction.
pub fn pill_line_y(row_top: f32, ppp: f32) -> f32 {
    ((row_top + PILL_LINE_TOP) * ppp).round() / ppp
}
/// One row's pitch (rows overlap by ROW_GAP's 1 px), for the density check: 44 − 2 from round 8's padding.
#[cfg(test)]
pub const ROW_PITCH: f32 = 42.0;

/// The 3 px epic stripe over a row card's left edge, snapped to whole physical pixels.
pub fn stripe_rect(row: Rect, ppp: f32) -> Rect {
    let snap = |v: f32| (v * ppp).round() / ppp;
    Rect::from_min_max(egui::pos2(snap(row.left()), row.top()), egui::pos2(snap(row.left() + 3.0), row.bottom()))
}

/// Stripe paint order key: false (first) for every row but the selected one (C3).
pub fn stripe_paints_last(idx: usize, selected: Option<usize>) -> bool {
    selected == Some(idx)
}

/// A row's epic stripe colour: the slot's stripe, except on a selected light row, where it takes the slot's text
/// tone (≥ 4.6:1 on #dfdfda; the stripe tone fell to 3.6, under dark's 3.9–5.1 there).
pub fn row_stripe(t: &theme::Tint, p: &Palette, selected: bool) -> egui::Color32 {
    if selected && !p.dark { t.fg } else { t.stripe }
}

/// The selected row's two rules, as (centre y, inset from each end): its top and bottom dividers in the
/// selected-row edge, on the list frame too at the outer edges (inset by the corner radius on the left; the right
/// side is square).
pub fn selected_dividers(rect: Rect, idx: usize, n: usize) -> Vec<(f32, f32)> {
    // An outer edge (the first row's top, the last row's bottom) is the list frame's rounded border: the rule
    // goes over it too, stopping where the corners start.
    let inset = |outer: bool| if outer { f32::from(theme::RADIUS) } else { 0.0 };
    vec![(rect.top() + 0.5, inset(idx == 0)), (rect.bottom() - 0.5, inset(idx + 1 == n))]
}

/// A status pill's fill: its wash resolved over the plain row, so the pill is one opaque colour on every row
/// (a wash composited over the selection shifted its colour, round 8). On a selected row a neutral wash that
/// would melt into the selection (the grey "New", 1.03:1 light, 1.10 dark) steps one notch (light #cdccc6 at
/// 1.20:1, dark #4a4a47 at 1.23), so the pill keeps a fill.
pub fn status_fill(t: &theme::Tint, p: &Palette, selected: bool) -> egui::Color32 {
    let fill = theme::over(t.bg, p.surface);
    let spread = fill.r().max(fill.g()).max(fill.b()) - fill.r().min(fill.g()).min(fill.b());
    if selected && spread <= 6 && theme::contrast(fill, p.selected_row) < SELECTED_PILL_STEP {
        if p.dark { egui::Color32::from_rgb(0x4a, 0x4a, 0x47) } else { egui::Color32::from_rgb(0xcd, 0xcc, 0xc6) }
    } else {
        fill
    }
}

/// A neutral status fill must stand this far off the selected row, or it steps (`status_fill`).
const SELECTED_PILL_STEP: f32 = 1.12;

/// The list's outer left corners are rounded (6); the seams between rows are square, and so is the right side,
/// which has no border (`RIGHT_EDGE_OPEN`): the pane divider 16 px away is the one rule there (K3).
pub fn row_corners(idx: usize, n: usize) -> CornerRadius {
    let r = theme::RADIUS;
    let (top, bottom) = (if idx == 0 { r } else { 0 }, if idx + 1 == n { r } else { 0 });
    CornerRadius { nw: top, ne: 0, sw: bottom, se: 0 }
}

/// The list card draws no right border (two parallel rules 16 apart with the pane divider read as noise).
pub const RIGHT_EDGE_OPEN: bool = true;

/// The span over a row's right border to paint in the row's fill, as (x, top, bottom): the 1 px column inside
/// its right edge, between the top and bottom borders, so those still run to the edge.
pub fn right_border_cover(rect: Rect) -> (f32, f32, f32) {
    (rect.right() - 0.5, rect.top() + 1.0, rect.bottom() - 1.0)
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
        Tok::K(s) => ui.painter().layout_job(key_job(s, p.text)),
        Tok::W(s) => ui.painter().layout_no_wrap(s.to_string(), theme::body(12.0), p.muted),
    };
    let word = |s: &str| ui.painter().layout_no_wrap(s.to_string(), theme::body(12.0), p.muted);
    // The glyphs' rasters as the tessellator places them (`ink_rect`), so with the origins on whole pixels every
    // gap lands exactly, after a mono run too (round 14 J3: 4.5 after "Enter", 4 after "⌘1").
    // Trimmed to the rasters' visible coverage (round 15 H1/H2: a faint outer column read 4.5 and 12.5).
    let ctx = ui.ctx().clone();
    let ink = |g: &std::sync::Arc<egui::Galley>| {
        let r = theme::visible_ink(&ctx, g);
        if r.is_positive() { (r.left(), r.right()) } else { (0.0, 0.0) }
    };
    let ink_w = |g: &std::sync::Arc<egui::Galley>| {
        let (l, r) = ink(g);
        r - l
    };
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
    let ppp = ui.ctx().pixels_per_point();
    for (g, x) in items.into_iter().zip(xs) {
        let top = line - crate::views::galley_baseline(&g);
        // Each galley carries its own colour (keys `text`, words `muted`).
        ui.painter().galley(egui::pos2(snap_px(rect.left() + x, ppp), top), g, p.muted);
    }
}

/// A hint key's runs: modifier glyphs (⌘ ⇧ ⌥ ⌃, which SF Mono lacks or draws thin) apart from the rest, as
/// (text, mono), so the letters stay in the mono face whatever the fallback does with the modifier (I2). A pair's
/// "/" is a run of its own, so `key_job` can close it up between letters.
pub fn key_runs(key: &str) -> Vec<(String, bool)> {
    let modifier = |c: char| matches!(c, '⌘' | '⇧' | '⌥' | '⌃');
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut alone = false;
    for c in key.chars() {
        let mono = !modifier(c);
        let slash = c == '/' && key.len() > 1;
        match out.last_mut() {
            Some((t, m)) if *m == mono && !slash && !alone => t.push(c),
            _ => out.push((c.to_string(), mono)),
        }
        alone = slash;
    }
    out
}

/// How far a pair's "/" closes up on a mono letter beside it: SF Mono's narrow letters sit in wide cells, so
/// "j/k" read "j / k" beside "↑/↓", whose arrows fill theirs (I2: one pair style, no spaces around the slash).
pub const PAIR_TIGHTEN: f32 = 1.5;

/// The leading space before run `i` of `runs`: −PAIR_TIGHTEN on each side of a "/" that touches an ASCII letter
/// or digit, else 0.
pub fn run_lead(runs: &[(String, bool)], i: usize) -> f32 {
    let alnum = |s: Option<&(String, bool)>, last: bool| {
        s.and_then(|(t, _)| if last { t.chars().last() } else { t.chars().next() }).is_some_and(|c| c.is_ascii_alphanumeric())
    };
    let is_slash = |j: usize| runs.get(j).is_some_and(|(t, _)| t == "/");
    let before_slash = is_slash(i) && i > 0 && alnum(runs.get(i - 1), true);
    let after_slash = i > 0 && is_slash(i - 1) && alnum(runs.get(i), false);
    if before_slash || after_slash { -PAIR_TIGHTEN } else { 0.0 }
}

/// A hint key as one galley of two kinds of run: modifiers in the 12.5 system face, the rest in the key font,
/// a pair's "/" closed up on letters (`run_lead`). Help chips draw their keys with it too.
pub fn key_job(key: &str, color: egui::Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let runs = key_runs(key);
    for (i, (t, mono)) in runs.iter().enumerate() {
        let font_id = if *mono { theme::key_font() } else { theme::body(12.5) };
        job.append(t, run_lead(&runs, i), egui::TextFormat { font_id, color, ..Default::default() });
    }
    job
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
        Frame::new().fill(crate::views::dialog_fill(p)).stroke(crate::views::dialog_stroke(p)).corner_radius(theme::RADIUS).inner_margin(GOTO_MARGIN).show(
            ui,
            |ui| {
                crate::views::dialog_fields(ui, p);
                ui.set_width(goto_inner_w(crate::views::dialog_stroke(p).width));
                goto_title(ui, p);
                // Proportional 14 like the search box: mono is for rendered ids, not input.
                let r = crate::views::add_field(ui, None, text, |t| crate::views::input_field(t, "9001, CI-9001, or a browse link", theme::body(14.0), p));
                // The box keeps focus while open, so Enter counts whether or not the field just gave it up.
                if (r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                } else {
                    r.request_focus();
                }
            },
        );
    });
    if go {
        app.actions.push(Action::GoTo);
    }
}

/// The go-to box's outer width and padding.
const GOTO_W: f32 = 444.0;
const GOTO_PAD: f32 = 12.0;
/// The box's margin: GOTO_PAD at the sides and bottom; the top is the title's, set to its ink (`goto_title`).
const GOTO_MARGIN: Margin = Margin { left: GOTO_PAD as i8, right: GOTO_PAD as i8, top: 0, bottom: GOTO_PAD as i8 };
pub const GOTO_TITLE: &str = "Go to issue";

/// The go-to title: GOTO_PAD from the box's inner edge to its ink, and GOTO_PAD from its ink to the field
/// (round 13 I1: 15 above and 13.5 below to the line box's 12 and 4).
fn goto_title(ui: &mut egui::Ui, p: &Palette) {
    let g = ui.painter().layout_no_wrap(GOTO_TITLE.to_string(), theme::bold(13.5), p.text);
    let ink = theme::ink_rect(&g);
    // The frame's content starts inside its edge.
    ui.add_space(GOTO_PAD - ink.top());
    let h = g.size().y;
    ui.add(egui::Label::new(g));
    ui.add_space(crate::views::pickers::gap_space(GOTO_PAD - (h - ink.bottom()), ui.spacing().item_spacing.y));
}

/// The go-to box's content width: GOTO_W less the padding and the frame's stroke, which egui adds outside the
/// margin (it made the box 446 wide and 1 px right of centre; round 12 I2).
pub fn goto_inner_w(stroke: f32) -> f32 {
    GOTO_W - 2.0 * (GOTO_PAD + stroke)
}

/// The go-to box's top edge: 12 px below the header rule.
pub fn goto_top() -> f32 {
    crate::views::HEADER_H + 12.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_goto_box_is_centred_at_its_width() {
        // I2: outer width (content + padding + stroke) is GOTO_W, so round((window - GOTO_W) / 2) centres it.
        let stroke = crate::views::dialog_stroke(&theme::LIGHT).width;
        assert_eq!(goto_inner_w(stroke) + 2.0 * GOTO_PAD + 2.0 * stroke, GOTO_W);
        let x = ((1440.0 - GOTO_W) / 2.0_f32).round();
        assert_eq!(x, 1440.0 - (x + GOTO_W), "equal margins at 1440");
    }

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
        // D1: the suffix is its own (muted) part; the count alone keeps `text`.
        assert_eq!(count_parts(2, None, false, CountSuffix::Jql), ("2 issues".into(), " · JQL".into()));
        assert_eq!(count_parts(3, Some(9), false, CountSuffix::Text), ("3 of 9 issues".into(), " · text match".into()));
        assert_eq!(count_parts(5, None, false, CountSuffix::None), ("5 issues".into(), String::new()));
        assert_eq!(count_label(3, Some(20), false, CountSuffix::None), "3 of 20 issues", "local text filter");
        assert_eq!(count_label(20, Some(20), false, CountSuffix::None), "20 issues", "a filter that hides nothing");
        // B1: Mine and Roadmap narrow locally too, so they count "N of M" like the text filter; server results don't.
        assert!(locally_narrowed(true, false, false), "Mine");
        assert!(locally_narrowed(false, true, false), "Roadmap");
        assert!(locally_narrowed(false, false, true), "text filter");
        assert!(!locally_narrowed(false, false, false), "server results alone: N issues");
        assert_eq!(count_label(4, Some(18), false, CountSuffix::None), "4 of 18 issues", "Mine or Roadmap");
        assert_eq!(count_label(3, Some(9), false, CountSuffix::Text), "3 of 9 issues · text match", "Mine over a text match");
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
    fn the_goto_title_sits_12_from_edge_and_field() {
        // Round 13 I1: 12 from the box's inner edge to the title's ink, and 12 from that ink to the field.
        let p = crate::theme::LIGHT;
        let mut text = String::new();
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 400.0), |ui| {
            Frame::new().fill(crate::views::dialog_fill(&p)).stroke(crate::views::dialog_stroke(&p)).inner_margin(GOTO_MARGIN).show(ui, |ui| {
                crate::views::dialog_fields(ui, &p);
                ui.set_width(goto_inner_w(1.0));
                goto_title(ui, &p);
                ui.add(crate::views::input_field(&mut text, "Hint", theme::body(14.0), &p));
            });
        });
        let fills = crate::testutil::fills(&shapes);
        let dialog = fills.iter().find(|(_, c)| *c == crate::views::dialog_fill(&p)).expect("box").0;
        let field = fills.iter().find(|(_, c)| *c == theme::dialog_input_fill(&p)).expect("field").0;
        let ink = crate::testutil::ink_of(&shapes, GOTO_TITLE);
        assert!((ink.top() - dialog.top() - 1.0 - GOTO_PAD).abs() <= 0.5, "above {}", ink.top() - dialog.top() - 1.0);
        assert!((field.top() - ink.bottom() - GOTO_PAD).abs() <= 0.5, "below {}", field.top() - ink.bottom());
    }

    #[test]
    fn spinner_to_verb_is_one_ink_gap() {
        // Round 13 D4: "Updating…" and "Loading issues…" sit SPINNER_TO_LABEL from the ring's ink, real fonts.
        let p = crate::theme::LIGHT;
        for text in ["Updating…", "Loading issues…"] {
            let mut left = 0.0;
            let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 200.0), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = COUNT_GAP;
                    left = ui.cursor().left();
                    spinner_label(ui, &p, text);
                });
            });
            let ring_right = left + crate::views::icons::INLINE_SPINNER - (crate::views::icons::INLINE_SPINNER - crate::views::icons::ICON_INK) / 2.0;
            let gap = crate::testutil::ink_of(&shapes, text).left() - ring_right;
            assert!((gap - SPINNER_TO_LABEL).abs() <= 0.5, "{text}: {gap}");
        }
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
        assert_eq!(SEARCH_HINT, "Filter or search, Enter run as JQL (⌘F)", "the hint bar's words (B4)");
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
        // Rows pad POPUP_ROW_PAD (7) either side; the frame is 10 (4 + stroke 1, each side); 2 px rounding room.
        assert_eq!(popup_width(190.0, 100.0, POPUP_ROW_PAD, 0.0, 500.0), 190.0, "the combo's width when the options fit");
        assert_eq!(popup_width(120.0, 100.0, POPUP_ROW_PAD, 0.0, 500.0), 126.0, "the widest option is never cut");
        assert_eq!(popup_width(190.0, 300.0, POPUP_ROW_PAD, 0.0, 500.0), 326.0, "grows for a wide option");
        let bar = popup_bar_style().allocated_width();
        assert_eq!(popup_width(190.0, 300.0, POPUP_ROW_PAD, bar, 500.0), (326.0 + bar).ceil(), "B1: room for the bar, so the widest row is never cut");
        assert_eq!(popup_width(190.0, 900.0, POPUP_ROW_PAD, 0.0, 500.0), 500.0, "capped at the pane's right edge");
        assert_eq!(popup_width(190.0, 900.0, POPUP_ROW_PAD, 0.0, 100.0), 190.0, "never narrower than the combo");
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
    fn count_dots_sit_on_whole_pixels() {
        // Round 15 D4: every "·" in the count row starts on the device-pixel grid; the text is unchanged.
        let _ = crate::testutil::paint_at2(egui::vec2(800.0, 100.0), |ui| {
            let p = theme::LIGHT;
            for (label, mode, note) in [("1 issue", " · JQL", false), ("18 issues", "", true), ("7 issues", " · text", true)] {
                let job = ui.ctx().fonts_mut(|f| {
                    let j = count_job(f, label, p.text, mode, note, &p);
                    snap_dots(f, j, 2.0)
                });
                let plain = ui.ctx().fonts_mut(|f| count_job(f, label, p.text, mode, note, &p)).text;
                assert_eq!(job.text, plain);
                let g = ui.painter().layout_job(job);
                for (i, gl) in g.rows[0].row.glyphs.iter().enumerate() {
                    if gl.chr == '·' {
                        let x = (g.rows[0].pos.x + gl.pos.x) * 2.0;
                        assert!((x - x.round()).abs() < 0.01, "{label}{mode}: dot {i} at {x} px");
                    }
                }
            }
        });
    }

    #[test]
    fn hint_gaps_hold_to_the_visible_ink() {
        // Round 15 H1 / H2: key → word 4 and 12 around the dots, to the rasters' visible coverage (the raster box
        // read 4.5 after "J/K", "c" and "Esc"). Real fonts, 2x.
        use crate::keys::Tok::{K, W};
        let hints: &[crate::keys::Hint] = &[&[K("J/K"), W("rank")], &[K("c"), W("copy")], &[K("Esc"), W("cancel")], &[K("j/k"), W("move")]];
        for p in [theme::LIGHT, theme::DARK] {
            let mut vis = Vec::new();
            let _ = crate::testutil::paint_at2(egui::vec2(1200.0, 40.0), |ui| {
                let shapes = ui.ctx().graphics_mut(|g| g.entry(ui.layer_id()).next_idx().0);
                hint_bar(ui, hints, &p);
                let texts = ui.ctx().graphics(|g| {
                    g.get(ui.layer_id())
                        .map(|l| {
                            l.all_entries()
                                .skip(shapes)
                                .filter_map(|c| match &c.shape {
                                    egui::Shape::Text(t) => Some((t.galley.clone(), t.pos)),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                });
                vis = texts
                    .into_iter()
                    .map(|(g, pos)| {
                        let r = theme::visible_ink(ui.ctx(), &g).translate((pos.to_vec2() * 2.0).round() / 2.0);
                        (g.job.text.clone(), r.left(), r.right())
                    })
                    .collect();
            });
            assert_eq!(vis.len(), 11, "{vis:?}");
            for w in vis.windows(2) {
                let gap = w[1].1 - w[0].2;
                let want = if w[0].0 == "·" || w[1].0 == "·" { SEP_SPACE } else { KEY_TO_WORD };
                assert!((gap - want).abs() < 0.01, "{} → {}: {gap}", w[0].0, w[1].0);
            }
        }
    }

    #[test]
    fn popups_show_whole_rows_and_a_bar_when_they_scroll() {
        assert_eq!(popup_scroll(3), (3.0 * POPUP_ROW_H, false));
        assert_eq!(popup_scroll(POPUP_MAX_ROWS), (POPUP_MAX_ROWS as f32 * POPUP_ROW_H, false));
        let (h, bar) = popup_scroll(35);
        assert!(bar, "35 epics: the bar shows while open");
        assert_eq!(h % POPUP_ROW_H, 0.0, "never a row cut mid-text");
        assert_eq!(POPUP_FRAME / 2.0 - 1.0, 4.0, "selected-row fill 4 inside the stroke");
        let t = popup_track(egui::pos2(10.0, 50.0), 200.0, 8.0 * POPUP_ROW_H);
        assert_eq!((t.top(), t.height()), (50.0, 8.0 * POPUP_ROW_H), "B2: the thumb's track starts at the rows' top");
    }

    #[test]
    fn the_count_keeps_one_baseline_with_the_enter_note() {
        // Round 14 I2: the mono "Enter" no longer lowers the count; the dot's space before it is NOTE_DOT_TIGHTEN
        // narrower than a word space.
        let p = theme::LIGHT;
        let (plain, noted, space, job) = crate::testutil::with_ui(|ui| {
            ui.ctx().fonts_mut(|f| {
                let (a, b) = (count_job(f, "18 issues", p.text, "", false, &p), count_job(f, "18 issues", p.text, "", true, &p));
                (f.layout_job(a), f.layout_job(b.clone()), f.glyph_width(&theme::body(12.0), ' '), b)
            })
        });
        let baseline = |g: &egui::Galley| g.rows[0].pos.y + g.rows[0].glyphs[0].pos.y;
        assert_eq!(baseline(&plain), baseline(&noted), "one baseline");
        assert_eq!(plain.size().y, noted.size().y, "one line box");
        assert_eq!(job.text, format!("18 issues{}", count_note(JQL_ENTER_NOTE)).replacen(" · ", " ·", 1));
        let enter = job.sections.iter().find(|s| job.text[s.byte_range.start.0..].starts_with("Enter")).expect("the key");
        assert!((enter.leading_space - (space - NOTE_DOT_TIGHTEN)).abs() < 1e-4);
    }

    #[test]
    fn the_count_sits_12_and_8_to_its_ink() {
        // Round 14 B1: 12 from the search box to the count's ink, 8 from its baseline to the first card. Real
        // fonts, 2x.
        for p in [theme::LIGHT, theme::DARK] {
            let shapes = crate::testutil::paint_at2(vec2(600.0, 300.0), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let (r, _) = ui.allocate_exact_size(vec2(300.0, 28.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, egui::Color32::RED);
                ui.add_space(count_gaps_in(ui, &p).0);
                count_row(ui, |ui| {
                    let job = ui.ctx().fonts_mut(|f| count_job(f, "18 issues", p.text, "", false, &p));
                    ui.label(job);
                });
                ui.add_space(count_gaps_in(ui, &p).1);
                let (r, _) = ui.allocate_exact_size(vec2(300.0, 44.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, egui::Color32::BLUE);
            });
            let fills = crate::testutil::fills(&shapes);
            let at = |c: egui::Color32| fills.iter().find(|(_, f)| *f == c).expect("a fill").0;
            let (search, card) = (at(egui::Color32::RED), at(egui::Color32::BLUE));
            let ink = crate::testutil::ink_of(&shapes, "18 issues");
            assert_eq!(ink.top() - search.bottom(), 12.0, "above");
            assert_eq!(card.top() - ink.bottom(), 8.0, "below");
        }
    }

    #[test]
    fn popups_stay_in_the_pane() {
        assert_eq!(popup_left(300.0, 200.0, 16.0, 588.0), 300.0, "fits under its combo");
        assert_eq!(popup_left(300.0, 400.0, 16.0, 588.0), 188.0, "shifts left to end at the pane edge");
        assert_eq!(popup_left(300.0, 572.0, 16.0, 588.0), 16.0, "as wide as the pane: from its left edge");
        assert_eq!(popup_left_end(588.0, 200.0, 16.0), 388.0, "epic: right edge on the combo's, grows leftward");
        assert_eq!(popup_left_end(588.0, 700.0, 16.0), 16.0, "bounded by the pane");
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
        assert_eq!(shorten_epic("secondary-Environment", 12), "secondary…", "no hyphen stub (K2)");
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
        assert_eq!(row_corners(0, 1), r(6, 0, 6, 0));
        assert_eq!(row_corners(0, 3), r(6, 0, 0, 0));
        assert_eq!(row_corners(1, 3), r(0, 0, 0, 0));
        assert_eq!(row_corners(2, 3), r(0, 0, 6, 0));
        // No right border: the cover sits on its 1 px column, inside the top and bottom borders.
        let rect = Rect::from_min_max(egui::pos2(0.0, 10.0), egui::pos2(300.0, 52.0));
        assert_eq!(right_border_cover(rect), (299.5, 11.0, 51.0));
    }

    #[test]
    fn combo_chevron_and_popup_thumb_mirror_their_insets() {
        // B1: the chevron's ink box ends INNER_X (12) from the combo's right edge, like the label's left inset.
        let right = 300.0;
        assert_eq!(right - (chevron_centre_x(right) + CHEVRON_HALF_W + CHEVRON_STROKE / 2.0), INNER_X);
        // B2: the popup thumb sits 4 in (it measured 3, the frame margin alone), mirroring the rows' 4 on the left.
        assert_eq!(f32::from(POPUP_MARGIN) + POPUP_THUMB_OUTER, 4.0);
        // B3 (round 11): rows sit 4 inside the stroke on all four sides.
        assert_eq!(POPUP_MARGIN, POPUP_MARGIN_Y);
        assert_eq!(POPUP_MARGIN, 4);
    }

    #[test]
    fn the_count_says_enter_runs_unrun_jql() {
        // B4 (round 11): the note uses the bar's words and the suffixes' separator, in one line with the count.
        assert_eq!(JQL_ENTER_NOTE, "Enter run as JQL");
        assert_eq!(count_note(JQL_ENTER_NOTE), " · Enter run as JQL");
        assert!(JQL_ENTER_NOTE.starts_with(JQL_ENTER_KEY), "the note leads with its key");
        assert!(count_label(2, None, false, CountSuffix::Jql).ends_with(&count_note("JQL")), "one separator");
        // B2: the search box's text is centred like a combo's; round 15 B1: a point less above, half a point up.
        let m = crate::views::search_margin();
        assert_eq!((m.top, m.bottom), (3, 4));
        assert_eq!(f32::from(m.left), crate::views::SEARCH_PAD_X);
        assert!(jql_awaiting_enter("status = Blocked", None, false, false), "typed, not run");
        assert!(!jql_awaiting_enter("status = Blocked", Some("status = Blocked"), true, false), "its rows are on screen");
        assert!(jql_awaiting_enter("status = Done", Some("status = Blocked"), true, false), "edited after a run");
        assert!(!jql_awaiting_enter("status = Blocked", None, false, true), "in flight: the spinner says it");
        assert!(!jql_awaiting_enter("alpha", None, false, false), "plain text filters locally");
        assert!(!jql_awaiting_enter("  ", None, false, false));
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
        assert_eq!(fade_strip(&rows, 100.0, true), Some((92.0, 100.0)), "cut by 30: 8 px of its strip fade");
        assert_eq!(fade_strip(&rows, 120.0, true), Some((112.0, 120.0)), "a short gradient, at most 8");
        assert_eq!(fade_strip(&rows, 200.0, true), None, "nothing at the edge");
        assert_eq!(fade_strip(&rows, 20.0, false), Some((20.0, 28.0)), "the same rule at the top");
        assert_eq!(fade_strip(&rows, 5.0, false), None);
        assert_eq!(fade_strip(&rows, 94.5, true), Some((87.0, 94.5)), "K1: never over the seam above the cut row");
    }

    #[test]
    fn a_bottom_sliver_is_hidden() {
        let rows = [(0.0, 44.0), (43.0, 87.0), (86.0, 130.0)];
        assert_eq!(bottom_sliver(&rows, 92.0), Some((87.0, 92.0)), "6 px of the next row: hidden under the seam");
        assert_eq!(bottom_sliver(&rows, 100.0), None, "14 px shows (and fades)");
        assert_eq!(bottom_sliver(&rows, 130.0), None, "every row whole");
        assert_eq!(bottom_sliver(&rows, 87.0), Some((87.0, 87.0)), "only the shared seam: nothing to cover");
        assert_eq!(sliver_row(&rows, 92.0), Some(2), "its stripe is not drawn");
        assert_eq!(sliver_row(&rows, 100.0), None);
    }

    #[test]
    fn popups_sit_on_whole_pixels() {
        // B1: a fractional combo rect (and width) lands the popup frame on whole physical pixels.
        let r = snap_rect(Rect::from_min_max(egui::pos2(10.3, 100.26), egui::pos2(210.7, 128.26)), 2.0);
        assert_eq!((r.min.x, r.min.y, r.max.x, r.max.y), (10.5, 100.5, 210.5, 128.5));
        assert_eq!(snap_px(199.8, 1.0), 200.0);
        // The frame around whole rows stays whole: rows × 28 + 2 × (4 + 1).
        let (h, _) = popup_scroll(12);
        assert_eq!((h + POPUP_FRAME).fract(), 0.0);
    }

    #[test]
    fn list_fades_go_toward_the_cut_rows_fill() {
        // Round 14 K3: toward the card's fill, not `bg`, so the fill reads unchanged to the last pixel row.
        let rows = [(0.0, 44.0), (43.0, 87.0)];
        assert_eq!(cut_row(&rows, 60.0), Some(1));
        assert_eq!(cut_row(&rows, 100.0), None);
        let same = |a: egui::Color32, b: egui::Color32| a.to_array().iter().zip(b.to_array()).all(|(x, y)| x.abs_diff(y) <= 1);
        for p in [theme::LIGHT, theme::DARK] {
            assert!(same(theme::over(fade_color(false, &p), p.surface), p.surface), "a plain card's fill stays");
            assert!(same(theme::over(fade_color(true, &p), p.selected_row), p.selected_row), "a selected one's too");
        }
    }

    #[test]
    fn bottom_fades_stop_a_pixel_short() {
        // C2: the last pixel row (on the hint bar's border) is never faded; at 2x that is half a point.
        assert_eq!(fade_end(800.0, 2.0), 799.5);
        assert_eq!(fade_end(800.0, 1.0), 799.0);
        // E1: the detail pane fades its bottom 8 while content continues, and not at the end.
        assert_eq!(detail_fade(800.0, 120.0, 2.0), Some((792.0, 799.5)));
        assert_eq!(detail_fade(800.0, 0.0, 2.0), None, "scrolled to the end");
        assert_eq!(detail_fade(800.0, -40.0, 2.0), None, "content shorter than the pane");
        const { assert!(FADE_MAX <= 8.0, "the E1 ruling: at most 8") };
    }

    #[test]
    fn fifteen_rows_fit_at_1440_by_900() {
        // 900 less the header (48), the filter block (12 + 28 + 12 + 28 + 12 + 28 + 12, the count row, 8) and the hint bar.
        let list_h = 900.0
            - crate::views::HEADER_H
            - (12.0
                + 28.0
                + 12.0
                + 28.0
                + 12.0
                + crate::views::INPUT_H
                + crate::testutil::with_ui(|ui| {
                    let (above, below) = count_gaps_in(ui, &theme::LIGHT);
                    above + COUNT_H + below
                }))
            - HINT_BAR_H;
        assert!((list_h / ROW_PITCH).floor() >= 15.0, "{}", list_h / ROW_PITCH);
    }

    #[test]
    fn narrow_windows_share_the_list_pane() {
        assert_eq!(list_pane_width(1000.0).0, 412.0, "45% held back so the description keeps 400");
        assert_eq!(list_pane_width(1199.0).0, (1199.0f32 * 0.45).floor());
        assert!(list_pane_width(700.0).0 >= 320.0);
        let (w, _) = list_pane_width(1440.0);
        assert_eq!(w, (1440.0f32 * 0.42).clamp(360.0, 720.0));
    }

    #[test]
    fn list_pane_drags_as_wide_as_the_detail_allows() {
        use super::super::detail::min_pane_width;
        // Wide windows: no 720 cap, the detail pane keeps its minimum.
        assert_eq!(list_pane_width(1440.0).1, 320.0..=1440.0 - min_pane_width(1440.0));
        assert_eq!(*list_pane_width(3840.0).1.end(), 3840.0 - min_pane_width(3840.0));
        assert!(*list_pane_width(3840.0).1.end() > 720.0);
        // Narrow: past the default, down to the detail's floor.
        assert_eq!(list_pane_width(1000.0).1, 320.0..=1000.0 - min_pane_width(1000.0));
        // Too narrow for both: the range stays valid at the list's minimum.
        assert_eq!(list_pane_width(500.0).1, 320.0..=320.0);
        // The default always sits inside the range.
        for win in [500.0, 800.0, 1000.0, 1199.0, 1200.0, 1440.0, 2560.0] {
            let (w, r) = list_pane_width(win);
            assert!(r.contains(&w), "window {win}: {w} in {r:?}");
        }
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
    #[cfg_attr(not(target_os = "macos"), ignore = "glyph-ink metrics are calibrated on macOS font rasterisation")]
    fn every_hint_key_is_drawn_mono() {
        use crate::keys::{HintContext::*, Tok, hints};
        let all =
            [Help, GoTo, Search, SearchText, SearchEmpty, Composer, EditPicker, AssignPicker, Picker, Selected, Browse, SignIn, SelectedSearch, BrowseSearch];
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
    fn hint_keys_keep_letters_mono_beside_a_modifier() {
        assert_eq!(key_runs("⌘K"), [("⌘".to_string(), false), ("K".to_string(), true)]);
        assert_eq!(key_runs("⌘Enter"), [("⌘".to_string(), false), ("Enter".to_string(), true)]);
        assert_eq!(key_runs("Tab/⇧Tab"), [("Tab".to_string(), true), ("/".to_string(), true), ("⇧".to_string(), false), ("Tab".to_string(), true)]);
        assert_eq!(key_runs("j/k"), [("j".to_string(), true), ("/".to_string(), true), ("k".to_string(), true)]);
        assert_eq!(key_runs("/"), [("/".to_string(), true)], "the slash key itself");
        assert_eq!(key_runs("?"), [("?".to_string(), true)]);
        // I2: a "/" closes up on letters only (arrows fill their cells; ⇧ is its own run).
        let lead = |k: &str| key_job(k, egui::Color32::WHITE).sections.iter().map(|s| s.leading_space).collect::<Vec<_>>();
        assert_eq!(lead("j/k"), [0.0, -PAIR_TIGHTEN, -PAIR_TIGHTEN]);
        assert!(lead("↑/↓").iter().all(|&l| l == 0.0), "arrows fill their cells");
        assert_eq!(lead("Tab/⇧Tab"), [0.0, -PAIR_TIGHTEN, 0.0, 0.0]);
        let job = key_job("⌘F", egui::Color32::WHITE);
        assert_eq!(job.sections.len(), 2);
        assert_eq!(job.sections[1].format.font_id, theme::key_font(), "the letter is in the mono face");
        // The real fonts: the letter run's advance is the mono advance.
        crate::testutil::with_ui(|ui| {
            let g = ui.painter().layout_job(key_job("⌘F", egui::Color32::WHITE));
            let mono_f = ui.fonts_mut(|f| f.glyph_width(&theme::key_font(), 'F'));
            let glyphs = &g.rows[0].glyphs;
            let f = glyphs.iter().find(|gl| gl.chr == 'F').expect("an F");
            assert!((f.advance_width - mono_f).abs() < 0.01, "F {:.2} vs mono {mono_f:.2}", f.advance_width);
        });
    }

    #[test]
    fn status_pills_are_one_opaque_colour_on_every_row() {
        let statuses = ["New", "Planning", "Ready for Work", "In Progress", "In Review", "Blocked", "Done", "Waiting for support", "Work in Progress"];
        for p in [&theme::LIGHT, &theme::DARK] {
            for s in statuses {
                let t = theme::status_tint(s, "indeterminate", p, &Default::default());
                for sel in [false, true] {
                    let fill = status_fill(&t, p, sel);
                    assert_eq!(fill.a(), 255);
                    assert!(theme::contrast(t.fg, fill) >= 4.5, "dark={} {s} sel={sel}: {:.2}", p.dark, theme::contrast(t.fg, fill));
                }
            }
        }
    }

    #[test]
    fn new_pill_keeps_a_fill_on_a_selected_row() {
        for (p, step) in [(&theme::LIGHT, egui::Color32::from_rgb(0xcd, 0xcc, 0xc6)), (&theme::DARK, egui::Color32::from_rgb(0x4a, 0x4a, 0x47))] {
            let new = theme::status_tint("New", "new", p, &Default::default());
            assert_eq!(status_fill(&new, p, true), step, "dark={}", p.dark);
            assert_eq!(status_fill(&new, p, false), theme::over(new.bg, p.surface), "plain rows keep the wash");
            assert!(theme::contrast(step, p.selected_row) >= SELECTED_PILL_STEP);
            // Round 10 (L2): light and dark step about as far (1.20 / 1.23).
            assert!(theme::contrast(step, p.selected_row) >= 1.19, "dark={}: {:.3}", p.dark, theme::contrast(step, p.selected_row));
            // A hued wash keeps its colour on the selection.
            let ip = theme::status_tint("In Progress", "indeterminate", p, &Default::default());
            assert_eq!(status_fill(&ip, p, true), status_fill(&ip, p, false), "dark={}", p.dark);
        }
    }

    #[test]
    fn light_selected_stripes_take_the_text_tone() {
        for s in 0..theme::EPIC_SLOTS {
            let t = theme::slot_tint(s, &theme::LIGHT);
            assert_eq!(row_stripe(&t, &theme::LIGHT, true), t.fg);
            assert_eq!(row_stripe(&t, &theme::LIGHT, false), t.stripe);
            let c = theme::contrast(row_stripe(&t, &theme::LIGHT, true), theme::LIGHT.selected_row);
            assert!(c >= 4.6, "slot {s}: {c:.2}");
            let d = theme::slot_tint(s, &theme::DARK);
            assert_eq!(row_stripe(&d, &theme::DARK, true), d.stripe, "dark unchanged");
        }
    }

    #[test]
    fn the_pill_line_sits_at_one_fixed_y() {
        assert_eq!(PILL_LINE_TOP, 19.0, "row top + 19 (round 15 C1)");
        // It is where the flow puts it: border 1 + pad 0.5 + the 18 title slot + the −0.5 line gap.
        assert_eq!(ROW_STROKE + f32::from(ROW_PAD_TOP) + ROW_PAD_TOP_EXTRA + 18.0 + ROW_LINE_GAP, PILL_LINE_TOP);
        // Whole physical pixels at 1× and 2×, from fractional row tops (a scroll offset) too.
        assert_eq!(pill_line_y(100.0, 2.0), 119.0);
        assert_eq!(pill_line_y(100.3, 2.0), 119.5);
        assert_eq!(pill_line_y(100.2, 2.0), 119.0);
        assert_eq!(pill_line_y(100.6, 1.0), 120.0);
        // Every row index and either scroll fraction: the same offset from the row's (pixel-rounded) top.
        for idx in 0..20 {
            let top = 197.0 + idx as f32 * ROW_PITCH;
            assert_eq!(pill_line_y(top, 2.0) - top, PILL_LINE_TOP, "row {idx}");
        }
        // The 18 slot the old flow relied on: the title galley (17, fallback glyphs too) under egui's 18 interact
        // height. The pin no longer depends on it, but the row's height still does.
        crate::testutil::with_ui(|ui| {
            assert_eq!(ui.spacing().interact_size.y, 18.0);
            for t in ["Autoscaler spend dashboard", "Fix → thing", "Ship ✅ it", "日本語"] {
                let h = ui.painter().layout_no_wrap(t.to_string(), theme::bold(13.5), egui::Color32::WHITE).size().y;
                assert!(h <= 18.0, "{t:?} {h}");
            }
        });
    }

    #[test]
    fn the_selected_stripe_paints_last() {
        let mut order = [(0, ()), (1, ()), (2, ()), (3, ())];
        order.sort_by_key(|(i, _)| stripe_paints_last(*i, Some(1)));
        assert_eq!(order.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 2, 3, 1]);
        assert!(!stripe_paints_last(1, None));
    }

    #[test]
    fn light_selected_rows_outline_pills_in_the_text_tone() {
        let l = &theme::LIGHT;
        assert!(theme::contrast(crate::views::selected_pill_edge(l), l.selected_row) >= 3.3);
        let d = &theme::DARK;
        let c = theme::contrast(crate::views::selected_pill_edge(d), d.selected_row);
        assert!(c >= 3.0, "round 15 L1: the dark outline ≥ 3:1 on the selection: {c:.2}");
        let off_text = theme::contrast(crate::views::selected_pill_edge(d), crate::views::selected_pill_text(d));
        assert!(off_text >= 1.69, "still apart from the pill text: {off_text:.3}");
        let t = theme::slot_tint(0, l);
        assert_eq!(row_stripe(&t, l, true), t.fg, "the epic outline takes the stripe's tone");
    }

    #[test]
    fn row_padding_is_six_four_six() {
        // Edge to ink as round 8 measured it: above, the border 1 and the title's ~4.5 of leading over the caps
        // (5.5 with a 0 pad); below, 1.5 more than the pad (7.5 with 6).
        assert_eq!(1.0 + 4.5 + f32::from(ROW_PAD_TOP) + ROW_PAD_TOP_EXTRA, 6.0);
        // Round 10: the bottom measured 4.5 at a 4 + 0.5 pad (the pills' edge is their ink). The pill row moves up
        // 1.5; the item spacing also follows the pill row, so the space under it nets ROW_LINE_GAP + EXTRA.
        let under_pills = f32::from(ROW_PAD_BOTTOM) + ROW_PAD_BOTTOM_EXTRA + ROW_LINE_GAP;
        assert_eq!(under_pills, 5.0, "5 below the pills (round 15 C1)");
        assert_eq!(ROW_LINE_GAP, -0.5, "the pill row 1 lower than round 10's −1.5");
        // Inside the frame: 0.5 + title + gap + pills + gap + extra; it was 0.5 + title + pills + 0.5.
        assert_eq!(ROW_PAD_TOP_EXTRA + 2.0 * ROW_LINE_GAP + ROW_PAD_BOTTOM_EXTRA, 1.0, "the row keeps its height (pitch unchanged)");
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
