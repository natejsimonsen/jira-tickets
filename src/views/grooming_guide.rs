//! The Grooming screen's guidance, in the grooming runbook's own words: what each mode is for, how to judge a
//! ticket, and how each flow ends. Pure text; `grooming.rs` draws it.

use crate::app::GroomMode;

/// One line under the mode toggle: what this mode is for and when it happens.
pub fn purpose(mode: GroomMode, status_new: &str, status_planning: &str, status_ready: &str) -> String {
    match mode {
        GroomMode::PreCheck => format!(
            "Before the meeting. Judge each {status_new} ticket against the readiness bar, then promote the ready set to {status_planning} and write the report."
        ),
        GroomMode::Session => {
            format!("In the meeting. Walk {status_planning} epic by epic and place each accepted ticket in rank beside the epic's {status_ready} queue.")
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
pub fn rubric_lines(mode: GroomMode, status_new: &str, status_ready: &str) -> Vec<(&'static str, String)> {
    match mode {
        GroomMode::PreCheck => vec![
            ("", "Can a stranger tell WHAT is being done and WHEN it is done?".into()),
            ("r", "Yes: ready.".into()),
            ("w", "Unclear what is being done.".into()),
            ("n", "Unclear when it is done.".into()),
            ("e", "Empty: nothing to judge.".into()),
            ("", "When in doubt, pass.".into()),
        ],
        GroomMode::Session => vec![
            ("y", format!("{status_ready}: accept and place it in rank.")),
            ("b", format!("Bounce to {status_new} with a one-line comment on what's missing.")),
            ("p", "Backlog: real work, lower priority.".into()),
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

/// The explanation at the top of the Promote dialog, before the lists.
pub fn promote_explainer(promoted: usize, held_back: usize, cap: usize, status_planning: &str, report_path: &str) -> String {
    let held = if held_back == 0 { String::new() } else { format!(" (top {cap} by rank, {held_back} held back)") };
    format!("Promote moves these {promoted} to {status_planning}{held} and writes {report_path}. Paste the report into the team channel.")
}

/// Today's report path as the user wrote the dir (`~` kept): "~/Documents/grooming-reports/2026-10-16.md".
pub fn report_path(report_dir: &str, today: chrono::NaiveDate) -> String {
    format!("{}/{}.md", report_dir.trim_end_matches('/'), today.format("%Y-%m-%d"))
}

/// The session's empty state for an epic with nothing in Planning.
pub fn nothing_in_planning(status_planning: &str) -> String {
    format!("Nothing in {status_planning}. Run a pre-check first, or pick another epic with ].")
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
            "In the meeting. Walk Planning epic by epic and place each accepted ticket in rank beside the epic's Ready for Work queue."
        );
    }

    #[test]
    fn rubric_titles_and_lines() {
        assert_eq!(rubric_title(GroomMode::PreCheck), "Readiness bar");
        assert_eq!(rubric_title(GroomMode::Session), "Decide");
        assert_eq!(
            rubric_lines(GroomMode::PreCheck, "New", "Ready for Work"),
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
            rubric_lines(GroomMode::Session, "New", "Ready for Work"),
            [
                ("y", "Ready for Work: accept and place it in rank.".to_string()),
                ("b", "Bounce to New with a one-line comment on what's missing.".to_string()),
                ("p", "Backlog: real work, lower priority.".to_string()),
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
        let path = "~/Documents/grooming-reports/2026-10-16.md";
        assert_eq!(
            promote_explainer(3, 0, 10, "Planning", path),
            "Promote moves these 3 to Planning and writes ~/Documents/grooming-reports/2026-10-16.md. Paste the report into the team channel."
        );
        assert_eq!(
            promote_explainer(10, 2, 10, "Planning", path),
            "Promote moves these 10 to Planning (top 10 by rank, 2 held back) and writes ~/Documents/grooming-reports/2026-10-16.md. Paste the report into the team channel."
        );
    }

    #[test]
    fn report_path_keeps_the_dir_as_written() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        assert_eq!(report_path("~/Documents/grooming-reports", d), "~/Documents/grooming-reports/2026-10-16.md");
        assert_eq!(report_path("~/Documents/grooming-reports/", d), "~/Documents/grooming-reports/2026-10-16.md", "no doubled slash");
    }

    #[test]
    fn nothing_in_planning_points_at_the_pre_check() {
        assert_eq!(nothing_in_planning("Planning"), "Nothing in Planning. Run a pre-check first, or pick another epic with ].");
    }
}
