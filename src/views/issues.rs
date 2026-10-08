//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{outline_pill, outline_pill_selected, pill, pill_outlined, small_link, toggle_chip};
use egui::{CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::collections::BTreeSet;

/// Gives hue slots to epics not seen yet this session: the loaded epic list in rank order, then the parents of
/// the rows on screen and of the selected issue (see `theme::extend_epic_slots`).
pub fn track_epics(app: &mut App) {
    let selected = app.selected.as_ref().and_then(|k| app.store.get(k)).and_then(|i| i.parent_key.clone());
    let keys: Vec<String> = app
        .store
        .list(ListId::Epics)
        .iter()
        .map(|e| e.key.clone())
        .chain(current_rows(app).iter().filter_map(|i| i.parent_key.clone()))
        .chain(selected)
        .collect();
    theme::extend_epic_slots(&mut app.epic_slots, keys.iter().map(String::as_str));
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

/// The row's leading pill: under All teams, its team labels in a neutral pill;
/// under one team, an amber "no <label> label" when it is the team's by membership only.
pub fn team_pill(f: &filter::IssueFilter, i: &Issue, p: &Palette) -> Option<(String, PillLook)> {
    match &f.team {
        filter::TeamScope::All => {
            let teams: Vec<&str> = i.labels.iter().filter(|l| f.team_labels.contains(l)).map(String::as_str).collect();
            (!teams.is_empty()).then(|| (teams.join(" · "), PillLook::Outline))
        }
        filter::TeamScope::Label(l) => {
            let members = f.team_members.get(l.as_str()).map(Vec::as_slice).unwrap_or_default();
            filter::missing_team_label(i, l, members).then(|| {
                let t = theme::tint_from_hue(p.at_risk, p);
                (format!("no {l} label"), PillLook::Tint(t.bg, t.fg))
            })
        }
        filter::TeamScope::None => None,
    }
}

/// The labels pill's text, without the label the team filter already names (every row would repeat it).
pub fn row_labels(labels: &[String], team_filter: Option<&str>) -> String {
    labels.iter().filter(|l| Some(l.as_str()) != team_filter).cloned().collect::<Vec<_>>().join(", ")
}

/// What a row pill is, for deciding what gives way when the row is narrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillKind {
    Assignee,
    Epic,
    Other,
}

/// An epic pill narrower than this is dropped rather than cut to a stub.
const EPIC_PILL_MIN: f32 = 64.0;

/// Below this row width no row shows the assignee pill; at or above it every row does (unless a row is so
/// crowded that even a cut epic does not fit). Decided once per list width, so rows never disagree.
const ASSIGNEE_ROW_MIN: f32 = 420.0;

pub fn rows_show_assignee(row_w: f32) -> bool {
    row_w >= ASSIGNEE_ROW_MIN
}

/// Fits a row's pills on one line: each gets Some(width) or None (dropped). When they do not all fit the
/// epic pill is cut to the remaining room (dropped below EPIC_PILL_MIN); only when the rest still overflows
/// does the assignee go, and the epic gets the room it frees.
pub fn fit_pills(pills: &[(PillKind, f32)], available: f32, spacing: f32) -> Vec<Option<f32>> {
    let total = |ws: &[Option<f32>]| {
        let n = ws.iter().flatten().count();
        ws.iter().flatten().sum::<f32>() + spacing * n.saturating_sub(1) as f32
    };
    let mut out: Vec<Option<f32>> = pills.iter().map(|(_, w)| Some(*w)).collect();
    if total(&out) <= available {
        return out;
    }
    let epic = pills.iter().position(|(k, _)| *k == PillKind::Epic);
    let fit_epic = |out: &mut Vec<Option<f32>>| {
        if let Some(e) = epic {
            out[e] = None;
            let room = available - total(out) - if out.iter().flatten().count() > 0 { spacing } else { 0.0 };
            out[e] = (room >= EPIC_PILL_MIN).then_some(room.min(pills[e].1).floor());
        }
    };
    fit_epic(&mut out);
    if total(&out) <= available {
        return out;
    }
    for (o, (k, _)) in out.iter_mut().zip(pills) {
        if *k == PillKind::Assignee {
            *o = None;
        }
    }
    fit_epic(&mut out);
    out
}

/// An epic pill's tooltip: its key and name, so two epics with one name stay apart.
pub fn epic_tip(key: &str, name: &str) -> String {
    if name.is_empty() { key.to_string() } else { format!("{key} · {name}") }
}

/// The amber "no <label> label" pill; true when it was shown.
pub fn missing_label_pill(ui: &mut Ui, f: &filter::IssueFilter, i: &Issue, label: &str, p: &Palette) -> bool {
    let members = f.team_members.get(label).map(Vec::as_slice).unwrap_or_default();
    let missing = filter::missing_team_label(i, label, members);
    if missing {
        let t = theme::tint_from_hue(p.at_risk, p);
        pill(ui, &format!("no {label} label"), t.bg, t.fg);
    }
    missing
}

/// The hint bar's height; the toast anchors above it.
pub const HINT_BAR_H: f32 = 24.0;
/// The hint bar's inner margin: 15, so the first glyph's ink (its side bearing adds ~1) sits 16 in.
const HINT_INSET: f32 = 15.0;

/// The hint bar's fill: `surface_2` in both themes (a step off the white light surface, so the undimmed bar
/// under a modal does not glow next to the grey backdrop).
pub fn hint_bar_fill(p: &Palette) -> egui::Color32 {
    p.surface_2
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    track_epics(app);
    let (list_w, list_range) = list_pane_width(ui.available_width());
    // First, so the strip spans the full width under both panels.
    let bar = egui::Panel::bottom("hints")
        .exact_size(HINT_BAR_H)
        .frame(Frame::new().fill(hint_bar_fill(p)).inner_margin(Margin::symmetric(HINT_INSET as i8, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let edit = app.picker.as_ref().map(|pk| match pk {
                    crate::app::Picker::Edit { .. } => crate::keys::PickerHint::Edit,
                    crate::app::Picker::Assign { .. } => crate::keys::PickerHint::Assign,
                    _ => crate::keys::PickerHint::Other,
                });
                let focus = text_focus(ui, app);
                let c = crate::keys::hint_context(app.help_open, app.goto.is_some(), edit, app.selected.is_some(), focus);
                hint_bar(ui, crate::keys::hints(c), p);
            });
        });
    // A top rule only, like the header's bottom rule.
    let r = bar.response.rect;
    ui.painter().hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, p.border));
    egui::Panel::left("issue-list")
        .resizable(true)
        .default_size(list_w)
        .size_range(list_range)
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 0 }))
        .show(ui, |ui| {
            // The filter block's gaps are explicit (all V_GAP), so no implicit item spacing is added on top.
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.scope(|ui| filters(app, ui, p));
            ui.add_space(V_GAP);
            list(app, ui, p);
        });
    let detail = egui::CentralPanel::default()
        .frame(Frame::new().fill(p.surface).inner_margin(Margin { left: 24, right: 16, top: 16, bottom: 0 }))
        .show(ui, |ui| super::detail::show(app, ui));
    let r = detail.response.rect;
    app.detail_x = Some((r.left(), r.right()));
    goto_box(app, ui.ctx(), p);
}

/// Which text field holds the keyboard, for the hint bar.
fn text_focus(ui: &Ui, app: &App) -> crate::keys::Focus {
    use crate::keys::Focus;
    let has = |id: egui::Id| ui.memory(|m| m.has_focus(id));
    if has(egui::Id::new("issue-search")) {
        Focus::Search(search_esc(!app.filter.text.is_empty(), app.search_active) == SearchEsc::Clear)
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
/// What the detail pane needs beside the list below NARROW: its 24 + 16 margins, a 400 px description
/// column, the 16 px gutter and the 140 px fields column (`detail::detail_columns`).
const NARROW_DETAIL_MIN: f32 = 40.0 + 400.0 + 16.0 + 140.0;

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

/// The combo widths, in row order team, sprint, epic, and whether the team combo uses its short labels.
/// One rule for all three: each is its widest option (`*_nat`, text + chevron), never the current label, so
/// nothing moves when a selection changes; only the search box stretches. When the row is short, sprint is
/// never cut; epic gives way first, down to EPIC_MIN; then team falls back to its short labels, then is cut.
pub fn combo_widths(team_full_nat: f32, team_short_nat: f32, sprint_nat: f32, epic_nat: f32, available: f32) -> ([f32; 3], bool) {
    let room = (available - 2.0 * GAP).max(0.0);
    let sprint = sprint_nat.min(room).ceil();
    let epic_nat = epic_nat.ceil();
    for (team, short) in [(team_full_nat.ceil(), false), (team_short_nat.ceil(), true)] {
        let left = room - sprint - team;
        if left >= EPIC_MIN.min(epic_nat) {
            return ([team, sprint, epic_nat.min(left).floor()], short);
        }
    }
    let team = (room - sprint - EPIC_MIN).max(TEAM_MIN).floor();
    let epic = (room - sprint - team).max(0.0).min(epic_nat).floor();
    ([team, sprint, epic], true)
}

/// A combo popup's outer width, one rule for all three: the combo's width, or the widest option plus its
/// row padding either side (`row_pad`), the popup frame and 2 px of rounding room when that is wider;
/// never past `cap`, the room from the combo's left edge to the list pane's right edge.
pub fn popup_width(combo_w: f32, widest_text: f32, row_pad: f32, cap: f32) -> f32 {
    (widest_text + 2.0 * row_pad + POPUP_FRAME + 2.0).ceil().max(combo_w).min(cap.max(combo_w))
}

/// A popup frame's margin (6) and stroke (1), both sides.
const POPUP_FRAME: f32 = 14.0;

/// The team combo's label: the full "PLAT · Platform" when it fits, else just "PLAT".
pub fn team_combo_label(full: &str, fits: bool) -> &str {
    if fits { full } else { full.split(" · ").next().unwrap_or(full) }
}

/// A small muted chevron for the combos.
fn chevron(color: egui::Color32) -> impl FnOnce(&Ui, Rect, &egui::style::WidgetVisuals, bool) + 'static {
    move |ui, rect, _, _| {
        let c = rect.center();
        let pts = vec![c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)];
        ui.painter().add(egui::Shape::line(pts, Stroke::new(1.5, color)));
    }
}

/// A ComboBox confined to `w`, its selected text truncated with "…". Its popup follows `popup_width` from
/// the options' widest text (`widest`) and the popup's own row padding; `contents` gets the room a row's text has.
fn combo(ui: &mut Ui, salt: &str, text: String, w: f32, widest: f32, p: &Palette, contents: impl FnOnce(&mut Ui, f32)) {
    let cap = ui.max_rect().right() - ui.cursor().left();
    ui.allocate_ui(vec2(w, crate::views::BUTTON_H), |ui| {
        egui::ComboBox::from_id_salt(salt).selected_text(text).width(w).truncate().icon(chevron(p.muted)).show_ui(ui, |ui| {
            let pad = ui.spacing().button_padding.x;
            let inner = popup_width(w, widest, pad, cap) - POPUP_FRAME;
            ui.set_min_width(inner);
            ui.set_max_width(inner);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            contents(ui, inner - 2.0 * pad);
        });
    });
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

fn filters(app: &mut App, ui: &mut Ui, p: &Palette) {
    ui.spacing_mut().button_padding = vec2(INNER_X, 4.0);
    ui.spacing_mut().icon_spacing = CHEVRON_GAP;
    ui.spacing_mut().icon_width = CHEVRON_W;
    ui.spacing_mut().interact_size.y = crate::views::BUTTON_H;
    let pane_w = ui.available_width();
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
    let team_short_nat = widest(ui, &mut team_options.iter().map(|(_, l)| team_combo_label(l, false).to_string())) + COMBO_CHROME;
    let sprint_widest = widest(ui, &mut sprint_options.iter().map(|(_, l)| l.clone()));
    let sprint_nat = sprint_widest.max(text_w(ui, SPRINT_SAMPLE)) + COMBO_CHROME;
    let epic_widest = widest(ui, &mut epics.iter().map(|(k, s)| format!("{}{}", entry(k, s), suffix(k))).chain(["All epics".to_string()]));
    let ([team_w, sprint_w, epic_w], team_short) = combo_widths(team_widest + COMBO_CHROME, team_short_nat, sprint_nat, epic_widest + COMBO_CHROME, pane_w);
    let team_text = team_combo_label(&team_label, !team_short).to_string();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        combo(ui, "team", team_text, team_w, team_widest, p, |ui, _| {
            for (scope, l) in &team_options {
                if ui.selectable_label(app.filter.team == *scope, l).clicked() {
                    app.actions.push(Action::SetTeam(scope.clone()));
                }
            }
        });
        combo(ui, "sprint", sprint_label, sprint_w, sprint_widest, p, |ui, _| {
            for (scope, l) in &sprint_options {
                if ui.selectable_label(app.filter.sprint == *scope, l).clicked() {
                    app.actions.push(Action::SetSprint(scope.clone()));
                }
            }
        });
        combo(ui, "epic", epic_label, epic_w, epic_widest, p, |ui, row_text_w| {
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
    let r = ui.add(crate::views::input_field(&mut app.filter.text, "Filter, or type JQL and press Enter (⌘F)", theme::body(14.0), p).id(id));
    if r.changed() {
        app.text_edited = Some(std::time::Instant::now());
    }
    if app.focus_search {
        r.request_focus();
        app.focus_search = false;
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let t = app.filter.text.trim().to_string();
        // JQL has an operator; a bare word is a local text filter. The JQL stays in the box.
        if filter::looks_like_jql(&t) {
            app.actions.push(Action::RunTypedSearch(t));
        }
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
        CountSuffix::Text => format!("{base} · text"),
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
    let row_spinner = crate::views::spinner_slots(syncing, pending, app.search_from_refresh, loading_first).1;
    let text_filter = !app.filter.text.trim().is_empty() && !filter::looks_like_jql(&app.filter.text) && !text_rows_shown(app);
    let total = text_filter.then(|| rows_without_text_filter(app));
    let showing_search = src == ListId::Search && app.store.lists.contains_key(&ListId::Search);
    let label = count_label(rows.len(), total, loading_first, count_suffix(app.typed_shown, app.text_shown.is_some(), showing_search));
    let box_text = !app.filter.text.trim().is_empty() || (app.typed_search && app.search_active);
    let show_clear = clear_visible(app.filter.quick.is_some(), app.filter.mine, app.filter.roadmap, box_text);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        // 7 in code reads as 8 ink to ink: the 12 px text's side bearing adds about 1.
        ui.spacing_mut().item_spacing.x = COUNT_GAP;
        // One pattern, muted 12, the spinner before the verb: "[count] ⟳ Updating…", or "⟳ Loading issues…"
        // when there is nothing to count yet.
        let color = if loading_first { p.muted } else { count_color(updating, p) };
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
    // Row width less the 12 px card padding either side; one assignee decision for every row.
    let show_assignee = rows_show_assignee(ui.available_width() - 24.0);
    // egui's own fade dims whatever sits near an edge; the list fades only a row that is really cut (`fade_strip`).
    ui.spacing_mut().scroll.fade = egui::style::ScrollFadeStyle { strength: 0.0, size: 0.0 };
    let mut row_spans: Vec<(f32, f32)> = Vec::with_capacity(rows.len());
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
                    ui.spacing_mut().item_spacing.y = ROW_LINE_GAP;
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&i.key).font(theme::mono(12.5)).color(p.muted)).selectable(false));
                        let painter = ui.painter().clone();
                        let title = crate::views::truncate_text(&i.summary, ui.available_width(), |t| {
                            painter.layout_no_wrap(t.to_string(), theme::bold(13.5), p.text).size().x
                        });
                        let r = ui.add(egui::Label::new(RichText::new(&title).font(theme::bold(13.5)).color(p.text)).extend().selectable(false));
                        if title != i.summary {
                            tips.push((r.rect, i.summary.clone()));
                        }
                    });
                    // One line of pills, never wrapped: see `fit_pills`.
                    let mut pills: Vec<(PillKind, String, PillLook)> = Vec::new();
                    if let Some((t, look)) = team_pill(&app.filter, i, p) {
                        pills.push((PillKind::Other, t, look));
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
                                (PillKind::Other, PillLook::Tint(t.bg, t.fg))
                            }
                            ("parent.summary" | "parent.key", Some(t)) => (PillKind::Epic, PillLook::Epic(t.fg, t.stripe)),
                            ("parent.summary" | "parent.key", None) => (PillKind::Epic, PillLook::Outline),
                            ("assignee", _) => (PillKind::Assignee, PillLook::Outline),
                            _ => (PillKind::Other, PillLook::Outline),
                        };
                        pills.push((kind, v, look));
                    }
                    if !show_assignee {
                        pills.retain(|(k, _, _)| *k != PillKind::Assignee);
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(PILL_GAP, 4.0);
                        let painter = ui.painter().clone();
                        let measure = |t: &str| painter.layout_no_wrap(t.to_string(), theme::body(12.0), p.text).size().x;
                        let widths: Vec<(PillKind, f32)> = pills.iter().map(|(k, t, _)| (*k, measure(t) + 2.0 * crate::views::ROW_PILL_MARGIN.x)).collect();
                        let fit = fit_pills(&widths, ui.available_width(), PILL_GAP);
                        for ((kind, full, look), w) in pills.iter().zip(fit) {
                            let Some(w) = w else { continue };
                            let text = crate::views::truncate_text(full, w - 2.0 * crate::views::ROW_PILL_MARGIN.x, measure);
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
                let stripe = CornerRadius { nw: corners.nw, sw: corners.sw, ne: 0, se: 0 };
                ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), stripe, t.stripe);
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
        // Room under the last row, so a toast (anchored above the hint bar) never covers it.
        ui.add_space(LIST_BOTTOM_PAD);
    });
    let view = out.inner_rect;
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

/// Between the count row's parts.
const COUNT_GAP: f32 = 7.0;
/// Count row → first card.
const COUNT_TO_ROWS: f32 = 4.0;
/// Space between a row's pills.
const PILL_GAP: f32 = 6.0;
/// Clearance under the last row: a toast's height (~34) plus its 16 px gap, rounded up.
const LIST_BOTTOM_PAD: f32 = 56.0;
/// Rows stack as one list: −1 overlaps neighbouring borders, so a single 1 px divider separates rows, and
/// only the list's outer corners are rounded (no doubled lines or notches at the seams).
const ROW_GAP: f32 = -1.0;
/// Row inner vertical padding. The title's line box carries ~4 px of leading above the caps, so 1 above reads
/// as ~5–6, as do the 4 under the pills; with 2 px between the lines (pills stay 16 tall) the pitch is 44 px,
/// rule 10's two-line row, and 15 full rows fit at 1440×900.
const ROW_PAD_TOP: i8 = 1;
const ROW_PAD_BOTTOM: i8 = 4;
const ROW_LINE_GAP: f32 = 2.0;
/// One row's pitch (rows overlap by ROW_GAP's 1 px), for the density check.
pub const ROW_PITCH: f32 = 44.0;

/// The list's outer corners are rounded (6); the seams between rows are square.
pub fn row_corners(idx: usize, n: usize) -> CornerRadius {
    let (top, bottom) = (if idx == 0 { 6 } else { 0 }, if idx + 1 == n { 6 } else { 0 });
    CornerRadius { nw: top, ne: top, sw: bottom, se: bottom }
}

fn row_id(key: &str) -> egui::Id {
    egui::Id::new(("issue-row", key))
}

/// One selection state: the selected row is filled; any other row fills only under the pointer.
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
    use egui::text::LayoutJob;
    let fmt = |t: &Tok| match t {
        Tok::K(_) => egui::TextFormat { font_id: theme::mono(12.5), color: p.text, ..Default::default() },
        Tok::W(_) => egui::TextFormat { font_id: theme::body(12.0), color: p.muted, ..Default::default() },
    };
    let muted = egui::TextFormat { font_id: theme::body(12.0), color: p.muted, ..Default::default() };
    let hint_job = |h: &[Tok]| {
        let mut job = LayoutJob::default();
        for (i, t) in h.iter().enumerate() {
            let (Tok::K(s) | Tok::W(s)) = t;
            job.append(s, if i > 0 { 4.0 } else { 0.0 }, fmt(t));
        }
        job
    };
    let width = |job: LayoutJob| ui.fonts_mut(|f| f.layout_job(job)).size().x;
    let one = |s: &str| {
        let mut j = LayoutJob::default();
        j.append(s, 0.0, muted.clone());
        j
    };
    let widths: Vec<f32> = hs.iter().map(|h| width(hint_job(h))).collect();
    let sep = width(one("·")) + 2.0 * SEP_SPACE;
    let (n, elided) = crate::keys::fit_hints(&widths, sep, width(one("…")), ui.available_width());
    let mut job = LayoutJob::default();
    for (i, h) in hs.iter().take(n).enumerate() {
        if i > 0 {
            job.append("·", SEP_SPACE, muted.clone());
        }
        for (j, t) in h.iter().enumerate() {
            let (Tok::K(s) | Tok::W(s)) = t;
            let lead = match (i, j) {
                (_, j) if j > 0 => 4.0,
                (0, 0) => 0.0,
                _ => SEP_SPACE,
            };
            job.append(s, lead, fmt(t));
        }
    }
    if elided {
        if n > 0 {
            job.append("·", SEP_SPACE, muted.clone());
        }
        job.append("…", if n > 0 { SEP_SPACE } else { 0.0 }, muted);
    }
    ui.add(egui::Label::new(job).selectable(false).wrap_mode(egui::TextWrapMode::Extend));
}

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
            .corner_radius(6)
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

/// The go-to box's top edge: 12 px below the 48 px header.
pub fn goto_top() -> f32 {
    48.0 + 12.0
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
        assert_eq!(count_label(14, None, false, CountSuffix::Text), "14 issues · text", "a text search's rows");
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
        let gap = goto_top() - 48.0;
        assert!((8.0..=16.0).contains(&gap), "{gap}");
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
        // Rows pad 12 either side; the frame is 14; 2 px rounding room.
        assert_eq!(popup_width(190.0, 100.0, 12.0, 500.0), 190.0, "the combo's width when the options fit");
        assert_eq!(popup_width(120.0, 100.0, 12.0, 500.0), 140.0, "the widest option is never cut");
        assert_eq!(popup_width(190.0, 300.0, 12.0, 500.0), 340.0, "grows for a wide option");
        assert_eq!(popup_width(190.0, 900.0, 12.0, 500.0), 500.0, "capped at the pane's right edge");
        assert_eq!(popup_width(190.0, 900.0, 12.0, 100.0), 190.0, "never narrower than the combo");
    }

    #[test]
    fn pills_fit_on_one_line() {
        use PillKind::*;
        let row = [(Other, 100.0), (Other, 40.0), (Epic, 150.0), (Assignee, 110.0)];
        // Everything fits: 400 + 3 gaps of 6.
        assert_eq!(fit_pills(&row, 418.0, 6.0), vec![Some(100.0), Some(40.0), Some(150.0), Some(110.0)]);
        // The epic is cut first (417 − 250 − 3 gaps), keeping the assignee.
        assert_eq!(fit_pills(&row, 417.0, 6.0), vec![Some(100.0), Some(40.0), Some(149.0), Some(110.0)]);
        assert_eq!(fit_pills(&row, 332.0, 6.0), vec![Some(100.0), Some(40.0), Some(64.0), Some(110.0)]);
        // Below the epic minimum it is dropped; while the rest fits the assignee stays.
        assert_eq!(fit_pills(&row, 300.0, 6.0), vec![Some(100.0), Some(40.0), None, Some(110.0)]);
        // Only when even that overflows does the assignee go, and the epic gets its room back.
        assert_eq!(fit_pills(&row, 250.0, 6.0), vec![Some(100.0), Some(40.0), Some(98.0), None]);
        assert_eq!(fit_pills(&row, 200.0, 6.0), vec![Some(100.0), Some(40.0), None, None]);
        assert!(fit_pills(&[], 10.0, 6.0).is_empty());
    }

    #[test]
    fn assignee_shows_on_all_rows_or_none() {
        assert!(!rows_show_assignee(314.0), "1000 px window");
        assert!(rows_show_assignee(548.0), "1440 px window");
        assert!(rows_show_assignee(ASSIGNEE_ROW_MIN));
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
    fn fifteen_rows_fit_at_1440_by_900() {
        // 900 less the header (48), the filter block (12 + 28 + 12 + 28 + 12 + 32 + 12 + 28 + 4) and the hint bar.
        let list_h = 900.0 - 48.0 - (12.0 + 28.0 + 12.0 + 28.0 + 12.0 + 32.0 + 12.0 + 28.0 + 4.0) - HINT_BAR_H;
        assert!((list_h / ROW_PITCH).floor() >= 15.0, "{}", list_h / ROW_PITCH);
    }

    #[test]
    fn narrow_windows_share_the_list_pane() {
        assert_eq!(list_pane_width(1000.0), (404.0, 320.0..=404.0), "45% held back so the description keeps 400");
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
}
