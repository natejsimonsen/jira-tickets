//! The selected issue: summary, status, description, fields, comments, composer.

use crate::app::{Action, App};
use crate::theme::{self, Palette};
use crate::views::{adf_view, button, outline_pill, pill, primary_button_if};
use egui::{Frame, Margin, RichText, Stroke, Ui, vec2};

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let Some(key) = app.selected.clone() else {
        // Hidden under the help overlay, where it would poke out beside the panel.
        if app.help_open {
            return;
        }
        ui.set_max_width(ui.available_width() - DETAIL_RIGHT);
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            crate::views::empty_state(ui, p, "Select an issue");
        });
        return;
    };
    let Some(issue) = app.store.get(&key).cloned() else {
        crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        return;
    };
    let loading = app.detail_loading.contains(&key);
    let saved = crate::views::set_thumb_fills(ui, p);
    // No edge fade: the pane ends flush on the hint bar's rule (the fade dimmed its last ~19 px).
    crate::views::no_scroll_fade(ui);
    // The viewport stops BAR_CLEARANCE above the hint bar, so a partly scrolled line's ascenders never show
    // against the bar's rule (C5).
    let view_h = (ui.available_height() - BAR_CLEARANCE).max(0.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(view_h).show(ui, |ui| {
        crate::views::restore_fills(ui, saved);
        // The pane has no right margin: the bar sits in this inset, clear of the content.
        ui.set_max_width(ui.available_width() - DETAIL_RIGHT);
        // One row the buttons' height, everything centred on it, so the key line sits level with the buttons.
        ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_height(crate::views::BUTTON_H);
            ui.label(RichText::new(&issue.key).font(theme::mono(12.5)).color(p.muted));
            if let (Some(pk), Some(ps)) = (&issue.parent_key, &issue.parent_summary) {
                ui.label(RichText::new("·").color(p.muted));
                let epic = theme::epic_tint_in(pk, &app.epic_slots, p, &app.cfg.issues.epic_colors);
                // In the epic's text colour, underlined on hover like every other link.
                let tip = super::issues::epic_tip(pk, ps);
                if crate::views::tooltip(crate::views::text_link(ui, &format!("{pk} {ps}"), theme::body(13.0), epic.fg), tip).clicked() {
                    app.actions.push(Action::SetEpic(Some(pk.clone())));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if crate::views::tooltip(ui.add(button("Open in Jira", p)), "Open in Jira (⌘O)").clicked() {
                    app.actions.push(Action::OpenUrl(app.cfg.browse_url(&issue.key)));
                }
                if crate::views::tooltip(ui.add(button("Copy link", p)), "Copy link (c)").clicked() {
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
            // The gutter is FIELDS_GUTTER exactly: no item spacing either side of it.
            ui.spacing_mut().item_spacing.x = 0.0;
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
                    // surface_2, fill only (like code blocks), in both themes; code inside steps off it (`chip_fill_on`).
                    Frame::new().fill(p.surface_2).corner_radius(theme::RADIUS).inner_margin(Margin::same(12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        comment_header(ui, p, &c.author, &short_date(&c.created));
                        ui.add_space(4.0);
                        adf_view::show_value(ui, &c.body, p, p.surface_2);
                    });
                    ui.add_space(COMMENT_GAP - ui.spacing().item_spacing.y);
                }
                // After the cards, 16 to the composer (12 + 4); with no cards it sits 16 below the label like one.
                if !issue.comments.is_empty() {
                    ui.add_space(4.0);
                }
                if composer_shown(crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed), snapshot_composer()) {
                    composer(app, ui, p, &key);
                } else {
                    // Read-only: one muted line where the box would be; nothing to type into or press. Its ink
                    // starts where a card's edge would.
                    ui.add_space(-NOTE_LIFT);
                    ui.label(RichText::new(READ_ONLY_NOTE).size(14.0).color(p.muted));
                }
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
/// The composer's inner margin: with its 1 px stroke, text sits 12 in, on a comment card's text edge.
const COMPOSER_PAD: f32 = 11.0;
/// The composer's stroke.
const COMPOSER_STROKE: f32 = 1.0;
/// Comment author → date.
const COMMENT_META_GAP: f32 = 8.0;
/// The detail viewport's bottom gap above the hint bar.
pub const BAR_CLEARANCE: f32 = 4.0;
/// The detail content's right inset, inside the scroll area: the pane's 16 px gutter, which holds the floating
/// scrollbar (its width + 4 at least).
pub const DETAIL_RIGHT: f32 = 16.0;

/// The read-only note's line box carries this much more above its ink than a card's edge does (round 8
/// measured label → note 21 vs label → first card 17.5), so the note moves up by it.
const NOTE_LIFT: f32 = 3.5;

/// What read-only mode shows instead of the composer.
pub const READ_ONLY_NOTE: &str = "Read-only, comments can't be posted";

/// A comment card's header: the author in bold, 8, the date muted. Returns both labels' rects.
pub fn comment_header(ui: &mut Ui, p: &Palette, author: &str, date: &str) -> (egui::Rect, egui::Rect) {
    ui.horizontal(|ui| {
        // Set here: the detail pane zeroes item spacing for its column gutter, and this row inherits it.
        ui.spacing_mut().item_spacing.x = COMMENT_META_GAP;
        let a = ui.label(RichText::new(author).font(theme::bold(13.5))).rect;
        let d = ui.label(RichText::new(date).color(p.muted).size(12.0)).rect;
        (a, d)
    })
    .inner
}

/// Whether the composer box and Comment button are drawn: not in read-only mode, unless the snapshot-only
/// override asks for the writable UI (writes stay blocked by READ_ONLY either way).
pub fn composer_shown(read_only: bool, snapshot_override: bool) -> bool {
    !read_only || snapshot_override
}

/// Captures only: `JIRA_TICKETS_SNAPSHOT_COMPOSER=1` in a `snapshot` build draws the writable composer so it
/// can be rated, while every write is still refused by READ_ONLY. Always false in a normal build.
pub fn snapshot_composer() -> bool {
    #[cfg(feature = "snapshot")]
    {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var("JIRA_TICKETS_SNAPSHOT_COMPOSER").as_deref() == Ok("1"))
    }
    #[cfg(not(feature = "snapshot"))]
    {
        false
    }
}

/// Writes are blocked and the composer is not drawn: the hint bar offers no composer keys.
pub fn composer_read_only() -> bool {
    !composer_shown(crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed), snapshot_composer())
}

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
        // The one input rule: a step under the detail pane's surface.
        .fill(theme::input_fill(p.surface, p))
        .stroke(Stroke::new(COMPOSER_STROKE, if focused { p.accent } else { p.border }))
        .corner_radius(theme::RADIUS)
        // 11 + the 1 px stroke: the comment cards' 12 to the text on every side.
        .inner_margin(Margin::same(COMPOSER_PAD as i8))
        .show(ui, |ui| ui.add_sized([ui.available_width(), COMPOSER_H - 2.0 * (COMPOSER_PAD + COMPOSER_STROKE)], composer_edit(draft, p).id(id)))
        .inner;
    // Clicking the frame's padding focuses the box too.
    let enabled = comment_enabled(draft, &app.posting);
    let post = resp.has_focus() && ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter)));
    // 12 px box → button, counting the layout's own item spacing.
    ui.add_space(BOX_TO_BUTTON - ui.spacing().item_spacing.y);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if crate::views::hover_tip_beside(ui.add(primary_button_if("Comment", enabled, p)), COMMENT_TIP, app).clicked() || (post && enabled) {
            app.actions.push(Action::Comment);
        }
        if app.posting.is_some() {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        }
    });
}

/// The composer's text field. The empty-field caret rule (I1) lives in the frame's margin: a TextEdit given its own
/// `frame` ignores `.margin()`, which is how the composer kept round 10's placeholder 2 right of the typed text
/// (round 12 F1). Caret CARET_LEAD left of the origin, placeholder and typed text on it.
pub fn composer_edit<'a>(draft: &'a mut String, p: &Palette) -> egui::TextEdit<'a> {
    let empty = draft.is_empty();
    egui::TextEdit::multiline(draft)
        .frame(Frame::new().inner_margin(crate::views::caret_margin(Margin::ZERO, empty)))
        .hint_text(crate::views::empty_field_hint(COMPOSER_HINT, theme::body(14.0), p))
}

/// The Comment button's tooltip.
pub const COMMENT_TIP: &str = "⌘Enter";

/// The composer's placeholder.
pub const COMPOSER_HINT: &str = "Add a comment (⌘Enter to post)";

/// Label, 4, value, then 16 to the next group.
fn field(ui: &mut Ui, p: &Palette, name: &str, value: &str) {
    ui.label(RichText::new(name).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(4.0);
    ui.label(RichText::new(if value.is_empty() { "—" } else { value }).size(14.0).color(p.text));
    ui.add_space(16.0);
}

/// Gap between the description column and the fields column (24; it measured 33 with item spacing on top).
const FIELDS_GUTTER: f32 = 24.0;

/// The description column's widest measure (about 75 characters of 14 px text).
const MAIN_MAX: f32 = 520.0;

/// (main column, fields column) widths: fields are at least 220 wide, or 132 below a 1200 px window so the
/// description keeps its measure; the main column takes the rest (300 to MAIN_MAX), and any room past
/// MAIN_MAX goes to the fields column.
pub fn detail_columns(window_w: f32, available: f32) -> (f32, f32) {
    let fields_min = if window_w < 1200.0 { NARROW_FIELDS } else { 220.0 };
    let main = (available - fields_min - FIELDS_GUTTER).clamp(300.0, MAIN_MAX);
    (main, (available - main - FIELDS_GUTTER).max(fields_min))
}

/// The fields column below a 1200 px window: 132, so the description keeps 400 with the 24 gutter (K1).
pub const NARROW_FIELDS: f32 = 132.0;

/// Composer box → Comment button.
const BOX_TO_BUTTON: f32 = 12.0;

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
        assert_eq!(detail_columns(1440.0, 800.0), (520.0, 256.0), "main capped; the rest to the fields");
        assert_eq!(detail_columns(1000.0, 580.0), (424.0, 132.0));
        // The narrow pane's reserve holds a 400 description beside the 132 fields and the 24 gutter.
        assert_eq!(detail_columns(1000.0, crate::views::issues::NARROW_DETAIL_MIN - 32.0), (400.0, NARROW_FIELDS));
        assert_eq!(detail_columns(1000.0, 300.0).0, 300.0, "floor");
        assert_eq!(detail_columns(2560.0, 1800.0).0, MAIN_MAX);
    }

    #[test]
    fn read_only_draws_no_composer() {
        assert!(!composer_shown(true, false), "read-only: only the note");
        assert!(composer_shown(false, false));
        assert!(composer_shown(true, true), "the snapshot override draws the writable UI");
        assert!(!snapshot_composer() || cfg!(feature = "snapshot"), "never on outside snapshot builds");
        assert_eq!(READ_ONLY_NOTE, "Read-only, comments can't be posted");
        assert_eq!(21.0 - NOTE_LIFT, 17.5, "the note's ink sits where the first card's edge does");
        let bar = egui::style::ScrollStyle::floating();
        assert!(DETAIL_RIGHT >= bar.bar_width + bar.bar_outer_margin + 4.0, "the scrollbar sits clear of the content");
        assert_eq!((COMMENT_TIP, COMPOSER_HINT), ("⌘Enter", "Add a comment (⌘Enter to post)"));
        assert_eq!(COMPOSER_PAD + COMPOSER_STROKE, 12.0, "text on the comment cards' 12 px edge");
        assert_eq!(BAR_CLEARANCE, 4.0, "the viewport stops 4 above the hint bar (C5)");
    }

    #[test]
    fn comment_author_and_date_stand_apart() {
        // Regression (round 7): "Alan Turing2026-07-29 13:28" — the pane's zero item spacing leaked in.
        let (a, d) = crate::testutil::with_ui(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            comment_header(ui, &crate::theme::LIGHT, "Alan Turing", "2026-07-29 13:28")
        });
        assert!(d.left() - a.right() >= 8.0, "gap {}", d.left() - a.right());
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
