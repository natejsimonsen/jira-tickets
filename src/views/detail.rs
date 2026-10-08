//! The selected issue: summary, status, description, fields, comments, composer.

use crate::app::{Action, App};
use crate::theme::{self, Palette};
use crate::views::{adf_view, button, outline_pill, pill, primary_button_if};
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
        crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        return;
    };
    let loading = app.detail_loading.contains(&key);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // One row the buttons' height, everything centred on it, so the key line sits level with the buttons.
        ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_height(crate::views::BUTTON_H);
            ui.label(RichText::new(&issue.key).font(theme::mono(12.5)).color(p.muted));
            if let (Some(pk), Some(ps)) = (&issue.parent_key, &issue.parent_summary) {
                ui.label(RichText::new("·").color(p.muted));
                let epic = theme::epic_tint_in(pk, &app.epic_slots, p, &app.cfg.issues.epic_colors);
                if ui.link(RichText::new(format!("{pk} {ps}")).color(epic.fg).size(13.0)).clicked() {
                    app.actions.push(Action::SetEpic(Some(pk.clone())));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add(button("Open in Jira", p)).on_hover_text("Open in Jira (⌘O)").clicked() {
                    app.actions.push(Action::OpenUrl(app.cfg.browse_url(&issue.key)));
                }
                if ui.add(button("Copy link", p)).on_hover_text("Copy link (c)").clicked() {
                    app.actions.push(Action::Copy(app.cfg.browse_url(&issue.key)));
                }
                if loading {
                    crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                }
            });
        });
        ui.add_space(8.0);
        ui.label(RichText::new(&issue.summary).font(theme::bold(22.0)).color(p.text));
        ui.add_space(TITLE_TO_PILLS);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
            let st = theme::status_tint(&issue.status.name, &issue.status.category, p, &app.cfg.issues.status_colors);
            pill(ui, &issue.status.name, st.bg, st.fg);
            if let crate::filter::TeamScope::Label(l) = &app.filter.team
                && super::issues::missing_label_pill(ui, &app.filter, &issue, l, p)
            {
                ui.label(RichText::new("l applies it").color(p.muted).size(12.0));
            }
            outline_pill(ui, &issue.issue_type, p, crate::views::PILL_MARGIN);
            if !issue.priority.is_empty() {
                outline_pill(ui, &issue.priority, p, crate::views::PILL_MARGIN);
            }
            for l in &issue.labels {
                outline_pill(ui, l, p, crate::views::PILL_MARGIN);
            }
        });
        ui.add_space(SECTION_GAP);
        let window_w = ui.ctx().content_rect().width();
        ui.horizontal_top(|ui| {
            let (main_w, fields_w) = detail_columns(window_w, ui.available_width());
            ui.allocate_ui_with_layout(vec2(main_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(main_w);
                section_heading(ui, p, "Description");
                match &issue.description {
                    Some(d) => adf_view::show_value(ui, d, p, p.surface),
                    None if loading => {
                        crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                    }
                    None => adf_view::show_value(ui, &serde_json::Value::Null, p, p.surface),
                }
                ui.add_space(SECTION_GAP);
                section_heading(ui, p, &format!("Comments ({})", issue.comments.len()));
                // The comment cards and the composer sit 16 below their label (8 already given).
                ui.add_space(LABEL_TO_BOX - 8.0 - ui.spacing().item_spacing.y);
                for c in &issue.comments {
                    // surface_2 with a 1 px border in both themes; code inside steps off it (`chip_fill_on`).
                    Frame::new().fill(p.surface_2).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::same(12)).show(
                        ui,
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&c.author).font(theme::bold(13.5)));
                                ui.label(RichText::new(short_date(&c.created)).color(p.muted).size(12.0));
                            });
                            ui.add_space(4.0);
                            adf_view::show_value(ui, &c.body, p, p.surface_2);
                        },
                    );
                    ui.add_space(COMMENT_GAP - ui.spacing().item_spacing.y);
                }
                // After the cards, 16 to the composer (12 + 4); with no cards it sits 16 below the label like one.
                if !issue.comments.is_empty() {
                    ui.add_space(4.0);
                }
                composer(app, ui, p, &key);
            });
            ui.add_space(FIELDS_GUTTER);
            ui.vertical(|ui| {
                ui.set_width(fields_w);
                ui.spacing_mut().item_spacing.y = 0.0;
                field(ui, p, "Assignee", &issue.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or("Unassigned".into()));
                field(ui, p, "Reporter", &issue.reporter.as_ref().map(|a| a.display_name.clone()).unwrap_or_default());
                field(ui, p, "Sprint", issue.sprint.as_deref().unwrap_or("—"));
                field(ui, p, "Story points", &issue.story_points.map(|n| format!("{n}")).unwrap_or("—".into()));
                field(ui, p, "Due", issue.due_date.as_deref().unwrap_or("—"));
                field(ui, p, "Created", &short_date(&issue.created));
                field(ui, p, "Updated", &short_date(&issue.updated));
            });
        });
        ui.add_space(24.0);
    });
}

/// Title → pills.
const TITLE_TO_PILLS: f32 = 12.0;
/// Pills → Description, Description → Comments.
const SECTION_GAP: f32 = 24.0;
/// Section label → the comment cards or composer box.
const LABEL_TO_BOX: f32 = 16.0;
/// Between comment cards: the cards' own 12 px padding.
const COMMENT_GAP: f32 = 12.0;
/// The composer's fixed height.
const COMPOSER_H: f32 = 72.0;
/// The composer's inner margin, the same as a comment card's.
const COMPOSER_PAD: f32 = 12.0;

/// A section heading; the 24 px above it comes from SECTION_GAP, 8 below.
fn section_heading(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(8.0);
}

/// The composer's text field id for an issue.
pub fn composer_id(key: &str) -> egui::Id {
    egui::Id::new(("composer", key))
}

/// Always-visible comment box (border, accent when focused) with the Comment button right-aligned under it.
fn composer(app: &mut App, ui: &mut Ui, p: &Palette, key: &str) {
    let id = composer_id(key);
    let focused = ui.memory(|m| m.has_focus(id));
    let draft = app.drafts.entry(key.to_string()).or_default();
    let resp = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, if focused { p.accent } else { p.border }))
        .corner_radius(theme::RADIUS)
        // The comment cards' 12 on every side.
        .inner_margin(Margin::same(COMPOSER_PAD as i8))
        .show(ui, |ui| {
            ui.add_sized(
                [ui.available_width(), COMPOSER_H - 2.0 * COMPOSER_PAD],
                egui::TextEdit::multiline(draft)
                    .id(id)
                    .frame(Frame::new())
                    .margin(vec2(0.0, 0.0))
                    .hint_text(RichText::new("Add a comment (⌘Enter to post)").color(p.muted)),
            )
        })
        .inner;
    // Clicking the frame's padding focuses the box too.
    let enabled = comment_enabled(draft, &app.posting);
    let post = resp.has_focus() && ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter)));
    // 12 px box → button, counting the layout's own item spacing.
    ui.add_space(BOX_TO_BUTTON - ui.spacing().item_spacing.y);
    let read_only = crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let tip = if read_only { "Writes are blocked (JIRA_TICKETS_READONLY)" } else { "Post the comment (⌘Enter)" };
        if ui.add(primary_button_if("Comment", comment_primary(enabled, read_only), p)).on_hover_text(tip).clicked() || (post && enabled) {
            app.actions.push(Action::Comment);
        }
        if app.posting.is_some() {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        }
    });
}

/// Label, 4, value, then 16 to the next group.
fn field(ui: &mut Ui, p: &Palette, name: &str, value: &str) {
    ui.label(RichText::new(name).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(4.0);
    ui.label(RichText::new(if value.is_empty() { "—" } else { value }).size(14.0).color(p.text));
    ui.add_space(16.0);
}

/// Gap between the description column and the fields column.
const FIELDS_GUTTER: f32 = 16.0;

/// The description column's widest measure (about 75 characters of 14 px text).
const MAIN_MAX: f32 = 520.0;

/// (main column, fields column) widths: fields are at least 220 wide, or 140 below a 1200 px window so the
/// description keeps its measure; the main column takes the rest (300 to MAIN_MAX), and any room past
/// MAIN_MAX goes to the fields column.
pub fn detail_columns(window_w: f32, available: f32) -> (f32, f32) {
    let fields_min = if window_w < 1200.0 { 140.0 } else { 220.0 };
    let main = (available - fields_min - FIELDS_GUTTER).clamp(300.0, MAIN_MAX);
    (main, (available - main - FIELDS_GUTTER).max(fields_min))
}

/// Composer box → Comment button.
const BOX_TO_BUTTON: f32 = 12.0;

/// The Comment button is the accent primary only when a post would be sent: never in read-only mode
/// (⌘Enter still posts there, and the refusal toast explains).
pub fn comment_primary(enabled: bool, read_only: bool) -> bool {
    enabled && !read_only
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
    fn fields_column_narrows_below_1200() {
        assert_eq!(detail_columns(1440.0, 800.0), (520.0, 264.0), "main capped; the rest to the fields");
        assert_eq!(detail_columns(1000.0, 580.0), (424.0, 140.0));
        assert_eq!(detail_columns(1000.0, 300.0).0, 300.0, "floor");
        assert_eq!(detail_columns(2560.0, 1800.0).0, MAIN_MAX);
    }

    #[test]
    fn read_only_keeps_the_comment_button_quiet() {
        assert!(comment_primary(true, false));
        assert!(!comment_primary(true, true));
        assert!(!comment_primary(false, false));
    }

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
