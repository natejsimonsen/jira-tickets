//! The Team screen: one column per configured member (`teamview::columns`), each with its "In progress" then
//! "Ready" groups and a "Done in the last 7 days" footer, then the Unassigned column with the top of the ready queue.
//! Members past the right edge are named on the last whole column's counts line ("+3 more: Rob, Matt, …").

use crate::app::{Action, App};
use crate::jira::Issue;
use crate::teamview::{self, Card, Column};
use crate::theme::{self, Palette};
use crate::views::components as c;
use egui::{Margin, RichText, Sense, Ui, vec2};

/// A column is never narrower than this; past that the columns scroll sideways.
pub const COL_MIN: f32 = 260.0;
/// Between columns, and under a column's header.
const COL_GAP: f32 = 16.0;
/// Between a card's pills.
const PILL_GAP: f32 = 8.0;
/// The focused member's underline: its thickness and gap under the name.
const FOCUS_RULE: f32 = 2.0;
/// From the done toggle's chevron box to its title.
const CHEVRON_GAP: f32 = 4.0;
/// The done footer's title.
pub const FOOTER_TITLE: &str = "Done in the last 7 days";

/// How many whole columns fit across `avail` at COL_MIN or wider, at least one and at most `n`.
pub fn columns_fit(avail: f32, n: usize) -> usize {
    (((avail + COL_GAP) / (COL_MIN + COL_GAP)).floor() as usize).clamp(1, n.max(1))
}

/// The column width: the columns that fit (`columns_fit`) share `avail` exactly, so the view never ends on a
/// half-cut column; the rest scroll in whole.
pub fn column_width(avail: f32, n: usize) -> f32 {
    let fit = columns_fit(avail, n) as f32;
    ((avail - COL_GAP * (fit - 1.0)) / fit).floor()
}

/// A member column's counts: "3 in progress · 2 ready".
pub fn counts_line(in_progress: usize, ready: usize) -> String {
    format!("{in_progress} in progress · {ready} ready")
}

/// A card's days-in-status text: "4d in status", "<1d in status" on the first day. Shared with Me.
pub fn days_text(days: i64) -> String {
    if days < 1 { "<1d in status".to_string() } else { format!("{days}d in status") }
}

/// The days label's tooltip. The count runs from Jira's `statuscategorychangedate`, so it says what that measures:
/// a move between two statuses of the same category does not reset it.
pub fn days_tip(days: i64) -> String {
    let unit = if days == 1 { "day" } else { "days" };
    format!("{days} {unit} since the status category changed")
}

/// The epic label's name: the epic's summary, else its key.
pub fn epic_name(key: &str, summary: Option<&str>) -> String {
    summary.filter(|s| !s.is_empty()).map_or_else(|| key.to_string(), str::to_string)
}

/// The left inset of a column's card content (the shared card's pad plus its 1 px edge): done rows and the done
/// toggle's title start here, on the card keys' edge.
pub fn content_inset(card_pad: f32) -> f32 {
    card_pad + c::card_stroke(&theme::LIGHT, false).width
}

/// The columns not wholly in view, as (first, last-exclusive) runs: those before the first whole column and
/// those after the last, for `n` columns `w` wide COL_GAP apart, scrolled `offset` into a `view` wide strip.
pub fn whole_range(offset: f32, view: f32, w: f32, n: usize) -> (usize, usize) {
    let pitch = w + COL_GAP;
    let left = |i: usize| i as f32 * pitch;
    let first = (0..n).find(|&i| left(i) >= offset - 0.5).unwrap_or(n);
    let end = (first..n).take_while(|&i| left(i) + w <= offset + view + 0.5).last().map_or(first, |i| i + 1);
    (first, end.max(first))
}

/// The overflow cue naming the members past the right edge: "+3 more: Rob, Matt, …", as many first names as
/// fit in `max_w` (`measure` gives a string's width), down to "+3 more". Empty when nobody is hidden.
pub fn more_cue(names: &[&str], max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    let n = names.len();
    if n == 0 {
        return String::new();
    }
    let first: Vec<&str> = names.iter().map(|s| s.split_whitespace().next().unwrap_or(s)).collect();
    for k in (1..=n).rev() {
        let mut t = format!("+{n} more: {}", first[..k].join(", "));
        if k < n {
            t.push_str(", …");
        }
        if measure(&t) <= max_w {
            return t;
        }
    }
    format!("+{n} more")
}

/// What one column's footer shows.
struct Footer<'a> {
    open: bool,
    loading: bool,
    done: Option<&'a [Issue]>,
    /// The member's done count from the cheap count load, for the closed row.
    count: Option<usize>,
}

/// The count beside the footer title: open, the list's length once it is known (the list on screen is the truth);
/// closed, the count load's number. None until one of them lands.
fn footer_count(open: bool, listed: Option<usize>, counted: Option<usize>) -> Option<usize> {
    if open { listed.or(counted) } else { counted }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
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
    // More members than fit: a visible scroll bar says so (whole columns fit, so none is cut mid-word).
    let overflow = columns_fit(ui.available_width(), cols.len()) < cols.len();
    let scroll = ui.spacing().scroll;
    crate::views::floating_thumb(&mut ui.spacing_mut().scroll);
    let saved = crate::views::set_thumb_fills(ui, p);
    let bars = if overflow { egui::scroll_area::ScrollBarVisibility::AlwaysVisible } else { egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded };
    // The sideways bar gets its own strip under the columns (not floating over the last cards): the scroll area
    // takes its width plus a line gap off the columns' height.
    let line_gap = theme::spacing(ui.ctx()).line_gap;
    let card_gap = theme::spacing(ui.ctx()).card_gap;
    let strip = &mut ui.spacing_mut().scroll;
    strip.floating = false;
    strip.bar_width = crate::views::THUMB_W;
    strip.bar_inner_margin = line_gap;
    // And a card gap under the bar, so it never sits on the hint bar.
    strip.bar_outer_margin = card_gap;
    // Who is past the right edge, from last frame's scroll (the columns are laid out at fixed pitch).
    let view_id = egui::Id::new("team-hscroll");
    let (offset, view) = ui.data(|d| d.get_temp::<(f32, f32)>(view_id)).unwrap_or((0.0, ui.available_width()));
    let (first, end) = whole_range(offset, view, w, cols.len());
    let hidden: Vec<(usize, String)> = (end..cols.len()).map(|i| (i, col_name(&cols[i]))).collect();
    let cue_col = (!hidden.is_empty() && end > first).then(|| end - 1);
    let out = egui::ScrollArea::horizontal().id_salt("team-columns").auto_shrink([false, false]).scroll_bar_visibility(bars).show(ui, |ui| {
        // The thumb style is for the sideways bar only: the columns' own scroll areas keep the default.
        crate::views::restore_fills(ui, saved);
        ui.spacing_mut().scroll = scroll;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = COL_GAP;
            let h = ui.available_height();
            for (ci, col) in cols.iter().enumerate() {
                let footer = col.member.as_ref().map(|m| Footer {
                    open: app.team.done_open.contains(&m.id),
                    loading: app.team.done_loading.contains(&m.id),
                    done: app.team.done.get(&m.id).map(Vec::as_slice),
                    count: app.team.done_count.get(&m.id).copied(),
                });
                let focus = (ci == fc).then_some(fr);
                let cue = (cue_col == Some(ci)).then_some(hidden.as_slice());
                ui.allocate_ui_with_layout(vec2(w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    if let Some(row) = column(app, ui, p, ci, col, known, focus, moved, footer, cue) {
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
                            // Focus the first hidden member: the view scrolls to their column.
                            Click::More(to) => acts.push(Action::TeamCol(to as i32 - fc as i32)),
                        }
                    }
                });
            }
        });
    });
    // Whole columns fit, so nothing is cut at the sides: the counts-line cue names who is past the edge instead of a fade.
    ui.data_mut(|d| d.insert_temp(view_id, (out.state.offset.x, out.inner_rect.width())));
}

/// A column's name for the overflow cue.
fn col_name(col: &Column) -> String {
    col.member.as_ref().map_or_else(|| "Unassigned".to_string(), |m| m.display.clone())
}

/// What a click in a column asked for: a card (and whether it was a double click, which opens it), the footer,
/// or the overflow cue (the column to move to).
enum Click {
    Card(usize, bool),
    Footer,
    More(usize),
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
    cue: Option<&[(usize, String)]>,
) -> Option<Click> {
    let mut click = None;
    if header(ui, p, col, known, focus.is_some(), cue) {
        click = cue.and_then(|h| h.first()).map(|(i, _)| Click::More(*i));
    }
    ui.add_space(COL_GAP - ui.spacing().item_spacing.y);
    crate::views::no_scroll_fade(ui);
    let out = egui::ScrollArea::vertical().id_salt(("team-col", ci)).auto_shrink([false, false]).show(ui, |ui| {
        let sp = theme::spacing(ui.ctx());
        // Each non-empty group under its own quiet label, the same in every column, so the first cards share a
        // top edge (the header's counts already say "0 in progress").
        let groups = [("In progress", &col.in_progress), ("Ready", &col.ready)];
        let mut ri = 0;
        for (gi, (title, cards)) in groups.into_iter().filter(|(_, cs)| !cs.is_empty()).enumerate() {
            if gi > 0 {
                // A card gap is already under the last card: one more between groups.
                ui.add_space(sp.card_gap);
            }
            ui.spacing_mut().item_spacing.y = 0.0;
            c::pane_label(ui, p, title, None);
            ui.add_space(sp.line_gap);
            ui.spacing_mut().item_spacing.y = sp.card_gap;
            for c in cards {
                let sel = focus == Some(ri);
                let resp = card(app, ui, p, (ci, ri), c, sel, false);
                if resp.double_clicked() {
                    click = Some(Click::Card(ri, true));
                } else if resp.clicked() {
                    click = Some(Click::Card(ri, false));
                }
                if sel && moved {
                    resp.scroll_to_me(None);
                }
                ri += 1;
            }
        }
        ui.spacing_mut().item_spacing.y = sp.card_gap;
        // The done toggle is one more item in the stack: a card gap under the last card, box to box.
        if let Some(f) = footer
            && done_footer(ui, p, ci, &f)
        {
            click = Some(Click::Footer);
        }
    });
    c::scroll_edge_fade(ui, &out, p.bg);
    click
}

/// The member's avatar and name (the shared section header, one step above card titles; the focused member's underlined in
/// the accent), then the counts, or "no tickets found" when no list matched them. `cue` (the members past the right
/// edge) puts "+3 more: Rob, Matt, …" at the counts line's right end; true when it was clicked.
fn header(ui: &mut Ui, p: &Palette, col: &Column, known: bool, focused: bool, cue: Option<&[(usize, String)]>) -> bool {
    let sp = theme::spacing(ui.ctx());
    // A member's avatar AVATAR_GAP before their name, centred on its line; "Unassigned" has none.
    let r = match &col.member {
        Some(m) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::AVATAR_GAP;
                crate::avatar::show(ui, m.account_id.as_deref(), &m.display, theme::AVATAR_HEADER);
                c::section_header(ui, p, &m.display, None)
            })
            .inner
        }
        // The same row height as the members' headers, so every column's counts line up.
        None => {
            ui.horizontal(|ui| {
                ui.set_min_height(theme::AVATAR_HEADER);
                c::section_header(ui, p, "Unassigned", None)
            })
            .inner
        }
    };
    if focused {
        let y = r.rect.bottom() + 1.0;
        ui.painter().rect_filled(egui::Rect::from_x_y_ranges(r.rect.x_range(), y..=y + FOCUS_RULE), 1.0, p.accent);
    }
    // Under the name: the density's line gap (room for the focus rule included).
    ui.add_space((sp.line_gap - ui.spacing().item_spacing.y).max(FOCUS_RULE + 1.0 - ui.spacing().item_spacing.y));
    ui.horizontal(|ui| {
        match &col.member {
            Some(_) if col.none_found && known => {
                c::missing_value(ui, p, "no tickets found");
            }
            Some(_) => {
                ui.label(RichText::new(counts_line(col.in_progress.len(), col.ready.len())).size(sp.small).color(p.muted));
            }
            None => {
                ui.label(RichText::new(format!("{} ready", col.ready.len())).size(sp.small).color(p.muted));
            }
        }
        let Some(hidden) = cue else { return false };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let names: Vec<&str> = hidden.iter().map(|(_, n)| n.as_str()).collect();
            let font = theme::body(sp.small);
            let painter = ui.painter().clone();
            let text = more_cue(&names, ui.available_width(), |t| painter.layout_no_wrap(t.to_string(), font.clone(), p.muted).size().x);
            let r = ui.add(egui::Label::new(RichText::new(text).font(font.clone()).color(p.muted)).sense(Sense::click()).selectable(false));
            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
            crate::views::tooltip(r, format!("Past the right edge: {}", names.join(", "))).clicked()
        })
        .inner
    })
    .inner
}

/// A ticket card (Team and Me): the shared card frame with card_pad on every side measured to the ink, the key
/// (mono), the summary (`card_title_font`, wrapped), the status pill with the days in status, then the
/// epic as a neutral dot label cut to the card's width: on its own line, or (`epic_inline`, for wide lists such as
/// Me) at the end of the status line, so every card is key, title, one meta line.
pub(crate) fn card(app: &App, ui: &mut Ui, p: &Palette, id: (usize, usize), c: &Card, sel: bool, epic_inline: bool) -> egui::Response {
    let i = c.issue;
    let sp = theme::spacing(ui.ctx());
    let rid = egui::Id::new(("team-card", id));
    let hovered = ui.ctx().read_response(rid).is_some_and(|r| r.hovered());
    let mut frame = c::card_frame(ui, p, sel);
    if hovered && !sel {
        frame = frame.fill(p.hover_row);
    }
    let fill = frame.fill;
    // The key's line box sits taller than its capitals: take that leading off the top pad so the ink, like the
    // pill's edge at the bottom, is card_pad from the frame.
    let key_font = theme::key_font();
    let lead = theme::Ink::of(ui.ctx(), &key_font, None).top;
    frame.inner_margin.top = (sp.card_pad - lead).round().max(0.0) as i8;
    let muted = theme::muted_on(fill, p);
    let r = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = sp.line_gap;
        ui.label(RichText::new(&i.key).font(key_font.clone()).color(muted));
        ui.add(egui::Label::new(RichText::new(&i.summary).font(c::card_title_font(ui)).color(p.text)).wrap());
        let epic = |ui: &mut Ui| {
            let Some(k) = &i.parent_key else { return };
            let t = theme::epic_tint_in(k, &app.epic_slots, p, &app.cfg.issues.epic_colors);
            let name = epic_name(k, i.parent_summary.as_deref());
            let painter = ui.painter().clone();
            let font = theme::body(sp.caption);
            let measure = |s: &str| painter.layout_no_wrap(s.to_string(), font.clone(), muted).size().x;
            let max_w = ui.available_width() - c::EPIC_DOT - c::EPIC_DOT_GAP;
            let text = crate::views::issues::fit_epic(&name, max_w, measure);
            let r = c::epic_label(ui, p, &text, t, fill);
            crate::views::tooltip(r, crate::views::issues::epic_tip(k, i.parent_summary.as_deref().unwrap_or_default()));
        };
        let meta = |ui: &mut Ui| {
            ui.spacing_mut().item_spacing = vec2(PILL_GAP, sp.line_gap);
            let st = theme::status_tint(&i.status.name, &i.status.category, p, &app.cfg.issues.status_colors);
            c::status_pill(ui, &i.status.name, st);
            if let Some(d) = c.days_in_status {
                let r = ui.label(RichText::new(days_text(d)).size(sp.caption).color(muted));
                crate::views::tooltip(r, days_tip(d));
            }
        };
        if epic_inline {
            // One line: the epic takes what is left after the pill and days, cut to fit.
            ui.horizontal(|ui| {
                meta(ui);
                ui.add_space(PILL_GAP);
                epic(ui);
            });
        } else {
            ui.horizontal_wrapped(meta);
            epic(ui);
        }
    });
    ui.interact(r.response.rect, rid, Sense::click())
}

/// The done toggle as a disclosure: a full-width row at least 24 tall (BUTTON_H), a chevron in the card pad, the
/// title one step under the member name (regular body `text`, the muted count beside it once known,
/// `footer_count`), the hover fill; then (open) the member's recent done keys and summaries on the card content's edge,
/// wrapped in full, or a spinner while they load. True when the toggle was clicked.
fn done_footer(ui: &mut Ui, p: &Palette, ci: usize, f: &Footer) -> bool {
    let sp = theme::spacing(ui.ctx());
    let id = egui::Id::new(("team-done-toggle", ci));
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let bg = ui.painter().add(egui::Shape::Noop);
    let row = ui
        .allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_size(vec2(ui.available_width(), crate::views::BUTTON_H));
            ui.spacing_mut().item_spacing.x = CHEVRON_GAP;
            let chevron_box = content_inset(sp.card_pad) - CHEVRON_GAP;
            let (r, _) = ui.allocate_exact_size(vec2(chevron_box, chevron_box), Sense::hover());
            let c0 = r.center();
            let pts = if f.open {
                vec![egui::pos2(c0.x - 4.0, c0.y - 2.0), egui::pos2(c0.x, c0.y + 2.0), egui::pos2(c0.x + 4.0, c0.y - 2.0)]
            } else {
                vec![egui::pos2(c0.x - 2.0, c0.y - 4.0), egui::pos2(c0.x + 2.0, c0.y), egui::pos2(c0.x - 2.0, c0.y + 4.0)]
            };
            ui.painter().add(egui::Shape::line(pts, egui::Stroke::new(1.5, p.muted)));
            done_title(ui, p, footer_count(f.open, f.done.map(<[Issue]>::len), f.count));
        })
        .response;
    let resp = ui.interact(row.rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let resp = crate::views::tooltip(resp, "d");
    ui.painter().set(bg, egui::epaint::RectShape::filled(row.rect.expand2(vec2(4.0, 0.0)), f32::from(c::ROW_RADIUS), c::row_hover_fill(hovered, p)));
    if !f.open {
        return resp.clicked();
    }
    ui.add_space(sp.line_gap);
    match f.done {
        Some([]) => {
            c::empty_state(ui, p, "Nothing done in the last 7 days", None);
        }
        Some(done) => {
            let inset = content_inset(sp.card_pad);
            for i in done {
                done_row(ui, p, inset, i);
            }
        }
        None if f.loading => {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        }
        None => {
            c::empty_state(ui, p, "Couldn't load", Some("Press d twice to retry."));
        }
    }
    resp.clicked()
}

/// One done row inside the card content's edges (`inset` either side, as the cards' text): the mono key, then the
/// whole summary wrapped, as card titles are.
fn done_row(ui: &mut Ui, p: &Palette, inset: f32, i: &Issue) {
    let sp = theme::spacing(ui.ctx());
    let painter = ui.painter().clone();
    let key = painter.layout_no_wrap(i.key.clone(), theme::key_font(), p.muted);
    let x = inset + key.size().x + PILL_GAP;
    let wrap_w = (ui.available_width() - x - inset).max(0.0);
    let font = theme::body(sp.body);
    let text = painter.layout(i.summary.clone(), font, p.text, wrap_w);
    // The key sits on the summary's first baseline.
    let line = crate::views::galley_baseline(&text);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), text.size().y.max(key.size().y)), Sense::hover());
    if ui.is_rect_visible(rect) {
        let key_top = crate::views::top_for_baseline(rect.top() + line, crate::views::galley_baseline(&key));
        painter.galley(egui::pos2(rect.left() + inset, key_top), key, p.muted);
        painter.galley(egui::pos2(rect.left() + x, rect.top()), text, p.text);
    }
}

/// The done toggle's title: "Done in the last 7 days" in regular body `text` (the member name above is bold h3),
/// then the count in `muted` on the same baseline, the shared header's gap apart.
fn done_title(ui: &mut Ui, p: &Palette, count: Option<usize>) {
    let size = theme::spacing(ui.ctx()).body;
    ui.spacing_mut().item_spacing.x = 0.0;
    let title = ui.painter().layout_no_wrap(FOOTER_TITLE.to_string(), theme::body(size), p.text);
    ui.label(RichText::new(FOOTER_TITLE).font(theme::body(size)).color(p.text));
    if let Some(n) = count {
        let n_g = ui.painter().layout_no_wrap(c::count_label(n), theme::body(size), p.muted);
        ui.add_space(theme::ink_gap(&title, &n_g, c::HEADER_COUNT_GAP));
        ui.label(RichText::new(c::count_label(n)).font(theme::body(size)).color(p.muted));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_columns_share_the_width() {
        assert_eq!(column_width(1200.0, 3), 389.0);
        assert_eq!(columns_fit(1200.0, 7), 4);
        assert_eq!(column_width(1200.0, 7), 288.0, "four whole columns, the rest scroll");
        assert_eq!(column_width(500.0, 0), 500.0);
        assert_eq!(columns_fit(200.0, 3), 1);
        assert!(column_width(1408.0, 7) >= COL_MIN);
    }

    #[test]
    fn a_long_epic_is_cut_to_the_card() {
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
                parent_summary: Some("Cost Insights & Automation for every cluster in every region of the world".into()),
                status_changed: Some(changed),
                ..Default::default()
            }],
        );
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let inks = crate::testutil::inks(&shapes);
        let days = inks.iter().find(|(t, _)| t == "277d in status").unwrap_or_else(|| panic!("the days label in {inks:?}")).1;
        let status = inks.iter().find(|(t, _)| t == "Ready for Work").expect("status pill").1;
        let epic = inks.iter().find(|(t, _)| t.starts_with("Cost Insights")).expect("epic label");
        assert!(days.top() < status.bottom() && status.top() < days.bottom(), "days beside the status: {days:?} {status:?}");
        assert!(epic.0.ends_with('…'), "cut to fit: {:?}", epic.0);
        let card_right = 16.0 + column_width(1200.0 - 32.0, 5) - 12.0;
        assert!(epic.1.right() <= card_right, "the epic ends inside the card: {:?} past {card_right}", epic.1);
    }

    #[test]
    fn done_rows_start_on_the_card_content_edge() {
        assert_eq!(content_inset(theme::COMPACT.card_pad), 13.0);
        assert_eq!(content_inset(theme::COMFORTABLE.card_pad), 17.0);
        assert!(content_inset(theme::COMPACT.card_pad) - CHEVRON_GAP >= 8.0, "the chevron fits its box");
    }

    #[test]
    fn the_footer_count_follows_the_list_once_open() {
        assert_eq!(footer_count(false, None, None), None, "nothing loaded yet");
        assert_eq!(footer_count(false, None, Some(3)), Some(3), "closed: the count load");
        assert_eq!(footer_count(false, Some(5), Some(3)), Some(3), "closed: a dropped list is not shown");
        assert_eq!(footer_count(true, Some(5), Some(3)), Some(5), "open: the list on screen");
        assert_eq!(footer_count(true, None, Some(3)), Some(3), "open, list loading: the count meanwhile");
    }

    #[test]
    fn whole_columns_in_view() {
        // 100 wide, 16 apart: pitch 116.
        assert_eq!(whole_range(0.0, 332.0, 100.0, 5), (0, 3));
        assert_eq!(whole_range(116.0, 332.0, 100.0, 5), (1, 4));
        assert_eq!(whole_range(50.0, 332.0, 100.0, 5), (1, 3), "a half-scrolled column is not whole");
        assert_eq!(whole_range(0.0, 1000.0, 100.0, 3), (0, 3));
        assert_eq!(whole_range(0.0, 10.0, 100.0, 3), (0, 0));
    }

    #[test]
    fn the_overflow_cue_names_who_fits() {
        // One unit a character.
        let m = |t: &str| t.chars().count() as f32;
        let names = ["Rob Pike", "Matt Trout", "Shawn Sorichetti"];
        assert_eq!(more_cue(&names, 100.0, m), "+3 more: Rob, Matt, Shawn");
        assert_eq!(more_cue(&names, 22.0, m), "+3 more: Rob, Matt, …");
        assert_eq!(more_cue(&names, 5.0, m), "+3 more");
        assert_eq!(more_cue(&[], 100.0, m), "");
    }

    #[test]
    fn wording() {
        assert_eq!(counts_line(3, 2), "3 in progress · 2 ready");
        assert_eq!(days_text(4), "4d in status");
        assert_eq!(days_text(0), "<1d in status");
        assert_eq!(epic_name("CI-1", Some("Blue Rollouts")), "Blue Rollouts");
        assert_eq!(epic_name("CI-1", None), "CI-1");
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
        for want in ["Grace Hopper", "0 in progress · 1 ready", "Ready", "zzz", "no tickets found", "Unassigned", "1 ready", "CI-2", "Ready one", "CI-3"] {
            assert!(drawn.iter().any(|t| t == want), "{want} in {drawn:?}");
        }
        assert!(!drawn.iter().any(|t| t == "In progress" || t == "nothing in progress"), "an empty group has no label: {drawn:?}");
        assert!(!drawn.iter().any(|t| t == "1"), "no count on a closed done row before it loads: {drawn:?}");
        assert_eq!(drawn.iter().filter(|t| *t == FOOTER_TITLE).count(), 2, "one footer per member, none under Unassigned");
        app.team.done_count.insert("abc".into(), 4);
        let closed = texts(&mut app);
        assert!(closed.iter().any(|t| t == "4"), "the count on a closed done row: {closed:?}");
        app.team.done_open.insert("abc".into());
        app.team.done.insert("abc".into(), vec![issue("CI-7", "Shipped it", status("Done", "done"), None)]);
        let open = texts(&mut app);
        for want in [FOOTER_TITLE, "1", "CI-7", "Shipped it"] {
            assert!(open.iter().any(|t| t == want), "{want} in {open:?}");
        }
    }
}
