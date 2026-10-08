//! Issues: list on the left, selected issue on the right.

use crate::app::{Action, App};
use crate::filter;
use crate::jira::Issue;
use crate::store::ListId;
use crate::theme::{self, Palette};
use crate::views::{button, pill};
use egui::{CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};

pub fn current_rows(app: &App) -> Vec<&Issue> {
    let src = if app.search_active { ListId::Search } else { ListId::TeamOpen };
    let all = app.store.list(src);
    filter::visible(&all, &app.filter, app.me.as_ref().map(|m| m.display_name.as_str()))
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let width = ui.available_width();
    let list_w = (width * 0.42).clamp(360.0, 720.0);
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
        // Epic picker: epics seen among TeamOpen parents, by summary.
        let mut epics: Vec<(String, String)> =
            app.store.list(ListId::TeamOpen).iter().filter_map(|i| Some((i.parent_key.clone()?, i.parent_summary.clone().unwrap_or_default()))).collect();
        epics.sort();
        epics.dedup();
        let label =
            app.filter.epic.as_ref().and_then(|k| epics.iter().find(|(key, _)| key == k)).map(|(k, s)| format!("{k} · {s}")).unwrap_or("All epics".into());
        egui::ComboBox::from_id_salt("epic").selected_text(label).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(app.filter.epic.is_none(), "All epics").clicked() {
                app.actions.push(Action::SetEpic(None));
            }
            for (k, s) in &epics {
                if ui.selectable_label(app.filter.epic.as_deref() == Some(k), format!("{k} · {s}")).clicked() {
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
            if ui.add(button(&f.name, p).fill(if active { p.selected_row } else { p.surface_2 })).clicked() {
                app.actions.push(Action::SetQuick(if active { None } else { Some(i) }));
            }
        }
        if ui.add(button("Mine", p).fill(if app.filter.mine { p.selected_row } else { p.surface_2 })).clicked() {
            app.actions.push(Action::ToggleMine);
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
    let src = if app.search_active { ListId::Search } else { ListId::TeamOpen };
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
