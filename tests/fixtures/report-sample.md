# PLAT pre-grooming — 2026-09-04

JQL: `project = CI AND labels = PLAT AND status = New ORDER BY rank ASC`
Boards: [Parent 100](https://example.atlassian.net/jira/software/c/projects/CI/boards/100/backlog) · [PLAT 101](https://example.atlassian.net/jira/software/c/projects/CI/boards/101/backlog?customFilter=201)
Triage: 40 tickets scored by 4 subagents · budget 30 min · cap 15

## Agenda

1. [CI-3727](https://example.atlassian.net/browse/CI-3727) — Add a blue-green option for early adopter apps · impact 5 · action 4 · ~3min (cum 3) · Sample rationale one. · **rollouts-adoption** · Epic CI-1860
2. [CI-3739](https://example.atlassian.net/browse/CI-3739) — Classify low-confidence health verdicts as inconclusive · impact 5 · action 4 · ~3min (cum 6) · Sample rationale two. · **rollouts-adoption** · Epic CI-1860
3. ~~CI-4102~~ — closed as Done after the pre-check (already shipped, see sweep below)
4. [CI-9156](https://example.atlassian.net/browse/CI-9156) — Bound the long-lived job annotations to fixed durations · impact 5 · action 4 · ~3min (cum 9) · Sample rationale three. · Epic CI-1989

Agenda: 3 tickets, ~9 min of 30 min budget

## Planning — by epic

### [CI-1860 Progressive Delivery](https://example.atlassian.net/jira/software/c/projects/CI/boards/101/backlog?customFilter=201&issueParent=10001)

- [CI-3727](https://example.atlassian.net/browse/CI-3727) — Add a blue-green option for early adopter apps · rank 175 · impact 5 · action 4 · older
- [CI-3739](https://example.atlassian.net/browse/CI-3739) — Classify low-confidence health verdicts as inconclusive · rank 176 · impact 5 · action 4 · older

### [CI-1989 Cost Insights & Automation](https://example.atlassian.net/jira/software/c/projects/CI/boards/101/backlog?customFilter=201&issueParent=10003)

- [CI-9156](https://example.atlassian.net/browse/CI-9156) — Bound the long-lived job annotations to fixed durations · rank 258 · impact 5 · action 4 · new (14d)
- [CI-9157](https://example.atlassian.net/browse/CI-9157) — Review idle capacity reports weekly · rank 259 · impact 3 · action 3 · older

Existing Ready for Work in this epic (1):
- [CI-3835](https://example.atlassian.net/browse/CI-3835) — Retire the old manifest archive

### [CI-768 Infrastructure Provisioning](https://example.atlassian.net/jira/software/c/projects/CI/boards/101/backlog?customFilter=201&issueParent=10004)

- ~~CI-4102~~ — closed as Done after the pre-check (already shipped)
- [CI-4050](https://example.atlassian.net/browse/CI-4050) — Investigate managing network config with a new tool · rank 90 · impact 4 · action 3 · older

## Needs detail (still in `New`)

### [CI-65 Developer Portal (ongoing)](https://example.atlassian.net/jira/software/c/projects/CI/boards/101/backlog?customFilter=201&issueParent=10005)

- [CI-2950](https://example.atlassian.net/browse/CI-2950) — Add a personalized portal homepage · rank 102 · impact 3 · created 2025-09-23 · missing: unclear-when, unclear-what
- [CI-2951](https://example.atlassian.net/browse/CI-2951) — Show team ownership on the portal · rank 103 · impact 3 · created 2025-09-24 · missing: empty

### [CI-483 Observability](https://example.atlassian.net/jira/software/c/projects/CI/boards/101/backlog?customFilter=201&issueParent=10006)

- [CI-4058](https://example.atlassian.net/browse/CI-4058) — Define alert notification preferences per app · rank 215 · impact 4 · created 2026-07-29 · missing: unclear-when
- [CI-4059](https://example.atlassian.net/browse/CI-4059) — Document dashboard naming conventions · rank 216 · impact 2 · created 2026-07-30 · missing: unclear-what

### [CI-768 Infrastructure Provisioning](https://example.atlassian.net/jira/software/c/projects/CI/boards/101/backlog?customFilter=201&issueParent=10004)

- [CI-4060](https://example.atlassian.net/browse/CI-4060) — Prototype a shared network module · rank 217 · impact 3 · created 2026-08-01 · missing: unclear-when

---

**Run summary:** 40 `New` tickets scanned (2 epics excluded) · 3 promoted to Planning (~9 min agenda) · 20 ready but held back (budget/cap) · 5 still need detail · 0 rows truncated from Needs detail (cap 10) · 0 failed transitions.

---

# Already-done sweep (post-check, 2026-09-04)

**Closed as Done (1):**
- [CI-4102](https://example.atlassian.net/browse/CI-4102) Sample gateway node class change — shipped, outcome recorded. *Was on today's agenda; removed.*

**Agenda after removal:** 2 tickets, ~6 of 30 min.
