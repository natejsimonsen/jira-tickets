//! The Team screen: one column per configured member (`teamview::columns`), each with its "In progress" then
//! "Ready" groups and a "Done in the last 7 days" footer, then the Unassigned column with the top of the ready queue.
//! When not every member fits, a quiet link above the columns names who is past the edge ("+3 more: Rob, Matt, Ziad →").

use crate::app::{Action, App};
use crate::jira::Issue;
use crate::teamview::{self, Card, Column, End};
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
/// A card title wraps to at most this many lines, then ends in "…" (the full title is its tooltip). One clamp in
/// both densities, so a comfortable column reads like a compact one, only roomier.
pub const TITLE_ROWS: usize = 3;

/// The quiet line under the last card (or over the first) when j or k could not move: where the column ends and,
/// at the bottom of a member's column with the done list closed, how to see more.
pub fn end_hint(end: End, done_closed: bool) -> &'static str {
    match end {
        End::First => "First ticket",
        End::Last if done_closed => "Last ticket · d shows done",
        End::Last => "Last ticket",
    }
}

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
    let end = end_state(app, ui, &cols, (fc, fr), moved);
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
    let (first, last) = whole_range(offset, view, w, cols.len());
    if overflow && let Some(to) = more_link(ui, p, &cols, first, last) {
        acts.push(Action::TeamCol(to as i32 - fc as i32));
    }
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
                let end = end.filter(|_| ci == fc);
                ui.allocate_ui_with_layout(vec2(w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    if let Some(row) = column(app, ui, p, (ci, col), known, (focus, moved, end), footer, acts) {
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
    // Whole columns fit, so nothing is cut at the sides: the link above names who is past the edge instead of a fade.
    ui.data_mut(|d| d.insert_temp(view_id, (out.state.offset.x, out.inner_rect.width())));
}

/// A column's name for the overflow cue.
fn col_name(col: &Column) -> String {
    col.member.as_ref().map_or_else(|| "Unassigned".to_string(), |m| m.display.clone())
}

/// The end-of-column hint's state: set when this frame's j/k moves (`Action::TeamMove`, pushed by the shortcuts
/// before the views draw) could not move the cursor, kept until the cursor moves.
fn end_state(app: &App, ui: &Ui, cols: &[Column], (fc, fr): (usize, usize), moved: bool) -> Option<End> {
    let id = egui::Id::new("team-end");
    let mut end = ui.data(|d| d.get_temp::<(usize, usize, End)>(id)).filter(|&(c, r, _)| !moved && (c, r) == (fc, fr));
    let sideways = app.actions.iter().any(|a| matches!(a, Action::TeamCol(_)));
    let d: i32 = app.actions.iter().map(|a| if let Action::TeamMove(d) = a { *d } else { 0 }).sum();
    let len = cols.get(fc).map_or(0, |c| c.in_progress.len() + c.ready.len());
    if !sideways && let Some(e) = teamview::end_hit(fr, len, d) {
        end = Some((fc, fr, e));
    }
    ui.data_mut(|d| match end {
        Some(e) => {
            d.insert_temp(id, e);
        }
        None => {
            d.remove::<(usize, usize, End)>(id);
        }
    });
    end.map(|(_, _, e)| e)
}

/// The overflow link, on its own row above the columns: a quiet ghost naming the members past the right edge
/// ("+3 more: Rob, Matt, Ziad →"), or once scrolled to the end, those past the left ("← +2 more: Nate, Dez").
/// The row is there whenever the columns overflow, so scrolling never moves the columns. Returns the column a click
/// moves to: the nearest hidden member.
fn more_link(ui: &mut Ui, p: &Palette, cols: &[Column], first: usize, last: usize) -> Option<usize> {
    let sp = theme::spacing(ui.ctx());
    let right: Vec<(usize, String)> = (last..cols.len()).map(|i| (i, col_name(&cols[i]))).collect();
    let left: Vec<(usize, String)> = (0..first.min(cols.len())).rev().map(|i| (i, col_name(&cols[i]))).collect();
    let r = ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let (hidden, rightward) = if right.is_empty() { (&left, false) } else { (&right, true) };
        let (&(to, _), names) = (hidden.first()?, hidden.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>());
        let font = theme::body(sp.body);
        let painter = ui.painter().clone();
        let measure = |t: &str| painter.layout_no_wrap(t.to_string(), font.clone(), p.muted).size().x;
        let cue = more_cue(&names, ui.available_width() / 2.0 - measure("← "), measure);
        let text = if rightward { format!("{cue} →") } else { format!("← {cue}") };
        let side = if rightward { "right" } else { "left" };
        let key = if rightward { "l" } else { "h" };
        let resp = crate::views::tooltip(ui.add(c::ghost(&text, p)), format!("Past the {side} edge: {} ({key})", names.join(", ")));
        resp.clicked().then_some(to)
    });
    ui.add_space(sp.line_gap - ui.spacing().item_spacing.y);
    r.inner
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
    (ci, col): (usize, &Column),
    known: bool,
    (focus, moved, end): (Option<usize>, bool, Option<End>),
    footer: Option<Footer>,
    acts: &mut Vec<Action>,
) -> Option<Click> {
    let mut click = None;
    header(ui, p, col, known, focus.is_some());
    let done_closed = footer.as_ref().is_some_and(|f| !f.open);
    let hint = |ui: &mut Ui, e: End| {
        let sp = theme::spacing(ui.ctx());
        ui.label(RichText::new(end_hint(e, done_closed)).size(sp.small).color(p.muted));
    };
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
                if sel && end == Some(End::First) {
                    hint(ui, End::First);
                }
                let (resp, done) = card_with(app, ui, p, (ci, ri), c, sel, false, sel);
                acts.extend(done);
                if sel && end == Some(End::Last) {
                    hint(ui, End::Last);
                }
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
/// the accent), then the counts, or "no tickets found" when no list matched them.
fn header(ui: &mut Ui, p: &Palette, col: &Column, known: bool, focused: bool) {
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
    ui.horizontal(|ui| match &col.member {
        Some(_) if col.none_found && known => {
            c::missing_value(ui, p, "no tickets found");
        }
        Some(_) => {
            ui.label(RichText::new(counts_line(col.in_progress.len(), col.ready.len())).size(sp.small).color(p.muted));
        }
        None => {
            ui.label(RichText::new(format!("{} ready", col.ready.len())).size(sp.small).color(p.muted));
        }
    });
}

/// A ticket card (Team and Me): the shared card frame with card_pad on every side measured to the ink, the key
/// (mono), the summary (`card_title_font`, wrapped), the status pill with the days in status, then the
/// epic as a neutral dot label cut to the card's width: on its own line, or (`epic_inline`, for wide lists such as
/// Me) at the end of the status line, so every card is key, title, one meta line. The title wraps to TITLE_ROWS.
pub(crate) fn card(app: &App, ui: &mut Ui, p: &Palette, id: (usize, usize), c: &Card, sel: bool, epic_inline: bool) -> egui::Response {
    card_with(app, ui, p, id, c, sel, epic_inline, false).0
}

/// `card`, plus (`done`) the quiet Move to Done action at the right end of the key line, laid over the card so
/// focusing a card never changes its height. The action is returned when clicked.
#[allow(clippy::too_many_arguments)]
fn card_with(app: &App, ui: &mut Ui, p: &Palette, id: (usize, usize), c: &Card, sel: bool, epic_inline: bool, done: bool) -> (egui::Response, Option<Action>) {
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
        let key_rect = ui.label(RichText::new(&i.key).font(key_font.clone()).color(muted)).rect;
        let mut job = egui::text::LayoutJob::simple(i.summary.clone(), c::card_title_font(ui), p.text, ui.available_width());
        job.wrap.max_rows = TITLE_ROWS;
        job.wrap.overflow_character = Some('…');
        let title = ui.painter().layout_job(job);
        let clamped = title.elided;
        ui.add(egui::Label::new(title));
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
        (key_rect, clamped)
    });
    let (key_rect, clamped) = r.inner;
    let mut resp = ui.interact(r.response.rect, rid, Sense::click());
    if clamped {
        resp = crate::views::tooltip(resp, i.summary.clone());
    }
    let mut act = None;
    if done {
        // On the key line's centre, inside the card's right pad; added after the card's own click, so it is on top.
        let right = r.response.rect.right() - sp.card_pad - c::card_stroke(p, sel).width;
        let half = crate::views::BUTTON_H / 2.0;
        let rect = egui::Rect::from_x_y_ranges(key_rect.right()..=right, (key_rect.center().y - half)..=(key_rect.center().y + half));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::right_to_left(egui::Align::Center)));
        act = c::done_button(&mut child, p, i, fill);
    }
    (resp, act)
}

/// The done toggle as a disclosure: a full-width row at least 24 tall (BUTTON_H), a chevron in the card pad, the
/// title as the column's other group labels ("In progress", "Ready": `pane_label`, the count beside it once known,
/// `footer_count`), the hover fill; then (open) the member's recent done keys and summaries on the card content's edge,
/// line_gap under the title as cards are under theirs, wrapped in full, or a spinner while they load. True when the
/// toggle was clicked.
fn done_footer(ui: &mut Ui, p: &Palette, ci: usize, f: &Footer) -> bool {
    let sp = theme::spacing(ui.ctx());
    let id = egui::Id::new(("team-done-toggle", ci));
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let bg = ui.painter().add(egui::Shape::Noop);
    let row = ui.allocate_ui_with_layout(vec2(ui.available_width(), crate::views::BUTTON_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
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
        c::pane_label(ui, p, FOOTER_TITLE, footer_count(f.open, f.done.map(<[Issue]>::len), f.count)).rect.height()
    });
    let (row, label_h) = (row.response, row.inner);
    let resp = ui.interact(row.rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let resp = crate::views::tooltip(resp, "d");
    ui.painter().set(bg, egui::epaint::RectShape::filled(row.rect.expand2(vec2(4.0, 0.0)), f32::from(c::ROW_RADIUS), c::row_hover_fill(hovered, p)));
    if !f.open {
        return resp.clicked();
    }
    // The row is BUTTON_H tall around a smaller label: take the slack under the label (and the item gap) back, so the
    // first row starts line_gap under the title, never above the row's own edge.
    let slack = (row.rect.height() - label_h) / 2.0;
    ui.add_space((sp.line_gap - slack).max(0.0) - ui.spacing().item_spacing.y);
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
        let more = inks.iter().find(|(t, _)| t.starts_with("+1 more")).expect("the overflow link").1;
        let counts = inks.iter().find(|(t, _)| t.contains("in progress ·")).expect("counts line").1;
        assert!(more.bottom() < counts.top(), "the overflow link sits on its own row above the columns: {more:?} {counts:?}");
        let card_right = 16.0 + column_width(1200.0 - 32.0, 5) - 12.0;
        assert!(epic.1.right() <= card_right, "the epic ends inside the card: {:?} past {card_right}", epic.1);
    }

    #[test]
    fn a_long_title_is_clamped() {
        let mut cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        cfg.team.members = vec!["abc".into()];
        let mut app = App::for_test(cfg, None);
        let who = Some(crate::jira::User { account_id: "abc".into(), display_name: "Grace Hopper".into() });
        let long = "Add nodeAffinity to keep splinter and other cluster-critical capacity class tolerating services off Karpenter-provisioned instances in every region and every tier we run";
        app.store.absorb(crate::store::ListId::TeamInProgress, vec![]);
        app.store.absorb(
            crate::store::ListId::TeamOpen,
            vec![Issue {
                key: "CI-1".into(),
                summary: long.into(),
                status: crate::jira::Status { name: "Ready for Work".into(), category: "new".into() },
                assignee: who,
                ..Default::default()
            }],
        );
        let shapes = crate::testutil::paint_at2(egui::vec2(700.0, 800.0), |ui| show(&mut app, ui));
        let title = crate::testutil::inks(&shapes).into_iter().find(|(t, _)| t == long).expect("the title").1;
        let line = theme::COMPACT.card_title * 1.5;
        assert!(title.height() < TITLE_ROWS as f32 * line, "at most {TITLE_ROWS} lines: {title:?}");
        assert!(title.height() > (TITLE_ROWS - 1) as f32 * line * 0.9, "it still wraps to {TITLE_ROWS}: {title:?}");
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
        assert_eq!(end_hint(End::Last, true), "Last ticket · d shows done");
        assert_eq!(end_hint(End::Last, false), "Last ticket");
        assert_eq!(end_hint(End::First, true), "First ticket");
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
