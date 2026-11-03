//! The Roadmap screen: a tab strip over four views of the initiatives `roadmap::derive` builds from the epics.
//! Tab 1 is Now / Next / Later with the shipped list, tab 3 the status review table; tab 2 (the timeline) and tab 4
//! (outcomes) live in the child modules.

mod outcomes;
mod timeline;

use crate::app::{Action, App};
use crate::roadmap::{self, Health, Initiative, Progress};
use crate::theme::{self, Palette};
use crate::views::components as c;
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
const SHARES: [f32; 7] = [0.20, 0.11, 0.10, 0.19, 0.14, 0.14, 0.12];
/// Under this the table scrolls sideways.
const TABLE_MIN_W: f32 = 900.0;
/// Between columns and tiles, and under the tab strip and each section.
const GAP: f32 = 16.0;
/// Between cards, and inside a column or the shipped box.
const CARD_GAP: f32 = 8.0;
const PAD: i8 = 12;
/// Between a card's lines and its chips.
const LINE_GAP: f32 = 4.0;
/// The progress bar's height, and its fully rounded ends.
const BAR_H: f32 = 6.0;
const BAR_CORNERS: egui::CornerRadius = egui::CornerRadius::same(3);
/// The widest count a progress bar's label column is sized for, so every track ends on one edge.
const PROGRESS_TEMPLATE: &str = "0000 / 0000";
/// The P0 marker's words and its tooltip: one neutral pill on cards, table rows and timeline bars.
pub const P0_LABEL: &str = "P0";
pub const P0_TIP: &str = "Top priority: the epic has the P0 label";
/// What the progress bar's segments mean.
pub const PROGRESS_TIP: &str = "Solid green: done. Striped blue: in progress. Grey: to do.";
/// The status review's scope line, under its "Now" header.
pub const STATUS_SCOPE: &str = "Reviews the Now initiatives only, the ones in progress this quarter. Next and Later are on tab 1.";
/// The one phrase for a missing value anywhere on the Roadmap (a table cell shows EMPTY_CELL and says this on hover).
pub const NOT_SET: &str = "Not set";
/// An empty status-table cell: a quiet dash, so the real values stand out.
pub const EMPTY_CELL: &str = "—";
/// Under the status tiles while any row has a gap: how each column gets filled. `code` spans are literal input.
pub const STATUS_HINT: &str = "Health comes from a `health:on-track`, `health:at-risk` or `health:off-track` label on the epic, and the owner \
     is its assignee. Map the other columns in the `[roadmap]` section of `config.toml`.";
/// A legend swatch's box.
const SWATCH: f32 = 12.0;
/// Between a legend swatch and its word.
const SWATCH_GAP: f32 = 6.0;

/// A legend entry: what one mark on the Roadmap's bars and cards means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// The progress bar's green, blue and grey segments.
    Done,
    Doing,
    Todo,
    /// The hollow health ring, and a filled health dot.
    NoHealth,
    Health,
    /// The P0 pill.
    P0,
    /// The timeline's "runs past the year" chevron and today line.
    RunsOn,
    Today,
}

impl Mark {
    pub fn label(self) -> &'static str {
        match self {
            Mark::Done => "Done",
            Mark::Doing => "In progress",
            Mark::Todo => "To do",
            Mark::NoHealth => "No health set",
            Mark::Health => "Health set",
            Mark::P0 => "Top priority",
            Mark::RunsOn => "Runs past the year",
            Mark::Today => "Today",
        }
    }
}

/// The one legend for tabs 1 to 3, in one order and wording: the progress segments, the health glyphs the shown
/// items use, the P0 pill when one shows, then the timeline's own chevron and today line. None on Outcomes.
pub fn legend_marks(tab: u8, items: &[Initiative], year: i32) -> Vec<Mark> {
    let shown: Vec<&Initiative> = match tab {
        1 => roadmap::now_next_later(items).into_iter().flatten().collect(),
        3 => roadmap::status_rows(items),
        2 => timeline::bars(items, year),
        _ => return Vec::new(),
    };
    let mut marks = Vec::new();
    if !shown.is_empty() {
        marks.extend([Mark::Done, Mark::Doing, Mark::Todo]);
    }
    // Done timeline bars carry no health glyph.
    let healthy: Vec<&&Initiative> = shown.iter().filter(|i| tab != 2 || i.horizon != roadmap::Horizon::Done).collect();
    if healthy.iter().any(|i| i.health == Health::None) {
        marks.push(Mark::NoHealth);
    }
    if healthy.iter().any(|i| i.health != Health::None) {
        marks.push(Mark::Health);
    }
    if shown.iter().any(|i| is_p0(i)) {
        marks.push(Mark::P0);
    }
    if tab == 2 {
        if shown.iter().any(|i| timeline::continues(i, year) != (false, false)) {
            marks.push(Mark::RunsOn);
        }
        marks.push(Mark::Today);
    }
    marks
}

/// The status table's columns that carry a value on at least one row, in HEADS order. Initiative and Progress
/// always show; a column empty on every row is left out (and named by `hidden_note`), so the table is not mostly
/// dashes.
pub fn shown_columns(rows: &[&Initiative]) -> Vec<usize> {
    let any = |f: &dyn Fn(&Initiative) -> bool| rows.iter().any(|i| f(i));
    let has = [
        true,
        any(&|i| i.health != Health::None),
        any(&|i| i.confidence.is_some()),
        any(&|i| i.latest_update.as_ref().is_some_and(|u| !u.2.is_empty())),
        any(&|i| i.next_milestone.is_some()),
        any(&|i| i.definition_of_done.is_some()),
        true,
    ];
    (0..HEADS.len()).filter(|&c| has[c]).collect()
}

/// The one line that names the columns `shown_columns` left out: "No initiative has health, confidence or
/// next milestone set yet." None when every column shows.
pub fn hidden_note(shown: &[usize]) -> Option<String> {
    let names: Vec<String> = (0..HEADS.len()).filter(|c| !shown.contains(c)).map(|c| HEADS[c].to_lowercase()).collect();
    let list = match names.as_slice() {
        [] => return None,
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    };
    Some(format!("No initiative has {list} set yet, so the table leaves those columns out."))
}

/// Whether the status review has a gap the hint explains: any row missing health, owner, confidence, milestone or
/// definition of done.
pub fn status_has_gaps(rows: &[&Initiative]) -> bool {
    rows.iter().any(|i| i.health == Health::None || i.owner.is_none() || i.confidence.is_none() || i.next_milestone.is_none() || i.definition_of_done.is_none())
}

/// Each column's empty state: what is missing, then how an initiative gets there.
pub const COLUMN_EMPTY: [(&str, &str); 3] = [
    ("Nothing in progress", "An epic with the `roadmap` label shows here once it is in progress."),
    ("Nothing committed next", "Add the `horizon:next` label to an epic to commit it here."),
    ("Nothing parked for later", "Epics with the `roadmap` label that have not started land here."),
];

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

/// A health glyph's colour (a mark, so at risk takes `at_risk_mark`).
fn health_color(h: Health, p: &Palette) -> Color32 {
    match h {
        Health::OffTrack => p.off_track,
        Health::AtRisk => p.at_risk_mark(),
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

/// Whether lanes tell initiatives apart: false when every one has the same lane, so the lane chip is noise.
pub fn lanes_vary(items: &[Initiative]) -> bool {
    items.windows(2).any(|w| w[0].lane != w[1].lane)
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
    (!names.is_empty()).then(|| format!("Also ongoing (keep-the-lights-on work, not initiatives): {}", names.join(", ")))
}

/// The owners tile's figure: "2 / 7".
pub fn owners_text((set, total): (usize, usize)) -> String {
    format!("{set} / {total}")
}

pub fn show(app: &mut App, ui: &mut Ui) {
    show_at(app, ui, chrono::Local::now().date_naive());
}

/// `show` as of `today` (the shipped list's and the timeline's year, and the today line).
pub fn show_at(app: &mut App, ui: &mut Ui, today: NaiveDate) {
    let p = theme::palette(ui.ctx());
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            let tab = app.roadmap.tab;
            let (items, ongoing) = app.roadmap_items();
            let marks = if items.is_empty() { Vec::new() } else { legend_marks(tab, &items, today.year()) };
            ui.horizontal(|ui| {
                tab_strip(ui, p, tab, &mut acts);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| legend(ui, p, &marks));
            });
            ui.add_space(GAP);
            let rows = tab_rows(tab, &items);
            let cursor = app.roadmap.cursor.min(rows.len().saturating_sub(1));
            let last_id = egui::Id::new("roadmap-last-cursor");
            let moved = ui.data(|d| d.get_temp::<(u8, usize)>(last_id)) != Some((tab, cursor));
            ui.data_mut(|d| d.insert_temp(last_id, (tab, cursor)));
            let loading = roadmap_loading(app.client.is_some(), app.lists.get(&crate::store::ListId::TeamOpen), app.lists.get(&crate::store::ListId::Epics));
            let focus = Focus { cursor, moved, count: rows.len() };
            // The focused initiative's epic, for its Move to Done action (the one `D` acts on): tabs 1 and 3 only.
            let done_issue = app.done_target().and_then(|k| app.store.get(&k).cloned()).filter(|i| done_shown(Some(i)));
            crate::views::no_scroll_fade(ui);
            let out = egui::ScrollArea::vertical().id_salt(("roadmap-tab", tab)).auto_shrink([false, false]).show(ui, |ui| {
                if items.is_empty() {
                    if loading {
                        crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                    } else {
                        empty_state(ui, p, "No roadmap initiatives yet", "", p.bg);
                    }
                    return;
                }
                match tab {
                    1 => now_next_later(ui, p, &items, &ongoing, today.year(), focus, done_issue.as_ref(), &mut acts),
                    3 => status_review(ui, p, &items, focus, done_issue.as_ref(), &mut acts),
                    2 => timeline::show(ui, p, &items, today),
                    _ => outcomes::show(ui, p, &items),
                }
            });
            crate::views::components::scroll_edge_fade(ui, &out, p.bg);
        });
    app.actions.extend(acts);
}

/// `text` with its `code` spans (between backticks) in the mono code style on a chip `container` steps off, the
/// rest in `color`: label names and config keys read as literal input, not prose.
pub fn code_job(text: &str, size: f32, body: f32, color: Color32, container: Color32, p: &Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (n, part) in text.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        let fmt = if n % 2 == 1 {
            egui::TextFormat {
                font_id: theme::mono(crate::views::adf_view::code_size(size, body)),
                color: p.text,
                background: theme::chip_fill_on(container, p),
                valign: egui::Align::Center,
                ..Default::default()
            }
        } else {
            egui::TextFormat { font_id: theme::body(size), color, valign: egui::Align::Center, ..Default::default() }
        };
        job.append(part, 0.0, fmt);
    }
    job
}

/// `text` as drawn: its backticks gone.
#[cfg(test)]
pub fn plain(text: &str) -> String {
    text.replace('`', "")
}

/// A hint at `small` size in `color` with code spans, wrapped at the density's prose measure.
fn code_hint(ui: &mut Ui, p: &Palette, text: &str, color: Color32, container: Color32) {
    let sp = theme::spacing(ui.ctx());
    let job = code_job(text, sp.small, sp.body, color, container, p);
    ui.scope(|ui| {
        ui.set_max_width(sp.prose_max_width.min(ui.available_width()));
        ui.add(egui::Label::new(job).wrap());
    });
}

/// The Roadmap's empty state: `components::empty_state`'s look (the message in `text` at body, the hint in `muted`
/// at `small`, `line_gap` apart), but the hint may carry code spans on `container`. No hint when `hint` is empty.
fn empty_state(ui: &mut Ui, p: &Palette, message: &str, hint: &str, container: Color32) {
    if !hint.contains('`') {
        return c::empty_state(ui, p, message, (!hint.is_empty()).then_some(hint));
    }
    let sp = theme::spacing(ui.ctx());
    ui.label(RichText::new(message).color(p.text).size(sp.body));
    ui.add_space(sp.line_gap - ui.spacing().item_spacing.y);
    code_hint(ui, p, hint, theme::muted_on(container, p), container);
}

/// The column panel (tab 1's columns, the Outcomes empty state): `surface_2`, RADIUS, PAD inside.
fn panel<R>(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new().fill(p.surface_2).corner_radius(theme::RADIUS).inner_margin(Margin::same(PAD)).show(ui, add).inner
}

/// Whether an empty Roadmap is still waiting on its data rather than truly empty: Epics load after TeamOpen lands,
/// so either in flight counts, and so does a cold start (with a client) before Epics has ever landed, unless one
/// of them failed.
fn roadmap_loading(client_present: bool, team_open: Option<&crate::app::Loaded>, epics: Option<&crate::app::Loaded>) -> bool {
    let busy = |l: Option<&crate::app::Loaded>| l.is_some_and(|l| l.loading);
    let failed = |l: Option<&crate::app::Loaded>| l.is_some_and(|l| l.error.is_some());
    let never_fetched = epics.is_none_or(|l| l.fetched.is_none());
    busy(team_open) || busy(epics) || (client_present && never_fetched && !failed(team_open) && !failed(epics))
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

/// The numbered views as the shared segmented switcher.
fn tab_strip(ui: &mut Ui, p: &Palette, tab: u8, acts: &mut Vec<Action>) {
    if let Some(i) = c::segmented(ui, p, &TABS, usize::from(tab.saturating_sub(1))) {
        acts.push(Action::RoadmapTab(i as u8 + 1));
    }
}

/// A health dot: filled in the health's colour, a hollow `no_health` ring when none is set.
fn health_dot(ui: &mut Ui, h: Health, p: &Palette) {
    if h == Health::None {
        let (rect, _) = ui.allocate_exact_size(vec2(HEALTH_DOT, HEALTH_DOT), Sense::hover());
        paint_health(ui.painter(), rect.center(), h, p);
    } else {
        crate::views::health_dot(ui, health_color(h, p));
    }
}

/// The health glyph then `text`, top-aligned like a plain label: the glyph centred on the first line of `font`, so a
/// cell or tile with a glyph shares its row's baseline.
fn health_line(ui: &mut Ui, h: Health, p: &Palette, font: &egui::FontId, text: impl FnOnce(&mut Ui)) -> egui::Response {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = SWATCH_GAP;
        let line = ui.fonts_mut(|f| f.row_height(font));
        let dot = ui.vertical(|ui| {
            ui.add_space(((line - HEALTH_DOT) / 2.0).max(0.0));
            health_dot(ui, h, p);
        });
        crate::views::tooltip(dot.response, health_label(h));
        text(ui);
    })
    .response
}

/// The health glyph's box.
const HEALTH_DOT: f32 = 9.0;

/// The roadmap's one health glyph centred on `c`: the hollow `no_health` ring, or a dot in the health's colour.
fn paint_health(painter: &egui::Painter, c: egui::Pos2, h: Health, p: &Palette) {
    if h == Health::None {
        painter.circle_stroke(c, 3.75, Stroke::new(1.5, p.no_health));
    } else {
        painter.circle_filled(c, 4.0, health_color(h, p));
    }
}

/// The one legend line (every tab that has marks): swatch and `muted` caption word per entry, GAP apart, laid out
/// in the caller's (right-to-left) layout so the line ends on the tab row's right edge.
fn legend(ui: &mut Ui, p: &Palette, marks: &[Mark]) {
    let size = theme::spacing(ui.ctx()).caption;
    ui.spacing_mut().item_spacing.x = SWATCH_GAP;
    for (n, &m) in marks.iter().rev().enumerate() {
        if n > 0 {
            ui.add_space(GAP - SWATCH_GAP);
        }
        ui.label(RichText::new(m.label()).size(size).color(p.muted));
        if m == Mark::P0 {
            p0_chip(ui, p);
            continue;
        }
        let (r, _) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::hover());
        paint_mark(ui.painter(), r, m, p);
    }
}

/// A legend swatch drawn in `r`, with the same colours and shapes as the mark it explains.
fn paint_mark(painter: &egui::Painter, r: Rect, m: Mark, p: &Palette) {
    let seg = Rect::from_center_size(r.center(), vec2(r.width(), BAR_H));
    match m {
        Mark::Done => {
            painter.rect_filled(seg, BAR_H / 2.0, p.on_track);
        }
        Mark::Doing => paint_doing(painter, seg, BAR_CORNERS, egui::Rangef::new(seg.left() + BAR_H / 2.0, seg.right() - BAR_H / 2.0), p),
        Mark::Todo => {
            painter.rect_filled(seg, BAR_H / 2.0, track(p));
        }
        Mark::NoHealth => paint_health(painter, r.center(), Health::None, p),
        Mark::Health => paint_health(painter, r.center(), Health::OnTrack, p),
        // Drawn as the pill itself by `legend`.
        Mark::P0 => {}
        Mark::RunsOn => timeline::chevron(painter, r.center() + vec2(2.0, 0.0), true, p),
        Mark::Today => {
            painter.rect_filled(Rect::from_center_size(r.center(), vec2(2.0, r.height())), 0.0, p.accent);
        }
    }
}

/// A missing value: the shared placeholder role, never the danger colour.
fn missing(ui: &mut Ui, p: &Palette, text: &str) -> egui::Response {
    c::missing_value(ui, p, text)
}

/// The one P0 marker (cards, table rows, timeline bars): a neutral pill with the tooltip that says what it means.
fn p0_chip(ui: &mut Ui, p: &Palette) -> egui::Response {
    crate::views::tooltip(c::neutral_pill(ui, p, P0_LABEL), P0_TIP)
}

/// The P0 pill inline before a title in `font`: centred on the title's first line, so every surface puts it in
/// the same place (cards, table rows and timeline bars) and a P0 card is no taller than any other.
fn p0_before_title(ui: &mut Ui, p: &Palette, font: &egui::FontId) {
    let line = ui.fonts_mut(|f| f.row_height(font));
    let pill = ui.fonts_mut(|f| f.row_height(&theme::body(12.0))) + 2.0 * crate::views::PILL_MARGIN.y;
    ui.vertical(|ui| {
        ui.add_space(((line - pill) / 2.0).max(0.0));
        p0_chip(ui, p);
    });
}

fn is_p0(i: &Initiative) -> bool {
    i.priority.as_deref() == Some("P0")
}

/// The progress track (the to-do part, and its legend swatch): the `no_health` neutral, the quietest palette grey
/// that stands 3:1 off a card (light 3.5, dark 3.9; `surface_2` read 1.1).
fn track(p: &Palette) -> Color32 {
    p.no_health
}

/// The in-progress segment's base and stripe: the quiet accent tint crossed by `accent` diagonals, so it differs
/// from the solid done green by pattern and lightness, not hue alone (the two solids read 1.1:1).
pub fn doing_colors(p: &Palette) -> (Color32, Color32) {
    (p.accent_subtle(), p.accent)
}

/// Between the in-progress segment's stripes, and a stripe's width.
const HATCH_STEP: f32 = 4.0;
const HATCH_W: f32 = 1.5;

/// The in-progress pattern inside `shape` (the rounded segment, already clipped by the caller), striped only over
/// `span`, the segment's x range kept clear of a rounded end so no stripe pokes past a corner.
pub(super) fn paint_doing(painter: &egui::Painter, shape: Rect, corners: egui::CornerRadius, span: egui::Rangef, p: &Palette) {
    let (base, stripe) = doing_colors(p);
    painter.rect_filled(shape, corners, base);
    let area = Rect::from_x_y_ranges(span, shape.y_range()).intersect(painter.clip_rect());
    if !area.is_positive() {
        return;
    }
    let clip = painter.with_clip_rect(area);
    let h = area.height();
    let mut x = (area.left() / HATCH_STEP).floor() * HATCH_STEP - h;
    while x < area.right() {
        clip.line_segment([egui::pos2(x, area.bottom()), egui::pos2(x + h, area.top())], Stroke::new(HATCH_W, stripe));
        x += HATCH_STEP;
    }
}

/// A 6 px bar of done (`on_track`, solid), doing (`doing_colors`, striped) and to-do (`track`) children, then "done / total"
/// right-aligned in a fixed label column (PROGRESS_TEMPLATE wide), so stacked tracks end on one edge.
fn progress_bar(ui: &mut Ui, p: &Palette, pr: &Progress) {
    let size = theme::spacing(ui.ctx()).caption;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = CARD_GAP;
        let text = ui.painter().layout_no_wrap(progress_text(pr), theme::body(size), p.muted);
        let col = ui.painter().layout_no_wrap(PROGRESS_TEMPLATE.into(), theme::body(size), p.muted).size().x.ceil();
        let w = (ui.available_width() - col - CARD_GAP).max(24.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, text.size().y), Sense::hover());
        crate::views::tooltip(resp, PROGRESS_TIP);
        let bar = Rect::from_center_size(rect.center(), vec2(w, BAR_H));
        let painter = ui.painter();
        painter.rect_filled(bar, BAR_H / 2.0, track(p));
        let total = (pr.todo + pr.doing + pr.done) as f32;
        if total > 0.0 {
            let done = bar.width() * pr.done as f32 / total;
            let doing = bar.width() * pr.doing as f32 / total;
            let clip = painter.with_clip_rect(bar);
            // Segments drawn as one rounded bar each from the left edge, so their joins stay square; the stripes
            // stop short of the bar's rounded right end.
            let reach = bar.left() + done + doing;
            let lo = if pr.done > 0 { bar.left() + done } else { bar.left() + BAR_H / 2.0 };
            let span = egui::Rangef::new(lo, reach.min(bar.right() - BAR_H / 2.0));
            paint_doing(&clip, Rect::from_min_size(bar.min, vec2(done + doing, BAR_H)), BAR_CORNERS, span, p);
            clip.rect_filled(Rect::from_min_size(bar.min, vec2(done, BAR_H)), BAR_H / 2.0, p.on_track);
        }
        let (slot, _) = ui.allocate_exact_size(vec2(col, text.size().y), Sense::hover());
        ui.painter().galley(egui::pos2(slot.right() - text.size().x, slot.top()), text, p.muted);
    });
}

#[allow(clippy::too_many_arguments)]
fn now_next_later(
    ui: &mut Ui,
    p: &Palette,
    items: &[Initiative],
    ongoing: &[String],
    year: i32,
    focus: Focus,
    done: Option<&crate::jira::Issue>,
    acts: &mut Vec<Action>,
) {
    let cols = roadmap::now_next_later(items);
    let lanes = lanes_vary(items);
    let sp = theme::spacing(ui.ctx());
    let mut first = 0;
    ui.spacing_mut().item_spacing.x = GAP;
    ui.columns(3, |uis| {
        for (ci, ((ui, col), (title, sub))) in uis.iter_mut().zip(&cols).zip(COLUMNS).enumerate() {
            let start = first;
            first += col.len();
            panel(ui, p, |ui| {
                ui.set_width(ui.available_width());
                // `columns` hands out justified layouts, which stretch wrapped text to both edges: left-align it.
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    c::section_header(ui, p, title, Some(col.len()));
                    ui.add_space(sp.line_gap);
                    ui.add(egui::Label::new(RichText::new(sub).size(sp.small).color(p.muted)).wrap());
                    ui.add_space(PAD as f32);
                    ui.spacing_mut().item_spacing.y = sp.card_gap;
                    if col.is_empty() {
                        let (msg, hint) = COLUMN_EMPTY[ci];
                        empty_state(ui, p, msg, hint, p.surface_2);
                    }
                    let mut bottoms = Vec::with_capacity(col.len());
                    for (k, i) in col.iter().enumerate() {
                        let at = start + k;
                        let sel = at == focus.cursor;
                        let (resp, slot) = card(ui, p, i, lanes, sel, at, sel && done.is_some());
                        bottoms.push(resp.rect.bottom());
                        focus.click(at, &resp, acts);
                        if let (Some(slot), Some(issue)) = (slot, done) {
                            acts.extend(place_done(ui, p, issue, slot, c::card_fill(p, true)));
                        }
                    }
                    more_below(ui, p, below_fold(&bottoms, ui.clip_rect().bottom()));
                    if ci == 0
                        && let Some(line) = ktlo_line(ongoing)
                    {
                        ui.add_space(PAD as f32 - sp.card_gap);
                        ui.add(egui::Label::new(RichText::new(line).size(sp.small).color(p.muted)).wrap());
                    }
                });
            });
        }
    });
    ui.add_space(GAP);
    shipped(ui, p, items, year);
}

/// How many cards (by their bottom edges) the viewport ending at `fold` cuts off, whole or in part.
pub fn below_fold(bottoms: &[f32], fold: f32) -> usize {
    bottoms.iter().filter(|&&b| b > fold + 0.5).count()
}

/// "↓ 2 more": the cue on a column whose cards run past the bottom of the page, so its count matches what can be found.
pub fn more_text(n: usize) -> String {
    format!("↓ {n} more")
}

/// The `more_text` pill pinned to the bottom right of the viewport in the column, on the card surface so it reads
/// over the cut card (clear of its title, which starts on the left). Nothing when no card is cut off.
fn more_below(ui: &mut Ui, p: &Palette, n: usize) {
    if n == 0 {
        return;
    }
    let text = more_text(n);
    let galley = ui.painter().layout_no_wrap(text.clone(), theme::body(12.0), p.muted);
    let size = galley.size() + 2.0 * crate::views::PILL_MARGIN;
    let clip = ui.clip_rect();
    let right = ui.max_rect().right() - PAD as f32;
    let rect = Rect::from_min_size(egui::pos2(right - size.x, clip.bottom() - CARD_GAP - size.y), size);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        crate::views::pill_sized(ui, &text, p.surface, p.muted, Stroke::new(1.0, p.border), crate::views::PILL_MARGIN);
    });
}

/// The card's update line's label. A card with no update has no update line.
pub const UPDATE_LABEL: &str = "Latest update: ";

/// The card's update line: the UPDATE_LABEL in `label` then the update in `text`, on one line cut with "…".
pub fn update_job(update: &str, size: f32, label: Color32, p: &Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let fmt = |color| egui::TextFormat { font_id: theme::body(size), color, ..Default::default() };
    job.append(UPDATE_LABEL, 0.0, fmt(label));
    job.append(update, 0.0, fmt(p.text));
    job.wrap = egui::text::TextWrapping::truncate_at_width(f32::INFINITY);
    job
}

/// A card, one anatomy in every column: the health ring, the P0 pill and the name on one row, the chips (lane when
/// lanes vary, up to two metrics), the labelled latest update on one line when there is one (the full text and its
/// author on hover), then the progress bar.
/// With `done_slot`, the card ends in a row kept for the Move to Done button, returned so the caller places the
/// button over the card's click area (`place_done`).
fn card(ui: &mut Ui, p: &Palette, i: &Initiative, lanes: bool, sel: bool, at: usize, with_done: bool) -> (egui::Response, Option<Rect>) {
    let id = egui::Id::new(("roadmap-card", at));
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let sp = theme::spacing(ui.ctx());
    let mut frame = c::card_frame(ui, p, sel);
    if hovered && !sel {
        frame = frame.fill(p.hover_row);
    }
    let fill = frame.fill;
    let title = theme::bold(sp.body);
    let r = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = sp.line_gap;
        health_line(ui, i.health, p, &title, |ui| {
            ui.add_space(CARD_GAP - SWATCH_GAP);
            if is_p0(i) {
                p0_before_title(ui, p, &title);
                ui.add_space(CARD_GAP - SWATCH_GAP);
            }
            ui.add(egui::Label::new(RichText::new(&i.name).font(title.clone()).color(p.text)).wrap());
        });
        chips(ui, p, i, lanes);
        if let Some((author, date, text)) = i.latest_update.as_ref().filter(|u| !u.2.is_empty()) {
            let resp = ui.add(egui::Label::new(update_job(text, sp.small, theme::muted_on(fill, p), p)).truncate());
            crate::views::tooltip(resp, format!("{text}\n{author} · {date}"));
        }
        progress_bar(ui, p, &i.progress);
        done_slot(ui, with_done)
    });
    (ui.interact(r.response.rect, id, Sense::click()), r.inner)
}

/// Whether the focused initiative's epic gets a Move to Done button (`components::done_button_shown`).
fn done_shown(issue: Option<&crate::jira::Issue>) -> bool {
    issue.is_some_and(|i| c::done_button_shown(crate::views::detail::composer_read_only(), &i.status.category))
}

/// A BUTTON_H row the width of `ui`, kept for the focused item's Move to Done button; none when `on` is false.
fn done_slot(ui: &mut Ui, on: bool) -> Option<Rect> {
    on.then(|| ui.allocate_exact_size(vec2(ui.available_width(), crate::views::BUTTON_H), Sense::hover()).0)
}

/// The Move to Done button at the right end of `slot`, on `fill`. Placed after the card's or row's click area, so
/// the button, not the card, takes its clicks.
fn place_done(ui: &mut Ui, p: &Palette, issue: &crate::jira::Issue, slot: Rect, fill: Color32) -> Option<Action> {
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    ui.scope_builder(egui::UiBuilder::new().max_rect(slot).layout(layout), |ui| c::done_button(ui, p, issue, fill)).inner
}

/// The lane (only when lanes vary) and up to two metrics, all neutral pills (P0 sits on the title row).
fn chips(ui: &mut Ui, p: &Palette, i: &Initiative, lanes: bool) {
    let lane = lanes.then_some(&i.lane);
    if lane.is_none() && i.metrics.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(LINE_GAP, LINE_GAP);
        for t in lane.into_iter().chain(i.metrics.iter().take(2)) {
            c::neutral_pill(ui, p, t);
        }
    });
}

/// "Shipped in {year}": the Done initiatives, two columns of name and target quarter, as a plain section on the
/// page (like the timeline's Unscheduled), so no frame edge peeks above the hint bar when it starts below the fold.
fn shipped(ui: &mut Ui, p: &Palette, items: &[Initiative], year: i32) {
    let done = shipped_in(items, year);
    ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        c::section_header(ui, p, &shipped_title(year), Some(done.len()));
        ui.add_space(CARD_GAP - ui.spacing().item_spacing.y);
        if done.is_empty() {
            empty_state(ui, p, "Nothing shipped yet", "Initiatives show here when their epic is done.", p.bg);
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
                            None => {
                                missing(ui, p, "Target: Not set");
                            }
                        }
                    });
                }
            }
        });
    });
}

fn status_review(ui: &mut Ui, p: &Palette, items: &[Initiative], focus: Focus, done: Option<&crate::jira::Issue>, acts: &mut Vec<Action>) {
    let rows = roadmap::status_rows(items);
    let counts = roadmap::health_counts(&rows);
    let lanes = lanes_vary(items);
    let sp = theme::spacing(ui.ctx());
    // The scope first: the review covers Now only, which nothing else on the tab says.
    c::section_header(ui, p, COLUMNS[0].0, Some(rows.len()));
    ui.add_space(sp.line_gap - ui.spacing().item_spacing.y);
    ui.label(RichText::new(STATUS_SCOPE).size(sp.small).color(p.muted));
    ui.add_space(GAP - ui.spacing().item_spacing.y);
    // Five equal tiles; the table takes their row's width, so both end on one edge.
    let (tile_w, total) = tile_widths(ui.available_width().max(TABLE_MIN_W), TILES);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        for (h, n) in [Health::OffTrack, Health::AtRisk, Health::OnTrack, Health::None].into_iter().zip(counts) {
            tile(ui, p, tile_w, &n.to_string(), n == 0, |ui| {
                let small = theme::spacing(ui.ctx()).small;
                health_line(ui, h, p, &theme::body(small), |ui| {
                    ui.label(RichText::new(health_label(h)).size(small).color(p.muted));
                });
            });
        }
        let (set, total) = roadmap::owners_set(&rows);
        // No dot here, so no dot slot: the label starts on the figure's edge.
        tile(ui, p, tile_w, &owners_text((set, total)), set == 0, |ui| {
            ui.label(RichText::new(OWNERS_LABEL).size(theme::spacing(ui.ctx()).small).color(p.muted));
        });
    });
    let shown = shown_columns(&rows);
    let note = hidden_note(&shown);
    if status_has_gaps(&rows) {
        ui.add_space(sp.line_gap + CARD_GAP - ui.spacing().item_spacing.y);
        let text = match &note {
            Some(n) => format!("{n} {STATUS_HINT}"),
            None => STATUS_HINT.to_string(),
        };
        code_hint(ui, p, &text, p.muted, p.bg);
    }
    ui.add_space(GAP);
    if rows.is_empty() {
        panel(ui, p, |ui| empty_state(ui, p, "Nothing in Now", "Initiatives in progress this quarter are reviewed here.", p.surface_2));
        return;
    }
    egui::ScrollArea::horizontal().id_salt("roadmap-status").show(ui, |ui| table(ui, p, &rows, &shown, lanes, total, focus, done, acts));
}

/// The status tiles: four health counts and the owners tile.
const TILES: usize = 5;
/// The tile's side padding.
const TILE_PAD_X: i8 = 16;
/// The owners tile's label (sentence case, like the health labels beside it).
pub const OWNERS_LABEL: &str = "Owners set";

/// One outer width for all `n` tiles GAP apart (the horizon columns' gap) within `avail`, in whole points, and the
/// row's total width (at most `avail`), which the table below takes so both end on one edge.
pub fn tile_widths(avail: f32, n: usize) -> (f32, f32) {
    let n = n.max(1);
    let gaps = GAP * (n - 1) as f32;
    let w = ((avail - gaps) / n as f32).floor();
    (w, w * n as f32 + gaps)
}

/// A summary tile `w` wide outside: the figure (bold h2, `muted` when it is zero, so the non-zero ones stand out)
/// over its label line.
fn tile(ui: &mut Ui, p: &Palette, w: f32, figure: &str, zero: bool, label: impl FnOnce(&mut Ui)) {
    let sp = theme::spacing(ui.ctx());
    let edge = Stroke::new(1.0, p.border);
    Frame::new().fill(p.surface).stroke(edge).corner_radius(theme::RADIUS).inner_margin(Margin::symmetric(TILE_PAD_X, PAD)).show(ui, |ui| {
        // Inside: the outer width less the side padding and the edge.
        let inner = (w - 2.0 * (f32::from(TILE_PAD_X) + edge.width)).max(0.0);
        ui.set_min_width(inner);
        ui.set_max_width(inner);
        ui.spacing_mut().item_spacing.y = LINE_GAP;
        ui.vertical(|ui| {
            ui.label(RichText::new(figure).font(theme::bold(sp.h2)).color(if zero { p.muted } else { p.text }));
            label(ui);
        });
    });
}

/// The widths of the `shown` columns for `total`: each its SHARES of `total`, floored, and the Initiative column
/// (always first) the rest, so hidden columns give their room to the names and the columns (row fills, cursor
/// ring) end exactly on the table's right edge.
pub fn column_widths(total: f32, shown: &[usize]) -> Vec<f32> {
    let mut w: Vec<f32> = shown.iter().map(|&c| (total * SHARES[c]).floor()).collect();
    if let Some(first) = w.first_mut() {
        *first = 0.0;
        let rest: f32 = w.iter().sum();
        w[0] = total - rest;
    }
    w
}

/// The status table: the header on `surface_2`, body rows on the card `surface` split by `border` rules (no
/// stripes, so the table reads as one card), the cursor row in the shared selection (fill and accent ring), the
/// frame's edge drawn last so no row paints over it.
#[allow(clippy::too_many_arguments)]
fn table(
    ui: &mut Ui,
    p: &Palette,
    rows: &[&Initiative],
    shown: &[usize],
    lanes: bool,
    total: f32,
    focus: Focus,
    done: Option<&crate::jira::Issue>,
    acts: &mut Vec<Action>,
) {
    let widths = column_widths(total, shown);
    let last = shown.len() - 1;
    let (head, sel) = (p.surface_2, p.selection());
    let cursor = focus.cursor;
    let mut rects: Vec<Rect> = vec![Rect::NOTHING; rows.len()];
    let small = theme::spacing(ui.ctx()).small;
    let mut slot = None;
    let frame = Frame::new().fill(p.surface).corner_radius(theme::RADIUS).show(ui, |ui| {
        ui.set_width(total);
        egui::Grid::new("roadmap-status-grid")
            .num_columns(shown.len())
            .spacing([0.0, 0.0])
            .min_row_height(0.0)
            .with_row_color(move |row, _| match row {
                0 => Some(head),
                r if r - 1 == cursor => Some(sel),
                _ => None,
            })
            .show(ui, |ui| {
                for (k, &c) in shown.iter().enumerate() {
                    cell(ui, widths[k], k == last, |ui| {
                        ui.label(RichText::new(HEADS[c]).font(theme::bold(theme::spacing(ui.ctx()).caption)).color(p.muted));
                    });
                }
                ui.end_row();
                for (ri, i) in rows.iter().enumerate() {
                    let mut row = Rect::NOTHING;
                    for (k, &c) in shown.iter().enumerate() {
                        row = row.union(cell(ui, widths[k], k == last, |ui| match c {
                            0 => initiative_cell(ui, p, i, lanes),
                            // No health: the hollow ring alone (it says "No health set" on hover), not ring and dash.
                            1 => {
                                health_line(ui, i.health, p, &theme::body(small), |ui| {
                                    if i.health != Health::None {
                                        ui.label(RichText::new(health_label(i.health)).size(small).color(p.text));
                                    }
                                });
                            }
                            2 => text_or_missing(ui, p, i.confidence.as_deref()),
                            3 => match &i.latest_update {
                                Some((author, date, text)) if !text.is_empty() => {
                                    ui.add(egui::Label::new(RichText::new(text).size(small).color(p.text)).wrap());
                                    ui.label(RichText::new(format!("{author} · {date}")).size(theme::spacing(ui.ctx()).caption).color(p.muted));
                                }
                                _ => empty_cell(ui, p),
                            },
                            4 => text_or_missing(ui, p, i.next_milestone.as_deref()),
                            5 => text_or_missing(ui, p, i.definition_of_done.as_deref()),
                            _ => {
                                progress_bar(ui, p, &i.progress);
                                if ri == cursor {
                                    slot = done_slot(ui, done.is_some());
                                }
                            }
                        }));
                    }
                    ui.end_row();
                    rects[ri] = row;
                }
            });
    });
    // Every row spans the frame edge to edge, so the cursor ring sits flush on both sides.
    let span = frame.response.rect.x_range();
    for r in rects.iter_mut().filter(|r| r.is_positive()) {
        *r = Rect::from_x_y_ranges(span, r.y_range());
    }
    // A `border` rule between body rows (the head's fill marks its own edge).
    for r in rects.iter().skip(1).filter(|r| r.is_positive()) {
        ui.painter().rect_filled(Rect::from_x_y_ranges(span, r.top()..=r.top() + 1.0), 0.0, p.border);
    }
    if let Some(r) = rects.get(cursor).filter(|r| r.is_positive()) {
        ui.painter().rect_stroke(*r, 0.0, Stroke::new(1.0, p.selection_edge()), egui::StrokeKind::Inside);
    }
    ui.painter().rect_stroke(frame.response.rect, theme::RADIUS, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    for (ri, r) in rects.into_iter().enumerate() {
        let resp = ui.interact(r, egui::Id::new(("roadmap-status-row", ri)), Sense::click());
        focus.click(ri, &resp, acts);
    }
    if let (Some(slot), Some(issue)) = (slot, done) {
        acts.extend(place_done(ui, p, issue, slot, sel));
    }
}

/// One table cell `w` wide: 12 in from its left (and the `last` column's right), 8 above and below plus the
/// density's row room.
fn cell(ui: &mut Ui, w: f32, last: bool, content: impl FnOnce(&mut Ui)) -> Rect {
    let right = if last { PAD } else { 0 };
    let y = (8.0 + theme::spacing(ui.ctx()).row_extra) as i8;
    let r = Frame::new().inner_margin(Margin { left: PAD, right, top: y, bottom: y }).show(ui, |ui| {
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

/// The P0 marker and name (bold), then "[lane · ]key · Owner: Name", the owner the placeholder "Not set" when unset.
fn initiative_cell(ui: &mut Ui, p: &Palette, i: &Initiative, lanes: bool) {
    let sp = theme::spacing(ui.ctx());
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = CARD_GAP;
        let font = theme::bold(sp.body);
        if is_p0(i) {
            p0_before_title(ui, p, &font);
        }
        ui.add(egui::Label::new(RichText::new(&i.name).font(font).color(p.text)).wrap());
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let small = |t: String| RichText::new(t).size(sp.caption).color(p.muted);
        if lanes {
            ui.label(small(format!("{} · ", i.lane)));
        }
        ui.label(small(i.jira_key.clone()));
        ui.label(small(" · Owner: ".into()));
        match &i.owner {
            Some(o) => {
                ui.label(small(o.clone()));
            }
            None => {
                crate::views::tooltip(ui.label(RichText::new(EMPTY_CELL).size(sp.caption).color(p.placeholder())), NOT_SET);
            }
        }
    });
}

/// A value in the table's one cell size, or the empty cell.
fn text_or_missing(ui: &mut Ui, p: &Palette, text: Option<&str>) {
    match text {
        Some(t) => {
            ui.add(egui::Label::new(RichText::new(t).size(theme::spacing(ui.ctx()).small).color(p.text)).wrap());
        }
        None => empty_cell(ui, p),
    }
}

/// An empty table cell: the quiet EMPTY_CELL dash in the placeholder role, NOT_SET on hover.
fn empty_cell(ui: &mut Ui, p: &Palette) {
    crate::views::tooltip(missing(ui, p, EMPTY_CELL), NOT_SET);
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
    fn cold_start_shows_the_spinner_not_the_empty_state() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        app.screen = crate::app::Screen::Roadmap;
        app.lists.entry(crate::store::ListId::TeamOpen).or_default().loading = true;
        let drawn = texts(&mut app);
        assert!(!drawn.iter().any(|t| t == "No roadmap initiatives yet"), "TeamOpen in flight: {drawn:?}");
        app.lists.entry(crate::store::ListId::TeamOpen).or_default().loading = false;
        has(&texts(&mut app), "No roadmap initiatives yet");
    }

    #[test]
    fn roadmap_loading_waits_for_team_open_and_the_first_epics() {
        use crate::app::Loaded;
        let busy = Loaded { loading: true, ..Default::default() };
        let landed = Loaded { fetched: Some(std::time::Instant::now()), ..Default::default() };
        let failed = Loaded { error: Some("down".into()), ..Default::default() };
        assert!(roadmap_loading(false, Some(&busy), None), "TeamOpen in flight");
        assert!(roadmap_loading(false, Some(&landed), Some(&busy)), "Epics in flight");
        assert!(roadmap_loading(true, None, None), "cold start with a client: Epics never landed");
        assert!(!roadmap_loading(false, None, None), "no client: nothing will land");
        assert!(!roadmap_loading(true, Some(&failed), None), "TeamOpen failed: Epics never loads");
        assert!(!roadmap_loading(true, Some(&landed), Some(&landed)), "both landed: truly empty");
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
        assert_eq!(ktlo_line(&["Türkiye İzleme (ongoing)".into()]).as_deref(), Some("Also ongoing (keep-the-lights-on work, not initiatives): Türkiye İzleme"));
        assert_eq!(ktlo_line(&["İİİİ (ONGOING) x".into()]).as_deref(), Some("Also ongoing (keep-the-lights-on work, not initiatives): İİİİ x"));
        assert_eq!(
            ktlo_line(&["Audit (ongoing)".into(), "Support/Enablement (Ongoing) work".into()]).as_deref(),
            Some("Also ongoing (keep-the-lights-on work, not initiatives): Audit, Support/Enablement work")
        );
    }

    #[test]
    fn lane_chips_show_only_when_lanes_differ() {
        let app = app();
        let (mut items, _) = app.roadmap_items();
        assert!(!lanes_vary(&items));
        items[0].lane = "Platform".into();
        assert!(lanes_vary(&items));
        assert!(!lanes_vary(&items[..1]));
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
            P0_LABEL,
            "Lead Time",
            "0 / 0",
            "Autoscaling everywhere",
            "Also ongoing (keep-the-lights-on work, not initiatives): Audit",
            "Shipped in 2026",
            "Cost Insights",
            "Q3",
        ] {
            has(&drawn, want);
        }
        assert!(!drawn.iter().any(|t| t.starts_with(UPDATE_LABEL)), "no update, no update line: {drawn:?}");
        for m in [Mark::Done, Mark::Doing, Mark::Todo, Mark::NoHealth] {
            has(&drawn, m.label());
        }
        assert!(!drawn.iter().any(|t| t == roadmap::NO_LANE), "one lane for all: the lane chip is noise");
    }

    #[test]
    fn status_review_draws_headless() {
        let mut app = app();
        app.roadmap.tab = 3;
        let drawn = texts(&mut app);
        for want in [
            "Initiative",
            "Health",
            "Progress",
            "Canary Deployments",
            "Cluster Upgrades",
            "CI-1",
            "At risk",
            "No health set",
            EMPTY_CELL,
            OWNERS_LABEL,
            " · Owner: ",
            &format!("{} {}", hidden_note(&[0, 1, 6]).unwrap(), plain(STATUS_HINT)),
            "In progress",
            "0 / 2",
            "Off track",
            "Now",
            STATUS_SCOPE,
        ] {
            has(&drawn, want);
        }
        assert!(!drawn.iter().any(|t| t == "Autoscaling everywhere"), "Next items stay off the status review");
        for gone in ["Confidence", "Latest update", "Next milestone", "Definition of done"] {
            assert!(!drawn.iter().any(|t| t == gone), "empty on every row: {gone:?} column hidden");
        }
        for gone in [NOT_SET, "not set", "not defined", "no update yet", "owners set"] {
            assert!(!drawn.iter().any(|t| t == gone), "one placeholder phrasing: {gone:?} in {drawn:?}");
        }
    }

    #[test]
    fn timeline_draws_headless() {
        let mut app = app();
        let mut all = app.store.list(crate::store::ListId::Epics).into_iter().cloned().collect::<Vec<_>>();
        all.push(epic("CI-9", "Pilot Rollout", "indeterminate", &["roadmap", "PLAT", "P0", "start:2026-Q2", "target:2026-Q4"]));
        app.store.absorb(crate::store::ListId::Epics, all);
        app.roadmap.tab = 2;
        let drawn = texts(&mut app);
        for want in ["Subteam", "Q1 2026", "Q4 2026", "Platform", "Pilot Rollout", roadmap::NO_LANE, "✓ Cost Insights", "Oct 27", "Unscheduled"] {
            has(&drawn, want);
        }
        has(&drawn, "Canary Deployments");
        has(&drawn, P0_LABEL);
        has(&drawn, Mark::Today.label());
        assert!(!drawn.iter().any(|t| t == Mark::RunsOn.label()), "no bar runs past 2026: no chevron entry");
        assert_eq!(drawn.iter().filter(|t| *t == "Cost Insights").count(), 0, "Done items with no target stay out of the tray");
    }

    #[test]
    fn legends_list_only_the_marks_shown() {
        let app = app();
        let (mut items, _) = app.roadmap_items();
        let progress = [Mark::Done, Mark::Doing, Mark::Todo];
        // CI-1 is P0 and at risk; the rest have no health.
        assert_eq!(legend_marks(1, &items, 2026), [&progress[..], &[Mark::NoHealth, Mark::Health, Mark::P0]].concat());
        assert_eq!(legend_marks(3, &items, 2026), legend_marks(1, &items, 2026), "the same marks, the same legend");
        assert_eq!(legend_marks(4, &items, 2026), []);
        for i in &mut items {
            i.health = Health::OnTrack;
            i.priority = None;
        }
        assert_eq!(legend_marks(3, &items, 2026), [&progress[..], &[Mark::Health]].concat(), "no ring or P0 to explain");
        // Timeline: only Cost Insights (Done, 2026-Q3) has a bar; it has progress segments but no health glyph.
        assert_eq!(legend_marks(2, &items, 2026), [&progress[..], &[Mark::Today]].concat());
        let k = items.iter().position(|i| i.key == "CI-2").unwrap();
        items[k].start = Some(roadmap::Quarter { year: 2026, q: 3 });
        items[k].target = Some(roadmap::Quarter { year: 2027, q: 1 });
        items[k].priority = Some("P0".into());
        assert_eq!(legend_marks(2, &items, 2026), [&progress[..], &[Mark::Health, Mark::P0, Mark::RunsOn, Mark::Today]].concat(), "same order as tabs 1 and 3");
    }

    #[test]
    fn the_more_cue_counts_cards_the_fold_cuts() {
        assert_eq!(below_fold(&[100.0, 200.0, 300.0], 250.0), 1, "the third card is cut");
        assert_eq!(below_fold(&[100.0, 200.0], 200.0), 0, "a card ending on the fold is whole");
        assert_eq!(more_text(2), "↓ 2 more");
    }

    #[test]
    fn empty_columns_hide_and_are_named_once() {
        let app = app();
        let (mut items, _) = app.roadmap_items();
        let rows = roadmap::status_rows(&items);
        assert_eq!(shown_columns(&rows), [0, 1, 6], "only CI-1 has a health");
        assert_eq!(
            hidden_note(&shown_columns(&rows)).as_deref(),
            Some("No initiative has confidence, latest update, next milestone or definition of done set yet, so the table leaves those columns out.")
        );
        for i in &mut items {
            i.health = Health::None;
            i.confidence = Some("High".into());
        }
        let rows = roadmap::status_rows(&items);
        assert_eq!(shown_columns(&rows), [0, 2, 6]);
        assert_eq!(hidden_note(&[0, 1, 2, 4, 5, 6]).as_deref(), Some("No initiative has latest update set yet, so the table leaves those columns out."));
        assert_eq!(hidden_note(&(0..HEADS.len()).collect::<Vec<_>>()), None);
    }

    #[test]
    fn status_hint_shows_while_anything_is_missing() {
        let app = app();
        let (mut items, _) = app.roadmap_items();
        assert!(status_has_gaps(&roadmap::status_rows(&items)));
        for i in &mut items {
            i.health = Health::OnTrack;
            i.owner = Some("Ada".into());
            i.confidence = Some("High".into());
            i.next_milestone = Some("Beta".into());
            i.definition_of_done = Some("All clusters".into());
        }
        assert!(!status_has_gaps(&roadmap::status_rows(&items)));
    }

    #[test]
    fn table_and_tiles_end_on_one_edge() {
        for total in [900.0, 1408.0, 1333.0] {
            let all: Vec<usize> = (0..HEADS.len()).collect();
            assert_eq!(column_widths(total, &all).iter().sum::<f32>(), total);
            assert_eq!(column_widths(total, &[0, 1, 6]).iter().sum::<f32>(), total);
            assert_eq!(column_widths(total, &[0, 6])[1], column_widths(total, &all)[6], "a hidden column widens the names, not Progress");
            let (w, row) = tile_widths(total, TILES);
            assert_eq!(w, w.floor(), "whole points");
            assert_eq!(w * TILES as f32 + GAP * (TILES as f32 - 1.0), row, "equal tiles make the row the table takes");
            assert!(row <= total && total - row < TILES as f32, "the row loses less than a point per tile");
        }
    }

    #[test]
    fn timeline_marks_keep_red_for_off_track() {
        for p in [&theme::LIGHT, &theme::DARK] {
            // Today is the accent: its label reads on the grid; bars are neutral, never the selection tint.
            assert!(theme::contrast(p.accent, p.surface) >= 4.5, "dark={}", p.dark);
            let (fill, fg) = timeline::bar_colors(p, false);
            assert_ne!(fill, p.selection());
            assert_ne!(fill, p.accent_subtle());
            assert!(theme::contrast(fg, fill) >= 7.0, "dark={}", p.dark);
            assert!(theme::contrast(timeline::bar_colors(p, true).1, timeline::bar_colors(p, true).0) >= 4.5, "dark={}", p.dark);
        }
    }

    #[test]
    fn card_update_line_is_always_labelled() {
        let p = &theme::LIGHT;
        let job = update_job("app deployed to build", 13.0, p.muted, p);
        assert_eq!(job.text, "Latest update: app deployed to build");
        assert_eq!(job.wrap.max_rows, 1, "one line, cut with an ellipsis");
    }

    #[test]
    fn code_spans_are_mono_and_drop_their_backticks() {
        let p = &theme::LIGHT;
        let job = code_job("Add the `horizon:next` label", 13.0, 14.0, p.muted, p.surface_2, p);
        assert_eq!(job.text, plain("Add the `horizon:next` label"));
        let mono: Vec<_> = job.sections.iter().filter(|s| s.format.font_id.family == egui::FontFamily::Monospace).collect();
        assert_eq!(mono.len(), 1);
        for h in [STATUS_HINT, COLUMN_EMPTY[1].1, outcomes::NO_METRICS.1] {
            assert_eq!(h.matches('`').count() % 2, 0, "balanced code spans: {h}");
        }
    }

    #[test]
    fn done_and_in_progress_differ_by_more_than_hue() {
        for p in [&theme::LIGHT, &theme::DARK] {
            let (base, stripe) = doing_colors(p);
            // The striped segment's base steps well off the solid done green, and its stripes read on the base.
            assert!(theme::contrast(base, p.on_track) >= 3.0, "dark={}", p.dark);
            assert!(theme::contrast(stripe, base) >= 3.0, "dark={}", p.dark);
            assert!(theme::contrast(base, track(p)) >= 1.5, "doing vs to do, dark={}", p.dark);
        }
    }

    #[test]
    fn progress_track_reads_on_every_card() {
        for p in [&theme::LIGHT, &theme::DARK] {
            assert!(theme::contrast(track(p), p.surface) >= 3.0, "dark={}", p.dark);
            assert!(theme::contrast(track(p), p.surface_2) >= 3.0, "legend on the page, dark={}", p.dark);
        }
    }

    #[test]
    fn outcomes_draws_headless() {
        let mut app = app();
        let mut all = app.store.list(crate::store::ListId::Epics).into_iter().cloned().collect::<Vec<_>>();
        all.push(epic("CI-9", "Toil Burner", "new", &["roadmap", "horizon:next", "metric:Toil"]));
        app.store.absorb(crate::store::ListId::Epics, all.clone());
        app.roadmap.tab = 4;
        let drawn = texts(&mut app);
        for want in ["Lead Time", "1 active", "Canary Deployments", "Toil", "0 active", "Toil Burner", "(next)", "Nothing in progress moves this metric."] {
            has(&drawn, want);
        }
        assert_eq!(drawn.iter().filter(|t| *t == "Nothing in progress moves this metric.").count(), 1, "Lead Time has an active item");
        for e in &mut all {
            e.labels.retain(|l| !l.starts_with("metric:"));
        }
        app.store.absorb(crate::store::ListId::Epics, all);
        has(&texts(&mut app), outcomes::NO_METRICS.0);
    }
}
