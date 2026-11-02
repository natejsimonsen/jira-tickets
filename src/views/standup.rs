//! The Standup screen: the review's window and progress, the standup summary (copyable), then one card per
//! suggested ticket with its quick actions.

use crate::app::{Action, App, StandupPhase};
use crate::standup::{self, Saved, Suggestion, Target};
use crate::theme::{self, Palette};
use crate::views::{PILL_MARGIN, button, outline_pill, primary_button, text_link};
use egui::{Frame, Margin, RichText, Stroke, Ui, vec2};

/// The page's reading width: wider reads poorly.
const MAX_W: f32 = 880.0;
/// Between sections, and between cards.
const GAP: f32 = 16.0;
const CARD_GAP: f32 = 8.0;
const CARD_PAD: i8 = 12;
const LINE_GAP: f32 = 6.0;

/// "0:07", "1:24".
pub fn elapsed_text(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// What a running review is doing, for the progress line.
pub fn progress_text(phase: StandupPhase, activity: Option<usize>, secs: u64) -> String {
    match phase {
        StandupPhase::Jira => format!("Reading your Jira activity… {}", elapsed_text(secs)),
        StandupPhase::Claude => {
            let n = activity.unwrap_or(0);
            let issues = if n == 1 { "1 Jira issue".to_string() } else { format!("{n} Jira issues") };
            format!("Asking Claude about your GitHub and Slack work ({issues} found)… {}", elapsed_text(secs))
        }
    }
}

/// The line under the title: the window, and when the shown review ran.
pub fn window_text(saved: &Saved, today: chrono::NaiveDate) -> String {
    match (saved.ran_on, saved.ran_at.and_then(|t| chrono::DateTime::from_timestamp(t, 0))) {
        (Some(day), Some(at)) => {
            let at = at.with_timezone(&chrono::Local);
            format!("Since {} · ran {}", standup::day_label(standup::window_start(day)), at.format("%a %H:%M"))
        }
        _ => format!("Since {} · not run yet", standup::day_label(standup::window_start(today))),
    }
}

/// "80% sure".
pub fn confidence_text(c: f32) -> String {
    format!("{:.0}% sure", (c * 100.0).round())
}

/// What the card list says when it has no cards.
pub fn no_cards_text(saved: &Saved) -> &'static str {
    match &saved.review {
        Some(r) if r.suggestions.is_empty() => "No untracked work found",
        Some(_) => "Every suggestion was dismissed",
        None => "No review yet. It runs by itself on Monday, Wednesday and Friday, or press Run now.",
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let p = theme::palette(ui.ctx());
    let mut acts = Vec::new();
    if app.standup.phase.is_some() {
        // The elapsed time ticks.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: crate::views::issues::PANE_TOP as i8, bottom: 0 }))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("standup").auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(MAX_W.min(ui.available_width()));
                ui.spacing_mut().item_spacing.y = LINE_GAP;
                heading(app, ui, p, &mut acts);
                ui.add_space(GAP);
                if let Some(review) = app.standup.saved.review.clone() {
                    summary(ui, p, &review, &mut acts);
                    ui.add_space(GAP);
                }
                cards(app, ui, p, &mut acts);
                ui.add_space(GAP);
            });
        });
    app.actions.extend(acts);
}

/// The title, the window line, the run button, the progress or the error.
fn heading(app: &App, ui: &mut Ui, p: &Palette, acts: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Standup").font(theme::bold(18.0)).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let running = app.standup.phase.is_some();
            let label = if app.standup.saved.review.is_some() { "Run again" } else { "Run now" };
            let r = ui.add_enabled(!running, primary_button(label, p));
            if crate::views::tooltip(r, "r").clicked() {
                acts.push(Action::StandupRun);
            }
        });
    });
    let today = chrono::Local::now().date_naive();
    ui.label(RichText::new(window_text(&app.standup.saved, today)).color(p.muted).size(12.0));
    if let (Some(phase), Some(started)) = (app.standup.phase, app.standup.started) {
        ui.horizontal(|ui| {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            ui.label(RichText::new(progress_text(phase, app.standup.activity_count, started.elapsed().as_secs())).color(p.muted).size(13.0));
        });
    }
    if let Some(e) = &app.standup.error
        && app.standup.phase.is_none()
        && crate::views::error_banner(ui, p, "The standup review failed", e)
    {
        acts.push(Action::StandupRun);
    }
}

/// The summary's three lists, a Copy button, and the review's own notes.
fn summary(ui: &mut Ui, p: &Palette, review: &standup::Review, acts: &mut Vec<Action>) {
    Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(theme::RADIUS).inner_margin(Margin::same(CARD_PAD)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = LINE_GAP;
        let s = &review.summary;
        for (i, (title, items)) in [("Since the last standup", &s.done), ("Today", &s.today), ("Blockers", &s.blockers)].into_iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(theme::bold(14.0)).color(p.text));
                if i == 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::views::tooltip(ui.add(button("Copy", p)), "y").clicked() {
                            acts.push(Action::StandupCopy);
                        }
                    });
                }
            });
            if items.is_empty() {
                ui.label(RichText::new("None").color(p.muted).size(13.0));
            }
            for item in items.iter() {
                ui.add(egui::Label::new(RichText::new(format!("•  {item}")).color(p.text).size(13.5)).wrap());
            }
            ui.add_space(LINE_GAP);
        }
        if !review.notes.is_empty() {
            ui.add(egui::Label::new(RichText::new(&review.notes).italics().color(p.muted).size(12.0)).wrap());
        }
    });
}

/// "Suggested tickets (N)", then the cards, or why there are none.
fn cards(app: &mut App, ui: &mut Ui, p: &Palette, acts: &mut Vec<Action>) {
    let visible: Vec<Suggestion> = app.standup.saved.visible().into_iter().cloned().collect();
    let dismissed = app.standup.saved.dismissed.len();
    let mut head = format!("Suggested tickets ({})", visible.len());
    if dismissed > 0 {
        head.push_str(&format!(" · {dismissed} dismissed"));
    }
    ui.label(RichText::new(head).font(theme::bold(14.0)).color(p.text));
    if visible.is_empty() {
        crate::views::empty_state(ui, p, no_cards_text(&app.standup.saved));
        return;
    }
    let cursor = app.standup.cursor.min(visible.len() - 1);
    let last_id = egui::Id::new("standup-last-cursor");
    let moved = ui.data(|d| d.get_temp::<usize>(last_id)) != Some(cursor);
    ui.data_mut(|d| d.insert_temp(last_id, cursor));
    ui.spacing_mut().item_spacing.y = CARD_GAP;
    for (i, s) in visible.iter().enumerate() {
        let key = standup::dismiss_key(s);
        let sel = i == cursor;
        let focus_title = sel && std::mem::take(&mut app.standup.focus_title);
        let added = app.standup.saved.added.get(&key).cloned();
        let adding = app.standup.adding.contains(&key);
        let title = app.standup.titles.entry(key.clone()).or_insert_with(|| s.title.clone());
        let r = card(ui, p, s, title, CardState { sel, focus_title, added: added.as_deref(), adding, browse: &app.cfg.site }, i, acts);
        if sel && moved {
            r.scroll_to_me(None);
        }
    }
    // Untouched titles are not kept as edits.
    let originals: std::collections::HashMap<String, String> = visible.iter().map(|s| (standup::dismiss_key(s), s.title.clone())).collect();
    app.standup.titles.retain(|k, t| originals.get(k) != Some(t));
}

struct CardState<'a> {
    sel: bool,
    focus_title: bool,
    added: Option<&'a str>,
    adding: bool,
    browse: &'a str,
}

/// One suggestion: its tags, its editable title, what and why, its source, and the quick actions.
fn card(ui: &mut Ui, p: &Palette, s: &Suggestion, title: &mut String, st: CardState, i: usize, acts: &mut Vec<Action>) -> egui::Response {
    let fill = if st.sel { p.selected_row } else { p.surface };
    let edge = if st.sel { p.accent } else { p.border };
    let r = Frame::new().fill(fill).stroke(Stroke::new(1.0, edge)).corner_radius(theme::RADIUS).inner_margin(Margin::same(CARD_PAD)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = LINE_GAP;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, LINE_GAP);
            outline_pill(ui, s.source_kind.name(), p, PILL_MARGIN);
            outline_pill(ui, &s.issue_type, p, PILL_MARGIN);
            outline_pill(ui, &confidence_text(s.confidence), p, PILL_MARGIN);
            ui.label(RichText::new(format!("suggests {}", s.suggested_target.name().to_lowercase())).color(p.muted).size(12.0));
        });
        match st.added {
            Some(issue) => {
                ui.horizontal(|ui| {
                    if text_link(ui, issue, theme::mono(12.5), p.accent).clicked() {
                        acts.push(Action::OpenUrl(format!("{}/browse/{issue}", st.browse.trim_end_matches('/'))));
                    }
                    ui.add(egui::Label::new(RichText::new(title.as_str()).font(theme::bold(14.0)).color(p.text)).wrap());
                });
            }
            None => {
                let id = egui::Id::new(("standup-title", standup::dismiss_key(s)));
                let r = crate::views::add_field(ui, None, title, |t| crate::views::input_field(t, "Ticket title", theme::bold(14.0), p).id(id));
                if st.focus_title {
                    r.request_focus();
                }
                if r.gained_focus() {
                    acts.push(Action::StandupFocus(i));
                }
            }
        }
        if !s.description.is_empty() {
            ui.add(egui::Label::new(RichText::new(&s.description).color(p.text).size(13.0)).wrap());
        }
        if !s.reason.is_empty() {
            ui.add(egui::Label::new(RichText::new(&s.reason).italics().color(p.muted).size(12.0)).wrap());
        }
        if !s.source_url.is_empty() {
            let painter = ui.painter().clone();
            let shown = crate::views::truncate_text(&s.source_url, ui.available_width(), |t| {
                painter.layout_no_wrap(t.to_string(), theme::body(12.0), p.accent).size().x
            });
            let r = crate::views::tooltip(text_link(ui, &shown, theme::body(12.0), p.accent), format!("{} (o)", s.source_url));
            if r.clicked() {
                acts.push(Action::StandupFocus(i));
                acts.push(Action::StandupOpenSource);
            }
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(issue) = st.added {
                ui.label(RichText::new(format!("Added as {issue}")).color(p.on_track).size(13.0));
            } else if st.adding {
                crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
                ui.label(RichText::new("Adding…").color(p.muted).size(13.0));
            } else {
                for (target, key) in [(Target::CurrentSprint, "c"), (Target::NextSprint, "n"), (Target::Backlog, "b")] {
                    let b = if target == s.suggested_target { primary_button(target.name(), p) } else { button(target.name(), p) };
                    if crate::views::tooltip(ui.add(b), key).clicked() {
                        acts.push(Action::StandupFocus(i));
                        acts.push(Action::StandupAdd(target));
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::views::tooltip(ui.add(button("✕", p)), "Dismiss for good (x)").clicked() {
                    acts.push(Action::StandupFocus(i));
                    acts.push(Action::StandupDismiss);
                }
            });
        });
    });
    r.response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standup::Review;

    #[test]
    fn wording() {
        assert_eq!(elapsed_text(7), "0:07");
        assert_eq!(elapsed_text(84), "1:24");
        assert_eq!(progress_text(StandupPhase::Jira, None, 3), "Reading your Jira activity… 0:03");
        assert_eq!(progress_text(StandupPhase::Claude, Some(1), 60), "Asking Claude about your GitHub and Slack work (1 Jira issue found)… 1:00");
        assert_eq!(progress_text(StandupPhase::Claude, Some(4), 61), "Asking Claude about your GitHub and Slack work (4 Jira issues found)… 1:01");
        assert_eq!(confidence_text(0.8), "80% sure");
        assert_eq!(confidence_text(0.0), "0% sure");
    }

    #[test]
    fn window_line_names_the_window_and_the_run() {
        let mon = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        assert_eq!(window_text(&Saved::default(), mon), "Since Fri Oct 30 · not run yet");
        let ran = Saved { ran_on: chrono::NaiveDate::from_ymd_opt(2026, 11, 4), ran_at: Some(1_790_000_000), ..Default::default() };
        assert!(window_text(&ran, mon).starts_with("Since Mon Nov 2 · ran "), "the window of the day it ran");
    }

    #[test]
    fn empty_card_lists_say_why() {
        assert!(no_cards_text(&Saved::default()).starts_with("No review yet"));
        let none = Saved { review: Some(Review::default()), ..Default::default() };
        assert_eq!(no_cards_text(&none), "No untracked work found");
        let review = Review { suggestions: vec![Suggestion { title: "A".into(), ..Default::default() }], ..Default::default() };
        let all_gone = Saved { review: Some(review), ..Default::default() };
        assert_eq!(no_cards_text(&all_gone), "Every suggestion was dismissed");
    }

    #[test]
    fn screen_draws_headless() {
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        app.screen = crate::app::Screen::Standup;
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 900.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let empty = texts(&mut app);
        assert!(empty.iter().any(|t| t == "Run now") && empty.iter().any(|t| t.starts_with("No review yet")), "{empty:?}");
        let sug = |title: &str, url: &str| Suggestion {
            title: title.into(),
            description: "Post each rollout.".into(),
            issue_type: "Story".into(),
            source_url: url.into(),
            reason: "No ticket names this PR".into(),
            confidence: 0.8,
            suggested_target: Target::NextSprint,
            ..Default::default()
        };
        app.standup.saved.review = Some(Review {
            summary: standup::Summary { done: vec!["Shipped the rollout fix".into()], today: vec![], blockers: vec![] },
            suggestions: vec![sug("Rollout notifications", "https://github.com/o/r/pull/7"), sug("Logger fix", "https://github.com/o/r/pull/8")],
            notes: "Slack was not available".into(),
        });
        app.standup.saved.added.insert("url:https://github.com/o/r/pull/8".into(), "CI-42".into());
        app.standup.phase = Some(StandupPhase::Claude);
        app.standup.started = Some(std::time::Instant::now());
        app.standup.activity_count = Some(2);
        let drawn = texts(&mut app);
        for want in [
            "Run again",
            "Since the last standup",
            "•  Shipped the rollout fix",
            "Slack was not available",
            "Suggested tickets (2)",
            "Rollout notifications",
            "Post each rollout.",
            "80% sure",
            "Current sprint",
            "Next sprint",
            "Backlog",
            "Added as CI-42",
            "https://github.com/o/r/pull/7",
        ] {
            assert!(drawn.iter().any(|t| t == want), "{want} in {drawn:?}");
        }
        assert!(drawn.iter().any(|t| t.starts_with("Asking Claude")), "progress shows: {drawn:?}");
        assert_eq!(drawn.iter().filter(|t| *t == "Current sprint").count(), 1, "the added card has no add buttons");
        assert!(app.standup.titles.is_empty(), "untouched titles are not kept as edits");
    }
}
