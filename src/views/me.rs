//! The Me screen: the signed-in user's tickets (`me::sections`) as Team-style cards in three sections on the
//! left, and the focused ticket's detail on the right.

use crate::app::{Action, App};
use crate::me;
use crate::store::ListId;
use crate::teamview::Card;
use crate::theme::{self, Palette};
use crate::views::components as c;
use egui::{Frame, Margin, Ui};

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let mut acts = Vec::new();
    // The detail pane shows the focused ticket: keep the selection on it as rows land and the cursor moves.
    let focused = app.me_focused();
    if let Some(key) = &focused
        && app.selected.as_ref() != Some(key)
    {
        acts.push(Action::Select(key.clone()));
    }
    let (list_w, list_range) = super::issues::list_pane_width(ui.available_width());
    egui::Panel::left("me-list")
        .resizable(true)
        .default_size(list_w)
        .size_range(list_range)
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 0, top: super::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| list(app, ui, p, &mut acts));
    let pane = ui.available_rect_before_wrap();
    app.detail_x = Some((pane.left(), pane.right()));
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.surface).inner_margin(Margin { left: 16, right: 0, top: super::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            if focused.is_some() {
                super::detail::show(app, ui);
            } else {
                super::empty_state(ui, p, "Nothing of yours to show");
            }
        });
    app.actions.extend(acts);
}

/// The list's right inset inside its panel (the thumb runs in it).
const LIST_RIGHT: f32 = 16.0;
/// A long section shows this many cards, then "N more", so every section header fits on a laptop screen.
const PREVIEW: usize = 3;

/// How many of a section's `n` cards to draw: all when `open` or when hiding would save only one, else PREVIEW.
pub fn shown(n: usize, open: bool) -> usize {
    if open || n <= PREVIEW + 1 { n } else { PREVIEW }
}

/// The space to add before a section header so the gap from the previous section's last item is `section_gap`,
/// given that item already left `item_gap` (the card gap) under itself.
pub fn section_space(section_gap: f32, item_gap: f32) -> f32 {
    (section_gap - item_gap).max(0.0)
}

/// The space above the list's first section header that puts its capitals level with the detail pane's key: the
/// key sits centred in a `row_h` row (`line_h` its line box, `key_top` its capitals into it), the header's capitals
/// `header_top` into its own line box.
pub fn first_header_space(row_h: f32, line_h: f32, key_top: f32, header_top: f32) -> f32 {
    ((row_h - line_h) / 2.0 + key_top - header_top).max(0.0)
}

fn list(app: &App, ui: &mut Ui, p: &Palette, acts: &mut Vec<Action>) {
    let loaded = app.lists.get(&ListId::Me);
    let known = app.store.lists.contains_key(&ListId::Me);
    if !known {
        ui.set_max_width(ui.available_width() - LIST_RIGHT);
        match loaded.and_then(|l| l.error.as_deref()) {
            Some(e) => {
                if super::error_banner(ui, p, "Couldn't load your tickets.", e, !app.offline) {
                    acts.push(Action::Refresh);
                }
            }
            None => {
                super::icons::spinner(ui, super::icons::INLINE_SPINNER, p);
            }
        }
        return;
    }
    let sections = app.me_sections();
    let cursor = me::moved(app.me_view.cursor, 0, me::row_count(&sections));
    let last_id = egui::Id::new("me-last-cursor");
    let moved = ui.data(|d| d.get_temp::<usize>(last_id)) != Some(cursor);
    ui.data_mut(|d| d.insert_temp(last_id, cursor));
    let sp = theme::spacing(ui.ctx());
    // The same thumb as the detail pane: shown at rest whenever the list overflows. The panel has no right margin:
    // the bar runs in the LIST_RIGHT inset the content leaves, clear of the cards.
    crate::views::no_scroll_fade(ui);
    crate::views::floating_thumb(&mut ui.spacing_mut().scroll);
    let saved = crate::views::set_thumb_fills(ui, p);
    let track = crate::views::thumb_track(ui.available_rect_before_wrap());
    let out = egui::ScrollArea::vertical().id_salt("me-sections").auto_shrink([false, false]).scroll_bar_rect(track).show(ui, |ui| {
        crate::views::restore_fills(ui, saved);
        ui.set_max_width(ui.available_width() - LIST_RIGHT);
        // Both panes start on one line: the first header's capitals level with the detail key's.
        let ctx = ui.ctx().clone();
        let key_font = theme::mono(12.5);
        let key_line = ui.painter().layout_no_wrap("CI".to_string(), key_font.clone(), p.muted).size().y;
        let header_top = theme::Ink::of(&ctx, &theme::bold(sp.h3), None).top;
        ui.add_space(first_header_space(crate::views::BUTTON_H, key_line, theme::Ink::of(&ctx, &key_font, None).top, header_top));
        let mut row = 0;
        for (si, (s, cards)) in sections.iter().enumerate() {
            // The density's section gap between sections, ink edge to ink edge: the last card (or empty state)
            // already left a card gap under itself.
            if si > 0 {
                ui.add_space(section_space(sp.section_gap, sp.card_gap));
            }
            // A header sits a card gap over its own cards.
            ui.spacing_mut().item_spacing.y = 0.0;
            c::section_header(ui, p, s.title(), Some(cards.len()));
            ui.add_space(sp.card_gap);
            ui.spacing_mut().item_spacing.y = sp.card_gap;
            if cards.is_empty() {
                c::empty_state(ui, p, s.empty(), None);
            }
            // A long section folds after PREVIEW cards; the cursor moving onto a folded card opens it.
            let open_id = egui::Id::new(("me-section-open", si));
            let start = row;
            let cursor_inside = (start + PREVIEW..start + cards.len()).contains(&cursor);
            let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) || cursor_inside;
            let n = shown(cards.len(), open);
            for (ci, c) in cards.iter().enumerate().take(n) {
                let sel = row == cursor;
                let resp = card(app, ui, p, (si, ci), c, sel);
                if resp.clicked() || resp.double_clicked() {
                    acts.push(Action::MeMove(row as i32 - cursor as i32));
                }
                if resp.double_clicked() {
                    acts.push(Action::MeOpen);
                }
                if sel && moved {
                    resp.scroll_to_me(None);
                }
                row += 1;
            }
            row = start + cards.len();
            let folded = cards.len() - n;
            let toggle = if folded > 0 {
                Some((format!("{folded} more"), true))
            } else if shown(cards.len(), false) < cards.len() && !cursor_inside {
                Some(("Show fewer".to_string(), false))
            } else {
                None
            };
            if let Some((label, to)) = toggle
                && ui.add(c::ghost(&label, p)).clicked()
            {
                ui.data_mut(|d| d.insert_temp(open_id, to));
            }
        }
    });
    c::scroll_edge_fade(ui, &out, p.bg);
}

/// The Team screen's card, its egui ids kept apart from Team's by the section offset. The list is wide, so the
/// epic sits on the status line and every card keeps one height.
fn card(app: &App, ui: &mut Ui, p: &Palette, (si, ci): (usize, usize), c: &Card, sel: bool) -> egui::Response {
    super::team::card(app, ui, p, (usize::MAX - si, ci), c, sel, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status};

    fn cfg() -> crate::config::Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap()
    }

    #[test]
    fn sections_draw_headless_and_select_the_first_row() {
        let mut app = App::for_test(cfg(), None);
        app.screen = crate::app::Screen::Me;
        let issue = |key: &str, summary: &str, cat: &str| Issue {
            key: key.into(),
            summary: summary.into(),
            status: Status { name: "Doing".into(), category: cat.into() },
            ..Default::default()
        };
        app.store.absorb(ListId::Me, vec![issue("CI-1", "Mine now", "indeterminate"), issue("CI-2", "Shipped", "done")]);
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let drawn: Vec<String> = crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect();
        for want in ["In progress", "To do in open sprints", "Done in the last 7 days", "1", "0", "nothing to do in an open sprint", "CI-1", "Mine now", "CI-2"]
        {
            assert!(drawn.iter().any(|t| t == want), "{want} in {drawn:?}");
        }
        assert!(!app.actions.is_empty(), "the first in-progress ticket opens in the detail pane");
        assert!(app.actions.iter().all(|a| matches!(a, Action::Select(k) if k == "CI-1")), "and nothing else");
    }

    #[test]
    fn a_long_section_folds() {
        assert_eq!(shown(6, false), PREVIEW);
        assert_eq!(shown(6, true), 6);
        assert_eq!(shown(PREVIEW + 1, false), PREVIEW + 1, "never \"1 more\"");
        assert_eq!(shown(0, false), 0);
    }

    #[test]
    fn long_to_do_folds_so_done_shows() {
        let mut app = App::for_test(cfg(), None);
        app.screen = crate::app::Screen::Me;
        let issue = |key: String, cat: &str| Issue {
            key,
            summary: "Something".into(),
            status: Status { name: "Doing".into(), category: cat.into() },
            ..Default::default()
        };
        let todo: Vec<Issue> = (1..=6).map(|n| issue(format!("CI-{n}"), "new")).collect();
        app.store.absorb(ListId::Me, todo);
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let drawn: Vec<String> = crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect();
        assert!(drawn.iter().any(|t| t == "3 more"), "{drawn:?}");
        assert!(!drawn.iter().any(|t| t == "CI-4"), "folded: {drawn:?}");
        assert!(drawn.iter().any(|t| t == "Done in the last 7 days"), "{drawn:?}");
    }

    #[test]
    fn sections_are_one_section_gap_apart() {
        assert_eq!(section_space(theme::COMPACT.section_gap, theme::COMPACT.card_gap), 24.0);
        assert_eq!(section_space(theme::COMFORTABLE.section_gap, theme::COMFORTABLE.card_gap), 28.0);
    }

    #[test]
    fn the_first_header_lines_up_with_the_detail_key() {
        assert_eq!(first_header_space(28.0, 16.0, 4.0, 3.0), 7.0);
        assert_eq!(first_header_space(16.0, 20.0, 1.0, 5.0), 0.0, "never negative");
    }

    #[test]
    fn an_unloaded_list_draws_no_sections() {
        let mut app = App::for_test(cfg(), None);
        let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 800.0), |ui| show(&mut app, ui));
        let drawn: Vec<String> = crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect();
        assert!(!drawn.iter().any(|t| t == "In progress"), "{drawn:?}");
        assert!(app.actions.is_empty());
    }
}
