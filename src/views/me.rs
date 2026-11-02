//! The Me screen: the signed-in user's tickets (`me::sections`) as Team-style cards in three sections on the
//! left, and the focused ticket's detail on the right.

use crate::app::{Action, App};
use crate::me::{self, Section};
use crate::store::ListId;
use crate::teamview::Card;
use crate::theme::{self, Palette};
use egui::{Frame, Margin, RichText, Ui};

/// Between sections, and under a section's header.
const SECTION_GAP: f32 = 16.0;
/// Between cards.
const CARD_GAP: f32 = 8.0;
/// Under a section title, before its count.
const LINE_GAP: f32 = 4.0;

/// A section's header count: "2 tickets".
pub fn count_text(n: usize) -> String {
    format!("{n} {}", if n == 1 { "ticket" } else { "tickets" })
}

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
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: super::issues::PANE_TOP as i8, bottom: 0 }))
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

fn list(app: &App, ui: &mut Ui, p: &Palette, acts: &mut Vec<Action>) {
    let loaded = app.lists.get(&ListId::Me);
    let known = app.store.lists.contains_key(&ListId::Me);
    if !known {
        match loaded.and_then(|l| l.error.as_deref()) {
            Some(e) => {
                if super::error_banner(ui, p, "Couldn't load your tickets.", e) {
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
    egui::ScrollArea::vertical().id_salt("me-sections").auto_shrink([false, false]).show(ui, |ui| {
        let mut row = 0;
        for (si, (s, cards)) in sections.iter().enumerate() {
            if si > 0 {
                ui.add_space(SECTION_GAP);
            }
            header(ui, p, *s, cards.len());
            ui.add_space(SECTION_GAP / 2.0);
            ui.spacing_mut().item_spacing.y = CARD_GAP;
            if cards.is_empty() {
                ui.label(RichText::new(s.empty()).italics().color(p.muted).size(12.0));
            }
            for (ci, c) in cards.iter().enumerate() {
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
        }
    });
}

fn header(ui: &mut Ui, p: &Palette, s: Section, n: usize) {
    ui.label(RichText::new(s.title()).font(theme::bold(13.5)).color(p.text));
    ui.add_space(LINE_GAP - ui.spacing().item_spacing.y);
    ui.label(RichText::new(count_text(n)).color(p.muted).size(12.0));
}

/// The Team screen's card, its egui ids kept apart from Team's by the section offset.
fn card(app: &App, ui: &mut Ui, p: &Palette, (si, ci): (usize, usize), c: &Card, sel: bool) -> egui::Response {
    super::team::card(app, ui, p, (usize::MAX - si, ci), c, sel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status};

    fn cfg() -> crate::config::Config {
        toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap()
    }

    #[test]
    fn wording() {
        assert_eq!(count_text(0), "0 tickets");
        assert_eq!(count_text(1), "1 ticket");
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
        for want in [
            "In progress",
            "To do in open sprints",
            "Done in the last 7 days",
            "1 ticket",
            "0 tickets",
            "nothing to do in an open sprint",
            "CI-1",
            "Mine now",
            "CI-2",
        ] {
            assert!(drawn.iter().any(|t| t == want), "{want} in {drawn:?}");
        }
        assert!(!app.actions.is_empty(), "the first in-progress ticket opens in the detail pane");
        assert!(app.actions.iter().all(|a| matches!(a, Action::Select(k) if k == "CI-1")), "and nothing else");
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
