//! The Roadmap screen: a tab strip over four views of the initiatives `roadmap::derive` builds from the epics.
//! Tab 1 is Now / Next / Later with the shipped list, tab 3 the status review table; tabs 2 and 4 come next.

use crate::app::{Action, App};
use crate::roadmap::{self, Health, Initiative, Progress};
use crate::theme::{self, Palette};
use crate::views::{PILL_MARGIN, pill_sized};
use chrono::{Datelike, NaiveDate};
use egui::{Color32, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};

/// The tab strip's labels, numbered as their keys.
pub const TABS: [&str; 4] = ["1 Now / Next / Later", "2 Timeline", "3 Status review", "4 Outcomes"];
/// Tab 1's column heads and subtitles.
pub const COLUMNS: [(&str, &str); 3] =
    [("Now", "In progress this quarter"), ("Next", "Committed, starts when capacity frees up"), ("Later", "Ideas and parked work, not committed")];
/// Tab 3's table heads.
pub const HEADS: [&str; 7] = ["Initiative", "Health", "Confidence", "Latest update", "Next milestone", "Definition of done", "Progress"];
/// Each table column's share of the table's width.
const SHARES: [f32; 7] = [0.19, 0.10, 0.09, 0.22, 0.14, 0.16, 0.10];
/// Under this the table scrolls sideways.
const TABLE_MIN_W: f32 = 900.0;
/// Between tabs, as between the header's tabs.
const TAB_GAP: f32 = 24.0;
/// Between columns and tiles, and under the tab strip and each section.
const GAP: f32 = 16.0;
/// Between cards, and inside a column or the shipped box.
const CARD_GAP: f32 = 8.0;
const PAD: i8 = 12;
/// Between a card's lines and its chips.
const LINE_GAP: f32 = 4.0;
/// The progress bar's height.
const BAR_H: f32 = 6.0;

/// The rows `j`/`k` walk on `tab`: tab 1's columns top to bottom, Now then Next then Later; tab 3's table rows.
pub fn tab_rows(tab: u8, items: &[Initiative]) -> Vec<&Initiative> {
    match tab {
        1 => roadmap::now_next_later(items).into_iter().flatten().collect(),
        3 => roadmap::status_rows(items),
        _ => Vec::new(),
    }
}

pub fn health_label(h: Health) -> &'static str {
    match h {
        Health::OffTrack => "Off track",
        Health::AtRisk => "At risk",
        Health::OnTrack => "On track",
        Health::None => "No health set",
    }
}

fn health_color(h: Health, p: &Palette) -> Color32 {
    match h {
        Health::OffTrack => p.off_track,
        Health::AtRisk => p.at_risk,
        Health::OnTrack => p.on_track,
        Health::None => p.no_health,
    }
}

/// "done / total" under a progress bar.
pub fn progress_text(pr: &Progress) -> String {
    format!("{} / {}", pr.done, pr.todo + pr.doing + pr.done)
}

/// The shipped list's heading.
pub fn shipped_title(year: i32) -> String {
    format!("Shipped in {year}")
}

/// Done initiatives targeted in `year`, and those with no target (flagged, so the gap gets filled).
pub fn shipped_in(items: &[Initiative], year: i32) -> Vec<&Initiative> {
    roadmap::shipped(items).into_iter().filter(|i| i.target.is_none_or(|t| t.year == year)).collect()
}

/// The KTLO footnote under Now: the ongoing epics by summary, without their "(ongoing)" marker. None when there are none.
pub fn ktlo_line(ongoing: &[String]) -> Option<String> {
    const MARK: &str = "(ongoing)";
    let names: Vec<String> = ongoing
        .iter()
        .map(|s| {
            // Searched on `s` itself (ASCII-insensitive), so the byte index is always a char boundary of `s`.
            let at = s.char_indices().map(|(i, _)| i).find(|&i| s.get(i..i + MARK.len()).is_some_and(|w| w.eq_ignore_ascii_case(MARK)));
            let rest = match at {
                Some(i) => format!("{}{}", &s[..i], &s[i + MARK.len()..]),
                None => s.clone(),
            };
            rest.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect();
    (!names.is_empty()).then(|| format!("Also ongoing (KTLO, not initiatives): {}", names.join(", ")))
}

/// The owners tile's figure: "2 / 7".
pub fn owners_text((set, total): (usize, usize)) -> String {
    format!("{set} / {total}")
}

pub fn show(app: &mut App, ui: &mut Ui) {
    show_at(app, ui, chrono::Local::now().date_naive());
}

/// `show` as of `today` (the shipped list's year).
pub fn show_at(app: &mut App, ui: &mut Ui, today: NaiveDate) {
    let p = theme::palette(ui.ctx());
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            let tab = app.roadmap.tab;
            tab_strip(ui, p, tab, &mut acts);
            ui.add_space(GAP);
            let (items, ongoing) = app.roadmap_items();
            let rows = tab_rows(tab, &items);
            let cursor = app.roadmap.cursor.min(rows.len().saturating_sub(1));
            let last_id = egui::Id::new("roadmap-last-cursor");
            let moved = ui.data(|d| d.get_temp::<(u8, usize)>(last_id)) != Some((tab, cursor));
            ui.data_mut(|d| d.insert_temp(last_id, (tab, cursor)));
            let loading = app.lists.get(&crate::store::ListId::Epics).is_some_and(|l| l.loading);
            let focus = Focus { cursor, moved, count: rows.len() };
            egui::ScrollArea::vertical().id_salt(("roadmap-tab", tab)).auto_shrink([false, false]).show(ui, |ui| {
                if items.is_empty() {
                    if loading {
                        crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                    } else {
                        crate::views::empty_state(ui, p, "No roadmap initiatives yet");
                    }
                    return;
                }
                match tab {
                    1 => now_next_later(ui, p, &items, &ongoing, today.year(), focus, &mut acts),
                    3 => status_review(ui, p, &items, focus, &mut acts),
                    2 => crate::views::empty_state(ui, p, "Timeline (coming in the next task)"),
                    _ => crate::views::empty_state(ui, p, "Outcomes (coming in the next task)"),
                }
            });
        });
    app.actions.extend(acts);
}

/// Where the cursor is on the tab being drawn, and whether it moved since the last frame (so it scrolls into view).
#[derive(Clone, Copy)]
struct Focus {
    cursor: usize,
    moved: bool,
    count: usize,
}

impl Focus {
    /// The actions a click on row `i` asks for: move there, and open it on a double click.
    fn click(&self, i: usize, resp: &egui::Response, acts: &mut Vec<Action>) {
        if resp.clicked() || resp.double_clicked() {
            acts.push(Action::RoadmapMove(i as i32 - self.cursor as i32));
        }
        if resp.double_clicked() {
            acts.push(Action::RoadmapOpen);
        }
        if i == self.cursor && self.moved && self.count > 0 {
            resp.scroll_to_me(None);
        }
    }
}

/// The numbered tabs, the current one in `text` over a 2 px accent underline on the strip's 1 px rule.
fn tab_strip(ui: &mut Ui, p: &Palette, tab: u8, acts: &mut Vec<Action>) {
    let mut under = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = TAB_GAP;
        for (i, t) in TABS.iter().enumerate() {
            let n = i as u8 + 1;
            let on = n == tab;
            let label = egui::Label::new(RichText::new(*t).font(theme::body(14.0)).color(if on { p.text } else { p.muted })).sense(Sense::click());
            let r = ui.add(label).on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.clicked() {
                acts.push(Action::RoadmapTab(n));
            }
            if on {
                under = Some(r.rect.x_range());
            }
        }
    });
    ui.add_space(CARD_GAP);
    let y = ui.cursor().min.y;
    let full = ui.max_rect().x_range();
    ui.painter().rect_filled(Rect::from_x_y_ranges(full, y..=y + 1.0), 0.0, p.border);
    if let Some(x) = under {
        ui.painter().rect_filled(Rect::from_x_y_ranges(x, y - 1.0..=y + 1.0), 0.0, p.accent);
    }
    ui.add_space(1.0);
}

/// A health dot: filled in the health's colour, a hollow `no_health` ring when none is set.
fn health_dot(ui: &mut Ui, h: Health, p: &Palette) {
    if h == Health::None {
        let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
        ui.painter().circle_stroke(rect.center(), 3.75, Stroke::new(1.5, p.no_health));
    } else {
        crate::views::health_dot(ui, health_color(h, p));
    }
}

/// A missing value: small `off_track` text, so the gap reads as one.
fn missing(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(p.off_track));
}

/// The P0 chip: `off_track` outline and text on no fill.
fn p0_chip(ui: &mut Ui, p: &Palette) {
    pill_sized(ui, "P0", Color32::TRANSPARENT, p.off_track, Stroke::new(1.0, p.off_track), PILL_MARGIN);
}

/// A 6 px bar of done (`on_track`), doing (`accent`) and to-do (`surface_2`) children, then "done / total".
fn progress_bar(ui: &mut Ui, p: &Palette, pr: &Progress) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = CARD_GAP;
        let text = ui.painter().layout_no_wrap(progress_text(pr), theme::body(12.0), p.muted);
        let w = (ui.available_width() - text.size().x - CARD_GAP).max(24.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, text.size().y), Sense::hover());
        let bar = Rect::from_center_size(rect.center(), vec2(w, BAR_H));
        let painter = ui.painter();
        painter.rect_filled(bar, BAR_H / 2.0, p.surface_2);
        let total = (pr.todo + pr.doing + pr.done) as f32;
        if total > 0.0 {
            let done = bar.width() * pr.done as f32 / total;
            let doing = bar.width() * pr.doing as f32 / total;
            let clip = painter.with_clip_rect(bar);
            // Segments drawn as one rounded bar each from the left edge, so their joins stay square.
            clip.rect_filled(Rect::from_min_size(bar.min, vec2(done + doing, BAR_H)), BAR_H / 2.0, p.accent);
            clip.rect_filled(Rect::from_min_size(bar.min, vec2(done, BAR_H)), BAR_H / 2.0, p.on_track);
        }
        ui.label(text);
    });
}

#[allow(clippy::too_many_arguments)]
fn now_next_later(ui: &mut Ui, p: &Palette, items: &[Initiative], ongoing: &[String], year: i32, focus: Focus, acts: &mut Vec<Action>) {
    let cols = roadmap::now_next_later(items);
    let mut first = 0;
    ui.spacing_mut().item_spacing.x = GAP;
    ui.columns(3, |uis| {
        for (ci, ((ui, col), (title, sub))) in uis.iter_mut().zip(&cols).zip(COLUMNS).enumerate() {
            let start = first;
            first += col.len();
            Frame::new().fill(p.surface_2).corner_radius(theme::RADIUS).inner_margin(Margin::same(PAD)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).font(theme::bold(15.0)).color(p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(col.len().to_string()).size(12.0).color(p.muted));
                    });
                });
                ui.add_space(LINE_GAP);
                ui.add(egui::Label::new(RichText::new(sub).size(12.0).color(p.muted)).wrap());
                ui.add_space(PAD as f32);
                ui.spacing_mut().item_spacing.y = CARD_GAP;
                if col.is_empty() {
                    ui.label(RichText::new("None").size(12.0).color(p.muted));
                }
                for (k, i) in col.iter().enumerate() {
                    let at = start + k;
                    let resp = card(ui, p, i, ci == 0, at == focus.cursor, at);
                    focus.click(at, &resp, acts);
                }
                if ci == 0
                    && let Some(line) = ktlo_line(ongoing)
                {
                    ui.add_space(PAD as f32 - CARD_GAP);
                    ui.add(egui::Label::new(RichText::new(line).size(12.0).color(p.muted)).wrap());
                }
            });
        }
    });
    ui.add_space(GAP);
    shipped(ui, p, items, year);
}

/// A card: (Now) health dot and name, the P0, lane and up to two metric chips, (Now) the latest update, then the
/// progress bar.
fn card(ui: &mut Ui, p: &Palette, i: &Initiative, now: bool, sel: bool, at: usize) -> egui::Response {
    let id = egui::Id::new(("roadmap-card", at));
    let bg = ui.painter().add(egui::Shape::Noop);
    // The chips sit one step off the card's fill, which follows last frame's hover.
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let fill = crate::views::issues::row_fill(sel, hovered, p);
    let r = Frame::new().stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::same(PAD)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = LINE_GAP;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            if now {
                let line = ui.fonts_mut(|f| f.row_height(&theme::bold(13.5)));
                let dot = ui.vertical(|ui| {
                    ui.add_space(((line - 9.0) / 2.0).max(0.0));
                    health_dot(ui, i.health, p);
                });
                crate::views::tooltip(dot.response, health_label(i.health));
            }
            ui.add(egui::Label::new(RichText::new(&i.name).font(theme::bold(13.5)).color(p.text)).wrap());
        });
        chips(ui, p, i, fill);
        if now {
            match &i.latest_update {
                Some((_, _, text)) if !text.is_empty() => {
                    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.muted)).wrap());
                }
                _ => missing(ui, p, "no update yet"),
            }
        }
        progress_bar(ui, p, &i.progress);
    });
    let rect = r.response.rect;
    let resp = ui.interact(rect, id, Sense::click());
    ui.painter().set(bg, egui::epaint::RectShape::filled(rect, theme::RADIUS, fill));
    resp
}

/// The P0 chip, the lane, and up to two metrics, in chips one step off `on`.
fn chips(ui: &mut Ui, p: &Palette, i: &Initiative, on: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(LINE_GAP, LINE_GAP);
        if i.priority.as_deref() == Some("P0") {
            p0_chip(ui, p);
        }
        let fill = theme::chip_fill_on(on, p);
        for t in std::iter::once(&i.lane).chain(i.metrics.iter().take(2)) {
            crate::views::pill(ui, t, fill, p.muted);
        }
    });
}

/// "Shipped in {year}": the Done initiatives, two columns of name and target quarter.
fn shipped(ui: &mut Ui, p: &Palette, items: &[Initiative], year: i32) {
    let done = shipped_in(items, year);
    Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::symmetric(16, PAD)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(shipped_title(year)).font(theme::bold(12.0)).color(p.muted));
        ui.add_space(CARD_GAP - ui.spacing().item_spacing.y);
        if done.is_empty() {
            ui.label(RichText::new("Nothing shipped yet").size(12.0).color(p.muted));
            return;
        }
        let half = done.len().div_ceil(2);
        ui.spacing_mut().item_spacing.x = 32.0;
        ui.columns(2, |uis| {
            for (ui, part) in uis.iter_mut().zip(done.chunks(half)) {
                ui.spacing_mut().item_spacing.y = LINE_GAP;
                for i in part {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = CARD_GAP;
                        ui.label(RichText::new(&i.name).font(theme::bold(13.5)).color(p.text));
                        match i.target {
                            Some(t) => {
                                ui.label(RichText::new(format!("Q{}", t.q)).size(12.0).color(p.muted));
                            }
                            None => missing(ui, p, "target not set"),
                        }
                    });
                }
            }
        });
    });
}

fn status_review(ui: &mut Ui, p: &Palette, items: &[Initiative], focus: Focus, acts: &mut Vec<Action>) {
    let rows = roadmap::status_rows(items);
    let counts = roadmap::health_counts(&rows);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(PAD as f32, PAD as f32);
        for (h, n) in [Health::OffTrack, Health::AtRisk, Health::OnTrack, Health::None].into_iter().zip(counts) {
            tile(ui, p, &n.to_string(), |ui| {
                health_dot(ui, h, p);
                ui.label(RichText::new(health_label(h)).size(12.0).color(p.muted));
            });
        }
        tile(ui, p, &owners_text(roadmap::owners_set(&rows)), |ui| {
            ui.label(RichText::new("owners set").size(12.0).color(p.muted));
        });
    });
    ui.add_space(GAP);
    if rows.is_empty() {
        crate::views::empty_state(ui, p, "Nothing in Now");
        return;
    }
    egui::ScrollArea::horizontal().id_salt("roadmap-status").show(ui, |ui| table(ui, p, &rows, focus, acts));
}

/// A summary tile: the figure (bold 22) over its label line.
fn tile(ui: &mut Ui, p: &Palette, figure: &str, label: impl FnOnce(&mut Ui)) {
    Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::symmetric(16, PAD)).show(ui, |ui| {
        ui.set_min_width(120.0);
        ui.spacing_mut().item_spacing.y = LINE_GAP;
        ui.vertical(|ui| {
            ui.label(RichText::new(figure).font(theme::bold(22.0)).color(p.text));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                label(ui);
            });
        });
    });
}

/// The status table: a striped grid, the header on `surface_2` and the cursor row on `selected_row`.
fn table(ui: &mut Ui, p: &Palette, rows: &[&Initiative], focus: Focus, acts: &mut Vec<Action>) {
    let total = ui.available_width().max(TABLE_MIN_W);
    let widths = SHARES.map(|s| (total * s).floor());
    let (head, sel, stripe) = (p.surface_2, p.selected_row, p.bg);
    let cursor = focus.cursor;
    let mut rects: Vec<Rect> = vec![Rect::NOTHING; rows.len()];
    Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).show(ui, |ui| {
        ui.set_width(total);
        egui::Grid::new("roadmap-status-grid")
            .num_columns(HEADS.len())
            .spacing([0.0, 0.0])
            .min_row_height(0.0)
            .with_row_color(move |row, _| match row {
                0 => Some(head),
                r if r - 1 == cursor => Some(sel),
                r if r % 2 == 0 => Some(stripe),
                _ => None,
            })
            .show(ui, |ui| {
                for (c, h) in HEADS.iter().enumerate() {
                    cell(ui, widths[c], c, |ui| {
                        ui.label(RichText::new(*h).font(theme::bold(12.0)).color(p.muted));
                    });
                }
                ui.end_row();
                for (ri, i) in rows.iter().enumerate() {
                    let mut row = Rect::NOTHING;
                    let mut add = |r: Rect| row = row.union(r);
                    add(cell(ui, widths[0], 0, |ui| initiative_cell(ui, p, i)));
                    add(cell(ui, widths[1], 1, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            health_dot(ui, i.health, p);
                            ui.label(RichText::new(health_label(i.health)).color(p.text));
                        });
                    }));
                    add(cell(ui, widths[2], 2, |ui| text_or_missing(ui, p, i.confidence.as_deref(), 14.0, "not set")));
                    add(cell(ui, widths[3], 3, |ui| match &i.latest_update {
                        Some((author, date, text)) if !text.is_empty() => {
                            ui.add(egui::Label::new(RichText::new(text).color(p.text)).wrap());
                            ui.label(RichText::new(format!("{author} · {date}")).size(12.0).color(p.muted));
                        }
                        _ => missing(ui, p, "no update yet"),
                    }));
                    add(cell(ui, widths[4], 4, |ui| text_or_missing(ui, p, i.next_milestone.as_deref(), 14.0, "not set")));
                    add(cell(ui, widths[5], 5, |ui| text_or_missing(ui, p, i.definition_of_done.as_deref(), 12.0, "not defined")));
                    add(cell(ui, widths[6], 6, |ui| progress_bar(ui, p, &i.progress)));
                    ui.end_row();
                    rects[ri] = row;
                }
            });
    });
    for (ri, r) in rects.into_iter().enumerate() {
        let resp = ui.interact(r, egui::Id::new(("roadmap-status-row", ri)), Sense::click());
        focus.click(ri, &resp, acts);
    }
}

/// One table cell `w` wide: 12 in from its left (and the last column's right), 8 above and below.
fn cell(ui: &mut Ui, w: f32, col: usize, content: impl FnOnce(&mut Ui)) -> Rect {
    let right = if col == HEADS.len() - 1 { PAD } else { 0 };
    let r = Frame::new().inner_margin(Margin { left: PAD, right, top: 8, bottom: 8 }).show(ui, |ui| {
        let inner = w - PAD as f32 - right as f32;
        ui.set_width(inner);
        ui.spacing_mut().item_spacing.y = LINE_GAP;
        ui.vertical(|ui| {
            ui.set_width(inner);
            content(ui);
        });
    });
    r.response.rect
}

/// Name (bold) and P0 chip, then "lane · key · owner Name", the owner red "missing" when unset.
fn initiative_cell(ui: &mut Ui, p: &Palette, i: &Initiative) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = CARD_GAP;
        ui.label(RichText::new(&i.name).font(theme::bold(13.5)).color(p.text));
        if i.priority.as_deref() == Some("P0") {
            p0_chip(ui, p);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let small = |t: String| RichText::new(t).size(12.0).color(p.muted);
        ui.label(small(format!("{} · ", i.lane)));
        ui.label(RichText::new(&i.jira_key).font(theme::mono(12.5)).color(p.muted));
        ui.label(small(" · owner ".into()));
        match &i.owner {
            Some(o) => {
                ui.label(small(o.clone()));
            }
            None => missing(ui, p, "missing"),
        }
    });
}

fn text_or_missing(ui: &mut Ui, p: &Palette, text: Option<&str>, size: f32, gap: &str) {
    match text {
        Some(t) => {
            ui.add(egui::Label::new(RichText::new(t).size(size).color(p.text)).wrap());
        }
        None => missing(ui, p, gap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status};

    fn epic(key: &str, summary: &str, cat: &str, labels: &[&str]) -> Issue {
        Issue {
            key: key.into(),
            summary: summary.into(),
            status: Status { name: "x".into(), category: cat.into() },
            labels: labels.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }

    fn app() -> App {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        app.screen = crate::app::Screen::Roadmap;
        app.store.absorb(
            crate::store::ListId::Epics,
            vec![
                epic("CI-1", "Canary Deployments", "indeterminate", &["roadmap", "P0", "health:at-risk", "metric:Lead Time"]),
                epic("CI-2", "Cluster Upgrades", "indeterminate", &["roadmap"]),
                epic("CI-3", "Autoscaling everywhere", "new", &["roadmap", "horizon:next"]),
                epic("CI-4", "Ideas box", "new", &["roadmap"]),
                epic("CI-5", "Cost Insights", "done", &["roadmap", "target:2026-Q3"]),
                epic("CI-6", "Audit (ongoing)", "indeterminate", &[]),
            ],
        );
        app
    }

    fn texts(app: &mut App) -> Vec<String> {
        let today = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let shapes = crate::testutil::paint_at2(egui::vec2(1440.0, 900.0), |ui| show_at(app, ui, today));
        crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect()
    }

    fn has(drawn: &[String], want: &str) {
        assert!(drawn.iter().any(|t| t == want), "{want:?} in {drawn:?}");
    }

    #[test]
    fn wording() {
        assert_eq!(TABS, ["1 Now / Next / Later", "2 Timeline", "3 Status review", "4 Outcomes"]);
        assert_eq!(progress_text(&Progress { todo: 2, doing: 1, done: 3 }), "3 / 6");
        assert_eq!(shipped_title(2026), "Shipped in 2026");
        assert_eq!(owners_text((0, 7)), "0 / 7");
        assert_eq!(health_label(Health::None), "No health set");
        assert_eq!(ktlo_line(&[]), None);
        // "İ" lowercases to three bytes from two: an index into the lowercased text would cut a char.
        assert_eq!(ktlo_line(&["Türkiye İzleme (ongoing)".into()]).as_deref(), Some("Also ongoing (KTLO, not initiatives): Türkiye İzleme"));
        assert_eq!(ktlo_line(&["İİİİ (ONGOING) x".into()]).as_deref(), Some("Also ongoing (KTLO, not initiatives): İİİİ x"));
        assert_eq!(
            ktlo_line(&["Audit (ongoing)".into(), "Support/Enablement (Ongoing) work".into()]).as_deref(),
            Some("Also ongoing (KTLO, not initiatives): Audit, Support/Enablement work")
        );
    }

    #[test]
    fn tab_rows_walk_the_columns_then_the_table() {
        let app = app();
        let (items, _) = app.roadmap_items();
        let keys = |v: Vec<&Initiative>| v.iter().map(|i| i.key.clone()).collect::<Vec<_>>();
        assert_eq!(keys(tab_rows(1, &items)), ["CI-1", "CI-2", "CI-3", "CI-4"], "Now (P0 first), Next, Later; Done is not a row");
        assert_eq!(keys(tab_rows(3, &items)), ["CI-1", "CI-2"], "Now only, worst health first");
        assert!(tab_rows(2, &items).is_empty() && tab_rows(4, &items).is_empty());
    }

    #[test]
    fn shipped_keeps_this_year_and_flags_no_target() {
        let mut app = app();
        let mut old = epic("CI-7", "Old win", "done", &["roadmap", "target:2025-Q4"]);
        old.summary = "Old win".into();
        let undated = epic("CI-8", "Undated win", "done", &["roadmap"]);
        let mut all = app.store.list(crate::store::ListId::Epics).into_iter().cloned().collect::<Vec<_>>();
        all.extend([old, undated]);
        app.store.absorb(crate::store::ListId::Epics, all);
        let (items, _) = app.roadmap_items();
        assert_eq!(shipped_in(&items, 2026).iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["CI-5", "CI-8"]);
    }

    #[test]
    fn now_next_later_draws_headless() {
        let mut app = app();
        let drawn = texts(&mut app);
        for want in [
            "1 Now / Next / Later",
            "4 Outcomes",
            "Now",
            "In progress this quarter",
            "Committed, starts when capacity frees up",
            "Ideas and parked work, not committed",
            "Canary Deployments",
            "P0",
            "No lane",
            "Lead Time",
            "no update yet",
            "0 / 0",
            "Autoscaling everywhere",
            "Also ongoing (KTLO, not initiatives): Audit",
            "Shipped in 2026",
            "Cost Insights",
            "Q3",
        ] {
            has(&drawn, want);
        }
        assert_eq!(drawn.iter().filter(|t| *t == "no update yet").count(), 2, "the update line is drawn in Now only");
    }

    #[test]
    fn status_review_draws_headless() {
        let mut app = app();
        app.roadmap.tab = 3;
        let drawn = texts(&mut app);
        for want in [
            "Initiative",
            "Definition of done",
            "Progress",
            "Canary Deployments",
            "Cluster Upgrades",
            "CI-1",
            "missing",
            "At risk",
            "No health set",
            "not set",
            "not defined",
            "owners set",
            "0 / 2",
            "Off track",
        ] {
            has(&drawn, want);
        }
        assert!(!drawn.iter().any(|t| t == "Autoscaling everywhere"), "Next items stay off the status review");
        app.roadmap.tab = 2;
        has(&texts(&mut app), "Timeline (coming in the next task)");
    }
}
