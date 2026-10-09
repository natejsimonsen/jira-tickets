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
    let bar = header(app, ui, p);
    // Every signed-in screen keeps the hint bar.
    issues::hints_panel(app, ui, p);
    if app.offline {
        offline_banner(app, ui, p);
    }
    match app.screen {
        Screen::Issues => issues::show(app, ui),
        Screen::Grooming => placeholder(ui, p, "Grooming"),
        Screen::Team => placeholder(ui, p, "Team"),
        Screen::Roadmap => placeholder(ui, p, "Roadmap"),
    }
    // One 1 px rule just under the 48 px bar (so the buttons sit 8/8 and the 2 px underline sits on the rule,
    // not under it), painted after the panels below so their fills do not cover it; modals are other layers.
    ui.painter().hline(bar.x_range(), header_rule_y(bar.bottom()), Stroke::new(1.0, p.border));
}

/// "<Tab> is coming soon", named as the tab is.
pub fn placeholder_text(tab: &str) -> String {
    format!("{tab} is coming soon")
}

fn placeholder(ui: &mut Ui, p: &Palette, tab: &str) {
    centred(ui, "placeholder", |ui| empty_state(ui, p, &placeholder_text(tab)));
}

/// The one empty-state line (placeholders, "Select an issue"): body 14, muted, a label without a period.
pub fn empty_state(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).color(p.muted).size(EMPTY_STATE_SIZE));
}

/// The empty-state text size.
pub const EMPTY_STATE_SIZE: f32 = 14.0;

/// The top space that centres `content_h` in `avail_h` (never negative).
pub fn centre_top(avail_h: f32, content_h: f32) -> f32 {
    ((avail_h - content_h) / 2.0).max(0.0).round()
}

/// The one centring rule (placeholders, sign-in): `content` centred horizontally and vertically in the
/// space left, using its height from the previous frame.
fn centred(ui: &mut Ui, id: &str, content: impl FnOnce(&mut Ui)) {
    let id = egui::Id::new(("centred", id));
    let last_h: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    ui.vertical_centered(|ui| {
        ui.add_space(centre_top(ui.available_height(), last_h));
        let top = ui.cursor().top();
        content(ui);
        let h = ui.min_rect().bottom() - top;
        if (h - last_h).abs() > 0.5 {
            ui.ctx().request_repaint();
        }
        ui.data_mut(|d| d.insert_temp(id, h));
    });
}

/// The tabs, in ⌘1–⌘4 order.
const TABS: [(Screen, &str); 4] = [(Screen::Issues, "Issues"), (Screen::Grooming, "Grooming"), (Screen::Team, "Team"), (Screen::Roadmap, "Roadmap")];

/// Every tab label is regular 14 (the underline marks the active one), so a tab's slot is its label's width
/// whatever its state and switching tabs never moves anything.
pub fn tab_width(label_w: f32) -> f32 {
    label_w.ceil()
}

/// The header rule's centre line: the first point under the bar, so the bar keeps its full 48.
pub fn header_rule_y(bar_bottom: f32) -> f32 {
    bar_bottom + 0.5
}

/// The active tab's 2.0 underline, 7 below the tab slot, on whole points.
pub fn underline_rect(tab: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(tab.left(), (tab.bottom() + 6.0).round()), vec2(tab.width(), 2.0))
}

/// Gap between tabs.
const TAB_GAP: f32 = 24.0;
/// Gap from the title to the first tab: TAB_GAP (round-6 ruling); the title's weight sets it apart.
const TITLE_GAP: f32 = 24.0;
/// Gap between the signed-in name and "Read-only": a muted "·" centred in it, 8 either side.
const NAME_GAP: f32 = 20.0;
/// The signed-in name's slot, in "m" glyphs of the 12 px name font: reserved before the user record loads.
const NAME_SLOT_M: f32 = 20.0;

/// The name's slot width: a fixed run of "m" glyphs, never the name itself, so the header's buttons sit at
/// the same x before and after the user record arrives (round-6 D2). Longer names are cut to it.
pub fn name_slot(m_w: f32) -> f32 {
    (m_w * NAME_SLOT_M).ceil()
}

/// The right-hand label cluster: its width (name slot, then "Read-only" when shown; the buttons sit left of
/// it) and the shown name's left edge from the cluster's left, right-aligned in its slot. The width ignores
/// the name, so it is the same with `name_w` None (pending) and Some.
pub fn header_labels(slot: f32, name_w: Option<f32>, read_only_w: Option<f32>) -> (f32, Option<f32>) {
    let w = (slot + read_only_w.map_or(0.0, |w| w + NAME_GAP)).ceil();
    (w, name_w.map(|n| (slot - n.min(slot)).max(0.0)))
}

/// The y of a galley's first baseline, from its top.
pub fn galley_baseline(g: &egui::Galley) -> f32 {
    baseline(g)
}

fn baseline(g: &egui::Galley) -> f32 {
    g.rows.first().map_or(0.0, |r| r.pos.y + r.row.glyphs.first().map_or(0.0, |gl| gl.pos.y))
}

/// The top at which to paint a galley so its baseline lands on `line_baseline`.
pub fn top_for_baseline(line_baseline: f32, galley_baseline: f32) -> f32 {
    line_baseline - galley_baseline
}

/// The header spinner is for syncing the refresh lists; a search shows its own spinner in the count row.
pub fn header_syncing<'a>(lists: impl IntoIterator<Item = (&'a crate::store::ListId, &'a crate::app::Loaded)>) -> bool {
    lists.into_iter().any(|(id, l)| *id != crate::store::ListId::Search && l.loading)
}

/// One spinner per operation: (header, count row). A first load (nothing on screen yet, whatever started it)
/// is the count row's: "⟳ Loading issues…", and the header stays still. Otherwise a header refresh (⌘R, and
/// the sync after a write) spins the header only, including the quick-filter search it reruns; a search
/// started from the filters or the search box spins the count row only.
pub fn spinner_slots(lists_syncing: bool, search_pending: bool, search_from_refresh: bool, first_load: bool) -> (bool, bool) {
    if first_load {
        return (false, true);
    }
    (lists_syncing || (search_pending && search_from_refresh), search_pending && !search_from_refresh)
}

fn header(app: &mut App, ui: &mut Ui, p: &Palette) -> egui::Rect {
    let panel = egui::Panel::top("header").exact_size(48.0).frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(16, 0))).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = TAB_GAP;
            // The title and the tabs share the tabs' baseline (the 15 bold title sat 1 px below it).
            let tab_probe = ui.painter().layout_no_wrap("Issues".into(), theme::body(14.0), p.text);
            let title = ui.painter().layout_no_wrap("Jira Tickets".into(), theme::bold(15.0), p.text);
            let (title_rect, _) = ui.allocate_exact_size(vec2(title.size().x.ceil(), 32.0), Sense::hover());
            let line = (title_rect.center().y - tab_probe.size().y / 2.0).round() + baseline(&tab_probe);
            ui.painter().galley(egui::pos2(title_rect.left(), top_for_baseline(line, baseline(&title))), title, p.text);
            ui.add_space(TITLE_GAP - TAB_GAP);
            for (i, (s, name)) in TABS.into_iter().enumerate() {
                let active = app.screen == s;
                let galley = ui.painter().layout_no_wrap(name.to_string(), theme::body(14.0), if active { p.text } else { p.muted });
                let w = tab_width(galley.size().x);
                let (rect, r) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
                // Left-aligned in its slot, so every gap is exactly TAB_GAP.
                let pos = egui::pos2(rect.left(), top_for_baseline(line, baseline(&galley)));
                ui.painter().galley(pos, galley, p.text);
                if active {
                    // A filled 2.0 bar on whole points, not a stroke centred on a line, so it is exactly 2 tall.
                    ui.painter().rect_filled(underline_rect(rect), 0.0, p.accent);
                }
                let r = r.on_hover_text(format!("{name} (⌘{})", i + 1));
                if r.clicked() {
                    app.actions.push(Action::SetScreen(s));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                // The name and "Read-only", both small 12 muted so they never compete with the active tab and read
                // as two labels, share one baseline, NAME_GAP apart.
                let read_only = crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed);
                let painter = ui.painter().clone();
                let m_w = painter.layout_no_wrap("m".into(), theme::body(12.0), p.muted).size().x;
                let slot = name_slot(m_w);
                // The name is cut to its slot and right-aligned in it, beside "Read-only"; the slot is reserved
                // before the user record loads, so the buttons to its left never move.
                let name = app.me.as_ref().map(|me| {
                    let shown = truncate_text(&me.display_name, slot, |t| painter.layout_no_wrap(t.to_string(), theme::body(12.0), p.muted).size().x);
                    (painter.layout_no_wrap(shown, theme::body(12.0), p.muted), me.display_name.clone())
                });
                let ro = read_only.then(|| painter.layout_no_wrap("Read-only".into(), theme::body(12.0), p.muted));
                let (cluster_w, name_left) = header_labels(slot, name.as_ref().map(|(g, _)| g.size().x), ro.as_ref().map(|g| g.size().x));
                let (rect, _) = ui.allocate_exact_size(vec2(cluster_w, 32.0), Sense::hover());
                // One baseline for both labels, from a 12 px line box centred in the slot.
                let probe = painter.layout_no_wrap("Read-only".into(), theme::body(12.0), p.muted);
                let line = (rect.center().y - probe.size().y / 2.0).round() + baseline(&probe);
                if let (Some((g, full)), Some(dx)) = (name, name_left) {
                    let top = top_for_baseline(line, baseline(&g));
                    let r = egui::Rect::from_min_size(egui::pos2(rect.left() + dx, top), g.size());
                    ui.painter().galley(r.min, g, p.muted);
                    let resp = ui.interact(r, egui::Id::new("header-name"), Sense::click());
                    resp.on_hover_text(format!("{full} · right-click to sign out")).context_menu(|ui| {
                        if ui.button("Sign out").clicked() {
                            app.actions.push(Action::SignOut);
                        }
                    });
                }
                if let (Some(g), true) = (&ro, name_left.is_some()) {
                    // A muted "·" between the name and "Read-only", so they read as two labels.
                    let dot = painter.layout_no_wrap("·".into(), theme::body(12.0), p.muted);
                    let x = rect.right() - g.size().x - NAME_GAP / 2.0 - dot.size().x / 2.0;
                    ui.painter().galley(egui::pos2(x, top_for_baseline(line, baseline(&dot))), dot, p.muted);
                }
                if let Some(g) = ro {
                    let top = top_for_baseline(line, baseline(&g));
                    let r = egui::Rect::from_min_size(egui::pos2(rect.right() - g.size().x, top), g.size());
                    ui.painter().galley(r.min, g, p.muted);
                    ui.interact(r, egui::Id::new("header-read-only"), Sense::hover()).on_hover_text("Writes are blocked (JIRA_TICKETS_READONLY)");
                }
                // One fixed 32×32 slot: the spinner replaces the button at the same size.
                let first_load = app.screen == Screen::Issues && issues::first_load(app);
                if spinner_slots(header_syncing(&app.lists), app.search_pending.is_some(), app.search_from_refresh, first_load).0 {
                    icons::spinner_button(ui, p);
                } else if icons::icon_button(ui, icons::Icon::Sync, "Refresh (⌘R)", p).clicked() {
                    app.actions.push(Action::Refresh);
                }
                if icons::text_button(ui, "?", "Keyboard help (?)", p).clicked() {
                    app.actions.push(Action::ToggleHelp);
                }
            });
        });
    });
    panel.response.rect
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

/// The sign-in panel's first line: the token error when there is one (it replaces the prompt), else the prompt.
pub fn sign_in_lead(token_error: Option<&str>) -> (&str, bool) {
    match token_error {
        Some(e) => (e, true),
        None => ("Paste an Atlassian API token to connect.", false),
    }
}

/// The sign-in aside: prose in muted 12 and the identifiers in mono 12.5, on one line.
fn sign_in_aside(p: &Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let prose = egui::TextFormat { font_id: theme::body(12.0), color: p.muted, ..Default::default() };
    let code = egui::TextFormat { font_id: theme::mono(12.5), color: p.muted, ..Default::default() };
    for (i, part) in SIGN_IN_ASIDE.iter().enumerate() {
        job.append(part, 0.0, if i % 2 == 1 { code.clone() } else { prose.clone() });
    }
    job
}

/// Alternating prose and identifiers.
pub const SIGN_IN_ASIDE: [&str; 7] = ["Or set ", "JIRA_API_TOKEN", " or ", "api_token", " in ", "config.toml", ", then check again."];

/// Sign in → "Check again": the 8 the text button's own 28 px box used to add, now explicit at 12.
const SIGN_IN_TO_AGAIN: f32 = 12.0;

/// The sign-in aside → token field, in code (see the comment at its use).
const ASIDE_TO_FIELD: f32 = 15.0;

/// The sign-in panel's content width: 380, or the aside's one-line width when wider.
pub fn sign_in_width(aside_w: f32) -> f32 {
    aside_w.ceil().max(380.0)
}

pub fn sign_in(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    if app.finding_token {
        centred(ui, "finding-token", |ui| {
            icons::spinner(ui, icons::INLINE_SPINNER, p);
            ui.add_space(8.0);
            ui.label(RichText::new("Looking for your Jira token…").color(p.muted));
        });
        return;
    }
    let aside = sign_in_aside(p);
    let aside_w = ui.fonts_mut(|f| f.layout_job(aside.clone())).size().x;
    centred(ui, "sign-in", |ui| {
        Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::same(16)).show(ui, |ui| {
            let w = sign_in_width(aside_w);
            ui.set_width(w);
            ui.vertical(|ui| {
                // Every gap is explicit: 16 title → body, 8 body → aside, 16 aside → field, 12 → button, 8 → Check again.
                // The aside's mono runs make its line box ~1 px taller than its prose, so 15 reads as 16.
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("Sign in to Jira").font(theme::bold(22.0)).color(p.text));
                ui.add_space(16.0);
                let (lead, is_error) = sign_in_lead(app.token_error.as_deref());
                ui.label(RichText::new(lead).size(14.0).color(if is_error { p.off_track } else { p.text }));
                // A login means Jira rejected it (401): show what was sent.
                if let Some(login) = &app.login {
                    ui.add_space(8.0);
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
                    ui.add_space(8.0);
                    if ui.add(button("Sign out", p)).on_hover_text("Forget the saved token").clicked() {
                        app.actions.push(Action::SignOut);
                    }
                }
                ui.add_space(8.0);
                ui.add(egui::Label::new(aside).extend());
                ui.add_space(ASIDE_TO_FIELD);
                ui.visuals_mut().text_edit_bg_color = Some(theme::input_fill(p.surface, p));
                let r = ui.add(input_field(&mut app.token_input, "API token", theme::body(14.0), p).id(egui::Id::new("token-input")).password(true));
                if ui.memory(|m| m.focused().is_none()) {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.actions.push(Action::SubmitToken);
                }
                ui.add_space(12.0);
                // Full width and the field's height: the two stack as one unit.
                if ui.add_sized([w, INPUT_H], primary_button("Sign in", p)).clicked() {
                    app.actions.push(Action::SubmitToken);
                }
                ui.add_space(SIGN_IN_TO_AGAIN);
                // Only as tall as its text, so the panel's 16 bottom padding is all that shows under it.
                let again = egui::Button::new(RichText::new("Check again").color(p.muted).size(14.0)).frame(false);
                let again_h = ui.fonts_mut(|f| f.row_height(&theme::body(14.0)));
                ui.spacing_mut().button_padding.y = 0.0;
                if ui.add_sized([w, again_h], again).on_hover_text("Look for JIRA_API_TOKEN or the config api_token again").clicked() {
                    app.actions.push(Action::FindToken);
                }
            });
        });
    });
}

/// The hint bar's height when it shows (signed in, on any screen), else None.
pub fn hint_bar_height(app: &App) -> Option<f32> {
    app.client.is_some().then_some(issues::HINT_BAR_H)
}

/// The toast's edge: none in light (the shadow separates the dark fill from the page; a `border` rim read as a
/// halo); in dark a warm step lighter than the fill so it separates from the near-black page.
pub fn toast_stroke(p: &Palette) -> Stroke {
    if p.dark { Stroke::new(1.0, Color32::from_rgb(0x63, 0x62, 0x5b)) } else { Stroke::NONE }
}

/// How far the toast's bottom sits above the window bottom: 16 px above the hint bar when one shows, else 24 px.
pub fn toast_offset(hint_bar: Option<f32>) -> f32 {
    hint_bar.map_or(24.0, |h| h + 16.0)
}

/// The toast's dot marks its type only: green for done, red for failures, none for a notice (read-only
/// refusals, "No changes"). The toast is a dark fill in both themes, so the dot takes the dark palette's tokens.
/// The failure dot: off_track lifted a step, so it holds 3:1 on the dark toast fill (#57564f).
const TOAST_ERROR_DOT: Color32 = Color32::from_rgb(0xf4, 0x8a, 0x83);

pub fn toast_dot(kind: crate::app::ToastKind) -> Option<Color32> {
    match kind {
        crate::app::ToastKind::Success => Some(theme::DARK.on_track),
        crate::app::ToastKind::Info => None,
        crate::app::ToastKind::Error => Some(TOAST_ERROR_DOT),
    }
}

/// The toast's centre x: the detail pane's centre when one shows (so it never covers the list's last row),
/// else the window's. The toast is pinned by its own bottom centre, so it is exact.
pub fn toast_centre_x(window_w: f32, detail: Option<(f32, f32)>) -> f32 {
    detail.map_or(window_w / 2.0, |(l, r)| (l + r) / 2.0)
}

/// The toast's padding: 16 either side; 7 above and 9 below, because the 14 px line box carries its leading
/// under the glyphs, so 8/8 set the text ~2 pt low.
const TOAST_MARGIN: Margin = Margin { left: 16, right: 16, top: 7, bottom: 9 };

/// The toast's shadow: heavier in dark, where a light shadow vanishes on the near-black page.
pub fn toast_shadow(p: &Palette) -> egui::Shadow {
    if p.dark {
        egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(200) }
    } else {
        egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(60) }
    }
}

pub fn toast(app: &mut App, ctx: &egui::Context) {
    let Some(t) = &app.toast else { return };
    if std::time::Instant::now() > t.until {
        app.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let p = theme::palette(ctx);
    let shadow = toast_shadow(p);
    let window = ctx.content_rect();
    let x = toast_centre_x(window.width(), app.detail_x.filter(|_| app.screen == Screen::Issues));
    let at = egui::pos2(x, window.bottom() - toast_offset(hint_bar_height(app)));
    egui::Area::new(egui::Id::new("toast")).pivot(egui::Align2::CENTER_BOTTOM).fixed_pos(at).show(ctx, |ui| {
        Frame::new().fill(p.tooltip).stroke(toast_stroke(p)).shadow(shadow).corner_radius(theme::RADIUS).inner_margin(TOAST_MARGIN).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if let Some(color) = toast_dot(t.kind) {
                    health_dot(ui, color);
                }
                ui.add(egui::Label::new(RichText::new(&t.text).color(p.tooltip_text).size(14.0)).wrap_mode(egui::TextWrapMode::Extend));
            });
        });
    });
}

// ---------- Shared widgets ----------

/// Height of every text button (and the filter combos), so rows of controls line up.
pub const BUTTON_H: f32 = 28.0;

/// The page dimming behind a modal: light needs much less to read as "behind" (100 dimmed it 2.78:1).
pub fn backdrop_alpha(p: &Palette) -> u8 {
    if p.dark { 110 } else { 64 }
}

/// A dialog's cursor row: `selected_row` in light; #4a4a47 in dark, where selected_row sat 1.16:1 off the
/// #333331 dialog.
pub fn dialog_cursor_fill(p: &Palette) -> Color32 {
    if p.dark { theme::DARK_DIALOG_CURSOR } else { p.selected_row }
}

/// A modal dialog's one edge colour, used for its 1 px stroke and for the fields and buttons inside it, in both
/// themes: a step darker than `border` in light (`border` sits between white and the dimmed page and
/// vanishes), the lifted #504f4b in dark (1.54:1 on the #333331 dialog).
pub fn dialog_stroke(p: &Palette) -> Stroke {
    Stroke::new(1.0, if p.dark { DARK_EDGE } else { Color32::from_rgb(0xb4, 0xb3, 0xac) })
}

pub use crate::theme::DARK_EDGE;

/// Outline pills on a selected row: #5c5b56 in dark (`border` equals the selected fill there, and the dialog
/// edge only reaches 1.22:1), `muted` at 40% in light, so the outline never vanishes into the fill.
pub fn selected_edge(p: &Palette) -> Color32 {
    if p.dark { Color32::from_rgb(0x5c, 0x5b, 0x56) } else { p.muted.gamma_multiply(0.4) }
}

/// Inside a dialog: fields one step under the dialog's fill (`theme::input_fill`, the app's one input rule)
/// with the dialog's edge (focus stays the accent).
pub fn dialog_fields(ui: &mut Ui, p: &Palette) {
    let edge = dialog_stroke(p).color;
    let v = ui.visuals_mut();
    v.text_edit_bg_color = Some(theme::input_fill(dialog_fill(p), p));
    v.widgets.inactive.bg_stroke.color = edge;
    v.widgets.hovered.bg_stroke.color = edge;
}

/// A secondary button inside a dialog: a step above the dialog's fill, with the dialog's edge.
pub fn dialog_button(text: &str, p: &Palette) -> egui::Button<'static> {
    button(text, p).fill(dialog_button_fill(p)).stroke(dialog_stroke(p))
}

/// The dialog button's fill: `surface_2` on the white light dialog; in dark the next step up (#3d3d3a).
pub fn dialog_button_fill(p: &Palette) -> Color32 {
    if p.dark { p.selected_row } else { p.surface_2 }
}

/// Placeholder text for a field, muted, starting exactly where typed text does (the first keystroke moves
/// nothing).
pub fn field_hint(text: &str, font: egui::FontId, p: &Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat { font_id: font, color: theme::placeholder(p), ..Default::default() });
    job
}

/// An input's text (and placeholder) inset: 12, the filter row's one text inset.
pub const INPUT_PAD_X: f32 = 12.0;

/// Height of every single-line input (search, go-to, token, picker fields): BUTTON_H, like every control.
pub const INPUT_H: f32 = BUTTON_H;

/// The one single-line input: 12/4 inner margin (typed text and placeholder both start 12 in, like combo
/// and chip text), INPUT_H tall, full width, `font` for text and placeholder. Focus draws the theme's one
/// 1 px accent stroke.
pub fn input_field<'a>(text: &'a mut String, hint: &str, font: egui::FontId, p: &Palette) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .font(font.clone())
        .hint_text(field_hint(hint, font, p))
        .desired_width(f32::INFINITY)
        .margin(vec2(INPUT_PAD_X, 4.0))
        .min_size(vec2(0.0, INPUT_H))
        // The text row (and caret, and placeholder) centred in the 32 px box, not riding at the top margin.
        .vertical_align(egui::Align::Center)
}

/// A modal dialog's fill: plain surface in light; in dark #333331, a step past surface_2, so the dialog lifts
/// off the dimmed page about as far as the light one does (the backdrop stays at 110).
pub fn dialog_fill(p: &Palette) -> Color32 {
    if p.dark { theme::DARK_DIALOG } else { p.surface }
}

/// The dimmed area: the window, less the hint bar at the bottom (it carries the dialog's keys, so it stays lit).
pub fn backdrop_rect(window: egui::Rect, hint_bar: Option<f32>) -> egui::Rect {
    let mut r = window;
    r.max.y -= hint_bar.unwrap_or(0.0);
    r
}

/// Paints the modal backdrop over the window above the hint bar and swallows its clicks.
pub fn backdrop(ctx: &egui::Context, id: &str, order: egui::Order, hint_bar: Option<f32>, p: &Palette) -> egui::Response {
    egui::Area::new(egui::Id::new(id))
        .order(order)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(ctx, |ui| {
            let r = backdrop_rect(ui.ctx().content_rect(), hint_bar);
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(backdrop_alpha(p)));
            ui.allocate_rect(r, Sense::click())
        })
        .inner
}

pub fn button(text: &str, p: &Palette) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(p.text).size(14.0))
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::RADIUS)
        .min_size(vec2(0.0, BUTTON_H))
}

/// A filter chip's inner padding on each side.
pub const CHIP_PAD: f32 = 12.0;

/// A chip is its label plus 12 either side in both states (the accent fill marks it on), so toggling never
/// moves its neighbours.
pub fn chip_width(label_w: f32) -> f32 {
    (2.0 * CHIP_PAD + label_w).ceil()
}

/// A config name in sentence case for a chip: words after the first are lowercased unless they are
/// acronyms (two or more capitals, e.g. "PLAT", "JQL"). "Only Opened" → "Only opened", "To Do" → "To do".
pub fn sentence_case(name: &str) -> String {
    name.split(' ')
        .enumerate()
        .map(|(i, w)| if i == 0 || w.chars().filter(|c| c.is_uppercase()).count() > 1 { w.to_string() } else { w.to_lowercase() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// An active toggle chip's fill: the one accent, in both themes (on_accent reads on it).
pub fn active_chip_fill(p: &Palette) -> Color32 {
    p.accent
}

/// A filter toggle chip: 28 px tall (BUTTON_H, like the combos), radius 6 like every button, label 12 in.
pub fn toggle_chip(ui: &mut Ui, text: &str, active: bool, p: &Palette) -> egui::Response {
    let fg = if active { p.on_accent } else { p.text };
    let label = ui.painter().layout_no_wrap(text.to_string(), theme::body(14.0), fg);
    let w = chip_width(label.size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, BUTTON_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, text));
    if ui.is_rect_visible(rect) {
        let (fill, stroke) = match (active, resp.hovered()) {
            (true, _) => (active_chip_fill(p), active_chip_fill(p)),
            (false, true) => (theme::control_hover(p), p.border),
            (false, false) => (p.surface_2, p.border),
        };
        ui.painter().rect(rect, f32::from(theme::RADIUS), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
        let y = (rect.center().y - label.size().y / 2.0).round();
        ui.painter().galley(egui::pos2(rect.left() + CHIP_PAD, y), label, fg);
    }
    resp
}

pub fn primary_button(text: &str, p: &Palette) -> egui::Button<'static> {
    primary_button_if(text, true, p)
}

/// The enabled primary button's fill and text: accent with `on_accent`, in both themes.
pub fn primary_colors(p: &Palette) -> (Color32, Color32) {
    (p.accent, p.on_accent)
}

/// A 12 px accent text link (no frame), underlined while hovered: "Clear filters", "Clear search".
pub fn small_link(ui: &mut Ui, text: &str, p: &Palette) -> egui::Response {
    text_link(ui, text, theme::body(12.0), p.accent)
}

/// A text link (no frame) in `color`, underlined while hovered.
pub fn text_link(ui: &mut Ui, text: &str, font: egui::FontId, color: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0, color));
        }
        ui.painter().galley(rect.min, galley, color);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A primary button that, when disabled, is drawn quiet (surface_2, muted, border) and ignores clicks,
/// rather than a faded accent.
pub fn primary_button_if(text: &str, enabled: bool, p: &Palette) -> egui::Button<'static> {
    let b = if enabled {
        let (fill, fg) = primary_colors(p);
        egui::Button::new(RichText::new(text.to_string()).color(fg).size(14.0)).fill(fill).stroke(Stroke::new(1.0, fill))
    } else {
        egui::Button::new(RichText::new(text.to_string()).color(p.muted).size(14.0)).fill(p.surface_2).stroke(Stroke::new(1.0, p.border)).sense(Sense::hover())
    };
    b.corner_radius(theme::RADIUS).min_size(vec2(0.0, BUTTON_H))
}

/// Pill inner padding (8/2) and radius (10).
pub const PILL_MARGIN: egui::Vec2 = vec2(8.0, 2.0);

pub fn pill(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    pill_sized(ui, text, bg, fg, Stroke::NONE, PILL_MARGIN)
}

/// A neutral pill (type, priority, labels, team, assignee): outline only, 1 px `border`, `muted` text, so
/// only the status (and epic) pills carry a filled tint.
pub fn outline_pill(ui: &mut Ui, text: &str, p: &Palette, margin: egui::Vec2) -> egui::Response {
    pill_sized(ui, text, Color32::TRANSPARENT, p.muted, Stroke::new(1.0, p.border), margin)
}

/// An outline pill on a selected row: the `selected_edge` outline so it never sinks into the fill; its text
/// (assignee, labels) is `text`, like the row's key, since `muted` only reaches ~4.6:1 on the selection.
pub fn outline_pill_selected(ui: &mut Ui, text: &str, p: &Palette, margin: egui::Vec2) -> egui::Response {
    pill_sized(ui, text, Color32::TRANSPARENT, p.text, Stroke::new(1.0, selected_edge(p)), margin)
}

/// The edge every list-row pill gets: its own text colour at 35%, so a wash stays visible on any row fill.
pub fn row_pill_stroke(fg: Color32) -> Stroke {
    Stroke::new(1.0, fg.gamma_multiply(0.35))
}

/// List-row pills: the one pill padding, 8/2.
pub const ROW_PILL_MARGIN: egui::Vec2 = PILL_MARGIN;

/// A list-row pill: one treatment on every row (plain, hovered, selected).
pub fn pill_outlined(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    pill_sized(ui, text, bg, fg, row_pill_stroke(fg), ROW_PILL_MARGIN)
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

/// `text` cut to fit `max_w` (as `measure` lays it out) with "…"; trailing whitespace is trimmed before the
/// ellipsis. Unchanged when it already fits.
pub fn truncate_text(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_w {
        return text.to_string();
    }
    let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).skip(1).chain(std::iter::once(text.len())).collect();
    let cut = |n: usize| format!("{}…", text[..if n == 0 { 0 } else { ends[n - 1] }].trim_end());
    // The longest prefix (in chars) whose cut form fits; widths grow with length.
    let (mut lo, mut hi) = (0, ends.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if measure(&cut(mid)) <= max_w { lo = mid } else { hi = mid - 1 }
    }
    cut(lo)
}

pub fn health_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// Returns true when Retry is clicked.
pub fn error_banner(ui: &mut Ui, p: &Palette, title: &str, raw: &str) -> bool {
    let mut retry = false;
    Frame::new().fill(p.danger_subtle).corner_radius(theme::RADIUS).inner_margin(Margin::same(12)).show(ui, |ui| {
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
    fn chips_pad_12_either_side() {
        assert_eq!(chip_width(40.0), 64.0);
        assert_eq!(chip_width(40.2), 65.0);
        assert_eq!(INPUT_PAD_X, CHIP_PAD, "typed text and placeholder start where chip text does");
        assert_eq!(INPUT_H, BUTTON_H, "one control height");
    }

    #[test]
    fn chip_names_are_sentence_case() {
        assert_eq!(sentence_case("Only Opened"), "Only opened");
        assert_eq!(sentence_case("To Do"), "To do");
        assert_eq!(sentence_case("Mine"), "Mine");
        assert_eq!(sentence_case("Open PLAT Work"), "Open PLAT work", "acronyms stay");
        assert_eq!(sentence_case("already sentence"), "already sentence");
    }

    #[test]
    fn truncation_trims_before_the_ellipsis() {
        let w = |t: &str| t.chars().count() as f32;
        assert_eq!(truncate_text("short", 10.0, w), "short");
        assert_eq!(truncate_text("hello world again", 7.0, w), "hello…", "no space before …");
        assert_eq!(truncate_text("abcdefgh", 5.0, w), "abcd…");
        assert_eq!(truncate_text("abc", 0.0, w), "…");
        assert_eq!(truncate_text("ééééé", 3.0, w), "éé…", "char boundaries");
    }

    #[test]
    fn one_centring_rule() {
        assert_eq!(centre_top(800.0, 300.0), 250.0);
        assert_eq!(centre_top(100.0, 300.0), 0.0, "never negative");
        assert_eq!(centre_top(801.0, 300.0), 251.0, "whole pixels");
    }

    #[test]
    fn sign_in_aside_reads_as_one_sentence() {
        assert_eq!(SIGN_IN_ASIDE.concat(), "Or set JIRA_API_TOKEN or api_token in config.toml, then check again.");
        assert_eq!(sign_in_width(300.0), 380.0);
        assert_eq!(sign_in_width(452.3), 453.0, "the aside fits on one line");
    }

    #[test]
    fn one_empty_state_size() {
        assert_eq!(EMPTY_STATE_SIZE, 14.0);
    }

    #[test]
    fn placeholders_name_the_tab() {
        assert_eq!(placeholder_text("Grooming"), "Grooming is coming soon");
    }

    #[test]
    fn sign_in_error_replaces_the_prompt() {
        assert_eq!(sign_in_lead(None), ("Paste an Atlassian API token to connect.", false));
        assert_eq!(sign_in_lead(Some("op read failed")), ("op read failed", true));
    }

    #[test]
    fn tab_underline_is_two_points_on_whole_points() {
        let r = underline_rect(egui::Rect::from_min_max(egui::pos2(10.0, 8.3), egui::pos2(60.0, 40.3)));
        assert_eq!(r.height(), 2.0);
        assert_eq!(r.top(), r.top().round());
        assert_eq!(r.width(), 50.0);
        // In the 48 bar the 32 tab slot ends at 40: the underline fills 46..48 and the rule draws 48..49, so
        // none of the underline is covered (round 6 measured 1 lp when the rule sat at 47..48).
        let u = underline_rect(egui::Rect::from_min_max(egui::pos2(0.0, 8.0), egui::pos2(50.0, 40.0)));
        assert_eq!((u.top(), u.bottom()), (46.0, 48.0));
        assert!(header_rule_y(48.0) - 0.5 >= u.bottom());
        assert_eq!(TITLE_GAP, TAB_GAP);
    }

    #[test]
    fn tabs_take_their_label_width() {
        assert_eq!(tab_width(52.3), 53.0);
        assert_eq!(tab_width(41.0), 41.0);
    }

    #[test]
    fn small_label_sits_on_the_names_baseline() {
        // A 14 px name with its baseline 13 down, drawn at top 10: a 12 px label (baseline 11 down) goes at 12.
        assert_eq!(top_for_baseline(10.0 + 13.0, 11.0), 12.0);
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
    fn primary_text_is_on_accent_in_both_themes() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert_eq!(primary_colors(p), (p.accent, p.on_accent));
            assert!(theme::contrast(p.on_accent, active_chip_fill(p)) >= 4.5, "dark={}", p.dark);
            assert_eq!(active_chip_fill(p), p.accent, "one accent");
        }
    }

    #[test]
    fn backdrop_stops_above_the_hint_bar() {
        let w = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1440.0, 900.0));
        assert_eq!(backdrop_rect(w, Some(24.0)).max.y, 876.0);
        assert_eq!(backdrop_rect(w, None), w);
    }

    #[test]
    fn row_pill_edge_is_the_text_colour_faded() {
        let s = row_pill_stroke(Color32::from_rgb(200, 100, 0));
        assert_eq!(s.width, 1.0);
        assert!(s.color.a() > 80 && s.color.a() < 100, "{}", s.color.a());
    }

    #[test]
    fn dialog_edge_shows_against_dialog_and_dimmed_page() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let edge = dialog_stroke(p).color;
            let dimmed = theme::over(Color32::from_black_alpha(backdrop_alpha(p)), p.bg);
            assert!(theme::contrast(edge, dialog_fill(p)) >= 1.5, "dark={}", p.dark);
            // What separates the dialog from the dimmed page: its edge in dark; in light the white fill itself
            // (the lighter 64 backdrop leaves the edge close to the page, 1.06:1, but the fill stands ~2:1).
            let lift = if p.dark { theme::contrast(edge, dimmed) } else { theme::contrast(dialog_fill(p), dimmed) };
            assert!(lift >= 1.3, "dark={}: {lift:.2}", p.dark);
        }
    }

    #[test]
    fn backdrop_is_heavier_in_dark() {
        assert!(backdrop_alpha(&theme::DARK) > backdrop_alpha(&theme::LIGHT));
        assert_eq!(dialog_fill(&theme::DARK), theme::DARK_DIALOG);
        assert_eq!(dialog_fill(&theme::LIGHT), theme::LIGHT.surface);
    }

    #[test]
    fn header_buttons_do_not_move_when_the_name_arrives() {
        let slot = name_slot(10.0);
        assert_eq!(slot, 200.0, "20 m glyphs");
        for ro in [None, Some(52.0)] {
            // The buttons sit left of the cluster: its width is the same before (None) and after (Some) the
            // user record lands, whatever the name's length.
            let (pending, none_left) = header_labels(slot, None, ro);
            assert_eq!(none_left, None);
            for name_w in [40.0, 120.0, 200.0] {
                let (w, left) = header_labels(slot, Some(name_w), ro);
                assert_eq!(w, pending, "ro={ro:?} name_w={name_w}");
                assert_eq!(left, Some(slot - name_w), "right-aligned in its slot, beside Read-only");
            }
        }
        // A long name is cut to the slot, not the slot to the name.
        let measure = |t: &str| t.chars().count() as f32 * 7.0;
        let cut = truncate_text("Bartholomew Featherstonehaugh-Smythe the Third", slot, measure);
        assert!(measure(&cut) <= slot, "{cut}");
    }

    #[test]
    fn toast_dot_marks_the_type_only() {
        use crate::app::ToastKind;
        assert_eq!(toast_dot(ToastKind::Error), Some(TOAST_ERROR_DOT));
        assert_eq!(toast_dot(ToastKind::Success), Some(theme::DARK.on_track));
        assert_eq!(toast_dot(ToastKind::Info), None, "no decorative dot on a notice");
        for p in [&theme::LIGHT, &theme::DARK] {
            for k in [ToastKind::Error, ToastKind::Success] {
                let c = toast_dot(k).unwrap();
                assert!(theme::contrast(c, p.tooltip) >= 3.0, "dark={} {k:?}: {:.2}", p.dark, theme::contrast(c, p.tooltip));
            }
        }
        let warm = theme::DARK.tooltip;
        assert!(warm.r() >= warm.b(), "the dark toast fill is a warm neutral");
    }

    #[test]
    fn selected_row_edges_show_on_the_selected_fill() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let edge = theme::over(selected_edge(p), p.selected_row);
            assert!(theme::contrast(edge, p.selected_row) >= if p.dark { 1.5 } else { 1.2 }, "dark={}: {:.2}", p.dark, theme::contrast(edge, p.selected_row));
        }
    }

    #[test]
    fn toast_centres_over_the_detail_pane() {
        assert_eq!(toast_centre_x(1440.0, None), 720.0);
        assert_eq!(toast_centre_x(1440.0, Some((604.0, 1440.0))), 1022.0);
        assert!(toast_shadow(&theme::DARK).color.a() > toast_shadow(&theme::LIGHT).color.a());
        assert_eq!(TOAST_MARGIN.left, TOAST_MARGIN.right);
    }

    #[test]
    fn dialog_parts_share_one_edge() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let edge = dialog_stroke(p).color;
            assert!(theme::contrast(edge, p.bg) >= 1.3, "field edge on its bg fill, dark={}", p.dark);
            assert!(theme::contrast(dialog_button_fill(p), dialog_fill(p)) >= 1.05, "Cancel steps off the dialog, dark={}", p.dark);
            assert!(theme::contrast(edge, dialog_button_fill(p)) >= 1.2, "Cancel's edge shows, dark={}", p.dark);
            assert!(theme::contrast(p.text, dialog_button_fill(p)) >= 7.0);
        }
    }

    #[test]
    fn one_spinner_per_operation() {
        assert_eq!(spinner_slots(true, false, false, false), (true, false), "⌘R: header");
        assert_eq!(spinner_slots(true, true, true, false), (true, false), "⌘R with a quick filter: header only");
        assert_eq!(spinner_slots(false, true, false, false), (false, true), "filter or JQL: count row only");
        assert_eq!(spinner_slots(false, false, false, false), (false, false));
        assert_eq!(spinner_slots(true, true, false, true), (false, true), "filtered first load: the count row only");
        assert_eq!(spinner_slots(true, false, false, true), (false, true), "first load: the count row only");
    }

    #[test]
    fn toast_sits_above_the_hint_bar_when_there_is_one() {
        assert_eq!(toast_offset(Some(24.0)), 40.0);
        assert_eq!(toast_offset(None), 24.0);
    }
}
