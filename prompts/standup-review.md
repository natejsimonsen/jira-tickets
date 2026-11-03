# Standup review

You are preparing {{USER}}'s standup. The team meets Monday, Wednesday and Friday. Review everything {{USER}} did
from {{WINDOW_START}} 00:00 local time up to now ({{TODAY}}), then:

1. write the standup: what got done since the last standup, what is next today, and any blockers;
2. find work that no Jira ticket tracks yet, and suggest a ticket for each piece.

This is read-only. Do not comment, push, merge, post, react, or change anything anywhere.

## Sources

### Jira (already gathered)

The app fetched {{USER}}'s Jira activity in the window: issues assigned to, reported by, or updated by them, with
the status changes, comments and creations inside the window. Treat it as the list of work that is already tracked.

{{JIRA_ACTIVITY}}

### GitHub (use `gh`)

Find {{USER}}'s pull requests and reviews in the window. For example:

```bash
gh search prs --author=@me --updated=">={{WINDOW_START}}" --json number,title,url,state,repository,updatedAt,closedAt --limit 50
gh search prs --reviewed-by=@me --updated=">={{WINDOW_START}}" --json number,title,url,state,repository,updatedAt --limit 50
gh search commits --author=@me --author-date=">={{WINDOW_START}}" --json sha,commit,repository,url --limit 50
gh pr view <url> --json title,body,headRefName,state,mergedAt
```

For each PR decide its true state, not just that it exists:

- **Shipped**: merged in the window.
- **In review**: open, not draft.
- **Draft or local only**: draft PR, or commits with no PR.
- **Abandoned**: closed without merging.

A PR is tracked when its branch name, title or body names a Jira key (for example `CI-3770` in
`nate.CI-3770.fix-rollouts`), or when it plainly belongs to one of the Jira issues above.

### Slack (use the Slack MCP tools)

Search for messages {{USER}} sent in the window, for example `from:me after:{{DAY_BEFORE_WINDOW}}` with
`mcp__slack__slack_search_public`, and open the threads that look like work (`mcp__slack__slack_read_thread`):
requests from other teams, incidents, questions {{USER}} answered at length, promises ("I'll look into it"). Skip
social chatter. If the Slack tools are not available, carry on without Slack and say so in `notes`.

## Reconcile

Group the work by Jira ticket or theme, not by PR or message. Collapse noise (fix-ups, reverts, re-runs) into the
outcome. A PR or thread counts as tracked when a ticket above covers it; everything else is a candidate.

Suggest a ticket only for real work: a PR with no ticket, a promise made in Slack, a request someone made of
{{USER}}, a follow-up an incident left behind, a cleanup that is clearly owed. Do not suggest a ticket for work a
ticket above already covers, for reviews of other people's PRs, or for chatter. Never suggest the same work twice.

Pick `suggested_target`:

- `current_sprint`: in progress now or due this sprint (open PRs, active incidents, things promised for this week);
- `next_sprint`: should happen soon but not this sprint;
- `backlog`: worth keeping, no date.

## Output

Reply with one JSON object and nothing else: no prose before or after it, no code fence. Use this shape:

```json
{
  "summary": {
    "done": ["One line per outcome since the last standup, tagged with its ticket key when there is one"],
    "today": ["What is next, most important first"],
    "blockers": ["Anything waiting on someone else; empty when none"]
  },
  "suggestions": [
    {
      "title": "Short imperative summary, at most 80 characters",
      "description": "Two to five sentences: what the work is, why it matters, and what done looks like",
      "issue_type": "Task",
      "source_kind": "github",
      "source_url": "https://github.com/org/repo/pull/123",
      "reason": "Why this needs a ticket: the PR names no ticket and none above covers it",
      "confidence": 0.8,
      "suggested_target": "current_sprint"
    }
  ],
  "notes": "Anything the reader should know about the review itself, such as a source that could not be read"
}
```

Rules:

- `issue_type` is `Task`, `Story` or `Bug`.
- `source_kind` is `github`, `slack` or `jira`.
- `source_url` is the PR, commit, Slack message permalink, or Jira issue the suggestion comes from. Always set it.
- `confidence` is a number from 0 to 1: how sure you are that this is untracked work that deserves a ticket.
- `suggested_target` is `current_sprint`, `next_sprint` or `backlog`.
- Standup lines are plain sentences of at most 15 words, with no Markdown.
- Use empty arrays, never null.
