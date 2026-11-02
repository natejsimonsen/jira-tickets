//! The Team screen: one column per configured member (`teamview::columns`), each with its in-progress then ready
//! cards and a "Done in the last 7 days" footer, then the Unassigned column with the top of the ready queue.

use crate::app::{Action, App};
use crate::jira::Issue;
use crate::teamview::{self, Card, Column};
use crate::theme::{self, Palette};
use crate::views::{PILL_MARGIN, pill_sized};
use egui::{Color32, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};

/// A column is never narrower than this; past that the columns scroll sideways.
pub const COL_MIN: f32 = 260.0;
/// Between columns, and under a column's header.
const COL_GAP: f32 = 16.0;
/// Between a card's pills.
const PILL_GAP: f32 = 8.0;

/// Equal column widths across `avail` with COL_GAP between, never under COL_MIN.
pub fn column_width(avail: f32, n: usize) -> f32 {
    let n = n.max(1) as f32;
    ((avail - COL_GAP * (n - 1.0)) / n).floor().max(COL_MIN)
}

/// A member column's counts: "3 in progress · 2 ready".
pub fn counts_line(in_progress: usize, ready: usize) -> String {
    format!("{in_progress} in progress · {ready} ready")
}

/// The footer's toggle, its arrow showing whether it is open.
pub fn footer_label(open: bool) -> &'static str {
    if open { "Done in the last 7 days ▾" } else { "Done in the last 7 days ▸" }
}

/// A card's days-in-status text: "4 d".
pub fn days_text(days: i64) -> String {
    format!("{days} d")
}

/// The days label's tooltip. The count runs from Jira's `statuscategorychangedate`, so it says what that measures:
/// a move between two statuses of the same category does not reset it.
pub fn days_tip(days: i64) -> String {
    let unit = if days == 1 { "day" } else { "days" };
    format!("{days} {unit} since the status category changed")
}

/// The epic chip's text width on the chip row: what is left after its own padding and, when a days label follows,
/// that label's width and the gap before it, so the chip truncates and the label stays on the row.
pub fn epic_text_width(avail: f32, days_w: Option<f32>) -> f32 {
    (avail - 2.0 * PILL_MARGIN.x - days_w.map_or(0.0, |w| w + PILL_GAP)).max(0.0)
}

/// Where the epic chip goes and how wide its text may be: `(next_row, text width)`. It stays on the status pill's
/// row (`rest` left there) when its whole text and the days label both fit, else it starts the next row (`full`
/// wide) and is cut to leave the label room (`epic_text_width`).
pub fn epic_place(rest: f32, full: f32, natural: f32, days_w: Option<f32>) -> (bool, f32) {
    if natural <= epic_text_width(rest, days_w) { (false, natural) } else { (true, epic_text_width(full, days_w)) }
}

/// What one column's footer shows.
struct Footer<'a> {
    open: bool,
    loading: bool,
    done: Option<&'a [Issue]>,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| columns(app, ui, p, &mut acts));
    app.actions.extend(acts);
}

fn columns(app: &App, ui: &mut Ui, p: &Palette, acts: &mut Vec<Action>) {
    let cols = app.team_columns();
    let (fc, fr) = teamview::clamp_cursor(app.team.col, app.team.row, &cols);
    // The team lists are in the store (cached or fetched), so "no tickets found" is a finding, not a load.
    let known = app.store.lists.contains_key(&crate::store::ListId::TeamOpen);
    let last_id = egui::Id::new("team-last-cursor");
    let moved = ui.data(|d| d.get_temp::<(usize, usize)>(last_id)) != Some((fc, fr));
    ui.data_mut(|d| d.insert_temp(last_id, (fc, fr)));
    let w = column_width(ui.available_width(), cols.len());
    egui::ScrollArea::horizontal().id_salt("team-columns").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = COL_GAP;
            let h = ui.available_height();
            for (ci, col) in cols.iter().enumerate() {
                let footer = col.member.as_ref().map(|m| Footer {
                    open: app.team.done_open.contains(&m.id),
                    loading: app.team.done_loading.contains(&m.id),
                    done: app.team.done.get(&m.id).map(Vec::as_slice),
                });
                let focus = (ci == fc).then_some(fr);
                ui.allocate_ui_with_layout(vec2(w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    if let Some(row) = column(app, ui, p, ci, col, known, focus, moved, footer) {
                        match row {
                            Click::Card(ri, open) => {
                                // TeamCol keeps the row (clamped to the new column); TeamMove then steps to the card.
                                let kept = teamview::clamp_cursor(ci, fr, &cols).1;
                                acts.push(Action::TeamCol(ci as i32 - fc as i32));
                                acts.push(Action::TeamMove(ri as i32 - kept as i32));
                                if open {
                                    acts.push(Action::TeamOpen);
                                }
                            }
                            Click::Footer => {
                                acts.push(Action::TeamCol(ci as i32 - fc as i32));
                                acts.push(Action::TeamToggleDone);
                            }
                        }
                    }
                });
            }
        });
    });
}

/// What a click in a column asked for: a card (and whether it was a double click, which opens it), or the footer.
enum Click {
    Card(usize, bool),
    Footer,
}

#[allow(clippy::too_many_arguments)]
fn column(
    app: &App,
    ui: &mut Ui,
    p: &Palette,
    ci: usize,
    col: &Column,
    known: bool,
    focus: Option<usize>,
    moved: bool,
    footer: Option<Footer>,
) -> Option<Click> {
    let mut click = None;
    header(ui, p, col, known);
    ui.add_space(COL_GAP - ui.spacing().item_spacing.y);
    egui::ScrollArea::vertical().id_salt(("team-col", ci)).auto_shrink([false, false]).show(ui, |ui| {
        // Between cards: the density's card gap.
        let card_gap = theme::spacing(ui.ctx()).card_gap;
        ui.spacing_mut().item_spacing.y = card_gap;
        if col.member.is_some() && col.in_progress.is_empty() {
            ui.label(RichText::new("nothing in progress").italics().color(p.muted).size(12.0));
        }
        for (ri, c) in col.in_progress.iter().chain(&col.ready).enumerate() {
            let sel = focus == Some(ri);
            let resp = card(app, ui, p, (ci, ri), c, sel);
            if resp.double_clicked() {
                click = Some(Click::Card(ri, true));
            } else if resp.clicked() {
                click = Some(Click::Card(ri, false));
            }
            if sel && moved {
                resp.scroll_to_me(None);
            }
        }
        if let Some(f) = footer {
            ui.add_space(COL_GAP - card_gap);
            if done_footer(ui, p, &f) {
                click = Some(Click::Footer);
            }
        }
    });
    click
}

/// The member's name (bold), then the counts, or "no tickets found" when no list matched them.
fn header(ui: &mut Ui, p: &Palette, col: &Column, known: bool) {
    let name = col.member.as_ref().map_or("Unassigned", |m| m.display.as_str());
    ui.label(RichText::new(name).font(theme::bold(13.5)).color(p.text));
    // Under the name: the density's line gap.
    ui.add_space(theme::spacing(ui.ctx()).line_gap - ui.spacing().item_spacing.y);
    let line = match &col.member {
        Some(_) if col.none_found && known => RichText::new("no tickets found").color(p.off_track),
        Some(_) => RichText::new(counts_line(col.in_progress.len(), col.ready.len())).color(p.muted),
        None => RichText::new(format!("{} ready", col.ready.len())).color(p.muted),
    };
    ui.label(line.size(12.0));
}

/// A card: key (mono), summary (bold, wrapped), then status and epic pills and the days since the status category changed.
fn card(app: &App, ui: &mut Ui, p: &Palette, id: (usize, usize), c: &Card, sel: bool) -> egui::Response {
    let i = c.issue;
    let bg = ui.painter().add(egui::Shape::Noop);
    let sp = theme::spacing(ui.ctx());
    let r = Frame::new().stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::same(sp.card_pad as i8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = sp.line_gap;
        ui.label(RichText::new(&i.key).font(theme::mono(12.5)).color(crate::views::issues::row_key_color(sel, p)));
        ui.add(egui::Label::new(RichText::new(&i.summary).font(theme::bold(13.5)).color(p.text)).wrap());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(PILL_GAP, sp.line_gap);
            let st = theme::status_tint(&i.status.name, &i.status.category, p, &app.cfg.issues.status_colors);
            crate::views::pill(ui, &i.status.name, crate::views::issues::status_fill(&st, p, sel), st.fg);
            let muted = if sel { crate::views::selected_pill_text(p) } else { p.muted };
            let days = c.days_in_status.map(|d| ui.painter().layout_no_wrap(days_text(d), theme::body(12.0), muted));
            if let Some(k) = &i.parent_key {
                let t = theme::epic_tint_in(k, &app.epic_slots, p, &app.cfg.issues.epic_colors);
                let name = i.parent_summary.as_ref().map_or_else(|| k.clone(), |s| format!("{k} {s}"));
                let painter = ui.painter().clone();
                let measure = |s: &str| painter.layout_no_wrap(s.to_string(), theme::body(12.0), t.fg).size().x;
                // The chip and the days label go together: on the status pill's row when both fit there, else
                // starting the next row, the chip cut to leave the label room either way.
                let rest = ui.max_rect().right() - ui.cursor().min.x - PILL_GAP;
                let (next_row, max_w) = epic_place(rest, ui.max_rect().width(), measure(&name), days.as_ref().map(|g| g.size().x));
                if next_row {
                    ui.end_row();
                }
                let text = crate::views::issues::fit_epic(&name, max_w, measure);
                let edge = crate::views::issues::row_stripe(&t, p, sel);
                let r = pill_sized(ui, &text, Color32::TRANSPARENT, t.fg, Stroke::new(1.0, edge), PILL_MARGIN);
                crate::views::tooltip(r, crate::views::issues::epic_tip(k, i.parent_summary.as_deref().unwrap_or_default()));
            }
            // One galley, never wrapped: its width was kept free on the chip row above.
            if let (Some(d), Some(g)) = (c.days_in_status, days) {
                let r = ui.add(egui::Label::new(g));
                crate::views::tooltip(r, days_tip(d));
            }
        });
    });
    let rect = r.response.rect;
    let resp = ui.interact(rect, egui::Id::new(("team-card", id)), Sense::click());
    let fill = crate::views::issues::row_fill(sel, resp.hovered(), p);
    ui.painter().set(bg, egui::epaint::RectShape::filled(rect, theme::RADIUS, fill));
    resp
}

/// The toggle, then (open) the member's recent done keys and summaries, or a spinner while they load. True when
/// the toggle was clicked.
fn done_footer(ui: &mut Ui, p: &Palette, f: &Footer) -> bool {
    let label = egui::Label::new(RichText::new(footer_label(f.open)).color(p.muted).size(12.0)).sense(Sense::click());
    let resp = crate::views::tooltip(ui.add(label), "d").on_hover_cursor(egui::CursorIcon::PointingHand);
    if !f.open {
        return resp.clicked();
    }
    match f.done {
        Some([]) => {
            ui.label(RichText::new("Nothing done in the last 7 days").color(p.muted).size(12.0));
        }
        Some(done) => {
            for i in done {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(RichText::new(&i.key).font(theme::mono(12.5)).color(p.muted));
                    let painter = ui.painter().clone();
                    let title = crate::views::truncate_title(&i.summary, ui.available_width(), |t| {
                        painter.layout_no_wrap(t.to_string(), theme::body(14.0), p.text).size().x
                    });
                    let r = ui.label(RichText::new(&title).font(theme::body(14.0)).color(p.text));
                    if title != i.summary {
                        crate::views::tooltip(r, i.summary.clone());
                    }
                });
            }
        }
        None if f.loading => {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        }
        None => {
            ui.label(RichText::new("Couldn't load; press d twice to retry").color(p.muted).size(12.0));
        }
    }
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_share_the_width_but_never_shrink_under_the_minimum() {
        assert_eq!(column_width(1200.0, 3), 389.0);
        assert_eq!(column_width(1200.0, 7), COL_MIN);
        assert_eq!(column_width(500.0, 0), 500.0);
    }

    #[test]
    fn the_epic_chip_leaves_room_for_the_days_label() {
        assert_eq!(epic_text_width(200.0, None), 200.0 - 2.0 * PILL_MARGIN.x);
        assert_eq!(epic_text_width(200.0, Some(30.0)), 200.0 - 2.0 * PILL_MARGIN.x - 30.0 - PILL_GAP);
        assert_eq!(epic_text_width(10.0, Some(30.0)), 0.0);
        let m = 2.0 * PILL_MARGIN.x;
        assert_eq!(epic_place(150.0, 234.0, 80.0, Some(30.0)), (false, 80.0), "chip and label fit beside the status pill");
        assert_eq!(epic_place(100.0, 234.0, 80.0, Some(30.0)), (true, 234.0 - m - 30.0 - PILL_GAP), "the chip alone would fit: both move down");
        assert_eq!(epic_place(100.0, 234.0, 400.0, None), (true, 234.0 - m), "no label: the chip takes the next row");
    }

    #[test]
    fn a_long_epic_keeps_the_days_label_on_the_chip_row() {
        let mut cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        cfg.team.members = vec!["abc".into(); 4];
        let mut app = App::for_test(cfg, None);
        let who = Some(crate::jira::User { account_id: "abc".into(), display_name: "Grace Hopper".into() });
        let changed = (chrono::Utc::now() - chrono::Duration::days(277)).format("%Y-%m-%dT%H:%M:%S%.3f+0000").to_string();
        app.store.absorb(crate::store::ListId::TeamInProgress, vec![]);
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![Issue {
                key: "CI-3219".into(),
                summary: "Long epic".into(),
                status: crate::jira::Status { name: "Ready for Work".into(), category: "new".into() },
                assignee: who,
                parent_key: Some("CI-1989".into()),
                parent_summary: Some("Cost Insights & Automation for every cluster in every region".into()),
                status_changed: Some(changed),
                ..Default::default()
            }],
        );
        // Five columns at 1200 wide: each at the 260 minimum, the tightest card.
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let inks = crate::testutil::inks(&shapes);
        let days = inks.iter().find(|(t, _)| t == "277 d").unwrap_or_else(|| panic!("one unwrapped days label in {inks:?}")).1;
        let chip = inks.iter().find(|(t, _)| t.starts_with("CI-1989")).expect("epic chip").1;
        let status = inks.iter().find(|(t, _)| t == "Ready for Work").expect("status pill").1;
        assert!(days.top() < chip.bottom() && chip.top() < days.bottom(), "days {days:?} on the chip row {chip:?}");
        assert!(days.left() > chip.right(), "after the chip");
        assert!(chip.top() > status.bottom(), "a long chip starts the next row: {status:?} {chip:?}");
        let card_right = 16.0 + column_width(1200.0 - 32.0, 5) - 12.0;
        assert!(days.right() <= card_right, "the label ends inside the card: {days:?} past {card_right}");
    }

    #[test]
    fn wording() {
        assert_eq!(counts_line(3, 2), "3 in progress · 2 ready");
        assert_eq!(footer_label(false), "Done in the last 7 days ▸");
        assert_eq!(footer_label(true), "Done in the last 7 days ▾");
        assert_eq!(days_text(4), "4 d");
        assert_eq!(days_tip(4), "4 days since the status category changed");
        assert_eq!(days_tip(1), "1 day since the status category changed");
    }

    #[test]
    fn columns_draw_headless() {
        let mut cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        cfg.team.members = vec!["abc".into(), "zzz".into()];
        let mut app = App::for_test(cfg, None);
        app.screen = crate::app::Screen::Team;
        let who = Some(crate::jira::User { account_id: "abc".into(), display_name: "Grace Hopper".into() });
        let status = |n: &str, cat: &str| crate::jira::Status { name: n.into(), category: cat.into() };
        let issue = |key: &str, summary: &str, st: crate::jira::Status, who: Option<crate::jira::User>| Issue {
            key: key.into(),
            summary: summary.into(),
            status: st,
            assignee: who,
            ..Default::default()
        };
        app.store.absorb(crate::store::ListId::TeamInProgress, vec![]);
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![issue("CI-2", "Ready one", status("Ready for Work", "new"), who), issue("CI-3", "Nobody's", status("Ready for Work", "new"), None)],
        );
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let drawn = texts(&mut app);
        for want in
            ["Grace Hopper", "0 in progress · 1 ready", "nothing in progress", "zzz", "no tickets found", "Unassigned", "1 ready", "CI-2", "Ready one", "CI-3"]
        {
            assert!(drawn.iter().any(|t| t == want), "{want} in {drawn:?}");
        }
        assert_eq!(drawn.iter().filter(|t| *t == footer_label(false)).count(), 2, "one footer per member, none under Unassigned");
        app.team.done_open.insert("abc".into());
        app.team.done.insert("abc".into(), vec![issue("CI-7", "Shipped it", status("Done", "done"), None)]);
        let open = texts(&mut app);
        for want in [footer_label(true), "CI-7", "Shipped it"] {
            assert!(open.iter().any(|t| t == want), "{want} in {open:?}");
        }
    }
}
