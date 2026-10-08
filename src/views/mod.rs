//! Header with the screen switcher, sign-in, toast, shared widgets.

pub mod adf_view;
pub mod detail;
pub mod help;
pub mod icons;
pub mod issues;
pub mod pickers;

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

/// The tabs, in ⌘1–⌘4 order.
const TABS: [(Screen, &str); 4] = [(Screen::Issues, "Issues"), (Screen::Grooming, "Grooming"), (Screen::Team, "Team"), (Screen::Roadmap, "Roadmap")];

/// Each tab is as wide as its bold (active) label, so switching tabs never moves its neighbours.
pub fn tab_width(bold_w: f32, regular_w: f32) -> f32 {
    bold_w.max(regular_w).ceil()
}

/// The header spinner is for syncing the refresh lists; a search shows its own spinner in the count row.
pub fn header_syncing<'a>(lists: impl IntoIterator<Item = (&'a crate::store::ListId, &'a crate::app::Loaded)>) -> bool {
    lists.into_iter().any(|(id, l)| *id != crate::store::ListId::Search && l.loading)
}

fn header(app: &mut App, ui: &mut Ui, p: &Palette) {
    egui::Panel::top("header")
        .exact_size(48.0)
        .frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(16, 0)).stroke(Stroke::new(1.0, p.border)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("Jira Tickets").font(theme::bold(15.0)).color(p.text));
                ui.add_space(16.0);
                ui.spacing_mut().item_spacing.x = 16.0;
                for (i, (s, name)) in TABS.into_iter().enumerate() {
                    let active = app.screen == s;
                    let bold = ui.painter().layout_no_wrap(name.to_string(), theme::bold(14.0), p.text);
                    let regular = ui.painter().layout_no_wrap(name.to_string(), theme::body(14.0), p.muted);
                    let w = tab_width(bold.size().x, regular.size().x);
                    let (rect, r) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
                    let galley = if active { bold } else { regular };
                    let pos = rect.center() - galley.size() / 2.0;
                    ui.painter().galley(pos, galley, p.text);
                    if active {
                        ui.painter().hline(rect.x_range(), rect.bottom() + 7.0, Stroke::new(2.0, p.accent));
                    }
                    let r = r.on_hover_text(format!("{name} (⌘{})", i + 1));
                    if r.clicked() {
                        app.actions.push(Action::SetScreen(s));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if let Some(me) = &app.me {
                        let r = ui.add(egui::Label::new(RichText::new(&me.display_name).color(p.muted).size(14.0)).selectable(false).sense(Sense::click()));
                        r.on_hover_text("Right-click to sign out").context_menu(|ui| {
                            if ui.button("Sign out").clicked() {
                                app.actions.push(Action::SignOut);
                            }
                        });
                    }
                    // One fixed 32×32 slot: the spinner replaces the button at the same size.
                    if header_syncing(&app.lists) {
                        icons::spinner_button(ui, p);
                    } else if icons::icon_button(ui, icons::Icon::Sync, "Refresh (⌘R)", p).clicked() {
                        app.actions.push(Action::Refresh);
                    }
                    if icons::text_button(ui, "?", "Keyboard help (?)", p).clicked() {
                        app.actions.push(Action::ToggleHelp);
                    }
                    if crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed) {
                        pill_sized(ui, "read-only", p.surface_2, p.muted, Stroke::NONE, HEADER_PILL_MARGIN);
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
            icons::spinner(ui, 20.0, p);
            ui.label(RichText::new("Looking for your Jira token…").color(p.muted));
            return;
        }
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(6).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(380.0);
            ui.vertical(|ui| {
                if let Some(e) = &app.token_error {
                    ui.label(RichText::new(e).color(p.off_track).size(13.0));
                    ui.add_space(4.0);
                }
                // A login means Jira rejected it (401): show what was sent.
                let prompt = if let Some(login) = &app.login {
                    ui.label(
                        RichText::new(format!(
                            "Site: {}\nEmail: {}\nToken: {} ({})\nCheck that the email matches your Atlassian account and the token is valid (https://id.atlassian.com/manage-profile/security/api-tokens).",
                            app.cfg.site,
                            login.email,
                            crate::auth::mask_token(&login.token),
                            login.source.describe()
                        ))
                        .color(p.muted)
                        .size(12.0),
                    );
                    ui.add_space(6.0);
                    if ui.add(button("Sign out", p)).on_hover_text("Forget the saved token").clicked() {
                        app.actions.push(Action::SignOut);
                    }
                    ui.add_space(8.0);
                    "Paste a new Atlassian API token."
                } else if app.cfg.api_token.is_some() && app.token_error.is_some() {
                    "The config api_token could not be resolved. Fix it and look again, or paste an Atlassian API token."
                } else if app.cfg.api_token.is_some() {
                    "Signed out. Look again to use the config api_token, or paste an Atlassian API token."
                } else {
                    "No JIRA_API_TOKEN, saved token, or config api_token was found. Paste an Atlassian API token."
                };
                ui.label(RichText::new(prompt).size(13.0).color(p.muted));
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

/// How far the toast's bottom sits above the window bottom: 16 px above the hint bar when one shows, else 24 px.
pub fn toast_offset(hint_bar: Option<f32>) -> f32 {
    hint_bar.map_or(24.0, |h| h + 16.0)
}

pub fn toast(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &app.toast else { return };
    if std::time::Instant::now() > t.until {
        app.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let p = theme::palette(ctx);
    let hint_bar = (app.client.is_some() && app.screen == Screen::Issues).then_some(issues::HINT_BAR_H);
    let shadow = egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(60) };
    egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -toast_offset(hint_bar))).show(ctx, |ui| {
        Frame::new().fill(p.tooltip).stroke(Stroke::new(1.0, p.border)).shadow(shadow).corner_radius(6).inner_margin(Margin::symmetric(16, 8)).show(ui, |ui| {
            let color = if t.error { Color32::from_rgb(0xff, 0x81, 0x82) } else { Color32::from_rgb(0x4a, 0xc2, 0x6b) };
            ui.horizontal(|ui| {
                health_dot(ui, color);
                ui.label(RichText::new(&t.text).color(Color32::WHITE).size(14.0));
            });
        });
    });
}

// ---------- Shared widgets ----------

/// Height of every text button (and the filter combos), so rows of controls line up.
pub const BUTTON_H: f32 = 28.0;

/// The page dimming behind a modal: light needs less to read as "behind".
pub fn backdrop_alpha(p: &Palette) -> u8 {
    if p.dark { 150 } else { 90 }
}

/// A modal dialog's fill: a step above the dimmed page in dark, plain surface in light.
pub fn dialog_fill(p: &Palette) -> Color32 {
    if p.dark { p.surface_2 } else { p.surface }
}

/// Paints the modal backdrop over the whole window and swallows its clicks.
pub fn backdrop(ctx: &egui::Context, id: &str, order: egui::Order, p: &Palette) -> egui::Response {
    egui::Area::new(egui::Id::new(id))
        .order(order)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(ctx, |ui| {
            let r = ui.ctx().content_rect();
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(backdrop_alpha(p)));
            ui.allocate_rect(r, Sense::click())
        })
        .inner
}

pub fn button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(p.text).size(14.0))
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(6)
        .min_size(vec2(0.0, BUTTON_H))
}

/// The check mark the active state shows; the inactive state reserves its width.
pub const CHECK: &str = "\u{2713} ";

/// The toggle's text: the check is always laid out (transparent when off) so toggling never moves neighbours.
pub fn toggle_job(text: &str, active: bool, fg: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let fmt = |color| egui::TextFormat { font_id: theme::body(14.0), color, ..Default::default() };
    job.append(CHECK, 0.0, fmt(if active { fg } else { Color32::TRANSPARENT }));
    job.append(text, 0.0, fmt(fg));
    job
}

pub fn toggle_button(text: &str, active: bool, p: &Palette) -> egui::Button<'static> {
    let (fill, fg, stroke) = if active { (p.accent, p.on_accent, p.accent) } else { (p.surface_2, p.text, p.border) };
    egui::Button::new(toggle_job(text, active, fg)).fill(fill).stroke(Stroke::new(1.0, stroke)).corner_radius(6).min_size(vec2(0.0, BUTTON_H))
}

pub fn primary_button(text: &str, p: &Palette) -> egui::Button<'static> {
    primary_button_if(text, true, p)
}

/// A primary button that, when disabled, is drawn quiet (surface_2, muted, border) and ignores clicks,
/// rather than a faded accent.
pub fn primary_button_if(text: &str, enabled: bool, p: &Palette) -> egui::Button<'static> {
    let b = if enabled {
        egui::Button::new(RichText::new(text.to_string()).color(p.on_accent).size(14.0)).fill(p.accent).stroke(Stroke::new(1.0, p.accent))
    } else {
        egui::Button::new(RichText::new(text.to_string()).color(p.muted).size(14.0)).fill(p.surface_2).stroke(Stroke::new(1.0, p.border)).sense(Sense::hover())
    };
    b.corner_radius(6).min_size(vec2(0.0, BUTTON_H))
}

/// Pill inner padding (8/2) and radius (10).
pub const PILL_MARGIN: egui::Vec2 = vec2(8.0, 2.0);
/// Header pills sit beside 32 px buttons, so they get more height (8/4).
pub const HEADER_PILL_MARGIN: egui::Vec2 = vec2(8.0, 4.0);

pub fn pill(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    pill_sized(ui, text, bg, fg, Stroke::NONE, PILL_MARGIN)
}

/// A pill with a 1 px `border` edge, for rows whose fill would swallow a translucent wash.
pub fn pill_outlined(ui: &mut Ui, text: &str, bg: Color32, fg: Color32, p: &Palette) -> egui::Response {
    pill_sized(ui, text, bg, fg, Stroke::new(1.0, p.border), PILL_MARGIN)
}

/// Small text in an exact `text + 2×margin` rect (radius 10); the stroke is drawn inside, so outlined
/// and plain pills are the same size and share a baseline.
pub fn pill_sized(ui: &mut Ui, text: &str, bg: Color32, fg: Color32, stroke: Stroke, margin: egui::Vec2) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::body(12.0), fg);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + margin * 2.0, Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(rect, 10.0, bg, stroke, egui::StrokeKind::Inside);
        ui.painter().galley(rect.min + margin, galley, fg);
    }
    resp
}

pub fn health_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// Returns true when Retry is clicked.
pub fn error_banner(ui: &mut Ui, p: &Palette, title: &str, raw: &str) -> bool {
    let mut retry = false;
    Frame::new().fill(p.danger_subtle).corner_radius(6).inner_margin(Margin::same(12)).show(ui, |ui| {
        ui.label(RichText::new(title).font(theme::bold(14.0)).color(p.off_track));
        ui.label(RichText::new(raw).color(p.muted).size(12.0));
        retry = ui.add(button("Retry", p)).clicked();
    });
    retry
}

#[cfg(test)]
mod widget_tests {
    use super::*;

    #[test]
    fn toggle_reserves_the_check_width_when_off() {
        let (on, off) = (toggle_job("Mine", true, Color32::WHITE), toggle_job("Mine", false, Color32::WHITE));
        assert_eq!(on.text, off.text, "same glyphs laid out either way, so the width never changes");
        assert_eq!(on.text, "\u{2713} Mine");
        assert_eq!(off.sections[0].format.color, Color32::TRANSPARENT);
        assert_eq!(on.sections[0].format.color, Color32::WHITE);
    }

    #[test]
    fn tabs_reserve_their_bold_width() {
        assert_eq!(tab_width(52.3, 48.0), 53.0);
        assert_eq!(tab_width(40.0, 41.0), 41.0, "never narrower than either face");
    }

    #[test]
    fn header_spinner_ignores_search() {
        use crate::app::Loaded;
        use crate::store::ListId;
        let loading = || Loaded { loading: true, ..Default::default() };
        let search = [(ListId::Search, loading())];
        assert!(!header_syncing(search.iter().map(|(i, l)| (i, l))));
        let sync = [(ListId::Search, Loaded::default()), (ListId::Epics, loading())];
        assert!(header_syncing(sync.iter().map(|(i, l)| (i, l))));
        assert!(!header_syncing(std::iter::empty()));
    }

    #[test]
    fn backdrop_is_heavier_in_dark() {
        assert!(backdrop_alpha(&theme::DARK) > backdrop_alpha(&theme::LIGHT));
        assert_eq!(dialog_fill(&theme::DARK), theme::DARK.surface_2);
        assert_eq!(dialog_fill(&theme::LIGHT), theme::LIGHT.surface);
    }

    #[test]
    fn toast_sits_above_the_hint_bar_when_there_is_one() {
        assert_eq!(toast_offset(Some(24.0)), 40.0);
        assert_eq!(toast_offset(None), 24.0);
    }
}
