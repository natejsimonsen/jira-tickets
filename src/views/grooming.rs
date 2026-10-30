//! The Grooming screen: a header with the mode switch, the report line and the Ready for Work ceiling, then the
//! Pre-check or Session body.

use crate::app::{Action, App, GroomMode, GroomingState};
use crate::grooming::{self, Level, Promotion, Verdict};
use crate::jira;
use crate::keys::GroomHint;
use crate::report::{self, Badge};
use crate::theme::{self, Palette};
use crate::views::grooming_guide as guide;
use crate::views::{PILL_MARGIN, empty_state, outline_pill, pill_sized, primary_button, toggle_chip};
use egui::{Color32, Frame, Margin, RichText, Stroke, Ui, vec2};

/// The gap between the header's mode chips, as between the Issues quick-filter chips.
const CHIP_GAP: f32 = 8.0;
/// The gap between the header's groups (the switch, the report line), and under the header.
const GROUP_GAP: f32 = 16.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            header(app, ui, p);
            ui.add_space(GROUP_GAP);
            match app.grooming.mode {
                GroomMode::PreCheck => pre_check_body(app, ui, p),
                GroomMode::Session => session_body(app, ui, p),
            }
        });
    dialogs(app, &ui.ctx().clone(), p);
}

/// The mode switch, the report line, and the ceiling pill at the right.
fn header(app: &mut App, ui: &mut Ui, p: &Palette) {
    let mode = app.grooming.mode;
    let line = report_line(app.grooming.report.as_ref().map(|(path, r)| (path.as_path(), r)), &app.cfg.grooming.report_dir);
    let g = &app.cfg.grooming;
    let ready = app.ready_total();
    let level = grooming::ceiling_level(ready, g.ready_soft_ceiling, g.ready_hard_ceiling);
    let ceiling = ceiling_text(ready, g.ready_soft_ceiling, g.ready_hard_ceiling);
    let purpose = guide::purpose(mode, &g.status_new, &g.status_planning, &g.status_ready);
    ui.horizontal(|ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        ui.spacing_mut().item_spacing.x = CHIP_GAP;
        for (m, name) in [(GroomMode::PreCheck, "Pre-check"), (GroomMode::Session, "Session")] {
            if crate::views::tooltip(toggle_chip(ui, name, mode == m, p), "Tab").clicked() && mode != m {
                app.actions.push(Action::GroomMode(m));
            }
        }
        ui.add_space(GROUP_GAP - CHIP_GAP);
        ui.label(RichText::new(line).color(p.muted).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let c = level_color(level, p);
            pill_sized(ui, &ceiling, Color32::TRANSPARENT, c, Stroke::new(1.0, c), PILL_MARGIN);
        });
    });
    // Always present in both modes, so the body never jumps on a mode switch. The 8 is measured to the ink, so the
    // layout's own item spacing comes off it.
    ui.add_space(8.0 - ui.spacing().item_spacing.y);
    ui.add(egui::Label::new(RichText::new(purpose).color(p.muted).size(14.0)).wrap());
}

/// The list column's share of the body, between these widths.
const LIST_MIN: f32 = 360.0;
const LIST_MAX: f32 = 560.0;
/// The gap between the list and the focused ticket, and the cards' inner padding.
const PANE_GAP: f32 = 16.0;
/// A list row's padding.
const ROW_PAD: Margin = Margin { left: 12, right: 12, top: 8, bottom: 8 };
/// Between a row's title line and its pill line, and between pills.
const LINE_GAP: f32 = 4.0;
const PILL_GAP: f32 = 6.0;
/// The Issues list's row overlap.
const ROW_GAP: f32 = -1.0;

/// What the Grooming hint bar describes: an open dialog, else the mode.
pub fn hint(g: &GroomingState) -> GroomHint {
    match (g.pending_report.is_some(), g.summary_open, g.mode) {
        (true, _, _) => GroomHint::Overwrite,
        (_, true, _) => GroomHint::Summary,
        (_, _, GroomMode::PreCheck) => GroomHint::PreCheck,
        (_, _, GroomMode::Session) if g.bounce_text.is_some() => GroomHint::Bounce,
        (_, _, GroomMode::Session) if g.placement.is_some() => GroomHint::Placement,
        (_, _, GroomMode::Session) => GroomHint::Session,
    }
}

/// A verdict's pill text.
pub fn verdict_text(v: &Verdict) -> String {
    match v {
        Verdict::Ready => "ready".to_string(),
        Verdict::NeedsDetail(codes) => format!("needs detail: {}", codes.iter().map(|c| c.slug()).collect::<Vec<_>>().join(", ")),
    }
}

/// The summary row's one line: "Summary · 3 ready · 2 need detail · 4 without a verdict".
pub fn summary_line(plan: &Promotion, unjudged: usize) -> String {
    let ready = plan.promoted.len() + plan.held_back.len();
    format!("Summary · {ready} ready · {} need detail · {unjudged} without a verdict", plan.needs_detail.len())
}

/// The summary's Promote button label.
pub fn promote_label(n: usize) -> String {
    format!("Promote {n}")
}

/// The `New` walk: the candidates (and the summary row) on the left, the focused ticket on the right.
fn pre_check_body(app: &mut App, ui: &mut Ui, p: &Palette) {
    let n = app.pre_check_candidates().len();
    if n == 0 && app.grooming.promote_failures.is_empty() {
        empty_state(ui, p, &format!("Nothing in {}", app.cfg.grooming.status_new));
        return;
    }
    let cursor = app.grooming.cursor.min(n);
    let list_w = (ui.available_width() * 0.42).clamp(LIST_MIN, LIST_MAX).min(ui.available_width());
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = PANE_GAP;
        let h = ui.available_height();
        ui.allocate_ui_with_layout(vec2(list_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(list_w);
            candidate_list(app, ui, p, cursor);
        });
        let right_w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(right_w);
            match app.pre_check_candidates().get(cursor).map(|c| c.issue.clone()) {
                Some(issue) => focused_ticket(app, ui, p, issue),
                None => summary_pane(app, ui, p),
            }
        });
    });
}

/// The candidates in rank order, each with its key, title, verdict (or report badge), epic, age and rank, then the
/// summary row.
fn candidate_list(app: &mut App, ui: &mut Ui, p: &Palette, cursor: usize) {
    let report = app.grooming.report.as_ref().map(|(path, r)| (report_date(path, r), r.clone()));
    let (plan, unjudged) = app.promotion_plan();
    let slots = app.epic_slots.clone();
    let epic_colors = app.cfg.issues.epic_colors.clone();
    let last_id = egui::Id::new("groom-last-cursor");
    let moved = ui.data(|d| d.get_temp::<usize>(last_id)) != Some(cursor);
    ui.data_mut(|d| d.insert_temp(last_id, cursor));
    let mut clicked: Option<usize> = None;
    egui::ScrollArea::vertical().id_salt("groom-list").auto_shrink([false, false]).show(ui, |ui| {
        // Rows stack as one list, as on Issues: neighbouring borders overlap into one divider.
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        let cands = app.pre_check_candidates();
        let total = cands.len() + 1;
        for (idx, c) in cands.iter().enumerate() {
            let i = c.issue;
            let sel = idx == cursor;
            let verdict = app.grooming.verdicts.get(&i.key);
            // The report's own words while the verdict is still the one it pre-filled.
            let badge = report.as_ref().and_then(|(d, r)| {
                let b = r.badge_for(&i.key)?;
                (verdict.is_none() || verdict == grooming::prefill(Some(r), &i.key).as_ref()).then(|| (b, d.clone()))
            });
            let resp = row_frame(ui, p, "groom-row", sel, idx, total, |ui| {
                title_line(ui, p, &i.key, &i.summary, sel);
                ui.add_space(LINE_GAP);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = PILL_GAP;
                    match (&badge, verdict) {
                        (Some((b, d)), _) => {
                            super::grooming::badge(ui, p, b, d);
                        }
                        (None, Some(v)) => {
                            let hue = if *v == Verdict::Ready { p.on_track } else { p.at_risk };
                            pill_sized(ui, &verdict_text(v), Color32::TRANSPARENT, hue, Stroke::new(1.0, hue), PILL_MARGIN);
                        }
                        (None, None) => {}
                    }
                    if let Some(k) = &i.parent_key {
                        let t = theme::epic_tint_in(k, &slots, p, &epic_colors);
                        let name = i.parent_summary.clone().unwrap_or_else(|| k.clone());
                        let measure = |s: &str| ui.painter().layout_no_wrap(s.to_string(), theme::body(12.0), t.fg).size().x;
                        let text = super::issues::fit_epic(&name, 180.0, measure);
                        let edge = super::issues::row_stripe(&t, p, sel);
                        crate::views::tooltip(
                            pill_sized(ui, &text, Color32::TRANSPARENT, t.fg, Stroke::new(1.0, edge), PILL_MARGIN),
                            super::issues::epic_tip(k, &name),
                        );
                    }
                    neutral_pill(ui, p, &c.age, sel);
                    neutral_pill(ui, p, &format!("rank {}", c.rank_pos), sel);
                });
            });
            if resp.clicked() {
                clicked = Some(idx);
            }
            if sel && moved {
                resp.scroll_to_me(None);
            }
        }
        let idx = cands.len();
        let sel = cursor == idx;
        let resp = row_frame(ui, p, "groom-row", sel, idx, total, |ui| {
            let g = ui.painter().layout_no_wrap(summary_line(&plan, unjudged), theme::bold(13.5), p.text);
            ui.label(g);
            ui.add_space(LINE_GAP);
            ui.label(RichText::new("Enter opens the promotion plan").color(p.muted).size(12.0));
        });
        if resp.clicked() {
            clicked = Some(idx);
        }
        if sel && moved {
            resp.scroll_to_me(None);
        }
    });
    if let Some(i) = clicked {
        app.actions.push(Action::GroomMove(i as i32 - cursor as i32));
    }
}

/// A list row card: `selected_row` under the cursor, the hover fill otherwise; returns its click response.
fn row_frame(ui: &mut Ui, p: &Palette, salt: &str, sel: bool, idx: usize, n: usize, content: impl FnOnce(&mut Ui)) -> egui::Response {
    let corners = super::issues::row_corners(idx, n);
    let bg = ui.painter().add(egui::Shape::Noop);
    let r = Frame::new().stroke(Stroke::new(1.0, p.border)).corner_radius(corners).inner_margin(ROW_PAD).show(ui, |ui| {
        ui.set_width(ui.available_width());
        content(ui);
    });
    let rect = r.response.rect;
    let resp = ui.interact(rect, egui::Id::new((salt, idx)), egui::Sense::click());
    let fill = super::issues::row_fill(sel, resp.hovered(), p);
    ui.painter().set(bg, egui::epaint::RectShape::filled(rect, corners, fill));
    resp
}

/// Key (mono, muted) and title (bold), the title cut to the row.
fn title_line(ui: &mut Ui, p: &Palette, key: &str, summary: &str, sel: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(key).font(theme::mono(12.5)).color(super::issues::row_key_color(sel, p)));
        let painter = ui.painter().clone();
        let title = crate::views::truncate_title(summary, ui.available_width(), |t| painter.layout_no_wrap(t.to_string(), theme::bold(13.5), p.text).size().x);
        let r = ui.label(RichText::new(&title).font(theme::bold(13.5)).color(p.text));
        if title != summary {
            crate::views::tooltip(r, summary.to_string());
        }
    });
}

fn neutral_pill(ui: &mut Ui, p: &Palette, text: &str, sel: bool) {
    if sel {
        crate::views::outline_pill_selected(ui, text, p, PILL_MARGIN);
    } else {
        outline_pill(ui, text, p, PILL_MARGIN);
    }
}

/// The focused ticket: key and epic, title, status, and the rendered description (its full detail is fetched).
fn focused_ticket(app: &mut App, ui: &mut Ui, p: &Palette, issue: jira::Issue) {
    let loading = app.detail_loading.contains(&issue.key);
    // Once per focus: a failed fetch is not retried every frame.
    if app.grooming.detail_for.as_deref() != Some(issue.key.as_str()) {
        app.actions.push(Action::GroomFocusDetail(issue.key.clone()));
    }
    let g = &app.cfg.grooming;
    rubric_card(ui, p, app.grooming.mode, &g.status_new, &g.status_planning, &g.status_ready);
    ui.add_space(PANE_GAP - ui.spacing().item_spacing.y);
    card(ui, p, |ui| {
        egui::ScrollArea::vertical().id_salt("groom-detail").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&issue.key).font(theme::mono(12.5)).color(p.muted));
                if let (Some(k), Some(s)) = (&issue.parent_key, &issue.parent_summary) {
                    let t = theme::epic_tint_in(k, &app.epic_slots, p, &app.cfg.issues.epic_colors);
                    ui.label(RichText::new("·").color(p.muted));
                    ui.label(RichText::new(format!("{k} {s}")).font(theme::body(13.0)).color(t.fg));
                }
                if loading {
                    crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                }
            });
            ui.add_space(8.0);
            ui.add(egui::Label::new(RichText::new(&issue.summary).font(theme::bold(22.0)).color(p.text)).wrap());
            ui.add_space(8.0);
            let st = theme::status_tint(&issue.status.name, &issue.status.category, p, &app.cfg.issues.status_colors);
            crate::views::pill(ui, &issue.status.name, st.bg, st.fg);
            ui.add_space(16.0);
            ui.label(RichText::new("Description").font(theme::bold(12.0)).color(p.muted));
            ui.add_space(8.0);
            match &issue.description {
                Some(d) => super::adf_view::show_value(ui, d, p, p.surface),
                None if loading => {}
                None => super::adf_view::show_value(ui, &serde_json::Value::Null, p, p.surface),
            }
        });
    });
}

/// The right pane on the summary row: the plan in brief, and the last Promote's failed transitions.
fn summary_pane(app: &mut App, ui: &mut Ui, p: &Palette) {
    let (plan, unjudged) = app.promotion_plan();
    let cap = app.cfg.grooming.planning_cap;
    let failures = app.grooming.promote_failures.clone();
    let promoting = app.grooming.promoting;
    let cands = app.pre_check_candidates();
    let judged = cands.iter().filter(|c| app.grooming.verdicts.contains_key(&c.issue.key)).count();
    let cue = guide::all_judged(judged, cands.len());
    card(ui, p, |ui| {
        ui.label(RichText::new("Pre-check summary").font(theme::bold(15.0)).color(p.text));
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(cue).color(p.muted).size(14.0)).wrap());
        ui.add_space(12.0);
        for line in plan_lines(&plan, unjudged, cap) {
            ui.label(RichText::new(line).color(p.text).size(14.0));
        }
        if !failures.is_empty() {
            ui.add_space(16.0);
            ui.label(RichText::new("Failed transitions").font(theme::bold(12.0)).color(p.muted));
            ui.add_space(8.0);
            for (k, s, e) in &failures {
                ui.label(RichText::new(format!("{k} {s}: {e}")).color(p.off_track).size(12.0));
            }
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if ui.add(crate::views::primary_button_if("Review and promote", !promoting, p)).clicked() && !promoting {
                app.actions.push(Action::OpenSummary);
            }
            if promoting {
                crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            }
        });
    });
}

/// The plan as counted lines.
pub fn plan_lines(plan: &Promotion, unjudged: usize, cap: usize) -> Vec<String> {
    vec![
        format!("{} to promote to Planning", plan.promoted.len()),
        format!("{} ready but held back (cap {cap})", plan.held_back.len()),
        format!("{} need detail", plan.needs_detail.len()),
        format!("{unjudged} without a verdict"),
    ]
}

/// A `surface` card with the shared border and radius, filling the pane's height.
fn card(ui: &mut Ui, p: &Palette, content: impl FnOnce(&mut Ui)) {
    card_frame(ui, p, true, content);
}

/// The card's frame; `fill` stretches it to the pane's bottom, otherwise it hugs its content.
fn card_frame(ui: &mut Ui, p: &Palette, fill: bool, content: impl FnOnce(&mut Ui)) {
    Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::same(PANE_GAP as i8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if fill {
            ui.set_min_height(ui.available_height() - 1.0);
        }
        content(ui);
    });
}

/// The mode's rubric over the focused ticket: the title, then each line with its key drawn as the hint bar draws keys.
fn rubric_card(ui: &mut Ui, p: &Palette, mode: GroomMode, status_new: &str, status_planning: &str, status_ready: &str) {
    card_frame(ui, p, false, |ui| {
        ui.label(RichText::new(guide::rubric_title(mode)).font(theme::bold(13.5)).color(p.text));
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        for (key, text) in guide::rubric_lines(mode, status_new, status_planning, status_ready) {
            let text = RichText::new(text).color(p.muted).size(12.0);
            if key.is_empty() {
                ui.add(egui::Label::new(text).wrap());
            } else {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(super::issues::key_job(key, p.text));
                    ui.add(egui::Label::new(text).wrap());
                });
            }
        }
    });
}

/// The dialogs over the Grooming screen: the overwrite confirm, else the summary.
fn dialogs(app: &mut App, ctx: &egui::Context, p: &Palette) {
    if app.help_open {
        return;
    }
    let hint_bar = crate::views::hint_bar_height(app);
    if app.grooming.pending_report.is_some() {
        crate::views::backdrop(ctx, "groom-backdrop", egui::Order::Middle, hint_bar, p);
        let path = app.cfg.report_dir().join(format!("{}.md", chrono::Local::now().date_naive().format("%Y-%m-%d")));
        dialog(ctx, p, "groom-overwrite", 400.0, |ui| {
            dialog_title(ui, p, "Report for today exists. Overwrite?");
            ui.label(RichText::new(path.display().to_string()).font(theme::mono(12.5)).color(crate::views::dialog_muted(p)));
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.add(primary_button("Overwrite", p)).clicked() {
                    app.actions.push(Action::Overwrite(true));
                }
                if ui.add(crate::views::dialog_button("Keep the old one", p)).clicked() {
                    app.actions.push(Action::Overwrite(false));
                }
            });
        });
        return;
    }
    if !app.grooming.summary_open {
        return;
    }
    if crate::views::backdrop(ctx, "groom-backdrop", egui::Order::Middle, hint_bar, p).clicked() {
        app.actions.push(Action::CloseSummary);
    }
    let (plan, unjudged) = app.promotion_plan();
    let cap = app.cfg.grooming.planning_cap;
    let titles: std::collections::HashMap<String, String> = app.pre_check_candidates().iter().map(|c| (c.issue.key.clone(), c.issue.summary.clone())).collect();
    let can = promote_enabled(plan.promoted.len(), jira::check_writable().is_ok(), app.grooming.promoting);
    let max_h = (ctx.content_rect().height() * 0.5).max(160.0);
    let g = &app.cfg.grooming;
    let path = guide::report_path(&g.report_dir, chrono::Local::now().date_naive());
    let explainer = guide::promote_explainer(plan.promoted.len(), plan.held_back.len(), cap, &g.status_planning, &path);
    dialog(ctx, p, "groom-summary", 520.0, |ui| {
        dialog_title(ui, p, "Promote to Planning");
        ui.add(egui::Label::new(RichText::new(explainer).color(crate::views::dialog_muted(p)).size(12.0)).wrap());
        ui.add_space(4.0);
        ui.label(RichText::new(format!("{unjudged} without a verdict stay in New untouched")).color(crate::views::dialog_muted(p)).size(12.0));
        ui.add_space(12.0);
        egui::ScrollArea::vertical().id_salt("groom-summary-rows").max_height(max_h).show(ui, |ui| {
            let line = |ui: &mut Ui, key: &str, tail: &str| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(key).font(theme::mono(12.5)).color(crate::views::dialog_muted(p)));
                    ui.label(RichText::new(titles.get(key).map(String::as_str).unwrap_or_default()).color(p.text).size(14.0));
                    if !tail.is_empty() {
                        ui.label(RichText::new(tail).color(crate::views::dialog_muted(p)).size(12.0));
                    }
                });
            };
            section(ui, p, &format!("Promote ({})", plan.promoted.len()));
            for k in &plan.promoted {
                line(ui, k, "");
            }
            if !plan.held_back.is_empty() {
                section(ui, p, &format!("Held back, past the cap of {cap} ({})", plan.held_back.len()));
                for k in &plan.held_back {
                    line(ui, k, "");
                }
            }
            if !plan.needs_detail.is_empty() {
                section(ui, p, &format!("Needs detail, stays in New ({})", plan.needs_detail.len()));
                for (k, codes) in &plan.needs_detail {
                    line(ui, k, &codes.iter().map(|c| c.slug()).collect::<Vec<_>>().join(", "));
                }
            }
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(crate::views::primary_button_if(&promote_label(plan.promoted.len()), can, p)).clicked() && can {
                app.actions.push(Action::Promote);
            }
            if ui.add(crate::views::dialog_button("Cancel", p)).clicked() {
                app.actions.push(Action::CloseSummary);
            }
        });
    });
}

/// "Promote N" is live only with something to promote, writes allowed, and no Promote running.
pub fn promote_enabled(n: usize, writable: bool, promoting: bool) -> bool {
    n > 0 && writable && !promoting
}

fn section(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).font(theme::bold(12.0)).color(crate::views::dialog_muted(p)));
    ui.add_space(4.0);
}

fn dialog_title(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::bold(15.0)).color(p.text));
    ui.add_space(8.0);
}

/// A dialog in the pickers' frame, a fifth down the window.
fn dialog(ctx: &egui::Context, p: &Palette, id: &str, width: f32, content: impl FnOnce(&mut Ui)) {
    let top = super::pickers::dialog_top(ctx.content_rect().height());
    egui::Area::new(egui::Id::new(id)).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, top)).show(ctx, |ui| {
        Frame::new()
            .fill(crate::views::dialog_fill(p))
            .stroke(crate::views::dialog_stroke(p))
            .corner_radius(theme::RADIUS)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(width);
                content(ui);
            });
    });
}

/// The rail's width.
const RAIL_W: f32 = 300.0;
/// The bounce box's height: two 14 pt lines and its padding.
const BOUNCE_H: f32 = 64.0;
/// The bounce box's placeholder.
pub const BOUNCE_HINT: &str = "What is missing? One or two lines (⌘Enter to post)";

/// The rail row's counts: tickets in the active sprint, the next sprint, Ready for Work, Proposed and New.
pub struct RailCounts {
    pub sprint: usize,
    pub next: usize,
    pub ready: usize,
    pub planning: usize,
    pub new: usize,
}

/// One epic on the session rail, taken from `grooming::epic_queues`.
struct RailRow {
    epic: Option<String>,
    summary: String,
    stage: grooming::Stage,
    counts: RailCounts,
    has_candidates: bool,
}

/// What the centre column draws for the selected epic, cloned out of the store so the app can be borrowed mutably.
struct QueueView {
    active: Option<String>,
    next: Option<String>,
    sprint: Vec<jira::Issue>,
    next_sprint: Vec<jira::Issue>,
    ready: Vec<jira::Issue>,
    planning: Vec<jira::Issue>,
    new: Vec<jira::Issue>,
}

/// The epic-by-epic walk: the stage rail, the selected epic's five blocks (sprint, next sprint, Ready for Work,
/// Proposed, New) and the focused ticket's description.
fn session_body(app: &mut App, ui: &mut Ui, p: &Palette) {
    // The per-ticket timer ticks.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    let (active, next) = grooming::sprint_names(&app.sprints);
    let (active, next) = (active.map(String::from), next.map(String::from));
    let queues = app.session_queues();
    let rail: Vec<RailRow> = queues
        .iter()
        .map(|q| RailRow {
            epic: q.epic.clone(),
            summary: q.summary.clone(),
            stage: q.stage.stage,
            counts: RailCounts { sprint: q.sprint.len(), next: q.next.len(), ready: q.ready.len(), planning: q.planning.len(), new: q.new.len() },
            has_candidates: !q.planning.is_empty() || !q.new.is_empty(),
        })
        .collect();
    let at = app.grooming.epic_index.min(rail.len().saturating_sub(1));
    let clone_all = |v: &[&jira::Issue]| v.iter().map(|i| (*i).clone()).collect::<Vec<_>>();
    let view = queues.get(at).map(|q| QueueView {
        active: active.clone(),
        next: next.clone(),
        sprint: clone_all(&q.sprint),
        next_sprint: clone_all(&q.next),
        ready: clone_all(&q.ready),
        planning: clone_all(&q.planning),
        new: clone_all(&q.new),
    });
    drop(queues);
    if rail.is_empty() {
        let g = &app.cfg.grooming;
        empty_state(ui, p, &format!("Nothing in {}, {} or {}", g.status_planning, g.status_ready, g.status_new));
        return;
    }
    let view = view.expect("a non-empty rail has a selected epic");
    let rows = view.ready.len() + view.planning.len() + view.new.len();
    let cursor = app.grooming.cursor.min(rows.saturating_sub(1));
    let focused: Option<jira::Issue> = view.ready.iter().chain(&view.planning).chain(&view.new).nth(cursor).cloned();
    // An epic with Ready rows only has nothing to groom, whatever the cursor is on.
    let nothing_to_groom = view.planning.is_empty() && view.new.is_empty();
    let list_w = (ui.available_width() * 0.36).clamp(LIST_MIN, LIST_MAX).min(ui.available_width());
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = PANE_GAP;
        let h = ui.available_height();
        ui.allocate_ui_with_layout(vec2(RAIL_W, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(RAIL_W);
            epic_rail(app, ui, p, &rail, at, active.as_deref(), next.as_deref());
        });
        ui.allocate_ui_with_layout(vec2(list_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(list_w);
            epic_queue(app, ui, p, &view, cursor);
        });
        let right_w = ui.available_width();
        ui.allocate_ui_with_layout(vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(right_w);
            match focused {
                Some(issue) if !nothing_to_groom => focused_ticket(app, ui, p, issue),
                _ => card(ui, p, |ui| {
                    ui.add(egui::Label::new(RichText::new(guide::nothing_to_groom(&app.cfg.grooming.status_new)).color(p.muted).size(14.0)).wrap());
                }),
            }
        });
    });
}

/// "CI-1860 Progressive Delivery · Sp 2 · Nx 1 · R 7 · P 3 · N 12": the epic, then its counts. The sprint counts
/// (`Sp`, `Nx`) are left out when zero; R, P and N always show. A key-only epic shows the key once.
pub fn rail_line(epic: Option<&str>, summary: &str, c: &RailCounts) -> String {
    let name = match epic {
        Some(k) if k != summary => format!("{k} {summary}"),
        _ => summary.to_string(),
    };
    format!("{name}{}", rail_counts(c))
}

/// The rail row's tail: " · Sp 2 · Nx 1 · R 7 · P 3 · N 12".
fn rail_counts(c: &RailCounts) -> String {
    let mut s = String::new();
    if c.sprint > 0 {
        s.push_str(&format!(" · Sp {}", c.sprint));
    }
    if c.next > 0 {
        s.push_str(&format!(" · Nx {}", c.next));
    }
    s.push_str(&format!(" · R {} · P {} · N {}", c.ready, c.planning, c.new));
    s
}

/// The rail row's tooltip: the counts spelt out.
fn rail_tip(c: &RailCounts, active: Option<&str>, next: Option<&str>, status_ready: &str, status_new: &str) -> String {
    let mut parts = Vec::new();
    if let Some(a) = active {
        parts.push(format!("{} in {a}", c.sprint));
    }
    if let Some(n) = next {
        parts.push(format!("{} in {n}", c.next));
    }
    parts.push(format!("{} {status_ready}", c.ready));
    parts.push(format!("{} Proposed", c.planning));
    parts.push(format!("{} {status_new}", c.new));
    parts.join(" · ")
}

/// The epics under their stage headers, each in its epic hue; those with nothing to groom are muted (`]`/`[` step over them).
fn epic_rail(app: &mut App, ui: &mut Ui, p: &Palette, rail: &[RailRow], at: usize, active: Option<&str>, next: Option<&str>) {
    let mut clicked = None;
    let g = app.cfg.grooming.clone();
    egui::ScrollArea::vertical().id_salt("groom-rail").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        let mut last_stage = None;
        for (idx, r) in rail.iter().enumerate() {
            if last_stage != Some(r.stage) {
                if last_stage.is_some() {
                    ui.add_space(GROUP_GAP);
                }
                ui.label(RichText::new(r.stage.label()).font(theme::bold(12.0)).color(p.muted));
                ui.add_space(4.0);
                last_stage = Some(r.stage);
            }
            let sel = idx == at;
            let color = match &r.epic {
                _ if !r.has_candidates => p.muted,
                Some(k) => theme::epic_tint_in(k, &app.epic_slots, p, &app.cfg.issues.epic_colors).fg,
                None => p.text,
            };
            let full = rail_line(r.epic.as_deref(), &r.summary, &r.counts);
            let resp = row_frame(ui, p, "groom-rail", sel, idx, rail.len(), |ui| {
                let font = theme::body(13.0);
                let painter = ui.painter().clone();
                let measure = |t: &str| painter.layout_no_wrap(t.to_string(), font.clone(), color).size().x;
                // The counts always show; the epic's name gives way.
                let tail = rail_counts(&r.counts);
                let head = full.strip_suffix(&tail).unwrap_or(&full);
                let head = crate::views::truncate_title(head, ui.available_width() - measure(&tail), measure);
                ui.label(RichText::new(format!("{head}{tail}")).font(font.clone()).color(color));
            });
            let resp = crate::views::tooltip(resp, format!("{full}\n{}", rail_tip(&r.counts, active, next, &g.status_ready, &g.status_new)));
            if resp.clicked() {
                clicked = Some(idx);
            }
        }
    });
    if let Some(i) = clicked {
        app.actions.push(Action::GroomEpic { delta: i as i32 - at as i32, skip_empty: false });
    }
}

/// A section label over a list: "Ready for Work · 7".
fn queue_label(ui: &mut Ui, p: &Palette, name: &str, n: usize) {
    ui.label(RichText::new(format!("{name} · {n}")).font(theme::bold(12.0)).color(p.muted));
    ui.add_space(8.0);
}

/// A context row: the key and title in muted text, not selectable.
fn context_row(ui: &mut Ui, p: &Palette, salt: &str, i: &jira::Issue, idx: usize, n: usize) {
    row_frame(ui, p, salt, false, idx, n, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label(RichText::new(&i.key).font(theme::mono(12.5)).color(p.muted));
            let painter = ui.painter().clone();
            let t =
                crate::views::truncate_title(&i.summary, ui.available_width(), |t| painter.layout_no_wrap(t.to_string(), theme::body(13.5), p.muted).size().x);
            ui.label(RichText::new(t).font(theme::body(13.5)).color(p.muted));
        });
    });
}

/// A sprint block: its label (the sprint's name) over muted context rows, or "None".
fn sprint_block(ui: &mut Ui, p: &Palette, name: &str, salt: &str, rows: &[jira::Issue]) {
    queue_label(ui, p, name, rows.len());
    if rows.is_empty() {
        ui.label(RichText::new("None").color(p.muted).size(12.0));
    }
    ui.spacing_mut().item_spacing.y = ROW_GAP;
    for (idx, i) in rows.iter().enumerate() {
        context_row(ui, p, salt, i, idx, rows.len());
    }
    ui.spacing_mut().item_spacing.y = ui.style().spacing.item_spacing.y;
    ui.add_space(GROUP_GAP);
}

/// A Proposed or New candidate row: title, then the report's badge, age and assignee pills, and the timer when selected.
#[allow(clippy::too_many_arguments)]
fn candidate_row(
    ui: &mut Ui,
    p: &Palette,
    i: &jira::Issue,
    idx: usize,
    n: usize,
    sel: bool,
    report: Option<&(String, report::Report)>,
    today: chrono::NaiveDate,
    elapsed: u64,
) -> egui::Response {
    row_frame(ui, p, "groom-cand", sel, idx, n, |ui| {
        title_line(ui, p, &i.key, &i.summary, sel);
        ui.add_space(LINE_GAP);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = PILL_GAP;
            if let Some((d, b)) = report.and_then(|(d, r)| r.badge_for(&i.key).map(|b| (d, b))) {
                badge(ui, p, &b, d);
            }
            neutral_pill(ui, p, &grooming::age_tag(&i.created, today), sel);
            neutral_pill(ui, p, i.assignee.as_ref().map_or("Unassigned", |u| u.display_name.as_str()), sel);
            if sel {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let c = timer_color(grooming::timer_level(elapsed), p);
                    crate::views::tooltip(
                        ui.label(RichText::new(grooming::fmt_elapsed(elapsed)).font(theme::mono(12.5)).color(c)),
                        "Time on this ticket: amber at 3 minutes, red at 5",
                    );
                });
            }
        });
    })
}

/// The selected epic's five blocks: the active sprint, the next sprint, Ready for Work (rank order, with the ghost row
/// while placing), then Proposed and New with the focused candidate's timer and, when open, the bounce box under it.
fn epic_queue(app: &mut App, ui: &mut Ui, p: &Palette, view: &QueueView, cursor: usize) {
    let report = app.grooming.report.as_ref().map(|(path, r)| (report_date(path, r), r.clone()));
    let placement = app.grooming.placement.map(|at| at.min(view.ready.len()));
    let elapsed = app.grooming.focused_since.map_or(0, |t| t.elapsed().as_secs());
    let today = chrono::Local::now().date_naive();
    let status = (app.cfg.grooming.status_ready.clone(), app.cfg.grooming.status_new.clone());
    let mut clicked = None;
    egui::ScrollArea::vertical().id_salt("groom-queue").auto_shrink([false, false]).show(ui, |ui| {
        if let Some(name) = &view.active {
            sprint_block(ui, p, name, "groom-sprint", &view.sprint);
        }
        if let Some(name) = &view.next {
            sprint_block(ui, p, name, "groom-next", &view.next_sprint);
        }
        let ready = &view.ready;
        queue_label(ui, p, &status.0, ready.len());
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        let rows = ready.len() + usize::from(placement.is_some());
        if rows == 0 {
            ui.label(RichText::new("None yet").color(p.muted).size(12.0));
        }
        let mut row = 0;
        for at in 0..=ready.len() {
            if placement == Some(at)
                && let Some(c) = view.planning.iter().chain(&view.new).nth(cursor.saturating_sub(ready.len()))
            {
                ghost_row(ui, p, c, row, rows);
                row += 1;
            }
            if let Some(i) = ready.get(at) {
                let sel = cursor == at;
                let resp = row_frame(ui, p, "groom-ready", sel, row, rows, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let color = if sel { p.text } else { p.muted };
                        ui.label(RichText::new(&i.key).font(theme::mono(12.5)).color(if sel { super::issues::row_key_color(true, p) } else { p.muted }));
                        let painter = ui.painter().clone();
                        let t = crate::views::truncate_title(&i.summary, ui.available_width(), |t| {
                            painter.layout_no_wrap(t.to_string(), theme::body(13.5), color).size().x
                        });
                        ui.label(RichText::new(t).font(theme::body(13.5)).color(color));
                    });
                });
                if resp.clicked() && app.grooming.placement.is_none() && app.grooming.bounce_text.is_none() {
                    clicked = Some(at);
                }
                row += 1;
            }
        }
        for (label, list, offset) in [("Proposed", &view.planning, ready.len()), (status.1.as_str(), &view.new, ready.len() + view.planning.len())] {
            ui.spacing_mut().item_spacing.y = ui.style().spacing.item_spacing.y;
            ui.add_space(GROUP_GAP);
            queue_label(ui, p, label, list.len());
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            for (idx, i) in list.iter().enumerate() {
                let sel = offset + idx == cursor;
                let resp = candidate_row(ui, p, i, idx, list.len(), sel, report.as_ref(), today, elapsed);
                if resp.clicked() && app.grooming.placement.is_none() && app.grooming.bounce_text.is_none() {
                    clicked = Some(offset + idx);
                }
                if sel && app.grooming.bounce_text.is_some() {
                    ui.spacing_mut().item_spacing.y = ui.style().spacing.item_spacing.y;
                    ui.add_space(8.0);
                    bounce_box(app, ui, p);
                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                }
            }
        }
    });
    if let Some(i) = clicked {
        app.actions.push(Action::GroomMove(i as i32 - cursor as i32));
    }
}

/// Where the accepted ticket would land: the selected-row fill with a muted outline.
fn ghost_row(ui: &mut Ui, p: &Palette, c: &jira::Issue, idx: usize, n: usize) {
    Frame::new()
        .fill(super::issues::row_fill(true, false, p))
        .stroke(Stroke::new(1.0, p.muted))
        .corner_radius(super::issues::row_corners(idx, n))
        .inner_margin(ROW_PAD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            title_line(ui, p, &c.key, &c.summary, true);
            ui.add_space(LINE_GAP);
            ui.label(RichText::new("Lands here · Enter to place").color(p.muted).size(12.0));
        });
}

/// The bounce comment box, under the focused candidate.
fn bounce_box(app: &mut App, ui: &mut Ui, p: &Palette) {
    let id = egui::Id::new("groom-bounce");
    let focus_now = std::mem::take(&mut app.grooming.bounce_focus);
    let status_new = app.cfg.grooming.status_new.clone();
    let Some(text) = app.grooming.bounce_text.as_mut() else { return };
    let focused = ui.memory(|m| m.has_focus(id));
    Frame::new()
        .fill(theme::input_fill(p.bg, p))
        .stroke(Stroke::new(1.0, if focused { p.accent } else { p.border }))
        .corner_radius(theme::RADIUS)
        .inner_margin(Margin::symmetric(11, 8))
        .show(ui, |ui| {
            let empty = text.is_empty();
            let size = vec2(ui.available_width(), BOUNCE_H);
            let r = crate::views::add_field(ui, Some(size), text, |t| {
                egui::TextEdit::multiline(t)
                    .id(id)
                    .font(theme::body(14.0))
                    .frame(Frame::new().inner_margin(crate::views::caret_margin(Margin::ZERO, empty)))
                    .hint_text(crate::views::empty_field_hint(BOUNCE_HINT, theme::body(14.0), p))
            });
            if focus_now {
                r.request_focus();
            }
        });
    let (note, warn) = bounce_note(text, &status_new);
    ui.add_space(4.0);
    ui.label(RichText::new(note).color(if warn { p.at_risk } else { p.muted }).size(12.0));
}

/// The line under the bounce box, and whether it warns.
pub fn bounce_note(text: &str, status_new: &str) -> (String, bool) {
    if grooming::bounce_comment_ok(text) {
        (format!("⌘Enter posts the comment, then moves the ticket to {status_new}"), false)
    } else if text.trim().is_empty() {
        ("A one- or two-line comment is required".to_string(), false)
    } else {
        ("Two lines at most".to_string(), true)
    }
}

/// The per-ticket timer's colour: plain text, then amber, then red.
pub fn timer_color(level: Level, p: &Palette) -> Color32 {
    match level {
        Level::Ok => p.text,
        Level::Amber => p.at_risk,
        Level::Red => p.off_track,
    }
}

/// "Report 2026-09-04 · 11 promoted · 14 need detail", or "No report found in <dir>" naming the configured dir.
pub fn report_line(report: Option<(&std::path::Path, &report::Report)>, dir: &str) -> String {
    match report {
        Some((path, r)) => format!("Report {} · {} promoted · {} need detail", report_date(path, r), r.promoted.len(), r.needs_detail.len()),
        None => format!("No report found in {dir}"),
    }
}

/// The report's own date, else its file name's stem.
pub fn report_date(path: &std::path::Path, r: &report::Report) -> String {
    if r.date.is_empty() { path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string() } else { r.date.clone() }
}

/// "Ready for Work 9 · soft 8 · hard 12".
pub fn ceiling_text(ready: usize, soft: usize, hard: usize) -> String {
    format!("Ready for Work {ready} · soft {soft} · hard {hard}")
}

/// A traffic-light level in the health colours.
pub fn level_color(level: Level, p: &Palette) -> Color32 {
    match level {
        Level::Ok => p.on_track,
        Level::Amber => p.at_risk,
        Level::Red => p.off_track,
    }
}

/// A report badge's text: what the last pre-check said about a ticket.
pub fn badge_text(badge: &Badge, report_date: &str) -> String {
    match badge {
        Badge::Promoted => "promoted by pre-check".to_string(),
        Badge::NeedsDetail(codes) => format!("needs detail: {} (report {report_date})", codes.join(", ")),
        Badge::Struck => "closed after pre-check".to_string(),
    }
}

/// The report badge as an outline pill (only status is filled): green when promoted, amber when it needs detail,
/// neutral when closed since.
pub fn badge(ui: &mut Ui, p: &Palette, badge: &Badge, report_date: &str) -> egui::Response {
    let text = badge_text(badge, report_date);
    let hue = match badge {
        Badge::Promoted => p.on_track,
        Badge::NeedsDetail(_) => p.at_risk,
        Badge::Struck => return outline_pill(ui, &text, p, PILL_MARGIN),
    };
    pill_sized(ui, &text, Color32::TRANSPARENT, hue, Stroke::new(1.0, hue), PILL_MARGIN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn report_line_counts_or_names_the_configured_dir() {
        let r = report::parse(
            "# x — 2026-09-04\n\n## Planning — by epic\n\n### (no epic)\n\n- [CI-1](u) — a · rank 1 · older\n\n## Needs detail (still in `New`)\n\n### (no epic)\n\n- [CI-2](u) — b · rank 2 · created 2026-09-01 · missing: empty\n",
        );
        assert_eq!(report_line(Some((Path::new("/r/2026-09-04.md"), &r)), "~/x"), "Report 2026-09-04 · 1 promoted · 1 need detail");
        assert_eq!(report_line(None, "~/Documents/grooming-reports"), "No report found in ~/Documents/grooming-reports");
        let undated = report::Report { date: String::new(), ..r };
        assert_eq!(report_date(Path::new("/r/2026-10-01.md"), &undated), "2026-10-01");
    }

    #[test]
    fn badges_say_what_the_report_said() {
        assert_eq!(badge_text(&Badge::Promoted, "2026-09-04"), "promoted by pre-check");
        let nd = Badge::NeedsDetail(vec!["unclear-when".into(), "empty".into()]);
        assert_eq!(badge_text(&nd, "2026-09-04"), "needs detail: unclear-when, empty (report 2026-09-04)");
        assert_eq!(badge_text(&Badge::Struck, "2026-09-04"), "closed after pre-check");
    }

    #[test]
    fn summary_wording_and_promote_gate() {
        let plan = Promotion {
            promoted: vec!["CI-1".into(), "CI-2".into()],
            held_back: vec!["CI-3".into()],
            needs_detail: vec![("CI-4".into(), vec![grooming::Code::UnclearWhat, grooming::Code::Empty])],
        };
        assert_eq!(summary_line(&plan, 4), "Summary · 3 ready · 1 need detail · 4 without a verdict");
        assert_eq!(plan_lines(&plan, 4, 2), ["2 to promote to Planning", "1 ready but held back (cap 2)", "1 need detail", "4 without a verdict"]);
        assert_eq!(promote_label(2), "Promote 2");
        assert!(promote_enabled(2, true, false));
        assert!(!promote_enabled(0, true, false), "nothing to promote");
        assert!(!promote_enabled(2, false, false), "read-only");
        assert!(!promote_enabled(2, true, true), "already running");
        assert_eq!(verdict_text(&Verdict::NeedsDetail(plan.needs_detail[0].1.clone())), "needs detail: unclear-what, empty");
        assert_eq!(verdict_text(&Verdict::Ready), "ready");
    }

    #[test]
    fn the_hint_follows_the_open_dialog() {
        let mut g = GroomingState::default();
        assert_eq!(hint(&g), GroomHint::PreCheck);
        g.summary_open = true;
        assert_eq!(hint(&g), GroomHint::Summary);
        g.pending_report = Some(String::new());
        assert_eq!(hint(&g), GroomHint::Overwrite, "the confirm sits over everything");
        let mut g = GroomingState { mode: GroomMode::Session, ..Default::default() };
        assert_eq!(hint(&g), GroomHint::Session);
        g.placement = Some(0);
        assert_eq!(hint(&g), GroomHint::Placement);
        g.bounce_text = Some(String::new());
        assert_eq!(hint(&g), GroomHint::Bounce);
    }

    #[test]
    fn ceiling_levels_take_the_health_colours() {
        let p = &theme::LIGHT;
        assert_eq!(level_color(Level::Ok, p), p.on_track);
        assert_eq!(level_color(Level::Amber, p), p.at_risk);
        assert_eq!(level_color(Level::Red, p), p.off_track);
        assert_eq!(ceiling_text(9, 8, 12), "Ready for Work 9 · soft 8 · hard 12");
    }

    #[test]
    fn both_modes_draw_headless() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        let status = |n: &str| crate::jira::Status { name: n.into(), category: "new".into() };
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![
                crate::jira::Issue { key: "CI-1".into(), labels: vec!["PLAT".into()], status: status("New"), ..Default::default() },
                crate::jira::Issue { key: "CI-2".into(), labels: vec!["PLAT".into()], status: status("Ready for Work"), ..Default::default() },
            ],
        );
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let pre = texts(&mut app);
        // `for_test` swaps the default report dir for one that does not exist.
        let no_report = format!("No report found in {}", app.cfg.grooming.report_dir);
        for want in [
            "Pre-check",
            "Session",
            no_report.as_str(),
            "Ready for Work 1 · soft 10 · hard 15",
            "Summary · 0 ready · 0 need detail · 1 without a verdict",
            "rank 1",
            "older",
        ] {
            assert!(pre.iter().any(|t| t == want), "{want} in {pre:?}");
        }
        let pre_purpose = guide::purpose(GroomMode::PreCheck, "New", "Planning", "Ready for Work");
        for want in [pre_purpose.as_str(), "Readiness bar", "Can a stranger tell WHAT is being done and WHEN it is done?", "When in doubt, pass."] {
            assert!(pre.iter().any(|t| t == want), "{want} in {pre:?}");
        }
        app.grooming.cursor = 1;
        let past = texts(&mut app);
        assert!(past.iter().any(|t| t == "0 of 1 judged. Enter opens the summary; j/k go back."), "{past:?}");
        app.grooming.verdicts.insert("CI-1".into(), Verdict::Ready);
        let past = texts(&mut app);
        assert!(past.iter().any(|t| t == "All 1 judged. Enter opens the summary."), "{past:?}");
        app.grooming.cursor = 0;
        app.grooming.summary_open = true;
        let summary = texts(&mut app);
        let path = guide::report_path(&app.cfg.grooming.report_dir, chrono::Local::now().date_naive());
        let explainer = guide::promote_explainer(1, 0, app.cfg.grooming.planning_cap, "Planning", &path);
        for want in ["ready", "Promote to Planning", explainer.as_str(), "Promote (1)", "Promote 1", "Cancel"] {
            assert!(summary.iter().any(|t| t == want), "{want} in {summary:?}");
        }
        app.grooming.summary_open = false;
        app.grooming.pending_report = Some("text".into());
        let confirm = texts(&mut app);
        for want in ["Report for today exists. Overwrite?", "Overwrite", "Keep the old one"] {
            assert!(confirm.iter().any(|t| t == want), "{want} in {confirm:?}");
        }
        app.grooming.pending_report = None;
        app.grooming.mode = GroomMode::Session;
        let session = texts(&mut app);
        assert!(session.iter().any(|t| t == "(no epic) · R 1 · P 0 · N 1"), "{session:?}");
        let session_purpose = guide::purpose(GroomMode::Session, "New", "Planning", "Ready for Work");
        for want in [session_purpose.as_str(), "No epic", "Ready for Work · 1", "Proposed · 0", "New · 1"] {
            assert!(session.iter().any(|t| t == want), "{want} in {session:?}");
        }
    }

    #[test]
    fn session_draws_stages_blocks_ghost_and_bounce_box() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        let issue = |k: &str, st: &str, epic: Option<&str>, sprint: Option<&str>| crate::jira::Issue {
            key: k.into(),
            summary: format!("Summary {k}"),
            labels: vec!["PLAT".into()],
            status: crate::jira::Status { name: st.into(), category: "new".into() },
            parent_key: epic.map(String::from),
            parent_summary: epic.map(|_| "Blue Green Ops".to_string()),
            sprint: sprint.map(String::from),
            ..Default::default()
        };
        app.sprints = vec![
            crate::jira::Sprint { id: 1, name: "Sprint 92".into(), state: "active".into() },
            crate::jira::Sprint { id: 2, name: "Sprint 93".into(), state: "future".into() },
        ];
        app.store.absorb(
            crate::store::ListId::Epics,
            vec![crate::jira::Issue {
                key: "CI-1860".into(),
                summary: "Blue Green Ops".into(),
                issue_type: "Epic".into(),
                labels: vec!["roadmap".into(), "P0".into()],
                ..Default::default()
            }],
        );
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![
                issue("CI-5", "Ready for Work", Some("CI-1860"), None),
                issue("CI-1", "Planning", Some("CI-1860"), None),
                issue("CI-3", "New", Some("CI-1860"), None),
                issue("CI-4", "Work in Progress", Some("CI-1860"), Some("Sprint 92")),
                issue("CI-6", "Ready for Work", Some("CI-1860"), Some("Sprint 93")),
                issue("CI-9", "New", None, None),
            ],
        );
        app.grooming.mode = GroomMode::Session;
        app.grooming.cursor = 1; // CI-1, the first candidate
        app.grooming.focused_since = Some(std::time::Instant::now());
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1400.0, 900.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let t = texts(&mut app);
        for want in [
            "Decide",
            "Ready for Work: accept and place it in rank.",
            "Move a Ready for Work ticket down or up in rank.",
            "Three to five minutes per ticket. Past that, bounce.",
        ] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        for want in [
            "Roadmap",
            "CI-1860 Blue Green Ops · Sp 1 · Nx 1 · R 1 · P 1 · N 1",
            "No epic",
            "(no epic) · R 0 · P 0 · N 1",
            "Sprint 92 · 1",
            "Sprint 93 · 1",
            "Ready for Work · 1",
            "Proposed · 1",
            "New · 1",
            "0:00",
            "Unassigned",
            "CI-5",
            "CI-4",
            "CI-6",
        ] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Ongoing" || x == "Other epics"), "stage headers only for stages with epics: {t:?}");
        app.grooming.placement = Some(0);
        let t = texts(&mut app);
        assert!(t.iter().any(|x| x == "Lands here · Enter to place"), "{t:?}");
        app.grooming.placement = None;
        app.grooming.bounce_text = Some(String::new());
        let t = texts(&mut app);
        assert!(t.iter().any(|x| x == "A one- or two-line comment is required"), "{t:?}");
        app.grooming.bounce_text = None;
        app.grooming.cursor = 0;
        let t = texts(&mut app);
        assert!(!t.iter().any(|x| x == "0:00"), "no timer on a Ready row: {t:?}");
    }

    #[test]
    fn session_wording() {
        let counts = |s, n, r, p, new| RailCounts { sprint: s, next: n, ready: r, planning: p, new };
        assert_eq!(rail_line(Some("CI-1860"), "Progressive Delivery", &counts(2, 1, 7, 3, 12)), "CI-1860 Progressive Delivery · Sp 2 · Nx 1 · R 7 · P 3 · N 12");
        assert_eq!(
            rail_line(Some("CI-1860"), "CI-1860", &counts(0, 0, 1, 0, 2)),
            "CI-1860 · R 1 · P 0 · N 2",
            "a key-only epic shows the key once; zero sprint counts are left out"
        );
        assert_eq!(rail_line(None, "(no epic)", &counts(0, 3, 0, 1, 0)), "(no epic) · Nx 3 · R 0 · P 1 · N 0");
        assert_eq!(bounce_note("Which clusters?", "New"), ("⌘Enter posts the comment, then moves the ticket to New".to_string(), false));
        assert_eq!(bounce_note(" ", "New").0, "A one- or two-line comment is required");
        assert_eq!(bounce_note("a\nb\nc", "New"), ("Two lines at most".to_string(), true));
        let p = &theme::LIGHT;
        assert_eq!(timer_color(Level::Ok, p), p.text);
        assert_eq!(timer_color(Level::Amber, p), p.at_risk);
        assert_eq!(timer_color(Level::Red, p), p.off_track);
    }

    #[test]
    fn an_epic_with_nothing_to_groom_says_so() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![crate::jira::Issue {
                key: "CI-2".into(),
                labels: vec!["PLAT".into()],
                status: crate::jira::Status { name: "Ready for Work".into(), category: "new".into() },
                parent_key: Some("CI-1".into()),
                parent_summary: Some("Metrics Retention".into()),
                ..Default::default()
            }],
        );
        app.grooming.mode = GroomMode::Session;
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let t: Vec<String> = crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect();
        assert!(t.iter().any(|x| x == "Nothing Proposed or New here. Run a pre-check first, or pick another epic with ]."), "{t:?}");
    }
}
