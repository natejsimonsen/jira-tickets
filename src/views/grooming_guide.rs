//! The Grooming screen's guidance, in the grooming runbook's own words: what each mode is for, how to judge a
//! ticket, and how each flow ends. Pure text; `grooming.rs` draws it.

use crate::app::GroomMode;
use crate::grooming::Code;

/// A readiness code in the readiness bar's words, for verdict pills and the Promote dialog.
pub fn code_words(c: Code) -> &'static str {
    match c {
        Code::UnclearWhat => "Unclear what is being done",
        Code::UnclearWhen => "Unclear when it is done",
        Code::Empty => "Empty",
    }
}

/// The codes a verdict shows, one pill each: "Empty" (nothing to judge) only when no other code applies, since a
/// ticket that is unclear about something is not empty.
pub fn shown_codes(codes: &[Code]) -> Vec<Code> {
    let others = codes.iter().any(|c| *c != Code::Empty);
    codes.iter().copied().filter(|c| !(others && *c == Code::Empty)).collect()
}

/// One quiet line for a run of empty lanes: "Nothing planned in Core 92, Core 93 or New".
pub fn empty_lanes_line(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("Nothing planned in {one}"),
        [rest @ .., last] => format!("Nothing planned in {} or {last}", rest.join(", ")),
    }
}

/// One line under the mode toggle: what this mode is for and when it happens.
pub fn purpose(mode: GroomMode, status_new: &str, status_planning: &str, status_ready: &str) -> String {
    match mode {
        GroomMode::PreCheck => format!(
            "Before the meeting. Judge each {status_new} ticket against the readiness bar, then promote the ready set to {status_planning} and write the report."
        ),
        GroomMode::Session => {
            format!("In the meeting. Walk the epics in order and accept each Proposed or {status_new} ticket into the epic's {status_ready} queue, in rank.")
        }
    }
}

/// The rubric card's title.
pub fn rubric_title(mode: GroomMode) -> &'static str {
    match mode {
        GroomMode::PreCheck => "Readiness bar",
        GroomMode::Session => "Decide",
    }
}

/// The rubric card's lines. Each line is (key or "", text). An empty key draws text only.
pub fn rubric_lines(mode: GroomMode, status_new: &str, status_planning: &str, status_ready: &str, team_label: &str) -> Vec<(&'static str, String)> {
    match mode {
        GroomMode::PreCheck => vec![
            ("", "Can a stranger tell WHAT is being done and WHEN it is done?".into()),
            ("r", "Yes: ready.".into()),
            ("w", format!("{}.", code_words(Code::UnclearWhat))),
            ("n", format!("{}.", code_words(Code::UnclearWhen))),
            ("e", format!("{}: nothing to judge.", code_words(Code::Empty))),
            ("", "When in doubt, pass.".into()),
        ],
        GroomMode::Session => vec![
            ("y", format!("{status_ready}: accept and place it in rank.")),
            ("b", format!("Bounce with a one-line comment on what's missing; {status_planning} goes back to {status_new}.")),
            ("p", "Backlog: real work, lower priority.".into()),
            ("J/K", format!("Move a {status_ready} ticket down or up in rank.")),
            ("l/a", format!("Unclaimed: not on the team's map yet. Label it {team_label} or assign it to bring it in.")),
            ("c", "Close: Done, or Won't Do when the work no longer matters.".into()),
            ("s", "Sprint: move it into a sprint or back to the backlog.".into()),
            ("", "Three to five minutes per ticket. Past that, bounce.".into()),
        ],
    }
}

/// The cue when the cursor has passed the last pre-check row (the summary row is current).
pub fn all_judged(judged: usize, total: usize) -> String {
    if judged == total {
        format!("All {total} judged. Enter opens the summary.")
    } else {
        format!("{judged} of {total} judged. Enter opens the summary; j/k go back.")
    }
}

/// The explanation at the top of the Promote dialog, before the lists. The report's path follows on its own line.
pub fn promote_explainer(promoted: usize, held_back: usize, cap: usize, status_planning: &str) -> String {
    let held = if held_back == 0 { String::new() } else { format!(" (top {cap} by rank, {held_back} held back)") };
    let tickets = if promoted == 1 { "ticket" } else { "tickets" };
    format!("Promote moves {promoted} {tickets} to {status_planning}{held} and writes today's report for the team channel:")
}

/// The Promote dialog's line on the tickets left alone: "258 without a verdict stay in New, untouched."
pub fn untouched_line(unjudged: usize, status_new: &str) -> String {
    let verb = if unjudged == 1 { "stays" } else { "stay" };
    format!("{unjudged} without a verdict {verb} in {status_new}, untouched.")
}

/// Today's report path as the user wrote the dir (`~` kept): "~/Documents/grooming-reports/2026-10-16.md".
pub fn report_path(report_dir: &str, today: chrono::NaiveDate) -> String {
    format!("{}/{}.md", report_dir.trim_end_matches('/'), today.format("%Y-%m-%d"))
}

/// The session's empty state for an epic with no Proposed and no New ticket.
pub fn nothing_to_groom(status_new: &str) -> String {
    format!("Nothing Proposed or {status_new} here. Run a pre-check first, or pick another epic with ].")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_says_what_each_mode_is_for() {
        assert_eq!(
            purpose(GroomMode::PreCheck, "New", "Planning", "Ready for Work"),
            "Before the meeting. Judge each New ticket against the readiness bar, then promote the ready set to Planning and write the report."
        );
        assert_eq!(
            purpose(GroomMode::Session, "New", "Planning", "Ready for Work"),
            "In the meeting. Walk the epics in order and accept each Proposed or New ticket into the epic's Ready for Work queue, in rank."
        );
    }

    #[test]
    fn empty_only_shows_when_nothing_else_applies() {
        assert_eq!(shown_codes(&[Code::UnclearWhat, Code::Empty]), [Code::UnclearWhat]);
        assert_eq!(shown_codes(&[Code::Empty]), [Code::Empty]);
        assert_eq!(shown_codes(&[Code::UnclearWhen, Code::UnclearWhat]), [Code::UnclearWhen, Code::UnclearWhat]);
    }

    #[test]
    fn empty_lanes_read_as_one_line() {
        assert_eq!(empty_lanes_line(&["New"]), "Nothing planned in New");
        assert_eq!(empty_lanes_line(&["Core 92", "New"]), "Nothing planned in Core 92 or New");
        assert_eq!(
            empty_lanes_line(&["Core 92", "Core 93", "Ready for Work", "Proposed", "New"]),
            "Nothing planned in Core 92, Core 93, Ready for Work, Proposed or New"
        );
    }

    #[test]
    fn rubric_titles_and_lines() {
        assert_eq!(rubric_title(GroomMode::PreCheck), "Readiness bar");
        assert_eq!(rubric_title(GroomMode::Session), "Decide");
        assert_eq!(
            rubric_lines(GroomMode::PreCheck, "New", "Planning", "Ready for Work", "PLAT"),
            [
                ("", "Can a stranger tell WHAT is being done and WHEN it is done?".to_string()),
                ("r", "Yes: ready.".to_string()),
                ("w", "Unclear what is being done.".to_string()),
                ("n", "Unclear when it is done.".to_string()),
                ("e", "Empty: nothing to judge.".to_string()),
                ("", "When in doubt, pass.".to_string()),
            ]
        );
        assert_eq!(
            rubric_lines(GroomMode::Session, "New", "Planning", "Ready for Work", "PLAT"),
            [
                ("y", "Ready for Work: accept and place it in rank.".to_string()),
                ("b", "Bounce with a one-line comment on what's missing; Planning goes back to New.".to_string()),
                ("p", "Backlog: real work, lower priority.".to_string()),
                ("J/K", "Move a Ready for Work ticket down or up in rank.".to_string()),
                ("l/a", "Unclaimed: not on the team's map yet. Label it PLAT or assign it to bring it in.".to_string()),
                ("c", "Close: Done, or Won't Do when the work no longer matters.".to_string()),
                ("s", "Sprint: move it into a sprint or back to the backlog.".to_string()),
                ("", "Three to five minutes per ticket. Past that, bounce.".to_string()),
            ]
        );
    }

    #[test]
    fn all_judged_cue_both_branches() {
        assert_eq!(all_judged(7, 7), "All 7 judged. Enter opens the summary.");
        assert_eq!(all_judged(5, 7), "5 of 7 judged. Enter opens the summary; j/k go back.");
    }

    #[test]
    fn promote_explainer_both_branches() {
        assert_eq!(promote_explainer(1, 0, 10, "Planning"), "Promote moves 1 ticket to Planning and writes today's report for the team channel:");
        assert_eq!(promote_explainer(3, 0, 10, "Planning"), "Promote moves 3 tickets to Planning and writes today's report for the team channel:");
        assert_eq!(
            promote_explainer(10, 2, 10, "Planning"),
            "Promote moves 10 tickets to Planning (top 10 by rank, 2 held back) and writes today's report for the team channel:"
        );
    }

    #[test]
    fn code_words_match_the_readiness_bar() {
        assert_eq!(code_words(Code::UnclearWhat), "Unclear what is being done");
        assert_eq!(code_words(Code::UnclearWhen), "Unclear when it is done");
        assert_eq!(code_words(Code::Empty), "Empty");
        assert_eq!(untouched_line(258, "New"), "258 without a verdict stay in New, untouched.");
        assert_eq!(untouched_line(1, "New"), "1 without a verdict stays in New, untouched.");
    }

    #[test]
    fn report_path_keeps_the_dir_as_written() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        assert_eq!(report_path("~/Documents/grooming-reports", d), "~/Documents/grooming-reports/2026-10-16.md");
        assert_eq!(report_path("~/Documents/grooming-reports/", d), "~/Documents/grooming-reports/2026-10-16.md", "no doubled slash");
    }

    #[test]
    fn nothing_to_groom_points_at_the_pre_check() {
        assert_eq!(nothing_to_groom("New"), "Nothing Proposed or New here. Run a pre-check first, or pick another epic with ].");
    }
}
