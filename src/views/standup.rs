//! The Standup screen: one centred reading column. A toolbar row (the window line, Copy and the run or Stop
//! button), the run's progress or error, the standup summary, the window's tickets as Jira-style cards (done since
//! the window opened, then in progress), one quiet card per suggested ticket (title, meta line, a short
//! description, the adds and Dismiss), then the review's own notes.

use crate::app::{Action, App, StandupPhase};
use crate::keys::{StandupHint, StandupRun};
use crate::standup::{self, Saved, Suggestion, Target};
use crate::teamview::Card;
use crate::theme::{self, Palette};
use crate::views::components as c;
use crate::views::{adf_view, button, primary_button, text_link};
use egui::text::{LayoutJob, TextFormat};
use egui::{Frame, Margin, RichText, Ui, vec2};

/// How many lines a collapsed card's description shows, More included.
const PREVIEW_LINES: usize = 2;

/// The column: prose width plus a card's padding either side, so card text runs at the prose measure; never wider
/// than the pane.
pub fn column_width(sp: &theme::Spacing, avail: f32) -> f32 {
    (sp.prose_max_width + 2.0 * sp.card_pad).min(avail).max(0.0)
}

/// "0:07", "1:24".
pub fn elapsed_text(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// What a running review is doing, then its clock after a separator: "Reading Jira activity · 1:24".
pub fn progress_text(phase: StandupPhase, activity: Option<usize>, secs: u64) -> String {
    let doing = match phase {
        StandupPhase::Jira => "Reading Jira activity".to_string(),
        StandupPhase::Claude => {
            let n = activity.unwrap_or(0);
            let issues = if n == 1 { "1 Jira issue".to_string() } else { format!("{n} Jira issues") };
            format!("Asking Claude about your GitHub and Slack work ({issues} found)")
        }
    };
    format!("{doing} · {}", elapsed_text(secs))
}

/// The toolbar's window line: the window, and when the shown review ran (or that the only run so far failed).
pub fn window_text(saved: &Saved, today: chrono::NaiveDate, failed: bool) -> String {
    match (saved.ran_on, saved.ran_at.and_then(|t| chrono::DateTime::from_timestamp(t, 0))) {
        (Some(day), Some(at)) => {
            let at = at.with_timezone(&chrono::Local);
            format!("Since {} · ran {}", standup::day_label(standup::window_start(day)), at.format("%a %H:%M"))
        }
        _ => format!("Since {} · {}", standup::day_label(standup::window_start(today)), if failed { "last run failed" } else { "not run yet" }),
    }
}

/// The largest `n` in `0..=total` with `fits(n)`, given `fits` holds up to some point and fails after it.
pub fn fit_count(total: usize, mut fits: impl FnMut(usize) -> bool) -> usize {
    if fits(total) {
        return total;
    }
    let (mut lo, mut hi) = (0, total);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits(mid) { lo = mid } else { hi = mid }
    }
    lo
}

/// "80% sure".
pub fn confidence_text(c: f32) -> String {
    format!("{:.0}% sure", (c * 100.0).round())
}

/// A card's quiet meta line, before its source link: "GitHub · Task · 85% sure". `issue_type` is what an add
/// creates (`App::standup_type_name`), left out while it is not known.
pub fn meta_text(s: &Suggestion, issue_type: Option<&str>) -> String {
    let ty = issue_type.map(|t| format!(" · {t}")).unwrap_or_default();
    format!("{}{ty} · {}", s.source_kind.name(), confidence_text(s.confidence))
}

/// What stands in for an add button when the board has no sprint for it: "No next sprint".
pub fn no_sprint_label(t: Target) -> &'static str {
    match t {
        Target::CurrentSprint => "No current sprint",
        Target::NextSprint => "No next sprint",
        Target::Backlog => "",
    }
}

/// An add's tooltip: where it goes, by sprint name when the board's sprints are known, and its key.
pub fn add_tip(t: Target, sprint: Option<&str>) -> String {
    match sprint {
        Some(name) => format!("Add to {name} ({})", key_of(t)),
        None => format!("{} ({})", add_label(t), key_of(t)),
    }
}

/// The primary action's label: "Add to current sprint".
pub fn add_label(t: Target) -> &'static str {
    match t {
        Target::CurrentSprint => "Add to current sprint",
        Target::NextSprint => "Add to next sprint",
        Target::Backlog => "Add to backlog",
    }
}

/// What `r` does now, named like its button. None while a review runs (the progress line takes its place).
pub fn run_key(running: bool, has_review: bool, failed: bool) -> Option<StandupRun> {
    match (running, failed, has_review) {
        (true, _, _) => None,
        (false, true, _) => Some(StandupRun::Retry),
        (false, false, false) => Some(StandupRun::Now),
        (false, false, true) => Some(StandupRun::Again),
    }
}

/// The header's run button and whether it is the page's primary action. With no review it is the one call to
/// action; otherwise a quiet "Run again" beside the cards. After a failure the retry sits in the error notice
/// instead, next to what went wrong.
pub fn run_button(running: bool, has_review: bool, failed: bool) -> Option<(StandupRun, bool)> {
    match run_key(running, has_review, failed)? {
        StandupRun::Retry => None,
        r => Some((r, r == StandupRun::Now)),
    }
}

/// The Standup hint bar's state: card keys only with cards, copy only with a review, `r` named like its button.
/// `views::issues::hints_panel` passes it to `keys::standup_hints` on this screen.
pub fn hint(app: &App) -> StandupHint {
    let s = &app.standup;
    StandupHint {
        cards: app.standup_cursor().is_some(),
        copy: s.saved.review.is_some(),
        run: run_key(s.phase.is_some(), s.saved.review.is_some(), s.error.is_some()),
        read_only: crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// When the review on screen ran: "Mon 12:54".
pub fn ran_label(saved: &Saved) -> Option<String> {
    let at = chrono::DateTime::from_timestamp(saved.ran_at?, 0)?.with_timezone(&chrono::Local);
    Some(at.format("%a %H:%M").to_string())
}

/// What to do about a failed run, by what failed.
pub fn error_next_step(raw: &str) -> &'static str {
    let low = raw.to_lowercase();
    if low.contains("took longer than") {
        "Try again. If it stops again, check that Claude can reach GitHub and Slack."
    } else if low.contains("jira activity") {
        "Check your connection to Jira, then try again."
    } else if low.contains("couldn't start") || low.contains("no such file") || low.contains("not found") {
        "Check that the claude command is installed and on your PATH, then try again."
    } else if low.contains("not logged in") || low.contains("log in") || low.contains("login") || low.contains("auth") {
        "Run the claude command once in a terminal to sign in, then try again."
    } else {
        "Try again, or check that Claude is signed in."
    }
}

/// A failed run's error in plain words, one sentence.
pub fn error_message(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(at) = raw.find("took longer than ")
        && let Some(secs) = raw[at + 17..].split_whitespace().next().and_then(|n| n.parse::<u64>().ok())
    {
        let span = if secs >= 60 && secs % 60 == 0 {
            let m = secs / 60;
            if m == 1 { "1 minute".to_string() } else { format!("{m} minutes") }
        } else {
            format!("{secs} seconds")
        };
        return format!("The review took longer than {span} and was stopped.");
    }
    let mut text = if raw.to_lowercase().starts_with("couldn't") { raw.to_string() } else { format!("The review failed: {raw}") };
    if let Some(first) = text.get(..1) {
        text = first.to_uppercase() + &text[1..];
    }
    if !text.ends_with(['.', '!', '?']) {
        text.push('.');
    }
    text
}

/// The toolbar's note when a review has no cards to show, so the first look (at the top of a long summary) says
/// so: "no untracked work found", "every suggestion dismissed".
pub fn no_cards_note(saved: &Saved) -> Option<String> {
    match &saved.review {
        Some(_) if saved.visible().is_empty() => Some(no_cards_text(saved).0.to_lowercase()),
        _ => None,
    }
}

/// What the page says when there are no cards: the message, and what to do next.
pub fn no_cards_text(saved: &Saved) -> (&'static str, &'static str) {
    match &saved.review {
        Some(r) if r.suggestions.is_empty() => ("No untracked work found", "Every piece of work in the window has a ticket."),
        Some(_) => ("Every suggestion was dismissed", "Run the review again to look for new work."),
        None => ("No review yet", "It runs by itself on Monday, Wednesday and Friday. Press Run now to start one."),
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
                let sp = theme::spacing(ui.ctx());
                let avail = ui.available_width();
                let w = column_width(sp, avail);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(((avail - w) / 2.0).floor());
                    ui.vertical(|ui| {
                        ui.set_width(w);
                        // Every vertical gap below is explicit.
                        ui.spacing_mut().item_spacing = vec2(sp.item.x, 0.0);
                        page(app, ui, p, &mut acts);
                    });
                });
            });
        });
    app.actions.extend(acts);
}

/// The column, top to bottom.
fn page(app: &mut App, ui: &mut Ui, p: &Palette, acts: &mut Vec<Action>) {
    let sp = theme::spacing(ui.ctx());
    if toolbar(app, ui, p, acts) {
        acts.push(Action::StandupStop);
    }
    // The status block: what the run is doing, or why it failed and what to do.
    if app.standup.phase.is_some() {
        ui.add_space(sp.section_gap);
        progress(app, ui, p);
    } else if let Some(e) = app.standup.error.clone() {
        ui.add_space(sp.section_gap);
        let mut next = error_next_step(&e).to_string();
        if app.standup.saved.review.is_some()
            && let Some(at) = ran_label(&app.standup.saved)
        {
            next.push_str(&format!(" The review below is the last good one, from {at}."));
        }
        if notice(ui, p, &error_message(&e), &next) {
            acts.push(Action::StandupRun);
        }
    }
    // The card the cursor is on is brought into view when the cursor moves, never on opening the screen: the
    // page opens at its top, the summary first.
    let cursor = app.standup_cursor();
    let last_id = egui::Id::new("standup-last-cursor");
    let last = ui.data(|d| d.get_temp::<Option<usize>>(last_id));
    ui.data_mut(|d| d.insert_temp(last_id, cursor));
    let moved = moved_since(last, cursor);
    let Some(review) = app.standup.saved.review.clone() else {
        // No review: the page's state, in the same block as a running review's; the tickets still show under it.
        if app.standup.phase.is_none() && app.standup.error.is_none() {
            ui.add_space(sp.section_gap);
            let (msg, hint) = no_cards_text(&app.standup.saved);
            status(ui, p, msg, hint, false);
        }
        tickets(app, ui, p, cursor, moved, acts);
        ui.add_space(sp.section_gap);
        return;
    };
    // Summary first, then the window's tickets, then the suggestions (or why there are none), then the review's own
    // notes.
    summary(app, ui, p, &review, acts);
    tickets(app, ui, p, cursor, moved, acts);
    cards(app, ui, p, cursor, moved, acts);
    notes(ui, p, &review);
    ui.add_space(sp.section_gap);
}

/// Whether the cursor moved since the last frame, so its card is scrolled into view. Not on the screen's first
/// frame (`last` unknown), so opening the screen keeps the page at its top.
pub fn moved_since(last: Option<Option<usize>>, now: Option<usize>) -> bool {
    last.is_some_and(|l| l != now) && now.is_some()
}

/// Wrapped secondary text (reasons, notes, notices, hints): `small`, at the prose line height, so every wrapped
/// secondary line has the same rhythm.
fn secondary(sp: &theme::Spacing, text: &str, color: egui::Color32) -> egui::Label {
    let size = sp.small;
    egui::Label::new(RichText::new(text).color(color).size(size).line_height(Some((size * sp.prose_line_height).round()))).wrap()
}

/// The toolbar row: the window line at the left, Copy and the run button (or Stop, while a review runs) at the
/// column's right edge. The selected tab names the screen, so there is no page title. True when Stop was clicked.
fn toolbar(app: &App, ui: &mut Ui, p: &Palette, acts: &mut Vec<Action>) -> bool {
    let sp = theme::spacing(ui.ctx());
    let s = &app.standup;
    let mut stop = false;
    ui.horizontal(|ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        let today = chrono::Local::now().date_naive();
        let mut line = window_text(&s.saved, today, s.error.is_some());
        if let Some(note) = no_cards_note(&s.saved) {
            line = format!("{line} · {note}");
        }
        ui.label(RichText::new(line).color(p.muted).size(sp.small));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some((run, primary)) = run_button(s.phase.is_some(), s.saved.review.is_some(), s.error.is_some()) {
                let b = if primary { primary_button(run.label(), p) } else { button(run.label(), p) };
                if crate::views::tooltip(ui.add(b), "r").clicked() {
                    acts.push(Action::StandupRun);
                }
            }
            if s.phase.is_some() && crate::views::tooltip(ui.add(button("Stop", p)), "Stop this review; the last one stays").clicked() {
                stop = true;
            }
            if s.saved.review.is_some() && crate::views::tooltip(ui.add(button("Copy", p)), "Copy the standup (y)").clicked() {
                acts.push(Action::StandupCopy);
            }
        });
    });
    stop
}

/// A running review: the phase and its clock; then what happens to the review on screen.
fn progress(app: &App, ui: &mut Ui, p: &Palette) {
    let (Some(phase), Some(started)) = (app.standup.phase, app.standup.started) else { return };
    let read_only = crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed);
    let note = running_note(app.standup.saved.review.is_some(), ran_label(&app.standup.saved).as_deref(), read_only);
    status(ui, p, &progress_text(phase, app.standup.activity_count, started.elapsed().as_secs()), &note, true);
}

/// What happens to the review on screen while a run goes. Only a writable app says that tickets added from it are
/// kept: in read-only mode nothing can be added.
pub fn running_note(has_review: bool, ran: Option<&str>, read_only: bool) -> String {
    if !has_review {
        return "Your standup and suggested tickets appear here when it finishes, usually in a minute or two.".to_string();
    }
    let last = match ran {
        Some(at) => format!("Below is the last review, from {at}."),
        None => "Below is the last review.".to_string(),
    };
    let adds = if read_only { "" } else { " Tickets you add from it before then are still created." };
    format!("{last} The new review replaces it when this run finishes.{adds}")
}

/// The page's state block (no review yet, or a review running): the state in `text` body, then what it means in
/// quieter secondary text `line_gap` under it, its left edge on the state's text (past the spinner, if any).
fn status(ui: &mut Ui, p: &Palette, title: &str, note: &str, spinning: bool) {
    let sp = theme::spacing(ui.ctx());
    let gap = ui.spacing().item_spacing.x;
    let lead = if spinning { crate::views::icons::INLINE_SPINNER + gap } else { 0.0 };
    ui.horizontal(|ui| {
        if spinning {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
        }
        ui.label(RichText::new(title).color(p.text).size(sp.body));
    });
    ui.add_space(sp.line_gap);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(lead);
        // The one secondary text colour, as the window line, meta lines and notices use.
        ui.add(secondary(sp, note, p.muted));
    });
}

/// The one error notice: the column's width, card padding; what went wrong, what to do next, and the retry.
/// True when Try again was clicked.
fn notice(ui: &mut Ui, p: &Palette, text: &str, next: &str) -> bool {
    let sp = theme::spacing(ui.ctx());
    Frame::new()
        .fill(p.danger_subtle)
        .corner_radius(theme::RADIUS)
        .inner_margin(Margin::same(sp.card_pad as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).color(p.text).size(sp.body)).wrap());
            ui.add_space(sp.line_gap);
            ui.add(secondary(sp, next, theme::muted_on(p.danger_subtle, p)));
            ui.add_space(sp.paragraph_gap);
            crate::views::tooltip(ui.add(primary_button(StandupRun::Retry.label(), p)), "r").clicked()
        })
        .inner
}

/// The summary as a document: section headers with real bullets, `section_gap` apart like every section. A ticket
/// key that has a card below draws as a key chip; clicking it focuses that card instead of opening Jira.
fn summary(app: &App, ui: &mut Ui, p: &Palette, review: &standup::Review, acts: &mut Vec<Action>) {
    let sp = theme::spacing(ui.ctx());
    let s = &review.summary;
    let site = &app.cfg.site;
    let keys: std::collections::BTreeSet<String> = app.standup.saved.tickets.all().map(|i| i.key.clone()).collect();
    for (title, items) in [("Since the last standup", &s.done), ("Today", &s.today), ("Blockers", &s.blockers)] {
        ui.add_space(sp.section_gap);
        c::section_header(ui, p, title, None);
        ui.add_space(sp.paragraph_gap);
        if items.is_empty() {
            c::missing_value(ui, p, "None");
        } else {
            adf_view::show(ui, &standup::bullets_doc(items, site, &keys), p, theme::chip_fill_on(p.bg, p));
        }
    }
    // A key chip's click asks for its Jira page (`adf_view`); a key with a card focuses the card instead.
    let rows: Vec<String> = app.standup.saved.tickets.all().map(|i| standup::browse_url(site, &i.key)).collect();
    ui.ctx().output_mut(|o| {
        o.commands.retain(|cmd| match cmd {
            egui::OutputCommand::OpenUrl(u) => match rows.iter().position(|r| *r == u.url) {
                Some(row) => {
                    acts.push(Action::StandupFocus(row));
                    false
                }
                None => true,
            },
            _ => true,
        })
    });
}

/// The window's tickets as Jira-style cards (the Team and Me card: key, title, status pill, epic): "Done since Fri
/// Oct 30", then "In progress", each under a quiet section header. The focused card carries Move to Done while it
/// is not done. Nothing before the first read lands; a failed read with nothing cached says so in one quiet line.
fn tickets(app: &App, ui: &mut Ui, p: &Palette, cursor: Option<usize>, moved: bool, acts: &mut Vec<Action>) {
    let sp = theme::spacing(ui.ctx());
    let t = &app.standup.saved.tickets;
    let Some(since) = t.since else {
        if let Some(e) = &app.standup.tickets_error {
            ui.add_space(sp.section_gap);
            ui.add(secondary(sp, &format!("Couldn't load your tickets: {e}"), p.muted));
        }
        return;
    };
    let now = chrono::Utc::now();
    let target = app.standup_done_target();
    let done_title = done_header(since);
    let mut row = 0;
    for (si, (title, list)) in [(done_title.as_str(), &t.done), ("In progress", &t.in_progress)].into_iter().enumerate() {
        ui.add_space(sp.section_gap);
        c::section_header(ui, p, title, Some(list.len()));
        ui.add_space(sp.paragraph_gap);
        if list.is_empty() {
            c::missing_value(ui, p, "None");
        }
        for (i, issue) in list.iter().enumerate() {
            if i > 0 {
                ui.add_space(sp.card_gap);
            }
            let sel = cursor == Some(row);
            let card = Card { issue, days_in_status: crate::teamview::days_in_status(issue.status_changed.as_deref(), now) };
            // Ids apart from Team's and Me's cards.
            let r = crate::views::team::card(app, ui, p, (usize::MAX - 8 - si, i), &card, sel, true);
            if sel
                && target.as_deref() == Some(issue.key.as_str())
                && let Some(a) = done_on_card(ui, p, r.rect, issue)
            {
                acts.push(a);
            }
            let r = crate::views::tooltip(r, "Open in Jira (o)");
            if r.clicked() || r.double_clicked() {
                acts.push(Action::StandupFocus(row));
            }
            if r.double_clicked() {
                acts.push(Action::StandupOpenSource);
            }
            if sel && moved {
                r.scroll_to_me(None);
            }
            row += 1;
        }
    }
}

/// "Done since Fri Oct 30".
pub fn done_header(since: chrono::NaiveDate) -> String {
    format!("Done since {}", standup::day_label(since))
}

/// The focused ticket card's Move to Done (`components::done_button`) at the right end of its key line, as on the
/// Me screen: the key is short, so the line has room, and the card keeps its height as the cursor moves. Drawn
/// over the card after it, so the button takes the click.
fn done_on_card(ui: &mut Ui, p: &Palette, card: egui::Rect, issue: &crate::jira::Issue) -> Option<Action> {
    let sp = theme::spacing(ui.ctx());
    let key = theme::Ink::of(ui.ctx(), &theme::key_font(), None);
    // As `team::card` places the key: its capitals card_pad under the frame's top.
    let key_mid = card.top() + (sp.card_pad - key.top).round().max(0.0) + key.line / 2.0;
    let slot = egui::Rect::from_x_y_ranges(
        (card.left() + sp.card_pad)..=(card.right() - sp.card_pad),
        (key_mid - crate::views::BUTTON_H / 2.0)..=(key_mid + crate::views::BUTTON_H / 2.0),
    );
    let mut child =
        ui.new_child(egui::UiBuilder::new().id_salt(("standup-done", &issue.key)).max_rect(slot).layout(egui::Layout::right_to_left(egui::Align::Center)));
    c::done_button(&mut child, p, issue, c::card_fill(p, true))
}

/// The review's own notes (sources it skipped, tools that failed): last, after the suggestions, in quieter text at
/// the prose measure.
fn notes(ui: &mut Ui, p: &Palette, review: &standup::Review) {
    let sp = theme::spacing(ui.ctx());
    if !review.notes.is_empty() {
        ui.add_space(sp.section_gap);
        c::section_header(ui, p, "Notes from the review", None);
        ui.add_space(sp.paragraph_gap);
        // The bullets' measure (`adf_view` caps prose at prose_max_width).
        let w = ui.available_width().min(sp.prose_max_width);
        ui.allocate_ui(vec2(w, 0.0), |ui| {
            ui.set_max_width(w);
            ui.add(secondary(sp, &review.notes, p.muted));
        });
    }
}

/// "Suggested tickets  N" (with, at the right, the one read-only note and the dismissed count), then the cards, or
/// why there are none.
fn cards(app: &mut App, ui: &mut Ui, p: &Palette, cursor: Option<usize>, moved: bool, acts: &mut Vec<Action>) {
    let sp = theme::spacing(ui.ctx());
    let visible: Vec<Suggestion> = app.standup.saved.visible().into_iter().cloned().collect();
    let dismissed = app.standup.saved.dismissed.len();
    let read_only = crate::jira::READ_ONLY.load(std::sync::atomic::Ordering::Relaxed);
    ui.add_space(sp.section_gap);
    ui.horizontal(|ui| {
        c::section_header(ui, p, "Suggested tickets", Some(visible.len()));
        if let Some(note) = header_note(read_only && !visible.is_empty(), dismissed) {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(note).color(p.muted).size(sp.small));
            });
        }
    });
    ui.add_space(sp.paragraph_gap);
    if visible.is_empty() {
        // The toolbar's window line says so too (`no_cards_note`), so the page can open at its top.
        let (msg, hint) = no_cards_text(&app.standup.saved);
        c::empty_state(ui, p, msg, Some(hint));
        return;
    }
    // Where each add goes, by the board's known sprints: a name for the tooltip, or why there is none.
    let targets: Vec<(Target, Result<Option<String>, String>)> =
        [Target::CurrentSprint, Target::NextSprint, Target::Backlog].into_iter().map(|t| (t, crate::app::target_sprint(t, &app.sprints))).collect();
    // The suggestions follow the ticket cards in cursor order; the title edit (`e`) is only for a suggestion.
    let offset = app.standup_ticket_count();
    if cursor.is_none_or(|c| c < offset) {
        app.standup.focus_title = false;
    }
    for (vi, s) in visible.iter().enumerate() {
        if vi > 0 {
            ui.add_space(sp.card_gap);
        }
        let i = offset + vi;
        let key = standup::dismiss_key(s);
        let sel = cursor == Some(i);
        let focus_title = sel && std::mem::take(&mut app.standup.focus_title);
        let added = app.standup.saved.added.get(&key).cloned();
        let adding = app.standup.adding.contains(&key);
        let issue_type = app.standup_type_name(s);
        let title = app.standup.titles.entry(key.clone()).or_insert_with(|| s.title.clone());
        let st = CardState { sel, focus_title, adding, read_only, key: &key, issue_type: issue_type.as_deref(), targets: &targets };
        let r = match added.as_deref() {
            Some(issue) => added_card(ui, p, title, issue, &app.cfg.site, &st, i, acts),
            None => card(ui, p, s, title, &st, i, acts),
        };
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
    adding: bool,
    /// Writes are blocked: the card has no adds (the section header says why, once for the list).
    read_only: bool,
    key: &'a str,
    /// The type an add creates; None while unknown.
    issue_type: Option<&'a str>,
    /// Each add's sprint (`app::target_sprint`).
    targets: &'a [(Target, Result<Option<String>, String>)],
}

/// The one read-only note, at the "Suggested tickets" header; the cards then show no adds.
pub const READ_ONLY_NOTE: &str = "Read-only: adding tickets is off";

/// The note at the right of the "Suggested tickets" header: the read-only note, the dismissed count, or both.
pub fn header_note(read_only: bool, dismissed: usize) -> Option<String> {
    let parts: Vec<String> =
        [read_only.then(|| READ_ONLY_NOTE.to_string()), (dismissed > 0).then(|| format!("{dismissed} dismissed"))].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The key that adds to `t`.
fn key_of(t: Target) -> &'static str {
    match t {
        Target::CurrentSprint => "c",
        Target::NextSprint => "n",
        Target::Backlog => "b",
    }
}

/// One suggestion, in four parts: its title (a field only while editing); one quiet meta line with the source link;
/// a short description in quieter text, with More straight after it; one action row, the adds (none in read-only
/// mode) and Dismiss at its right. More opens the whole description and the reason; Less closes them. Focus changes only
/// the card's fill and ring, never what it shows.
fn card(ui: &mut Ui, p: &Palette, s: &Suggestion, title: &mut String, st: &CardState, i: usize, acts: &mut Vec<Action>) -> egui::Response {
    let sp = theme::spacing(ui.ctx());
    let id = egui::Id::new(("standup-title", st.key));
    let editing = st.focus_title || ui.memory(|m| m.has_focus(id));
    let fill = c::card_fill(p, st.sel);
    let open_id = egui::Id::new(("standup-more", st.key));
    let opened = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let r = c::card_frame(ui, p, st.sel).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let muted = theme::muted_on(fill, p);
        let small = theme::body(sp.small);

        if editing {
            let r = title_field(ui, p, title, id, st.sel);
            if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                r.surrender_focus();
            }
            title.retain(|c| c != '\n');
            if st.focus_title {
                r.request_focus();
            }
            if r.gained_focus() {
                acts.push(Action::StandupFocus(i));
            }
        } else {
            let (text, color) = if title.trim().is_empty() { ("Untitled", p.placeholder()) } else { (title.as_str(), p.text) };
            let r = ui.add(egui::Label::new(RichText::new(text).font(c::card_title_font(ui)).color(color)).wrap().sense(egui::Sense::click()));
            if crate::views::tooltip(r, "Edit the title (e)").clicked() {
                ui.memory_mut(|m| m.request_focus(id));
                acts.push(Action::StandupFocus(i));
            }
        }

        ui.add_space(sp.line_gap);
        meta_line(ui, p, s, st.issue_type, muted, i, acts);

        let doc = standup::description_doc(&s.description);
        let reason = s.reason.trim();
        if opened {
            // The whole description in the card's quiet text (headings at body size), then why.
            if !doc.blocks.is_empty() {
                ui.add_space(sp.paragraph_gap);
                let quiet = Palette { text: muted, ..*p };
                adf_view::show(ui, &standup::card_doc(doc), &quiet, theme::chip_fill_on(fill, p));
            }
            if !reason.is_empty() {
                ui.add_space(sp.paragraph_gap);
                ui.add(secondary(sp, reason, muted));
            }
            ui.add_space(sp.line_gap);
            if text_link(ui, "Less", small, p.accent).clicked() {
                ui.data_mut(|d| d.insert_temp(open_id, false));
            }
        } else {
            // The first block in plain words (the reason when there is no description).
            let (text, more) = standup::preview_text(&doc);
            let (text, hidden) = if text.is_empty() { (reason.to_string(), false) } else { (text, more || !reason.is_empty()) };
            if !text.is_empty() {
                ui.add_space(sp.paragraph_gap);
                if preview(ui, p, &text, hidden, muted, st.key) {
                    ui.data_mut(|d| d.insert_temp(open_id, true));
                }
            }
        }

        ui.add_space(sp.paragraph_gap);
        actions_row(ui, p, s, st, muted, i, acts);
    });
    r.response
}

/// The meta line, "GitHub · Task · 85% sure · PR #7": quiet text, the source a link.
fn meta_line(ui: &mut Ui, p: &Palette, s: &Suggestion, issue_type: Option<&str>, muted: egui::Color32, i: usize, acts: &mut Vec<Action>) {
    let sp = theme::spacing(ui.ctx());
    let small = theme::body(sp.small);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new(meta_text(s, issue_type)).font(small.clone()).color(muted));
        if !s.source_url.trim().is_empty() {
            ui.label(RichText::new(" · ").font(small.clone()).color(muted));
            let link = text_link(ui, &standup::source_label(&s.source_url), small.clone(), p.accent);
            if crate::views::tooltip(link, format!("{} (o)", s.source_url.trim())).clicked() {
                acts.push(Action::StandupFocus(i));
                acts.push(Action::StandupOpenSource);
            }
        }
    });
}

/// The card's action row: the adds at the left (none in read-only mode): the suggested target as a bordered button,
/// the other targets quiet beside it, "Adding…" while one is in flight; Dismiss at the right, a quiet bordered
/// button like the toolbar's, so it reads as an action, not as one more meta field.
fn actions_row(ui: &mut Ui, p: &Palette, s: &Suggestion, st: &CardState, muted: egui::Color32, i: usize, acts: &mut Vec<Action>) {
    let sp = theme::spacing(ui.ctx());
    ui.horizontal(|ui| {
        ui.set_min_height(crate::views::BUTTON_H);
        if st.adding {
            crate::views::icons::spinner(ui, crate::views::icons::INLINE_SPINNER, p);
            ui.label(RichText::new("Adding…").color(muted).size(sp.small));
            return;
        }
        if !st.read_only {
            let primary = s.suggested_target;
            // A target the board has no sprint for says so in its place, quiet and not clickable.
            let sprint = |t: Target| st.targets.iter().find(|(x, _)| *x == t).map_or(Ok(None), |(_, r)| r.clone());
            for t in std::iter::once(primary).chain([Target::CurrentSprint, Target::NextSprint, Target::Backlog].into_iter().filter(|t| *t != primary)) {
                let name = match sprint(t) {
                    Ok(name) => name,
                    Err(why) => {
                        let r = ui.label(RichText::new(no_sprint_label(t)).color(muted).size(sp.small));
                        crate::views::tooltip(r, format!("Can't add here: {why}"));
                        continue;
                    }
                };
                let tip = add_tip(t, name.as_deref());
                let r = if t == primary { ui.add(button(add_label(t), p)) } else { ui.add(c::ghost(t.name(), p)) };
                if crate::views::tooltip(r, tip).clicked() {
                    acts.push(Action::StandupFocus(i));
                    acts.push(Action::StandupAdd(t));
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::views::tooltip(ui.add(button("Dismiss", p)), "Hide this suggestion for good (x)").clicked() {
                acts.push(Action::StandupFocus(i));
                acts.push(Action::StandupDismiss);
            }
        });
    });
}

/// A collapsed description as one text block: `text` in `color` at `small`, cut at a word to PREVIEW_LINES lines
/// ending in "…", and, when anything is left out (`hidden`, or the cut), More in the link style straight after its
/// last word. True when More was clicked.
fn preview(ui: &mut Ui, p: &Palette, text: &str, hidden: bool, color: egui::Color32, key: &str) -> bool {
    let sp = theme::spacing(ui.ctx());
    let w = ui.available_width();
    let text = text.trim();
    let font = theme::body(sp.small);
    let lh = (sp.small * sp.prose_line_height).round();
    let job = |shown: &str, more: bool| {
        let mut job = LayoutJob::default();
        job.wrap.max_width = w;
        let fmt = |c: egui::Color32| TextFormat { font_id: font.clone(), color: c, line_height: Some(lh), ..Default::default() };
        job.append(shown, 0.0, fmt(color));
        if more {
            job.append(" ", 0.0, fmt(color));
            job.append(MORE, 0.0, fmt(p.accent));
        }
        job
    };
    let shown_at = |n: usize| {
        let shown = standup::clip_words(text, n);
        let more = hidden || shown != text;
        (shown, more)
    };
    let total = text.chars().count();
    let cache = egui::Id::new(("standup-preview-fit", key, w.to_bits(), total, hidden));
    let n = ui.data(|d| d.get_temp::<usize>(cache)).unwrap_or_else(|| {
        let n = fit_count(total, |n| {
            let (shown, more) = shown_at(n);
            ui.fonts_mut(|f| f.layout_job(job(&shown, more))).rows.len() <= PREVIEW_LINES
        });
        ui.data_mut(|d| d.insert_temp(cache, n));
        n
    });
    let (shown, more) = shown_at(n);
    let galley = ui.fonts_mut(|f| f.layout_job(job(&shown, more)));
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    let mut clicked = false;
    if more
        && let Some(row) = galley.rows.last()
        && row.row.glyphs.len() >= MORE.len()
    {
        // More is the last row's last glyphs: a link like every other (`views::text_link`).
        let glyphs = &row.row.glyphs[row.row.glyphs.len() - MORE.len()..];
        let x0 = rect.left() + row.pos.x + glyphs[0].pos.x;
        let x1 = rect.left() + row.pos.x + glyphs.last().map_or(0.0, |g| g.pos.x + g.advance_width);
        let slot = egui::Rect::from_x_y_ranges(x0..=x1, (rect.top() + row.min_y())..=(rect.top() + row.max_y()));
        let resp = ui.interact(crate::views::link_hit(slot), ui.id().with(("standup-more-link", key)), egui::Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, MORE));
        let baseline = rect.top() + row.pos.y + glyphs[0].pos.y;
        ui.painter().hline(x0..=x1, adf_view::underline_y(baseline), egui::Stroke::new(1.0, p.accent));
        clicked = resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    }
    ui.painter().galley(rect.min, galley, color);
    clicked
}

/// The link that opens a collapsed card.
const MORE: &str = "More";

/// The card title as a field: the same font and line as the static title, its box on the card's content edge (the
/// text inset by the field's own padding, like every field), on the card's own surface with a quiet edge, so the
/// card's focus ring stays the only accent. Enter (or Esc) ends the edit and keeps the text, instead of adding a
/// line.
fn title_field(ui: &mut Ui, p: &Palette, title: &mut String, id: egui::Id, sel: bool) -> egui::Response {
    let font = c::card_title_font(ui);
    let m = crate::views::input_margin(crate::views::INPUT_PAD_X);
    let w = ui.available_width();
    let top = ui.cursor().min - vec2(0.0, m.top as f32);
    let mut field = ui.new_child(egui::UiBuilder::new().id_salt(("standup-title-field", id)).max_rect(egui::Rect::from_min_size(top, vec2(w, f32::INFINITY))));
    let v = field.visuals_mut();
    v.text_edit_bg_color = Some(if sel { p.surface } else { p.field() });
    v.selection.stroke = egui::Stroke::new(1.0, p.border);
    let edit = egui::TextEdit::multiline(title)
        .id(id)
        .font(font.clone())
        .hint_text(RichText::new("Ticket title").font(font).color(p.placeholder()))
        .desired_rows(1)
        .desired_width(w)
        .margin(m)
        .return_key(None);
    let r = field.add(edit);
    // Only the text's own height takes room in the card, so nothing below moves.
    ui.allocate_exact_size(vec2(w, (r.rect.height() - m.sum().y).max(0.0)), egui::Sense::hover());
    r
}

/// An added card's frame: done, so quieter than any open card: the page fill with the plain card edge. With the
/// cursor on it, the shared selected-card look, like any focused card.
pub fn added_frame(ui: &Ui, p: &Palette, sel: bool) -> Frame {
    if sel { c::card_frame(ui, p, true) } else { c::card_frame(ui, p, false).fill(p.bg).stroke(egui::Stroke::new(1.0, p.border)) }
}

/// An added suggestion, collapsed: "✓ Added as CI-9600 · its title", muted; the text hangs past the check mark
/// when it wraps. The key, shown once, links to it.
#[allow(clippy::too_many_arguments, reason = "one card's inputs")]
fn added_card(ui: &mut Ui, p: &Palette, title: &str, issue: &str, site: &str, st: &CardState, i: usize, acts: &mut Vec<Action>) -> egui::Response {
    let sp = theme::spacing(ui.ctx());
    let fill = if st.sel { p.selection() } else { p.bg };
    let r = added_frame(ui, p, st.sel).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let muted = theme::muted_on(fill, p);
        let font = theme::body(sp.body);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(RichText::new("✓ ").font(font.clone()).color(p.on_track));
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new("Added as ").font(font.clone()).color(muted));
                    let link = text_link(ui, issue, font.clone(), p.accent);
                    if crate::views::tooltip(link, "Open in Jira").clicked() {
                        acts.push(Action::StandupFocus(i));
                        acts.push(Action::OpenUrl(format!("{}/browse/{issue}", site.trim_end_matches('/'))));
                    }
                    ui.label(RichText::new(" · ").font(font.clone()).color(muted));
                    ui.add(egui::Label::new(RichText::new(title).font(font).color(muted)).wrap());
                });
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
        assert_eq!(progress_text(StandupPhase::Jira, None, 84), "Reading Jira activity · 1:24");
        assert_eq!(progress_text(StandupPhase::Claude, Some(1), 60), "Asking Claude about your GitHub and Slack work (1 Jira issue found) · 1:00");
        assert_eq!(progress_text(StandupPhase::Claude, Some(4), 61), "Asking Claude about your GitHub and Slack work (4 Jira issues found) · 1:01");
        assert_eq!(confidence_text(0.8), "80% sure");
        assert_eq!(confidence_text(0.0), "0% sure");
        let s = Suggestion { issue_type: "Story".into(), confidence: 0.85, ..Default::default() };
        assert_eq!(meta_text(&s, Some("Task")), "GitHub · Task · 85% sure", "the type that is created, not the one asked for");
        assert_eq!(meta_text(&s, None), "GitHub · 85% sure", "no type while it is not known");
        assert_eq!(add_tip(Target::NextSprint, Some("Sprint 93")), "Add to Sprint 93 (n)");
        assert_eq!(add_tip(Target::Backlog, None), "Add to backlog (b)");
        assert_eq!(no_sprint_label(Target::NextSprint), "No next sprint");
        assert_eq!(add_label(Target::CurrentSprint), "Add to current sprint");
        assert_eq!(header_note(true, 0).as_deref(), Some(READ_ONLY_NOTE));
        assert_eq!(header_note(true, 2).as_deref(), Some("Read-only: adding tickets is off · 2 dismissed"));
        assert_eq!(header_note(false, 2).as_deref(), Some("2 dismissed"));
        assert_eq!(header_note(false, 0), None);
    }

    #[test]
    fn running_note_only_promises_adds_when_writable() {
        assert!(running_note(false, None, true).starts_with("Your standup and suggested tickets appear here"));
        assert_eq!(running_note(true, Some("Mon 12:54"), true), "Below is the last review, from Mon 12:54. The new review replaces it when this run finishes.");
        assert_eq!(
            running_note(true, None, false),
            "Below is the last review. The new review replaces it when this run finishes. Tickets you add from it before then are still created."
        );
        assert!(!running_note(true, Some("Mon 12:54"), true).contains("add"), "read-only: nothing to add");
    }

    #[test]
    fn the_page_opens_at_its_top() {
        assert!(!moved_since(None, Some(0)), "the first frame does not scroll to the focused card");
        assert!(!moved_since(Some(Some(2)), Some(2)));
        assert!(moved_since(Some(Some(0)), Some(1)), "j scrolls the new card into view");
        assert!(moved_since(Some(None), Some(0)), "cards that just landed under an unknown cursor");
        assert!(!moved_since(Some(Some(1)), None));
        assert_eq!(done_header(chrono::NaiveDate::from_ymd_opt(2026, 10, 30).unwrap()), "Done since Fri Oct 30");
    }

    #[test]
    fn one_run_button_per_state() {
        assert_eq!(run_button(true, true, false), None, "the progress line replaces it");
        assert_eq!(run_button(true, false, true), None);
        assert_eq!(run_button(false, true, true), None, "the retry is in the error notice");
        assert_eq!(run_key(false, true, true), Some(StandupRun::Retry), "r still retries");
        assert_eq!(run_button(false, false, false), Some((StandupRun::Now, true)), "the one call to action");
        assert_eq!(run_button(false, true, false), Some((StandupRun::Again, false)), "quiet next to the cards' adds");
        assert_eq!(run_key(true, true, false), None, "no r while it runs");
    }

    #[test]
    fn errors_say_what_to_do_next() {
        assert!(error_next_step("claude took longer than 900 s and was stopped").starts_with("Try again."));
        assert_eq!(error_next_step("Couldn't read your Jira activity: 500"), "Check your connection to Jira, then try again.");
        assert!(error_next_step("couldn't start claude: No such file or directory").contains("PATH"));
        assert!(error_next_step("Not logged in").contains("sign in"));
        assert_eq!(error_next_step("claude printed nothing"), "Try again, or check that Claude is signed in.");
    }

    #[test]
    fn errors_read_as_plain_sentences() {
        assert_eq!(error_message("claude took longer than 900 s and was stopped"), "The review took longer than 15 minutes and was stopped.");
        assert_eq!(error_message("/x/claude took longer than 45 s and was stopped"), "The review took longer than 45 seconds and was stopped.");
        assert_eq!(error_message("Couldn't read your Jira activity: 500"), "Couldn't read your Jira activity: 500.");
        assert_eq!(error_message("claude printed nothing"), "The review failed: claude printed nothing.");
    }

    #[test]
    fn column_is_the_prose_measure_plus_card_padding() {
        for sp in [&theme::COMPACT, &theme::COMFORTABLE] {
            assert_eq!(column_width(sp, 2000.0), sp.prose_max_width + 2.0 * sp.card_pad);
            assert_eq!(column_width(sp, 300.0), 300.0);
        }
    }

    #[test]
    fn window_line_names_the_window_and_the_run() {
        let mon = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        assert_eq!(window_text(&Saved::default(), mon, false), "Since Fri Oct 30 · not run yet");
        assert_eq!(window_text(&Saved::default(), mon, true), "Since Fri Oct 30 · last run failed", "not 'not run yet' above a failed run");
        let ran = Saved { ran_on: chrono::NaiveDate::from_ymd_opt(2026, 11, 4), ran_at: Some(1_790_000_000), ..Default::default() };
        assert!(window_text(&ran, mon, true).starts_with("Since Mon Nov 2 · ran "), "the window of the day it ran");
    }

    #[test]
    fn fit_count_finds_the_last_fit() {
        assert_eq!(fit_count(10, |n| n <= 4), 4);
        assert_eq!(fit_count(10, |_| true), 10);
        assert_eq!(fit_count(10, |_| false), 0);
        assert_eq!(fit_count(0, |_| true), 0);
    }

    #[test]
    fn empty_card_lists_say_why() {
        assert_eq!(no_cards_text(&Saved::default()).0, "No review yet");
        let none = Saved { review: Some(Review::default()), ..Default::default() };
        assert_eq!(no_cards_text(&none).0, "No untracked work found");
        let review = Review { suggestions: vec![Suggestion { title: "A".into(), ..Default::default() }], ..Default::default() };
        let all_gone = Saved { review: Some(review), ..Default::default() };
        assert_eq!(no_cards_text(&all_gone).0, "Every suggestion was dismissed");
        assert_eq!(no_cards_note(&none).as_deref(), Some("no untracked work found"), "said at the top too");
        assert_eq!(no_cards_note(&Saved::default()), None, "no review: the empty state is the page");
    }

    #[test]
    fn screen_draws_headless() {
        // READ_ONLY is global: hold the lock other write tests hold while they flip it.
        let _g = crate::jira::tests::WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cfg: crate::config::Config = toml::from_str("site = \"https://x.atlassian.net\"\nemail = \"a@b.c\"\n").unwrap();
        let mut app = App::for_test(cfg, None);
        app.screen = crate::app::Screen::Standup;
        let texts = |app: &mut App| {
            let shapes = crate::testutil::paint_at2(egui::vec2(1200.0, 1800.0), |ui| show(app, ui));
            crate::testutil::inks(&shapes).into_iter().map(|(t, _)| t).collect::<Vec<_>>()
        };
        let empty = texts(&mut app);
        assert!(empty.iter().any(|t| t == "Run now") && empty.iter().any(|t| t == "No review yet"), "{empty:?}");
        // A first run: progress, no empty-state copy, no header button, no card header.
        app.standup.phase = Some(StandupPhase::Claude);
        app.standup.started = Some(std::time::Instant::now());
        let first = texts(&mut app);
        assert!(first.iter().any(|t| t.starts_with("Asking Claude")), "{first:?}");
        assert!(!first.iter().any(|t| t == "Run now" || t == "No review yet" || t.starts_with("Suggested")), "{first:?}");
        // A failed first run: one sentence, one retry.
        app.standup.phase = None;
        app.standup.error = Some("claude took longer than 900 s and was stopped".into());
        let failed = texts(&mut app);
        assert!(failed.iter().any(|t| t == "The review took longer than 15 minutes and was stopped."), "{failed:?}");
        assert_eq!(failed.iter().filter(|t| *t == "Try again" || *t == "Retry" || *t == "Run now").count(), 1, "{failed:?}");
        assert!(failed.iter().any(|t| t.starts_with("Try again. If it stops again")), "a next step: {failed:?}");
        assert_eq!(hint(&app), StandupHint { cards: false, copy: false, run: Some(StandupRun::Retry), read_only: false });
        app.standup.error = None;

        let sug = |title: &str, url: &str| Suggestion {
            title: title.into(),
            description: "Post each rollout.\n\n- one\n- two".into(),
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
        let ticket = |key: &str, summary: &str, cat: &str| crate::jira::Issue {
            key: key.into(),
            summary: summary.into(),
            status: crate::jira::Status { name: if cat == "done" { "Done".into() } else { "In Progress".into() }, category: cat.into() },
            ..Default::default()
        };
        app.standup.saved.tickets = standup::Tickets {
            since: chrono::NaiveDate::from_ymd_opt(2026, 10, 30),
            done: vec![ticket("CI-1", "Ship the rollout fix", "done")],
            in_progress: vec![ticket("CI-2", "Canary routing", "indeterminate")],
        };
        app.standup.phase = Some(StandupPhase::Claude);
        app.standup.started = Some(std::time::Instant::now());
        app.standup.activity_count = Some(2);
        let drawn = texts(&mut app);
        for want in [
            "Since the last standup",
            "Shipped the rollout fix",
            "Slack was not available",
            "Suggested tickets",
            "Rollout notifications",
            "Post each rollout. More",
            "GitHub · 80% sure",
            "PR #7",
            "Add to next sprint",
            "Current sprint",
            "Backlog",
            "Dismiss",
            "Added as ",
            "CI-42",
        ] {
            assert!(drawn.iter().any(|t| t == want), "{want} in {drawn:?}");
        }
        let at = |want: &str| drawn.iter().position(|t| t == want).unwrap_or_else(|| panic!("{want} in {drawn:?}"));
        assert!(at("Suggested tickets") < at("Notes from the review"), "summary, then suggestions, then notes: {drawn:?}");
        assert!(
            at("Blockers") < at("Done since Fri Oct 30") && at("Done since Fri Oct 30") < at("In progress") && at("In progress") < at("Suggested tickets"),
            "summary, then the window's tickets, then the suggestions: {drawn:?}"
        );
        for want in ["CI-1", "Ship the rollout fix", "Done", "CI-2", "Canary routing", "In Progress"] {
            assert!(drawn.iter().any(|t| t == want), "{want} as a ticket card in {drawn:?}");
        }
        assert!(!drawn.iter().any(|t| t == "one" || t.contains("No ticket names")), "a collapsed card hides the rest and the reason: {drawn:?}");
        assert!(!drawn.iter().any(|t| t == READ_ONLY_NOTE), "writable: no read-only note");
        assert!(drawn.iter().any(|t| t.starts_with("Asking Claude")), "progress shows: {drawn:?}");
        assert!(!drawn.iter().any(|t| t == "Run again"), "no run button while running");
        assert!(drawn.iter().any(|t| t == "Stop"), "a running review can be stopped: {drawn:?}");
        assert!(!drawn.iter().any(|t| t == "Standup"), "the tab names the screen; no page title");
        assert_eq!(drawn.iter().filter(|t| *t == "Add to next sprint").count(), 1, "the added card has no add buttons");
        assert_eq!(drawn.iter().filter(|t| *t == "Dismiss").count(), 1, "nor a Dismiss");
        assert_eq!(drawn.iter().filter(|t| t.contains("CI-42")).count(), 1, "the key shows once");
        assert!(app.standup.titles.is_empty(), "untouched titles are not kept as edits");
        assert!(drawn.iter().any(|t| t == "Notes from the review"), "the notes are labelled: {drawn:?}");
        assert!(drawn.iter().any(|t| t.starts_with("Below is the last review")), "the old review is marked while a run goes");
        assert!(drawn.iter().any(|t| t.contains("still created")), "writable: adds made meanwhile are kept: {drawn:?}");
        assert_eq!(hint(&app), StandupHint { cards: true, copy: true, run: None, read_only: false });

        // With the project's types known, the card names the type an add creates: no Story in it, so a Task. With
        // the board's sprints known and no future one, the next-sprint add says so instead of offering it.
        let ty = |id: &str, name: &str| crate::jira::IssueType { id: id.into(), name: name.into(), subtask: false };
        app.standup.saved.issue_types = Some(standup::ProjectTypes { project: app.cfg.scope.project.clone(), types: vec![ty("1", "Task"), ty("2", "Bug")] });
        app.sprints = vec![crate::jira::Sprint { id: 7, name: "Sprint 92".into(), state: "active".into() }];
        let known = texts(&mut app);
        assert!(known.iter().any(|t| t == "GitHub · Task · 80% sure"), "{known:?}");
        assert!(!known.iter().any(|t| t.contains("Story")), "never claude's guess: {known:?}");
        assert!(known.iter().any(|t| t == "No next sprint") && !known.iter().any(|t| t == "Add to next sprint"), "{known:?}");
        assert!(known.iter().any(|t| t == "Current sprint") && known.iter().any(|t| t == "Backlog"), "{known:?}");
        app.standup.saved.issue_types = None;
        app.sprints.clear();

        // Read-only: one note at the header, no adds on any card, Dismiss and the source link stay.
        {
            let _ro = crate::jira::tests::ReadOnlyGuard::new();
            let ro = texts(&mut app);
            assert_eq!(ro.iter().filter(|t| *t == READ_ONLY_NOTE).count(), 1, "{ro:?}");
            assert!(!ro.iter().any(|t| t.starts_with("Add to") || t == "Current sprint" || t == "Backlog"), "no dead adds: {ro:?}");
            assert!(ro.iter().any(|t| t == "Dismiss") && ro.iter().any(|t| t == "PR #7"), "{ro:?}");
            assert!(hint(&app).read_only, "the hint bar drops the add keys too");
            assert!(!ro.iter().any(|t| t.contains("still created") || t.contains("stay added")), "read-only promises no adds: {ro:?}");
        }

        // Every card dismissed: the summary, then the empty state under its section header.
        app.standup.phase = None;
        app.standup.saved.added.clear();
        for s in app.standup.saved.review.clone().unwrap().suggestions {
            app.standup.saved.dismissed.insert(standup::dismiss_key(&s));
        }
        let empty = texts(&mut app);
        let at = |want: &str| empty.iter().position(|t| t == want).unwrap_or_else(|| panic!("{want} in {empty:?}"));
        assert!(at("Since the last standup") < at("Suggested tickets") && at("Suggested tickets") < at("Every suggestion was dismissed"), "{empty:?}");
        assert_eq!(hint(&app), StandupHint { cards: true, copy: true, run: Some(StandupRun::Again), read_only: false }, "the ticket cards still take j/k");
        assert!(!empty.iter().any(|t| t == "Stop"), "nothing runs: no Stop");
    }
}
