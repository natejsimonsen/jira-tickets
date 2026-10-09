//! The keyboard help overlay (`?`), drawn from `keys::BINDINGS`. Its keys
//! (`?`, Esc) are handled in `App::shortcuts`; this only draws.

use crate::app::{Action, App};
use crate::keys::{self, BINDINGS, Group};
use crate::theme::{self, Palette};
use egui::{Frame, Margin, RichText, Stroke, vec2};

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.help_open {
        return;
    }
    let p = theme::palette(ctx);
    // Above the pickers' Foreground dialog: the backdrop at Foreground (shown
    // after the dialog, so on top of it) and the panel at Tooltip.
    if crate::views::backdrop(ctx, "help-backdrop", egui::Order::Foreground, crate::views::hint_bar_height(app), p).clicked() {
        app.actions.push(Action::ToggleHelp);
    }
    egui::Area::new(egui::Id::new("help")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
        // Issues on the left; Pickers then Global on the right (the columns end closer). Each column is as wide as its widest key cell
        // plus 16 plus its widest action, so the panel has 24 px on every side and no dead space.
        let left = [Group::Issues];
        let right = [Group::Pickers, Group::Global];
        let measure = |groups: &[Group]| {
            let rows = BINDINGS.iter().filter(|k| groups.contains(&k.group));
            let keys: Vec<f32> = rows.clone().map(|k| keys_width(ui, k.keys, p)).collect();
            let actions: Vec<f32> = rows.map(|k| ui.painter().layout_no_wrap(k.what.to_string(), theme::body(14.0), p.text).size().x).collect();
            (keys.iter().copied().fold(0.0, f32::max), column_width(&keys, &actions))
        };
        let (lk, lw) = measure(&left);
        let (rk, rw) = measure(&right);
        Frame::new()
            .fill(crate::views::dialog_fill(p))
            .stroke(crate::views::dialog_stroke(p))
            .corner_radius(theme::RADIUS)
            .inner_margin(Margin { left: PAD as i8, right: PAD as i8, top: TITLE_TOP as i8, bottom: PAD as i8 })
            .show(ui, |ui| {
                ui.set_width(lw + COL_GAP + rw);
                ui.spacing_mut().item_spacing.y = 0.0;
                let title = ui.painter().layout_no_wrap("Keyboard".to_string(), theme::bold(15.0), p.text);
                let below = title.size().y - theme::ink_rect(&title).bottom();
                ui.label(title);
                let (row, header) = (row_h(ui, p), header_h(ui));
                ui.add_space(TITLE_TO_HEADINGS - below - heading_ink(ui, p, row, header).0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = COL_GAP;
                    ui.allocate_ui_with_layout(vec2(lw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(lw);
                        group(ui, p, Group::Issues, lk);
                    });
                    ui.allocate_ui_with_layout(vec2(rw, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(rw);
                        group(ui, p, Group::Pickers, rk);
                        ui.add_space(section_gap(row, header));
                        group(ui, p, Group::Global, rk);
                    });
                });
            });
    });
}

/// The panel's padding on every side, and the gap between its columns.
const PAD: f32 = 24.0;
/// The top padding: 20, since the title's line box carries ~4 of leading above its ink (24 to the ink; it
/// measured 28 at 24).
const TITLE_TOP: f32 = 20.0;
const COL_GAP: f32 = 24.0;
/// Key cell → action: 16 px, never more.
const KEY_TO_ACTION: f32 = 16.0;
/// Between a chip and the "or" / "to" beside it.
const CHIP_GAP: f32 = 4.0;
/// One row's pitch: the row (chip height) plus the gap to the next.
const ROW_PITCH: f32 = 34.0;

/// A help column's width: its widest key cell, 16, its widest action.
pub fn column_width(keys: &[f32], actions: &[f32]) -> f32 {
    let max = |v: &[f32]| v.iter().copied().fold(0.0, f32::max);
    (max(keys) + KEY_TO_ACTION + max(actions)).ceil()
}

/// A chip's side padding, from its fill to the key's ink (round 13 H1: 6 to the galley box read 6.5–8).
const CHIP_INK_X: f32 = 8.0;
/// The narrowest chip ("?", "j").
const CHIP_MIN_W: f32 = 20.0;

/// A chip's width for a key whose ink is `ink_w` wide: CHIP_INK_X either side, at least CHIP_MIN_W, with the
/// padding on whole device pixels at 2x so the ink centres exactly.
pub fn chip_w(ink_w: f32) -> f32 {
    let pad = ((CHIP_MIN_W - ink_w) / 2.0).max(CHIP_INK_X);
    ink_w + 2.0 * (pad * 2.0).ceil() / 2.0
}

fn chip_size(ui: &egui::Ui, text: &str, p: &Palette) -> egui::Vec2 {
    let g = ui.painter().layout_job(crate::views::issues::key_job(text, p.text));
    let ink = theme::ink_rect(&g);
    let ink_w = if ink.is_positive() { ink.width() } else { g.size().x };
    vec2(chip_w(ink_w), g.size().y + 4.0)
}

/// A muted "or" / "to" and its ink's left and right bearings.
fn word_galley(ui: &egui::Ui, word: &str, p: &Palette) -> (std::sync::Arc<egui::Galley>, f32, f32) {
    let g = ui.painter().layout_no_wrap(word.into(), theme::body(12.0), crate::views::dialog_muted(p));
    let ink = theme::ink_rect(&g);
    let (lb, rb) = if ink.is_positive() { (ink.left(), g.size().x - ink.right()) } else { (0.0, 0.0) };
    (g, lb, rb)
}

/// A word's width in a key cell: its ink and CHIP_GAP to each chip's fill.
fn word_width(ui: &egui::Ui, word: &str, p: &Palette) -> f32 {
    let (g, lb, rb) = word_galley(ui, word, p);
    g.size().x - lb - rb + 2.0 * CHIP_GAP
}

/// The width of one binding's key cell: its chips, with a muted "or" between alternatives and "to" between
/// a range's ends, each CHIP_GAP from its chips to the word's ink (round 14 H3: 4 to the box read 5).
fn keys_width(ui: &egui::Ui, keys: &str, p: &Palette) -> f32 {
    let mut w = 0.0;
    for (i, alt) in keys::chips(keys).enumerate() {
        if i > 0 {
            w += word_width(ui, "or", p);
        }
        for (j, k) in keys::range(alt).into_iter().enumerate() {
            if j > 0 {
                w += word_width(ui, "to", p);
            }
            w += chip_size(ui, k, p).x;
        }
    }
    w
}

/// The space under a help row so rows sit ROW_PITCH apart whatever the row's own height.
pub fn row_gap(row_h: f32) -> f32 {
    (ROW_PITCH - row_h).max(0.0)
}

/// Header → its first row.
const HEADER_TO_ROWS: f32 = 8.0;

/// The space between a group's last row and the next group's header: the row gap, then enough that the header
/// (`header_h` plus HEADER_TO_ROWS) takes exactly one ROW_PITCH more, so the rows under a second header stay on
/// the column's 34 grid and both columns' last rows line up (round 10 measured the right one 1 pt low: the old
/// 12 extra + 15 header + 8 made 35).
pub fn section_gap(row_h: f32, header_h: f32) -> f32 {
    row_gap(row_h) + (ROW_PITCH - HEADER_TO_ROWS - header_h).max(0.0)
}

/// A section header's slot height, on whole points.
fn header_h(ui: &mut egui::Ui) -> f32 {
    slot_h(ui.fonts_mut(|f| f.row_height(&theme::bold(12.0))))
}

/// A help row's height: its tallest part, a chip or the 14 px action.
fn row_h(ui: &mut egui::Ui, p: &Palette) -> f32 {
    slot_h(chip_size(ui, "⌘", p).y.max(ui.fonts_mut(|f| f.row_height(&theme::body(14.0)))))
}

/// A header or row slot's height on whole points, so the rows under a second header (Global) do not sit half a
/// point low (round 9 measured 0.5 in the right column).
pub fn slot_h(h: f32) -> f32 {
    h.ceil()
}

/// A heading's baseline (its ink's foot: no heading has descenders) → its first row's chips (round 14 H1: 11.5 /
/// 11.5 / 9 under Issues, Pickers, Global).
const HEADING_TO_ROW: f32 = 8.0;
/// The title's ink → the headings' ink (round 14 H2: 17.5).
const TITLE_TO_HEADINGS: f32 = 16.0;

/// Where a heading's ink sits in its slot, as (ink top, line-box top), so its baseline is HEADING_TO_ROW above
/// the first row's chips (centred in the row, HEADER_TO_ROWS under the slot).
fn heading_ink(ui: &egui::Ui, p: &Palette, row: f32, header: f32) -> (f32, f32) {
    let ink = theme::Ink::of(ui.ctx(), &theme::bold(12.0), None);
    let chip_top = header + HEADER_TO_ROWS + (row - chip_size(ui, "⌘", p).y) / 2.0;
    let line_top = chip_top - HEADING_TO_ROW - ink.base;
    (line_top + ink.top, line_top)
}

/// A group: its heading (`heading_ink`), then its rows.
fn group(ui: &mut egui::Ui, p: &Palette, g: Group, key_w: f32) {
    let header = header_h(ui);
    let row = row_h(ui, p);
    // A fixed slot with the label painted in it, so the heading's place never moves the rows.
    let (slot, _) = ui.allocate_exact_size(vec2(ui.available_width(), header), egui::Sense::hover());
    let label = ui.painter().layout_no_wrap(g.name().to_string(), theme::bold(12.0), crate::views::dialog_muted(p));
    let y = crate::views::issues::snap_px(slot.top() + heading_ink(ui, p, row, header).1, ui.ctx().pixels_per_point());
    ui.painter().galley(egui::pos2(slot.left(), y), label, crate::views::dialog_muted(p));
    ui.add_space(HEADER_TO_ROWS);
    // Every row is as tall as its tallest part (a chip or the action), and chips, words and the action are
    // all centred on that row.
    let row_h = row_h(ui, p);
    for (i, k) in BINDINGS.iter().filter(|k| k.group == g).enumerate() {
        if i > 0 {
            ui.add_space(row_gap(row_h));
        }
        let centred = egui::Layout::left_to_right(egui::Align::Center);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), row_h), centred, |ui| {
            ui.set_min_height(row_h);
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(vec2(key_w, row_h), centred, |ui| {
                ui.set_width(key_w);
                ui.set_min_height(row_h);
                ui.spacing_mut().item_spacing.x = 0.0;
                // CHIP_GAP from each chip's fill to the word's ink.
                let word = |ui: &mut egui::Ui, w: &str| {
                    let (g, lb, rb) = word_galley(ui, w, p);
                    ui.add_space(CHIP_GAP - lb);
                    ui.label(g);
                    ui.add_space(CHIP_GAP - rb);
                };
                for (i, alt) in keys::chips(k.keys).enumerate() {
                    if i > 0 {
                        word(ui, "or");
                    }
                    for (j, c) in keys::range(alt).into_iter().enumerate() {
                        if j > 0 {
                            word(ui, "to");
                        }
                        chip(ui, c, p);
                    }
                }
            });
            ui.add_space(KEY_TO_ACTION);
            ui.add(egui::Label::new(RichText::new(k.what).font(theme::body(14.0)).color(p.text)).extend());
        });
    }
}

/// A key chip: mono 12.5 in a 6/2 frame, at least 20 wide, text centred. Its fill steps off the dialog's
/// (`chip_fill_on`). The key is the hint bar's `key_job`: ⌘ and ⇧ each in the system face (SF Mono's ⇧ is
/// thin), a pair's "/" closed up.
fn chip(ui: &mut egui::Ui, text: &str, p: &Palette) {
    let galley = ui.painter().layout_job(crate::views::issues::key_job(text, p.text));
    let size = chip_size(ui, text, p);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    // On whole pixels, so the fill's edges and the snapped glyphs sit the same half-pixel apart either side.
    let ppp = ui.ctx().pixels_per_point();
    let rect = crate::views::issues::snap_rect(rect, ppp);
    let fill = theme::chip_fill_on(crate::views::dialog_fill(p), p);
    ui.painter().rect(rect, 4.0, fill, chip_stroke(p), egui::StrokeKind::Inside);
    // Centred on the glyphs' ink, not their advance box: "⌘" carries extra advance on its right, "?" a bearing.
    let ink = theme::ink_rect(&galley);
    let x = crate::views::issues::snap_px(ink_left(rect.center().x, ink.center().x), ppp);
    ui.painter().galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, p.text);
}

/// A help chip's edge: none, since the fill alone stands off the panel (round 8: fill and edge read doubled);
/// in dark the dialog edge only if the fill were under 1.2:1 off the dialog.
pub fn chip_stroke(p: &Palette) -> Stroke {
    let fill = theme::chip_fill_on(crate::views::dialog_fill(p), p);
    if p.dark && theme::contrast(fill, crate::views::dialog_fill(p)) < 1.2 { Stroke::new(1.0, theme::DARK_EDGE) } else { Stroke::NONE }
}

/// Where to put a galley so its ink centre (`ink_centre`, from the galley's origin) lands on `centre`.
pub fn ink_left(centre: f32, ink_centre: f32) -> f32 {
    centre - ink_centre
}

#[cfg(test)]
mod tests {
    #[test]
    fn slots_are_whole_points() {
        assert_eq!(super::slot_h(14.5), 15.0);
        assert_eq!(super::slot_h(17.0), 17.0);
    }

    #[test]
    fn chips_centre_the_ink() {
        assert_eq!(super::ink_left(50.0, 6.0), 44.0);
        assert_eq!(super::chip_w(21.0), 37.0, "8 to the ink either side");
        assert_eq!(super::chip_w(5.5), 21.5, "a narrow key: at least 20, the padding on whole pixels");
        assert_eq!(super::chip_w(4.0), 20.0);
    }

    #[test]
    fn chips_pad_8_and_headings_sit_8_over_their_rows() {
        // Round 13 H1 with the real fonts at the captures' density: every chip's fill sits 8 from its key's ink on
        // both sides ("?" centred). Round 14 H1: every heading's ink 8 above its first row's chips, Global's rows
        // still on the 34 grid (so Global sits 33 under the last Pickers row; it was seated at 32).
        use crate::keys::Group;
        let p = crate::theme::LIGHT;
        let chip = crate::theme::chip_fill_on(crate::views::dialog_fill(&p), &p);
        let mut global_row_top = 0.0;
        let mut pickers_top = 0.0;
        let shapes = crate::testutil::paint_at2(egui::vec2(800.0, 800.0), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            pickers_top = ui.cursor().top();
            super::group(ui, &p, Group::Pickers, 120.0);
            let (row, header) = (super::row_h(ui, &p), super::header_h(ui));
            ui.add_space(super::section_gap(row, header));
            global_row_top = ui.cursor().top() + header + super::HEADER_TO_ROWS;
            super::group(ui, &p, Group::Global, 120.0);
        });
        let n = crate::keys::BINDINGS.iter().filter(|k| k.group == Group::Pickers).count() as f32;
        let header = crate::testutil::with_ui(super::header_h);
        let first_pickers_row = pickers_top + header + super::HEADER_TO_ROWS;
        assert_eq!(global_row_top - first_pickers_row, (n + 1.0) * super::ROW_PITCH, "Global's rows stay on the grid");
        let chips: Vec<egui::Rect> = crate::testutil::fills(&shapes).into_iter().filter(|(_, c)| *c == chip).map(|(r, _)| r).collect();
        let inks = crate::testutil::inks(&shapes);
        for g in [Group::Pickers, Group::Global] {
            let h = inks.iter().find(|(t, _)| t == g.name()).expect("heading").1;
            let first = chips.iter().filter(|r| r.top() > h.bottom()).map(|r| r.top()).fold(f32::MAX, f32::min);
            assert!((first - h.bottom() - super::HEADING_TO_ROW).abs() <= 0.5, "{}: {}", g.name(), first - h.bottom());
        }
        for c in &chips {
            let key = inks.iter().filter(|(_, r)| c.contains_rect(r.shrink(0.1))).map(|(_, r)| *r).reduce(|a, b| a.union(b)).expect("a key in the chip");
            let (l, r) = (key.left() - c.left(), c.right() - key.right());
            assert!((l - r).abs() <= 0.5, "centred: {l} / {r} in {c:?}");
            assert!(l >= super::CHIP_INK_X - 0.5, "8 to the ink: {l} in {c:?}");
        }
    }

    #[test]
    fn inline_words_sit_4_from_their_chips() {
        // Round 14 H3: "or" and "to" 4 from each chip's fill to their ink.
        let p = crate::theme::LIGHT;
        let chip = crate::theme::chip_fill_on(crate::views::dialog_fill(&p), &p);
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 900.0), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            super::group(ui, &p, crate::keys::Group::Issues, 200.0);
        });
        let chips: Vec<egui::Rect> = crate::testutil::fills(&shapes).into_iter().filter(|(_, c)| *c == chip).map(|(r, _)| r).collect();
        let words: Vec<egui::Rect> = crate::testutil::inks(&shapes).into_iter().filter(|(t, _)| t == "or" || t == "to").map(|(_, r)| r).collect();
        assert!(!words.is_empty());
        for w in words {
            let left = chips
                .iter()
                .filter(|c| c.right() <= w.left() && (c.center().y - w.center().y).abs() < 10.0)
                .map(|c| w.left() - c.right())
                .fold(f32::MAX, f32::min);
            let right = chips
                .iter()
                .filter(|c| c.left() >= w.right() && (c.center().y - w.center().y).abs() < 10.0)
                .map(|c| c.left() - w.right())
                .fold(f32::MAX, f32::min);
            assert!((left - super::CHIP_GAP).abs() <= 0.5 && (right - super::CHIP_GAP).abs() <= 0.5, "{left} / {right}");
        }
    }

    #[test]
    fn chips_are_fill_only() {
        for p in [&crate::theme::LIGHT, &crate::theme::DARK] {
            assert_eq!(super::chip_stroke(p), egui::Stroke::NONE, "dark={}", p.dark);
            let fill = crate::theme::chip_fill_on(crate::views::dialog_fill(p), p);
            assert!(crate::theme::contrast(fill, crate::views::dialog_fill(p)) >= if p.dark { 1.2 } else { 1.1 }, "dark={}", p.dark);
        }
    }

    #[test]
    fn rows_sit_34_apart() {
        assert_eq!(super::row_gap(18.0), 16.0);
        assert_eq!(super::row_gap(20.0), 14.0);
    }

    #[test]
    fn a_later_section_header_takes_one_row_pitch() {
        // H1: the header block (extra + header + 8) is one 34 pitch, so a second section's rows stay on the grid.
        for header in [15.0, 17.0] {
            let block = super::section_gap(18.0, header) - super::row_gap(18.0) + header + super::HEADER_TO_ROWS;
            assert_eq!(block, super::ROW_PITCH, "header {header}");
        }
        assert_eq!(super::section_gap(18.0, 15.0), super::row_gap(18.0) + 11.0);
        // Left: 11 Issues rows; right: 4 Pickers, a header block, 6 Global: last rows level.
        let (h, pitch) = (15.0, super::ROW_PITCH);
        let left = h + 8.0 + 10.0 * pitch;
        let right = h + 8.0 + 3.0 * pitch + pitch + (super::section_gap(18.0, h) - super::row_gap(18.0)) + h + 8.0 + 5.0 * pitch;
        assert_eq!(left, right);
    }

    #[test]
    fn help_columns_balance() {
        use crate::keys::{BINDINGS, Group};
        let n = |g: Group| BINDINGS.iter().filter(|k| k.group == g).count();
        // Left: Issues; right: Pickers then Global (one more header). Within a row of each other.
        let (left, right) = (n(Group::Issues), n(Group::Pickers) + n(Group::Global) + 1);
        assert!(left.abs_diff(right) <= 1, "left {left} rows, right {right}");
    }

    #[test]
    fn columns_hug_their_content() {
        assert_eq!(super::column_width(&[20.0, 72.5], &[100.0, 180.2]), 269.0);
        assert_eq!(super::column_width(&[], &[]), 16.0);
    }
}
