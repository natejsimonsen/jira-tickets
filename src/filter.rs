//! Pure list filtering, column formatting, and ⌘K parsing for the Issues view.

use crate::jira::Issue;

#[derive(Default, Clone, Debug)]
pub struct IssueFilter {
    pub epic: Option<String>,
    pub quick: Option<usize>,
    pub mine: bool,
    pub text: String,
}

pub fn visible<'a>(issues: &[&'a Issue], f: &IssueFilter, me: Option<&str>) -> Vec<&'a Issue> {
    let needle = f.text.trim().to_lowercase();
    issues
        .iter()
        .copied()
        .filter(|i| f.epic.as_deref().is_none_or(|e| i.parent_key.as_deref() == Some(e)))
        .filter(|i| !f.mine || i.assignee.as_ref().map(|a| a.display_name.as_str()) == me)
        .filter(|i| {
            needle.is_empty()
                || i.key.to_lowercase().contains(&needle)
                || i.summary.to_lowercase().contains(&needle)
                || i.labels.iter().any(|l| l.to_lowercase().contains(&needle))
                || i.assignee.as_ref().is_some_and(|a| a.display_name.to_lowercase().contains(&needle))
        })
        .collect()
}

pub fn column_value(issue: &Issue, field: &str) -> String {
    match field {
        "status" => issue.status.name.clone(),
        "priority" => issue.priority.clone(),
        "labels" => issue.labels.join(", "),
        "assignee" => issue.assignee.as_ref().map(|a| a.display_name.clone()).unwrap_or_default(),
        "issuetype" => issue.issue_type.clone(),
        "parent.summary" => issue.parent_summary.clone().unwrap_or_default(),
        "parent.key" => issue.parent_key.clone().unwrap_or_default(),
        "sprint" => issue.sprint.clone().unwrap_or_default(),
        "points" => issue.story_points.map(fmt_num).unwrap_or_default(),
        f if f.starts_with("customfield_") => issue.custom.get(f).map(json_text).unwrap_or_default(),
        _ => String::new(),
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
}

fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.as_f64().map(fmt_num).unwrap_or_default(),
        serde_json::Value::Object(o) => o.get("value").or(o.get("name")).or(o.get("displayName")).map(json_text).unwrap_or_default(),
        serde_json::Value::Array(a) => a.iter().map(json_text).collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

pub fn parse_goto(text: &str, project: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.split("/browse/").nth(1) {
        let key: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        return issue_key(&key);
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{project}-{t}"));
    }
    issue_key(t)
}

/// `PROJ-123`, uppercased: the project is `[A-Za-z][A-Za-z0-9_]*`, the number digits.
fn issue_key(t: &str) -> Option<String> {
    let (p, n) = t.split_once('-')?;
    let project_ok = p.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let number_ok = !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    (project_ok && number_ok).then(|| format!("{}-{n}", p.to_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::{Issue, Status, User};

    fn issue(key: &str, summary: &str, parent: Option<&str>, assignee: Option<&str>, labels: &[&str]) -> Issue {
        Issue {
            key: key.into(),
            summary: summary.into(),
            parent_key: parent.map(String::from),
            assignee: assignee.map(|a| User { account_id: a.into(), display_name: a.into() }),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            status: Status { name: "New".into(), category: "new".into() },
            priority: "High".into(),
            ..Default::default()
        }
    }

    #[test]
    fn filters_compose() {
        let a = issue("CI-1", "Remove the old autoscaler", Some("E-1"), Some("Ada"), &["PLAT"]);
        let b = issue("CI-2", "Deployer overlays", Some("E-2"), None, &["PLAT", "infra"]);
        let all = vec![&a, &b];
        let mut f = IssueFilter::default();
        assert_eq!(visible(&all, &f, Some("Ada")).len(), 2);
        f.epic = Some("E-1".into());
        assert_eq!(visible(&all, &f, None)[0].key, "CI-1");
        f.epic = None;
        f.mine = true;
        assert_eq!(visible(&all, &f, Some("Ada")).len(), 1);
        f.mine = false;
        f.text = "INFRA".into();
        assert_eq!(visible(&all, &f, None)[0].key, "CI-2");
    }

    #[test]
    fn column_values() {
        let mut i = issue("CI-1", "x", Some("E-1"), Some("Ada"), &["a", "b"]);
        i.parent_summary = Some("Epic one".into());
        i.custom.insert("customfield_1".into(), serde_json::json!({ "value": "Opt" }));
        i.custom.insert("customfield_2".into(), serde_json::json!(3.0));
        assert_eq!(column_value(&i, "labels"), "a, b");
        assert_eq!(column_value(&i, "assignee"), "Ada");
        assert_eq!(column_value(&i, "parent.summary"), "Epic one");
        assert_eq!(column_value(&i, "customfield_1"), "Opt");
        assert_eq!(column_value(&i, "customfield_2"), "3");
        assert_eq!(column_value(&i, "nope"), "");
    }

    #[test]
    fn goto_parsing() {
        assert_eq!(parse_goto("9001", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("ci-9001", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("https://example.atlassian.net/browse/CI-9001?x=1", "CI").as_deref(), Some("CI-9001"));
        assert_eq!(parse_goto("", "CI"), None);
        assert_eq!(parse_goto("my_proj2-7", "CI").as_deref(), Some("MY_PROJ2-7"));
        assert_eq!(parse_goto("https://x.atlassian.net/browse/9-1", "CI"), None);
        for bad in ["-12", "2CI-1", "C I-1", "CI$-1", "CI-", "CI-1a", "ci-1-2", "é-1"] {
            assert_eq!(parse_goto(bad, "CI"), None, "{bad}");
        }
    }
}
