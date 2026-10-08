//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{button, pill, toggle_button};
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
pub fn team_pills(ui: &mut Ui, f: &filter::IssueFilter, i: &Issue, p: &Palette) {
    match &f.team {
        filter::TeamScope::All => {
            let teams: Vec<&str> = i.labels.iter().filter(|l| f.team_labels.contains(l)).map(String::as_str).collect();
            if !teams.is_empty() {
                pill(ui, &teams.join(" · "), p.surface_2, p.text);
            }
        }
        filter::TeamScope::Label(l) => {
            missing_label_pill(ui, f, i, l, p);
        }
        filter::TeamScope::None => {}
    }
}

/// The amber "no <label> label" pill; true when it was shown.
pub fn missing_label_pill(ui: &mut Ui, f: &filter::IssueFilter, i: &Issue, label: &str, p: &Palette) -> bool {
    let members = f.team_members.get(label).map(Vec::as_slice).unwrap_or_default();
    let missing = filter::missing_team_label(i, label, members);
    if missing {
        pill(ui, &format!("no {label} label"), p.at_risk.gamma_multiply(0.15), p.at_risk);
    }
    missing
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let width = ui.available_width();
    let list_w = (width * 0.42).clamp(360.0, 720.0);
    // First, so the strip spans the full width under both panels.
    egui::Panel::bottom("hints")
        .exact_size(24.0)
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
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 12, top: 12, bottom: 0 }))
        .show(ui, |ui| {
            filters(app, ui, p);
            ui.add_space(8.0);
            list(app, ui, p);
        });
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.surface).inner_margin(Margin { left: 24, right: 24, top: 16, bottom: 0 }))
        .show(ui, |ui| super::detail::show(app, ui));
    goto_box(app, ui.ctx(), p);
}

fn filters(app: &mut App, ui: &mut Ui, p: &Palette) {
    ui.horizontal_wrapped(|ui| {
        // Team scope: All, each configured sub-team, or tickets with no team label.
        let options = filter::team_options(&app.cfg.scope.teams);
        let label = options.iter().find(|(t, _)| *t == app.filter.team).map_or_else(
            || match &app.filter.team {
                filter::TeamScope::Label(l) => l.clone(),
                _ => "All teams".to_string(),
            },
            |(_, l)| l.clone(),
        );
        egui::ComboBox::from_id_salt("team").selected_text(label).width(220.0).show_ui(ui, |ui| {
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
        egui::ComboBox::from_id_salt("epic").selected_text(label).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(app.filter.epic.is_none(), "All epics").clicked() {
                app.actions.push(Action::SetEpic(None));
            }
            for (k, s) in &epics {
                if ui.selectable_label(app.filter.epic.as_deref() == Some(k), entry(k, s)).clicked() {
                    app.actions.push(Action::SetEpic(Some(k.clone())));
                }
            }
        });
        // Sprint scope: All, the board's active and future sprints, or Backlog.
        let options = crate::filter::sprint_options(&app.sprints);
        let label = options.iter().find(|(s, _)| *s == app.filter.sprint).map_or("All sprints".to_string(), |(_, l)| l.clone());
        egui::ComboBox::from_id_salt("sprint").selected_text(label).width(200.0).show_ui(ui, |ui| {
            for (scope, l) in &options {
                if ui.selectable_label(app.filter.sprint == *scope, l).clicked() {
                    app.actions.push(Action::SetSprint(scope.clone()));
                }
            }
        });
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
        if ui.add(toggle_button("Roadmap", app.filter.roadmap, p)).clicked() {
            app.actions.push(Action::ToggleRoadmap);
        }
    });
    ui.add_space(6.0);
    let r = ui.add(
        egui::TextEdit::singleline(&mut app.filter.text)
            .hint_text("Filter, or type JQL and press Enter (⌘F)")
            .desired_width(f32::INFINITY)
            .margin(vec2(8.0, 6.0)),
    );
    if app.focus_search {
        r.request_focus();
        app.focus_search = false;
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let t = app.filter.text.trim().to_string();
        // JQL has an operator; a bare word is a local text filter.
        if t.contains('=') || t.contains(" in ") || t.contains('~') || t.contains('!') {
            app.filter.text.clear();
            // A typed search replaces any quick filter.
            app.filter.quick = None;
            app.actions.push(Action::RunSearch(t));
        }
    }
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
            (Some(jql), true) => Action::RunSearch(jql.clone()),
            _ => Action::Refresh,
        };
        app.actions.push(retry);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} issues", rows.len())).color(p.muted).size(12.0));
        if app.search_pending.is_some() {
            ui.add(egui::Spinner::new().size(14.0));
            ui.label(RichText::new("updating…").color(p.muted).size(12.0));
        }
        if app.search_active && ui.add(button("Clear search", p)).clicked() {
            app.actions.push(Action::RunSearch(String::new()));
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
        for (idx, i) in rows.iter().enumerate() {
            let is_sel = selected.as_deref() == Some(&i.key);
            let fill = if is_sel {
                p.selected_row
            } else if idx == app.cursor {
                p.hover_row
            } else {
                p.surface
            };
            let epic = i.parent_key.as_deref().map(|k| theme::epic_tint(k, p, epic_colors));
            let r = Frame::new().fill(fill).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&i.key).font(theme::mono(12.0)).color(p.muted));
                    ui.label(RichText::new(&i.summary).font(theme::bold(13.5)).color(p.text));
                });
                ui.horizontal_wrapped(|ui| {
                    team_pills(ui, &app.filter, i, p);
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
                        pill(ui, &v, bg, fg);
                    }
                });
            });
            if let Some(t) = epic {
                let rect = r.response.rect;
                ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, t.fg);
            }
            let r = r.response.interact(Sense::click());
            if r.clicked() {
                app.cursor = idx;
                app.last_cursor = idx;
                app.actions.push(Action::Select(i.key.clone()));
            }
            if r.double_clicked() {
                app.actions.push(Action::OpenUrl(app.cfg.browse_url(&i.key)));
            }
            if scroll_cursor && idx == app.cursor {
                r.scroll_to_me(None);
            }
            ui.add_space(4.0);
        }
    });
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
