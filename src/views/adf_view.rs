//! Paints an `adf::Doc` with egui widgets.

use crate::adf::{self, Block, Span};
use crate::theme::{self, Palette};
use egui::text::LayoutJob;
use egui::{Frame, Margin, RichText, Stroke, TextFormat, Ui};

pub fn show_value(ui: &mut Ui, v: &serde_json::Value, p: &Palette) {
    let doc = adf::parse(v);
    if doc.blocks.is_empty() {
        ui.label(RichText::new("No description").italics().color(p.muted));
        return;
    }
    show(ui, &doc, p);
}

/// Space after a paragraph (or any block).
const BLOCK_GAP: f32 = 12.0;

/// Space above a heading: 24 except at the top, where the section heading already spaced it.
pub fn heading_space_above(first: bool) -> f32 {
    if first { 0.0 } else { 24.0 - BLOCK_GAP }
}

pub fn show(ui: &mut Ui, doc: &adf::Doc, p: &Palette) {
    let n = doc.blocks.len();
    for (i, b) in doc.blocks.iter().enumerate() {
        if matches!(b, Block::Heading { .. }) {
            ui.add_space(heading_space_above(i == 0));
        }
        block(ui, b, p, 14.0);
        if i + 1 < n {
            ui.add_space(if matches!(b, Block::Heading { .. }) { 8.0 } else { BLOCK_GAP });
        }
    }
}

fn block(ui: &mut Ui, b: &Block, p: &Palette, size: f32) {
    match b {
        Block::Paragraph(s) => spans(ui, s, p, size, false),
        Block::Heading { level, spans: s } => {
            let sz = match level {
                1 => 22.0,
                2 => 18.0,
                3 => 16.0,
                _ => 14.5,
            };
            spans(ui, s, p, sz, true);
        }
        Block::List { ordered, items } => {
            for (i, item) in items.iter().enumerate() {
                ui.horizontal_top(|ui| {
                    ui.add_space(8.0);
                    let marker = if *ordered { format!("{}.", i + 1) } else { "•".to_string() };
                    ui.label(RichText::new(marker).color(p.muted).size(size));
                    ui.vertical(|ui| {
                        for ib in item {
                            block(ui, ib, p, size);
                        }
                    });
                });
            }
        }
        Block::Code { text, .. } => {
            Frame::new().fill(p.surface_2).corner_radius(6).inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).font(theme::mono(12.5)).color(p.text));
            });
        }
        Block::Quote(bs) => {
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 0.0), egui::Sense::hover());
                let top = rect.top();
                let inner = ui.vertical(|ui| {
                    for ib in bs {
                        block(ui, ib, p, size);
                    }
                });
                let bottom = inner.response.rect.bottom();
                ui.painter().vline(rect.left() + 1.5, top..=bottom, Stroke::new(3.0, p.border));
            });
        }
        Block::Panel { kind, blocks } => {
            let (bg, border) = match kind.as_str() {
                "warning" => (p.at_risk.gamma_multiply(0.15), p.at_risk),
                "error" => (p.off_track.gamma_multiply(0.15), p.off_track),
                "success" => (p.on_track.gamma_multiply(0.15), p.on_track),
                _ => (p.accent.gamma_multiply(0.12), p.accent),
            };
            Frame::new().fill(bg).stroke(Stroke::new(1.0, border)).corner_radius(6).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for ib in blocks {
                    block(ui, ib, p, size);
                }
            });
        }
        Block::Table(rows) => {
            egui::Grid::new(ui.next_auto_id()).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                for (ri, row) in rows.iter().enumerate() {
                    for cell in row {
                        spans(ui, cell, p, 13.0, ri == 0);
                    }
                    ui.end_row();
                }
            });
        }
        Block::Rule => {
            ui.separator();
        }
    }
}

/// The text format one span is painted with. Links are accent, underlined only while `hovered`.
/// Inline code is the mono face alone, no background.
fn span_format(sp: &Span, p: &Palette, size: f32, heading: bool, hovered: bool) -> TextFormat {
    let mut fmt = TextFormat { font_id: if sp.bold || heading { theme::bold(size) } else { theme::body(size) }, color: p.text, ..Default::default() };
    if sp.code {
        fmt.font_id = theme::mono(size - 1.5);
    }
    if sp.italic {
        fmt.italics = true;
    }
    if sp.strike {
        fmt.strikethrough = Stroke::new(1.0, p.muted);
    }
    if sp.link.is_some() {
        fmt.color = p.accent;
        if hovered {
            fmt.underline = Stroke::new(1.0, p.accent);
        }
    }
    fmt
}

fn spans(ui: &mut Ui, s: &[Span], p: &Palette, size: f32, heading: bool) {
    let s = adf::linkify(s);
    let has_link = s.iter().any(|sp| sp.link.is_some());
    // The label's id is the next auto id; last frame's hover decides the underline.
    let hovered = has_link && ui.ctx().read_response(ui.next_auto_id()).is_some_and(|r| r.hovered());
    let mut job = LayoutJob::default();
    for sp in &s {
        job.append(&sp.text, 0.0, span_format(sp, p, size, heading, hovered));
    }
    let sense = if has_link { egui::Sense::click() } else { egui::Sense::hover() };
    let r = ui.add(egui::Label::new(job).wrap().sense(sense));
    if has_link {
        r.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    // One link per paragraph is the common case; open the first on click.
    if r.clicked()
        && let Some(url) = s.iter().find_map(|sp| sp.link.clone())
        && crate::app::openable(&url)
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    fn palette() -> Palette {
        let c = |v: u8| Color32::from_gray(v);
        Palette {
            dark: false,
            bg: c(1),
            surface: c(2),
            surface_2: c(3),
            border: c(4),
            text: c(5),
            muted: c(6),
            accent: c(7),
            on_accent: c(18),
            on_track: c(8),
            at_risk: c(9),
            off_track: c(10),
            no_health: c(11),
            done: c(12),
            today: c(13),
            selected_row: c(14),
            hover_row: c(15),
            danger_subtle: c(16),
            tooltip: c(17),
        }
    }

    fn span(text: &str) -> Span {
        Span { text: text.into(), bold: false, italic: false, code: false, strike: false, link: None }
    }

    #[test]
    fn span_format_maps_marks() {
        let p = palette();
        let plain = span_format(&span("a"), &p, 14.0, false, false);
        assert_eq!(plain.font_id, theme::body(14.0));
        assert_eq!(plain.color, p.text);
        assert_eq!(plain.underline, Stroke::NONE);

        let bold = span_format(&Span { bold: true, ..span("b") }, &p, 14.0, false, false);
        assert_eq!(bold.font_id, theme::bold(14.0));
        assert_eq!(span_format(&span("h"), &p, 18.0, true, false).font_id, theme::bold(18.0));

        let code = span_format(&Span { code: true, ..span("c") }, &p, 14.0, false, false);
        assert_eq!(code.font_id, theme::mono(12.5));
        assert_eq!(code.background, Color32::TRANSPARENT, "inline code is the mono face only");

        let styled = span_format(&Span { italic: true, strike: true, ..span("s") }, &p, 14.0, false, false);
        assert!(styled.italics);
        assert_eq!(styled.strikethrough, Stroke::new(1.0, p.muted));

        let link = Span { link: Some("https://x".into()), ..span("l") };
        assert_eq!(span_format(&link, &p, 14.0, false, false).color, p.accent);
        assert_eq!(span_format(&link, &p, 14.0, false, false).underline, Stroke::NONE, "underline on hover only");
        assert_eq!(span_format(&link, &p, 14.0, false, true).underline, Stroke::new(1.0, p.accent));
    }

    #[test]
    fn headings_get_24_above_except_first() {
        assert_eq!(heading_space_above(true), 0.0);
        assert_eq!(heading_space_above(false) + BLOCK_GAP, 24.0);
    }
}
