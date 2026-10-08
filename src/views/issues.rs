//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{outline_pill, pill, pill_outlined, small_link, toggle_chip};
use egui::{CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::collections::BTreeSet;

/// Hue slots for the epics of the rows on screen (see `theme::epic_slots`), shared by the list and detail.
pub fn visible_epic_slots(app: &App) -> std::collections::BTreeMap<String, usize> {
    let keys: Vec<String> = current_rows(app).iter().filter_map(|i| i.parent_key.clone()).collect();
    theme::epic_slots(keys.iter().map(String::as_str))
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

pub fn current_rows(app: &App) -> Vec<&Issue> {
    let src = app.rows_list();
    let searched = src == ListId::Search;
    let all = filter::action_items(&app.store.list(src));
    let me = app.me.as_ref().map(|m| m.display_name.as_str());
    if app.filter.roadmap {
        let f = filter::IssueFilter { roadmap_epics: roadmap_epics(app), ..app.filter.clone() };
        filter::visible(&all, &f, searched, me)
    } else {
        filter::visible(&all, &app.filter, searched, me)
    }
}

/// How a row pill is drawn: a filled tint (status, epic, warnings) or outline only (neutral values).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PillLook {
    Tint(egui::Color32, egui::Color32),
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

/// Fits a row's pills on one line: each gets Some(width) or None (dropped). When they do not all fit the
/// assignee pill goes first, then the epic pill is cut to the remaining room (dropped below EPIC_PILL_MIN).
pub fn fit_pills(pills: &[(PillKind, f32)], available: f32, spacing: f32) -> Vec<Option<f32>> {
    let total = |ws: &[Option<f32>]| {
        let n = ws.iter().flatten().count();
        ws.iter().flatten().sum::<f32>() + spacing * n.saturating_sub(1) as f32
    };
    let mut out: Vec<Option<f32>> = pills.iter().map(|(_, w)| Some(*w)).collect();
    if total(&out) <= available {
        return out;
    }
    for (o, (k, _)) in out.iter_mut().zip(pills) {
        if *k == PillKind::Assignee {
            *o = None;
        }
    }
    if total(&out) <= available {
        return out;
    }
    if let Some(e) = pills.iter().position(|(k, _)| *k == PillKind::Epic) {
        out[e] = None;
        let rest = total(&out);
        let room = available - rest - if out.iter().flatten().count() > 0 { spacing } else { 0.0 };
        out[e] = (room >= EPIC_PILL_MIN).then_some(room.min(pills[e].1).floor());
    }
    out
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

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let (list_w, list_range) = list_pane_width(ui.available_width());
    // First, so the strip spans the full width under both panels.
    egui::Panel::bottom("hints")
        .exact_size(HINT_BAR_H)
        .frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(16, 0)).stroke(Stroke::new(1.0, p.border)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let edit = app.picker.as_ref().map(|pk| matches!(pk, crate::app::Picker::Edit { .. }));
                let search_focused = ui.memory(|m| m.has_focus(egui::Id::new("issue-search")));
                let c =
                    crate::keys::hint_context(app.help_open, app.goto.is_some(), edit, app.selected.is_some(), ui.ctx().text_edit_focused(), search_focused);
                hint_bar(ui, crate::keys::hints(c), p);
            });
        });
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
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.surface).inner_margin(Margin { left: 24, right: 24, top: 16, bottom: 0 }))
        .show(ui, |ui| super::detail::show(app, ui));
    goto_box(app, ui.ctx(), p);
}

/// Below this window width the list pane is fixed near 370 px and the detail pane gets the rest.
const NARROW: f32 = 1200.0;

/// The list pane's default width and the range it may be dragged in: 42% of the window (360–720) when
/// wide; a fixed 360–380 band below NARROW, so the description keeps a readable measure.
pub fn list_pane_width(window_w: f32) -> (f32, std::ops::RangeInclusive<f32>) {
    if window_w < NARROW {
        (370.0, 360.0..=380.0)
    } else {
        let w = (window_w * 0.42).clamp(360.0, 720.0);
        (w, 320.0..=w)
    }
}

/// Gap between filter controls in a row.
const GAP: f32 = 8.0;
/// Vertical gap between the filter block's rows (combos, chips, search, count).
const V_GAP: f32 = 12.0;
/// One inner horizontal padding for combo text and the search box.
const INNER_X: f32 = 8.0;
/// Room a combo needs beside its text: the chevron and its padding.
const COMBO_CHROME: f32 = 16.0 + 16.0;
const COMBO_MIN: f32 = 120.0;

/// Three combo widths that fill exactly `available` (so the row ends where the search box does).
/// Each starts at its content width (text + chevron + 16), within 120 and half the row; slack is shared
/// evenly among those under the cap. An overfull row cuts the widest combos down to a
/// common width (never below 120); when even three minimums do not fit, all shrink in proportion.
pub fn combo_widths(natural: [f32; 3], available: f32) -> [f32; 3] {
    let room = (available - 2.0 * GAP).max(0.0);
    let max = (room / 2.0).max(COMBO_MIN);
    let mut w = natural.map(|n| n.clamp(COMBO_MIN, max));
    let sum: f32 = w.iter().sum();
    if sum > room {
        if 3.0 * COMBO_MIN > room {
            return [(room / 3.0).floor(); 3];
        }
        // Water-fill from the top: the cap c where Σ min(w, c) = room.
        let mut cap = room / 3.0;
        for _ in 0..3 {
            let (small, n) = w.iter().fold((0.0, 0), |(s, n), &x| if x < cap { (s + x, n) } else { (s, n + 1) });
            if n > 0 {
                cap = ((room - small) / n as f32).max(COMBO_MIN);
            }
        }
        let mut w = w.map(|x| x.min(cap).floor());
        w[2] = room - w[0] - w[1];
        return w;
    }
    // Share the slack evenly among combos still under the half-row cap.
    for _ in 0..3 {
        let slack = room - w.iter().sum::<f32>();
        let open = w.iter().filter(|&&x| x < max).count();
        if slack <= 0.5 || open == 0 {
            break;
        }
        let each = slack / open as f32;
        w = w.map(|x| if x < max { (x + each).min(max) } else { x });
    }
    let mut w = w.map(f32::floor);
    // Floors leave a pixel or two; the last combo takes them so the right edges meet.
    w[2] = room - w[0] - w[1];
    w
}

/// Every combo popup: at least the combo, wide enough for its widest row plus 24, at most the list pane.
pub fn popup_width(combo_w: f32, widest_row: f32, pane_w: f32) -> f32 {
    (widest_row + 24.0).max(combo_w).min(pane_w.max(combo_w))
}

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

/// A ComboBox confined to `w`, its selected text truncated with "…", its popup `popup_w` wide.
fn combo(ui: &mut Ui, salt: &str, text: String, w: f32, popup_w: f32, p: &Palette, contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui(vec2(w, crate::views::BUTTON_H), |ui| {
        egui::ComboBox::from_id_salt(salt).selected_text(text).width(w).truncate().icon(chevron(p.muted)).show_ui(ui, |ui| {
            ui.set_min_width(popup_w);
            ui.set_max_width(popup_w);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            contents(ui);
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

    let mut natural = [text_w(ui, &team_label), text_w(ui, &epic_label), text_w(ui, &sprint_label)].map(|t| t + COMBO_CHROME);
    let mut widths = combo_widths(natural, pane_w);
    let team_fits = natural[0] <= widths[0];
    let team_text = team_combo_label(&team_label, team_fits).to_string();
    if !team_fits {
        // The short label frees room for the others.
        natural[0] = text_w(ui, &team_text) + COMBO_CHROME;
        widths = combo_widths(natural, pane_w);
    }
    let [team_w, epic_w, sprint_w] = widths;
    let widest = |ui: &Ui, rows: &mut dyn Iterator<Item = String>| rows.map(|r| text_w(ui, &r)).fold(0.0, f32::max);
    let team_popup = popup_width(team_w, widest(ui, &mut team_options.iter().map(|(_, l)| l.clone())), pane_w);
    let epic_popup = popup_width(epic_w, widest(ui, &mut epics.iter().map(|(k, s)| format!("{}{}", entry(k, s), suffix(k)))), pane_w);
    let sprint_popup = popup_width(sprint_w, widest(ui, &mut sprint_options.iter().map(|(_, l)| l.clone())), pane_w);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        combo(ui, "team", team_text, team_w, team_popup, p, |ui| {
            for (scope, l) in &team_options {
                if ui.selectable_label(app.filter.team == *scope, l).clicked() {
                    app.actions.push(Action::SetTeam(scope.clone()));
                }
            }
        });
        combo(ui, "epic", epic_label, epic_w, epic_popup, p, |ui| {
            if ui.selectable_label(app.filter.epic.is_none(), "All epics").clicked() {
                app.actions.push(Action::SetEpic(None));
            }
            for (k, s) in &epics {
                // The roadmap suffix is muted: it qualifies the epic, it is not part of its name.
                let mut job = egui::text::LayoutJob::default();
                job.append(&entry(k, s), 0.0, egui::TextFormat { font_id: theme::body(14.0), color: p.text, ..Default::default() });
                job.append(suffix(k), 0.0, egui::TextFormat { font_id: theme::body(14.0), color: p.muted, ..Default::default() });
                job.wrap = egui::text::TextWrapping::truncate_at_width(epic_popup - 16.0);
                let full = format!("{}{}", entry(k, s), suffix(k));
                if ui.selectable_label(app.filter.epic.as_deref() == Some(k), job).on_hover_text(full).clicked() {
                    app.actions.push(Action::SetEpic(Some(k.clone())));
                }
            }
        });
        combo(ui, "sprint", sprint_label, sprint_w, sprint_popup, p, |ui| {
            for (scope, l) in &sprint_options {
                if ui.selectable_label(app.filter.sprint == *scope, l).clicked() {
                    app.actions.push(Action::SetSprint(scope.clone()));
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
            if toggle_chip(ui, &f.name, active, p).clicked() {
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
    let r = ui.add(
        egui::TextEdit::singleline(&mut app.filter.text)
            .id(id)
            .hint_text("Filter, or type JQL and press Enter (⌘F)")
            .desired_width(f32::INFINITY)
            .margin(vec2(INNER_X, 6.0)),
    );
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
/// count before it), "· JQL" for a typed search's rows, or (nothing to show yet) a loading line.
pub fn count_label(shown: usize, total: Option<usize>, loading_first: bool, jql: bool) -> String {
    if shown == 0 && loading_first {
        return "Loading issues…".to_string();
    }
    let noun = |n: usize| if n == 1 { "issue" } else { "issues" };
    let base = match total {
        Some(t) if t != shown => format!("{shown} of {t} {}", noun(t)),
        _ => format!("{shown} {}", noun(shown)),
    };
    if jql { format!("{base} · JQL") } else { base }
}

/// Whether the count says "· JQL": while a search is pending the rows (and so the label) are the
/// previous ones; once it lands, whether it was typed.
pub fn count_shows_jql(pending: bool, typed_search: bool, typed_shown: bool) -> bool {
    if pending { typed_shown } else { typed_search && typed_shown }
}

/// The widest count the label slot is sized for, so Clear and the spinner never shift.
const COUNT_SLOT_TEXT: &str = "000 issues · JQL";

/// The clear control shows whenever a row filter is on: a quick filter, Mine, Roadmap, or typed JQL.
pub fn clear_visible(quick: bool, mine: bool, roadmap: bool, typed_jql: bool) -> bool {
    quick || mine || roadmap || typed_jql
}

/// "Clear search" while a typed JQL search is active; "Clear filters" otherwise.
pub fn clear_label(typed_jql: bool) -> &'static str {
    if typed_jql { "Clear search" } else { "Clear filters" }
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
            (Some(jql), true) => Action::RunSearch(jql.clone()),
            _ => Action::Refresh,
        };
        app.actions.push(retry);
    }
    // One fixed height in every state, so the list below never moves.
    let pending = app.search_pending.is_some();
    let updating = pending || loaded.is_some_and(|l| l.loading);
    let loading_first = updating && rows.is_empty();
    let text_filter = !app.filter.text.trim().is_empty() && !filter::looks_like_jql(&app.filter.text);
    let total = text_filter.then(|| rows_without_text_filter(app));
    let label = count_label(rows.len(), total, loading_first, count_shows_jql(pending, app.typed_search, app.typed_shown));
    let typed = app.typed_search && app.search_active;
    let show_clear = clear_visible(app.filter.quick.is_some(), app.filter.mine, app.filter.roadmap, typed);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        ui.spacing_mut().item_spacing.x = 8.0;
        let slot = ui.painter().layout_no_wrap(COUNT_SLOT_TEXT.into(), theme::body(12.0), p.muted).size().x;
        let galley = ui.painter().layout_no_wrap(label, theme::body(12.0), p.muted);
        let (rect, _) = ui.allocate_exact_size(vec2(slot.max(galley.size().x), galley.size().y), Sense::hover());
        ui.painter().galley(rect.min, galley, p.muted);
        if updating && !loading_first {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            ui.label(RichText::new("updating…").color(p.muted).size(12.0));
        }
        if show_clear {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_link(ui, clear_label(typed), p).clicked() {
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
    let slots = theme::epic_slots(rows.iter().filter_map(|i| i.parent_key.as_deref()));
    let team_filter = match &app.filter.team {
        filter::TeamScope::Label(l) => Some(l.clone()),
        _ => None,
    };
    // Keyboard moves change the cursor; bring its row into view once per change.
    let scroll_cursor = app.cursor != app.last_cursor;
    app.last_cursor = app.cursor;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        for (idx, i) in rows.iter().enumerate() {
            let is_sel = selected.as_deref() == Some(&i.key);
            let epic = i.parent_key.as_deref().map(|k| theme::epic_tint_in(k, &slots, p, epic_colors));
            // The fill goes under the content once we know whether the pointer is over the row.
            let bg = ui.painter().add(egui::Shape::Noop);
            let r = Frame::new()
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(6)
                .inner_margin(Margin { left: 12, right: 12, top: ROW_PAD_TOP, bottom: ROW_PAD_BOTTOM })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&i.key).font(theme::mono(12.5)).color(p.muted)).selectable(false));
                        let painter = ui.painter().clone();
                        let title = crate::views::truncate_text(&i.summary, ui.available_width(), |t| {
                            painter.layout_no_wrap(t.to_string(), theme::bold(13.5), p.text).size().x
                        });
                        let r = ui.add(egui::Label::new(RichText::new(&title).font(theme::bold(13.5)).color(p.text)).extend().selectable(false));
                        if title != i.summary {
                            r.on_hover_text(&i.summary);
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
                            ("parent.summary" | "parent.key", Some(t)) => (PillKind::Epic, PillLook::Tint(t.bg, t.fg)),
                            ("parent.summary" | "parent.key", None) => (PillKind::Epic, PillLook::Outline),
                            ("assignee", _) => (PillKind::Assignee, PillLook::Outline),
                            _ => (PillKind::Other, PillLook::Outline),
                        };
                        pills.push((kind, v, look));
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(PILL_GAP, 4.0);
                        let painter = ui.painter().clone();
                        let measure = |t: &str| painter.layout_no_wrap(t.to_string(), theme::body(12.0), p.text).size().x;
                        let widths: Vec<(PillKind, f32)> = pills.iter().map(|(k, t, _)| (*k, measure(t) + 2.0 * crate::views::ROW_PILL_MARGIN.x)).collect();
                        let fit = fit_pills(&widths, ui.available_width(), PILL_GAP);
                        for ((_, text, look), w) in pills.iter().zip(fit) {
                            let Some(w) = w else { continue };
                            let text = crate::views::truncate_text(text, w - 2.0 * crate::views::ROW_PILL_MARGIN.x, measure);
                            match look {
                                PillLook::Tint(bg, fg) => pill_outlined(ui, &text, *bg, *fg),
                                PillLook::Outline => outline_pill(ui, &text, p, crate::views::ROW_PILL_MARGIN),
                            };
                        }
                    });
                });
            let rect = r.response.rect;
            let r = ui.interact(rect, row_id(&i.key), Sense::click());
            let fill = row_fill(is_sel, r.hovered(), p);
            ui.painter().set(bg, egui::epaint::RectShape::filled(rect, 6, fill));
            if let Some(t) = epic {
                ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, t.stripe);
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
}

/// Count row → first card.
const COUNT_TO_ROWS: f32 = 4.0;
/// Space between a row's pills.
const PILL_GAP: f32 = 6.0;
/// Clearance under the last row: a toast's height (~34) plus its 16 px gap, rounded up.
const LIST_BOTTOM_PAD: f32 = 56.0;
/// Cards stack edge to edge: −1 overlaps neighbouring borders, so one 1 px line separates rows.
const ROW_GAP: f32 = -1.0;
/// Card inner vertical padding (the pill line's descender room makes 3 below read as 4); with 2 px between
/// the lines and 8/1 pills the pitch is about 46 px, 14+ rows at 900 px.
const ROW_PAD_TOP: i8 = 4;
const ROW_PAD_BOTTOM: i8 = 3;

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

/// Keys in mono 12.5 `text`, words in 12 `muted`, " · " between hints; elided from the right with "…".
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
    let (n, elided) = crate::keys::fit_hints(&widths, width(one(" · ")), width(one("…")), ui.available_width());
    let mut job = LayoutJob::default();
    for (i, h) in hs.iter().take(n).enumerate() {
        if i > 0 {
            job.append(" · ", 0.0, muted.clone());
        }
        for (j, t) in h.iter().enumerate() {
            let (Tok::K(s) | Tok::W(s)) = t;
            job.append(s, if j > 0 { 4.0 } else { 0.0 }, fmt(t));
        }
    }
    if elided {
        job.append(if n > 0 { " · …" } else { "…" }, 0.0, muted);
    }
    ui.add(egui::Label::new(job).selectable(false).wrap_mode(egui::TextWrapMode::Extend));
}

fn goto_box(app: &mut App, ctx: &egui::Context, p: &Palette) {
    let Some(text) = app.goto.as_mut() else { return };
    let mut go = false;
    egui::Area::new(egui::Id::new("goto")).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 80.0)).show(ctx, |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(8).inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Go to issue").font(theme::bold(13.0)));
            let r = ui.add(egui::TextEdit::singleline(text).hint_text("9001, CI-9001, or a browse link").desired_width(f32::INFINITY));
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
        assert_eq!(count_label(0, None, true, false), "Loading issues…", "nothing yet: no count");
        assert_eq!(count_label(0, None, true, true), "Loading issues…");
        assert_eq!(count_label(0, None, false, false), "0 issues");
        assert_eq!(count_label(1, None, false, false), "1 issue");
        assert_eq!(count_label(20, None, true, false), "20 issues", "previous rows stay with their count while updating");
        assert_eq!(count_label(2, None, false, true), "2 issues · JQL");
        assert_eq!(count_label(3, Some(20), false, false), "3 of 20 issues", "local text filter");
        assert_eq!(count_label(20, Some(20), false, false), "20 issues", "a filter that hides nothing");
        assert_eq!(count_label(0, Some(1), false, false), "0 of 1 issue");
        assert_eq!(count_label(3, Some(20), false, true), "3 of 20 issues · JQL");
    }

    #[test]
    fn jql_suffix_waits_for_the_result() {
        assert!(!count_shows_jql(true, true, false), "typed search pending over plain rows: previous label");
        assert!(count_shows_jql(true, false, true), "a pending search over typed rows keeps \"· JQL\"");
        assert!(count_shows_jql(false, true, true), "landed");
        assert!(!count_shows_jql(false, false, true), "cleared");
        assert!(!count_shows_jql(false, true, false), "never landed");
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
    fn esc_clears_then_blurs() {
        assert_eq!(search_esc(true, false), SearchEsc::Clear);
        assert_eq!(search_esc(false, true), SearchEsc::Clear);
        assert_eq!(search_esc(false, false), SearchEsc::Blur);
    }

    #[test]
    fn combos_are_content_sized_and_fill_the_row() {
        let fill = |w: [f32; 3], avail: f32| (w.iter().sum::<f32>() + 2.0 * GAP - avail).abs() < 0.01;
        // Room for everything: each gets its content, the slack is shared, the row ends at the edge.
        let w = combo_widths([200.0, 100.0, 150.0], 600.0);
        assert!(fill(w, 600.0), "{w:?}");
        assert!(w[0] >= 200.0 && w[1] >= 120.0 && w[2] >= 150.0, "{w:?}");
        // Slightly overfull: only the widest gives way; the others keep their content width.
        let w = combo_widths([300.0, 110.0, 130.0], 500.0);
        assert!(fill(w, 500.0), "{w:?}");
        assert_eq!((w[1], w[2]), (120.0, 130.0), "{w:?}");
        // Slack is shared evenly, never past half the row.
        let w = combo_widths([130.0, 120.0, 120.0], 600.0);
        assert!(w.iter().all(|&x| x <= 292.0), "{w:?}");
        assert!(fill(w, 600.0) && w[0] - w[1] <= 10.0 + 1.0, "{w:?}");
        // Overfull: the widest are cut to a common width, the short epic keeps its 120 min.
        let w = combo_widths([182.0, 92.0, 137.0], 388.0);
        assert!(fill(w, 388.0), "{w:?}");
        assert_eq!(w[1], 120.0, "min");
        assert_eq!(w[0], 126.0, "{w:?}");
        // Too narrow even for three minimums: equal thirds within the row.
        let w = combo_widths([120.0, 120.0, 120.0], 300.0);
        assert!(w.iter().sum::<f32>() + 2.0 * GAP <= 300.01, "{w:?}");
    }

    #[test]
    fn one_popup_width_rule() {
        assert_eq!(popup_width(190.0, 100.0, 500.0), 190.0, "never narrower than the combo");
        assert_eq!(popup_width(190.0, 300.0, 500.0), 324.0, "widest row + 24");
        assert_eq!(popup_width(190.0, 900.0, 500.0), 500.0, "capped at the pane");
    }

    #[test]
    fn pills_fit_on_one_line() {
        use PillKind::*;
        let row = [(Other, 100.0), (Other, 40.0), (Epic, 150.0), (Assignee, 110.0)];
        // Everything fits: 400 + 3 gaps of 6.
        assert_eq!(fit_pills(&row, 418.0, 6.0), vec![Some(100.0), Some(40.0), Some(150.0), Some(110.0)]);
        // The assignee goes first.
        assert_eq!(fit_pills(&row, 417.0, 6.0), vec![Some(100.0), Some(40.0), Some(150.0), None]);
        assert_eq!(fit_pills(&row, 302.0, 6.0), vec![Some(100.0), Some(40.0), Some(150.0), None]);
        // Then the epic is cut to the room left (300 − 140 − 2 gaps).
        assert_eq!(fit_pills(&row, 300.0, 6.0), vec![Some(100.0), Some(40.0), Some(148.0), None]);
        assert_eq!(fit_pills(&row, 220.0, 6.0), vec![Some(100.0), Some(40.0), Some(68.0), None]);
        // Below the minimum it is dropped rather than shown as a stub.
        assert_eq!(fit_pills(&row, 200.0, 6.0), vec![Some(100.0), Some(40.0), None, None]);
        assert!(fit_pills(&[], 10.0, 6.0).is_empty());
    }

    #[test]
    fn labels_pill_skips_the_filtered_team() {
        let l = ["PLAT".to_string(), "infra".to_string()];
        assert_eq!(row_labels(&l, Some("PLAT")), "infra");
        assert_eq!(row_labels(&l, None), "PLAT, infra");
        assert_eq!(row_labels(&l[..1], Some("PLAT")), "", "nothing left: no pill");
    }

    #[test]
    fn narrow_windows_fix_the_list_pane() {
        assert_eq!(list_pane_width(1000.0), (370.0, 360.0..=380.0));
        assert_eq!(list_pane_width(1199.0).0, 370.0);
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
