//! Header with the screen switcher, sign-in, toast, shared widgets.

pub mod adf_view;
pub mod detail;
pub mod issues;

use crate::app::{Action, App, Screen};
use crate::theme::{self, Palette};
use egui::{Color32, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};

pub fn main(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    header(app, ui, p);
    if app.offline {
        offline_banner(app, ui, p);
    }
    match app.screen {
        Screen::Issues => issues::show(app, ui),
        Screen::Grooming => placeholder(ui, p, "Grooming arrives in plan 2."),
        Screen::Team => placeholder(ui, p, "Team view arrives in plan 3."),
        Screen::Roadmap => placeholder(ui, p, "Roadmap arrives in plan 3."),
    }
}

fn placeholder(ui: &mut Ui, p: &Palette, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.label(RichText::new(text).color(p.muted).size(16.0));
    });
}

fn header(app: &mut App, ui: &mut Ui, p: &Palette) {
    egui::Panel::top("header")
        .exact_size(48.0)
        .frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(16, 0)).stroke(Stroke::new(1.0, p.border)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("Jira Tickets").font(theme::bold(15.0)).color(p.text));
                ui.add_space(16.0);
                for (i, (s, name)) in
                    [(Screen::Issues, "Issues"), (Screen::Grooming, "Grooming"), (Screen::Team, "Team"), (Screen::Roadmap, "Roadmap")].into_iter().enumerate()
                {
                    let active = app.screen == s;
                    let text =
                        RichText::new(name).font(if active { theme::bold(14.0) } else { theme::body(14.0) }).color(if active { p.text } else { p.muted });
                    let r = ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_text(format!("⌘{}", i + 1));
                    if active {
                        let rect = r.rect;
                        ui.painter().hline(rect.x_range(), rect.bottom() + 14.0, Stroke::new(2.0, p.accent));
                    }
                    if r.clicked() {
                        app.actions.push(Action::SetScreen(s));
                    }
                    ui.add_space(6.0);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(me) = &app.me {
                        let r = ui.label(RichText::new(&me.display_name).color(p.muted).size(13.0)).interact(Sense::click());
                        r.context_menu(|ui| {
                            if ui.button("Sign out").clicked() {
                                app.actions.push(Action::SignOut);
                            }
                        });
                    }
                    ui.add_space(8.0);
                    if app.lists.values().any(|l| l.loading) {
                        ui.add(egui::Spinner::new().size(16.0).color(p.muted));
                    } else if ui.add(button("Refresh", p)).on_hover_text("⌘R").clicked() {
                        app.actions.push(Action::Refresh);
                    }
                    if crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed) {
                        pill(ui, "read-only", p.danger_subtle, p.off_track);
                    }
                });
            });
        });
}

fn offline_banner(app: &mut App, ui: &mut Ui, p: &Palette) {
    let when = app
        .store
        .fetched_at
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or("earlier".into());
    Frame::new().fill(p.danger_subtle).inner_margin(Margin::symmetric(16, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Offline. Showing data from {when}.")).color(p.off_track).size(13.0));
            if ui.add(button("Retry", p)).clicked() {
                app.actions.push(Action::Refresh);
            }
        });
    });
}

pub fn sign_in(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 160.0).max(40.0));
        ui.label(RichText::new("Sign in to Jira").font(theme::body(24.0)).color(p.text));
        ui.add_space(16.0);
        if app.finding_token {
            ui.add(egui::Spinner::new().size(20.0));
            ui.label(RichText::new("Looking for your Jira token…").color(p.muted));
            return;
        }
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(380.0);
            ui.vertical(|ui| {
                if let Some(e) = &app.token_error {
                    ui.label(RichText::new(e).color(p.off_track).size(13.0));
                    ui.label(
                        RichText::new(format!(
                            "Site: {}\nEmail: {}\nCheck that the email matches your Atlassian account and the token is valid (https://id.atlassian.com/manage-profile/security/api-tokens).",
                            app.cfg.site, app.cfg.email
                        ))
                        .color(p.muted)
                        .size(12.0),
                    );
                    ui.add_space(8.0);
                }
                ui.label(RichText::new("No JIRA_API_TOKEN, saved token, or config api_token was found. Paste an Atlassian API token.").size(13.0).color(p.muted));
                ui.add_space(10.0);
                let r = ui.add(egui::TextEdit::singleline(&mut app.token_input).password(true).desired_width(f32::INFINITY).margin(vec2(8.0, 6.0)));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.actions.push(Action::SubmitToken);
                }
                ui.add_space(12.0);
                let w = ui.available_width();
                if ui.add_sized([w, 32.0], primary_button("Sign in", p)).clicked() {
                    app.actions.push(Action::SubmitToken);
                }
            });
        });
        ui.add_space(8.0);
        if ui.add(button("Look again", p)).clicked() {
            app.actions.push(Action::FindToken);
        }
    });
}

pub fn toast(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &app.toast else { return };
    if std::time::Instant::now() > t.until {
        app.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let p = theme::palette(ctx);
    egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -24.0)).show(ctx, |ui| {
        Frame::new().fill(p.tooltip).corner_radius(6).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
            let color = if t.error { Color32::from_rgb(0xff, 0x81, 0x82) } else { Color32::from_rgb(0x4a, 0xc2, 0x6b) };
            ui.horizontal(|ui| {
                health_dot(ui, color);
                ui.label(RichText::new(&t.text).color(Color32::WHITE).size(14.0));
            });
        });
    });
}

// ---------- Shared widgets ----------

pub fn button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(p.text).size(13.0)).fill(p.surface_2).stroke(Stroke::new(1.0, p.border)).corner_radius(6)
}

pub fn primary_button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(Color32::WHITE).size(13.0)).fill(p.accent).stroke(Stroke::NONE).corner_radius(6)
}

pub fn pill(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    Frame::new().fill(bg).corner_radius(10).inner_margin(Margin::symmetric(7, 1)).show(ui, |ui| ui.label(RichText::new(text).color(fg).size(11.0))).response
}

pub fn health_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// Returns true when Retry is clicked.
pub fn error_banner(ui: &mut Ui, p: &Palette, title: &str, raw: &str) -> bool {
    let mut retry = false;
    Frame::new().fill(p.danger_subtle).corner_radius(6).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.label(RichText::new(title).font(theme::bold(14.0)).color(p.off_track));
        ui.label(RichText::new(raw).color(p.muted).size(12.0));
        retry = ui.add(button("Retry", p)).clicked();
    });
    retry
}
