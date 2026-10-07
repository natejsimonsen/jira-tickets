//! The selected issue: summary, status, description, fields, comments, composer.

use crate::app::{Action, App};
use crate::theme::{self, Palette};
use crate::views::{adf_view, button, pill, primary_button};
use egui::{Frame, Margin, RichText, Stroke, Ui, vec2};

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let Some(key) = app.selected.clone() else {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.label(RichText::new("Select an issue").color(p.muted).size(16.0));
        });
        return;
    };
    let Some(issue) = app.store.get(&key).cloned() else {
        ui.add(egui::Spinner::new());
        return;
    };
    let loading = app.detail_loading.contains(&key);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&issue.key).font(theme::mono(13.0)).color(p.muted));
            if let (Some(pk), Some(ps)) = (&issue.parent_key, &issue.parent_summary) {
                ui.label(RichText::new("·").color(p.muted));
                if ui.link(RichText::new(format!("{pk} {ps}")).color(p.muted).size(13.0)).clicked() {
                    app.actions.push(Action::SetEpic(Some(pk.clone())));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(button("Open in Jira", p)).on_hover_text("⌘O").clicked() {
                    app.actions.push(Action::OpenUrl(app.cfg.browse_url(&issue.key)));
                }
                if ui.add(button("Copy link", p)).on_hover_text("c").clicked() {
                    app.actions.push(Action::Copy(app.cfg.browse_url(&issue.key)));
                }
                if loading {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
        ui.label(RichText::new(&issue.summary).font(theme::bold(22.0)).color(p.text));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            pill(ui, &issue.status.name, p.surface_2, theme::status_color(&issue.status.category, p));
            pill(ui, &issue.issue_type, p.surface_2, p.muted);
            if !issue.priority.is_empty() {
                pill(ui, &issue.priority, p.surface_2, p.muted);
            }
            for l in &issue.labels {
                pill(ui, l, p.surface_2, p.muted);
            }
        });
        ui.add_space(12.0);
        ui.horizontal_top(|ui| {
            let main_w = (ui.available_width() - 260.0).max(360.0);
            ui.allocate_ui_with_layout(vec2(main_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(main_w);
                ui.label(RichText::new("Description").font(theme::bold(12.0)).color(p.muted));
                ui.add_space(4.0);
                match &issue.description {
                    Some(d) => adf_view::show_value(ui, d, p),
                    None if loading => {
                        ui.add(egui::Spinner::new().size(14.0));
                    }
                    None => adf_view::show_value(ui, &serde_json::Value::Null, p),
                }
                ui.add_space(16.0);
                ui.label(RichText::new(format!("Comments ({})", issue.comments.len())).font(theme::bold(12.0)).color(p.muted));
                for c in &issue.comments {
                    ui.add_space(6.0);
                    Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::same(10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&c.author).font(theme::bold(13.0)));
                            ui.label(RichText::new(short_date(&c.created)).color(p.muted).size(12.0));
                        });
                        adf_view::show_value(ui, &c.body, p);
                    });
                }
                ui.add_space(10.0);
                let draft = app.drafts.entry(key.clone()).or_default();
                let resp = ui.add(egui::TextEdit::multiline(draft).hint_text("Add a comment (⌘Enter to post)").desired_rows(3).desired_width(f32::INFINITY));
                let enabled = comment_enabled(draft, &app.posting);
                let post = resp.has_focus() && ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter)));
                ui.horizontal(|ui| {
                    if ui.add_enabled(enabled, primary_button("Comment", p)).clicked() || (post && enabled) {
                        app.actions.push(Action::Comment);
                    }
                    if app.posting.is_some() {
                        ui.add(egui::Spinner::new().size(14.0));
                    }
                });
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(220.0);
                field(ui, p, "Assignee", &issue.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or("Unassigned".into()));
                field(ui, p, "Reporter", &issue.reporter.as_ref().map(|a| a.display_name.clone()).unwrap_or_default());
                field(ui, p, "Sprint", issue.sprint.as_deref().unwrap_or("—"));
                field(ui, p, "Story points", &issue.story_points.map(|n| format!("{n}")).unwrap_or("—".into()));
                field(ui, p, "Due", issue.due_date.as_deref().unwrap_or("—"));
                field(ui, p, "Created", &short_date(&issue.created));
                field(ui, p, "Updated", &short_date(&issue.updated));
                ui.add_space(8.0);
                ui.label(RichText::new("t transition · a assign · s sprint · e edit · J/K rank").color(p.muted).size(11.0));
            });
        });
        ui.add_space(24.0);
    });
}

fn field(ui: &mut Ui, p: &Palette, name: &str, value: &str) {
    ui.label(RichText::new(name).font(theme::bold(11.5)).color(p.muted));
    ui.label(RichText::new(if value.is_empty() { "—" } else { value }).size(13.0).color(p.text));
    ui.add_space(6.0);
}

/// The Comment button and ⌘Enter work when the draft has text and no post is in flight.
pub fn comment_enabled(draft: &str, posting: &Option<String>) -> bool {
    !draft.trim().is_empty() && posting.is_none()
}

/// "2026-10-03T09:00:00.000-0700" → "2026-10-03 09:00"
pub fn short_date(s: &str) -> String {
    match s.get(..10).zip(s.get(11..16)) {
        Some((day, time)) => format!("{day} {time}"),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_enabled_needs_text_and_idle() {
        assert!(comment_enabled("hi", &None));
        assert!(!comment_enabled("  \n ", &None));
        assert!(!comment_enabled("", &None));
        assert!(!comment_enabled("hi", &Some("ABC-1".into())));
    }

    #[test]
    fn short_date_formats_and_never_panics() {
        assert_eq!(short_date("2026-10-03T09:00:00.000-0700"), "2026-10-03 09:00");
        assert_eq!(short_date("2026-10"), "2026-10");
        // Byte 10 and 11 fall inside multi-byte chars; slicing would panic.
        let odd = "2026-10-0éé:00:00";
        assert_eq!(short_date(odd), odd);
        assert_eq!(short_date("ééééééééééééé"), "ééééééééééééé");
    }
}
