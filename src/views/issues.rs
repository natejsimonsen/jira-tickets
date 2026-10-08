//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{button, pill, pill_outlined, toggle_button};
use egui::{CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::collections::BTreeSet;

/// Keys of the loaded epics labelled as roadmap initiatives (cheap: a few dozen epics).
pub fn roadmap_epics(app: &App) -> BTreeSet<String> {
    filter::roadmap_epic_keys(&app.store.list(ListId::Epics), &app.cfg.roadmap.initiative_label)
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

/// The row's leading pills: under All teams, its team labels in a neutral pill;
/// under one team, an amber "no <label> label" when it is the team's by membership only.
pub fn team_pills(ui: &mut Ui, f: &filter::IssueFilter, i: &Issue, p: &Palette, outlined: bool) {
    match &f.team {
        filter::TeamScope::All => {
            let teams: Vec<&str> = i.labels.iter().filter(|l| f.team_labels.contains(l)).map(String::as_str).collect();
            if !teams.is_empty() {
                row_pill(ui, &teams.join(" · "), p.surface_2, p.text, outlined, p);
            }
        }
        filter::TeamScope::Label(l) => {
            let members = f.team_members.get(l.as_str()).map(Vec::as_slice).unwrap_or_default();
            if filter::missing_team_label(i, l, members) {
                let t = theme::tint_from_hue(p.at_risk, p);
                row_pill(ui, &format!("no {l} label"), t.bg, t.fg, outlined, p);
            }
        }
        filter::TeamScope::None => {}
    }
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
    let width = ui.available_width();
    let list_w = (width * 0.42).clamp(360.0, 720.0);
    // First, so the strip spans the full width under both panels.
    egui::Panel::bottom("hints")
        .exact_size(HINT_BAR_H)
        .frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(16, 0)).stroke(Stroke::new(1.0, p.border)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let hint = crate::keys::hints(app.picker.is_some(), app.selected.is_some(), app.goto.is_some() || ui.ctx().text_edit_focused());
                ui.label(RichText::new(hint).font(theme::mono(12.0)).color(p.muted));
            });
        });
    egui::Panel::left("issue-list")
        .resizable(true)
        .default_size(list_w)
        .size_range(320.0..=list_w.max(320.0))
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 0 }))
        .show(ui, |ui| {
            ui.scope(|ui| filters(app, ui, p));
            ui.add_space(8.0);
            list(app, ui, p);
        });
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.surface).inner_margin(Margin { left: 24, right: 24, top: 16, bottom: 0 }))
        .show(ui, |ui| super::detail::show(app, ui));
    goto_box(app, ui.ctx(), p);
}

/// Gap between filter controls.
const GAP: f32 = 8.0;
/// One inner horizontal padding for combo text, chip text, and the search box.
const INNER_X: f32 = 8.0;
/// The epic popup never grows past this.
const EPIC_POPUP_MAX: f32 = 520.0;

/// Each of the three combos gets a third of the row after the two gaps.
pub fn combo_width(available: f32) -> f32 {
    ((available - 2.0 * GAP) / 3.0).floor().max(80.0)
}

/// The epic popup: at least the combo, wide enough for the longest entry plus room, at most 520.
pub fn epic_popup_width(combo_w: f32, longest_entry: f32) -> f32 {
    (longest_entry + 24.0).clamp(combo_w, EPIC_POPUP_MAX.max(combo_w))
}

/// A small muted chevron for the combos.
fn chevron(color: egui::Color32) -> impl FnOnce(&Ui, Rect, &egui::style::WidgetVisuals, bool) + 'static {
    move |ui, rect, _, _| {
        let c = rect.center();
        let pts = vec![c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)];
        ui.painter().add(egui::Shape::line(pts, Stroke::new(1.5, color)));
    }
}

/// A ComboBox confined to `w`, its selected text truncated with "…".
fn combo(ui: &mut Ui, salt: &str, text: String, w: f32, p: &Palette, contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui(vec2(w, crate::views::BUTTON_H), |ui| {
        egui::ComboBox::from_id_salt(salt).selected_text(text).width(w).truncate().icon(chevron(p.muted)).show_ui(ui, |ui| {
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
    let w = combo_width(ui.available_width());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        // Team scope: All, each configured sub-team, or tickets with no team label.
        let options = filter::team_options(&app.cfg.scope.teams);
        let label = options.iter().find(|(t, _)| *t == app.filter.team).map_or_else(
            || match &app.filter.team {
                filter::TeamScope::Label(l) => l.clone(),
                _ => "All teams".to_string(),
            },
            |(_, l)| l.clone(),
        );
        combo(ui, "team", label, w, p, |ui| {
            for (scope, l) in &options {
                if ui.selectable_label(app.filter.team == *scope, l).clicked() {
                    app.actions.push(Action::SetTeam(scope.clone()));
                }
            }
        });
        // Epic picker: parents of the scoped team's issues (other filters ignored), by summary.
        let src = app.rows_list();
        let mut epics: Vec<(String, String)> = filter::in_team(&app.store.list(src), &app.filter, src == ListId::Search)
            .iter()
            .filter_map(|i| Some((i.parent_key.clone()?, i.parent_summary.clone().unwrap_or_default())))
            .collect();
        epics.sort();
        epics.dedup();
        let on_roadmap = roadmap_epics(app);
        let entry = |k: &str, s: &str| if on_roadmap.contains(k) { format!("{k} · {s} · roadmap") } else { format!("{k} · {s}") };
        let label = app.filter.epic.as_ref().and_then(|k| epics.iter().find(|(key, _)| key == k)).map(|(k, s)| entry(k, s)).unwrap_or("All epics".into());
        let longest = epics.iter().map(|(k, s)| ui.painter().layout_no_wrap(entry(k, s), theme::body(14.0), p.text).size().x).fold(0.0, f32::max);
        let popup_w = epic_popup_width(w, longest);
        combo(ui, "epic", label, w, p, |ui| {
            ui.set_min_width(popup_w);
            ui.set_max_width(popup_w);
            if ui.selectable_label(app.filter.epic.is_none(), "All epics").clicked() {
                app.actions.push(Action::SetEpic(None));
            }
            for (k, s) in &epics {
                if ui.selectable_label(app.filter.epic.as_deref() == Some(k), entry(k, s)).on_hover_text(entry(k, s)).clicked() {
                    app.actions.push(Action::SetEpic(Some(k.clone())));
                }
            }
        });
        // Sprint scope: All, the board's active and future sprints, or Backlog.
        let options = crate::filter::sprint_options(&app.sprints);
        let label = options.iter().find(|(s, _)| *s == app.filter.sprint).map_or("All sprints".to_string(), |(_, l)| l.clone());
        combo(ui, "sprint", label, w, p, |ui| {
            for (scope, l) in &options {
                if ui.selectable_label(app.filter.sprint == *scope, l).clicked() {
                    app.actions.push(Action::SetSprint(scope.clone()));
                }
            }
        });
    });
    ui.add_space(GAP);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        let quick = app.cfg.issues.filters.clone();
        for (i, f) in quick.iter().enumerate() {
            let active = app.filter.quick == Some(i);
            if ui.add(toggle_button(&f.name, active, p)).clicked() {
                app.actions.push(Action::SetQuick(if active { None } else { Some(i) }));
            }
        }
        if ui.add(toggle_button("Mine", app.filter.mine, p)).clicked() {
            app.actions.push(Action::ToggleMine);
        }
        if ui.add(toggle_button("Roadmap", app.filter.roadmap, p)).on_hover_text("m").clicked() {
            app.actions.push(Action::ToggleRoadmap);
        }
    });
    ui.add_space(GAP);
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

/// The count row's text: a plain count, a JQL count, or (nothing to show yet) a loading line.
pub fn count_label(rows: usize, loading_first: bool, typed_jql: bool) -> String {
    match (rows, loading_first, typed_jql) {
        (0, true, _) => "Loading issues…".to_string(),
        (1, _, false) => "1 issue".to_string(),
        (1, _, true) => "1 issue · JQL".to_string(),
        (n, _, false) => format!("{n} issues"),
        (n, _, true) => format!("{n} issues · JQL"),
    }
}

/// "Clear search" for a typed JQL search; "Clear filters" when only a quick filter (or team scope) drives it.
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
    let updating = app.search_pending.is_some() || loaded.is_some_and(|l| l.loading);
    let loading_first = updating && rows.is_empty();
    ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(count_label(rows.len(), loading_first, app.typed_search)).color(p.muted).size(12.0));
        if app.search_active && ui.add(button(clear_label(app.typed_search), p)).clicked() {
            app.actions.push(Action::RunSearch(String::new()));
        }
        if updating && !loading_first {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            ui.label(RichText::new("updating…").color(p.muted).size(12.0));
        }
    });
    let columns = app.cfg.issues.columns.clone();
    let cfg = app.cfg.clone();
    let (status_colors, epic_colors) = (&cfg.issues.status_colors, &cfg.issues.epic_colors);
    let selected = app.selected.clone();
    // Keyboard moves change the cursor; bring its row into view once per change.
    let scroll_cursor = app.cursor != app.last_cursor;
    app.last_cursor = app.cursor;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        for (idx, i) in rows.iter().enumerate() {
            let is_sel = selected.as_deref() == Some(&i.key);
            let epic = i.parent_key.as_deref().map(|k| theme::epic_tint(k, p, epic_colors));
            // The fill goes under the content once we know whether the pointer is over the row.
            let bg = ui.painter().add(egui::Shape::Noop);
            let hovered_before = ui.ctx().read_response(row_id(&i.key)).is_some_and(|r| r.hovered());
            let outlined = is_sel || hovered_before;
            let r = Frame::new().stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::symmetric(12, ROW_PAD_Y)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(&i.key).font(theme::mono(12.5)).color(p.muted)).selectable(false));
                    ui.add(egui::Label::new(RichText::new(&i.summary).font(theme::bold(13.5)).color(p.text)).truncate().selectable(false))
                        .on_hover_text(&i.summary);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                    team_pills(ui, &app.filter, i, p, outlined);
                    for c in &columns {
                        let v = filter::column_value(i, c);
                        if v.is_empty() {
                            continue;
                        }
                        let (bg, fg) = match (c.as_str(), epic) {
                            ("status", _) => {
                                let t = theme::status_tint(&i.status.name, &i.status.category, p, status_colors);
                                (t.bg, t.fg)
                            }
                            ("parent.summary" | "parent.key", Some(t)) => (t.bg, t.fg),
                            _ => (p.surface_2, p.muted),
                        };
                        row_pill(ui, &v, bg, fg, outlined, p);
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
    });
}

/// Space between cards.
const ROW_GAP: f32 = 4.0;
/// Card inner vertical padding: 4 keeps the two-line card near a 48 pt pitch (≥14 rows at 900 px).
const ROW_PAD_Y: i8 = 4;

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

/// A row pill: outlined on selected or hovered rows, so its wash stays visible on the row fill.
fn row_pill(ui: &mut Ui, text: &str, bg: egui::Color32, fg: egui::Color32, outlined: bool, p: &Palette) {
    if outlined {
        pill_outlined(ui, text, bg, fg, p);
    } else {
        pill(ui, text, bg, fg);
    }
}

fn goto_box(app: &mut App, ctx: &egui::Context, p: &Palette) {
    let Some(text) = app.goto.as_mut() else { return };
    let mut go = false;
    egui::Area::new(egui::Id::new("goto")).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 80.0)).show(ctx, |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(8).inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Go to issue").font(theme::bold(13.0)));
            let r = ui.add(egui::TextEdit::singleline(text).hint_text("9001, CI-9001, or a browse link").desired_width(f32::INFINITY));
            r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
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
        assert_eq!(count_label(0, true, false), "Loading issues…", "nothing yet: no count");
        assert_eq!(count_label(0, true, true), "Loading issues…");
        assert_eq!(count_label(0, false, false), "0 issues");
        assert_eq!(count_label(1, false, false), "1 issue");
        assert_eq!(count_label(20, true, false), "20 issues", "previous rows stay with their count while updating");
        assert_eq!(count_label(2, false, true), "2 issues · JQL");
    }

    #[test]
    fn clear_button_names_what_it_clears() {
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
    fn combos_share_the_row() {
        assert_eq!(combo_width(600.0), 194.0);
        assert!(3.0 * combo_width(568.0) + 2.0 * GAP <= 568.0);
        assert_eq!(combo_width(100.0), 80.0, "floor");
    }

    #[test]
    fn epic_popup_is_capped() {
        assert_eq!(epic_popup_width(190.0, 100.0), 190.0, "never narrower than the combo");
        assert_eq!(epic_popup_width(190.0, 300.0), 324.0);
        assert_eq!(epic_popup_width(190.0, 900.0), 520.0);
    }
}
