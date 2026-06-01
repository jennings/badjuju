---
name: plan-tickets
description: |
  Use when the user asks to plan a feature or change by breaking it into
  GitHub Issues for this repo's ticket tracker. Triggers: "plan this
  feature", "break this into tickets", "create tickets for", "draft
  tickets", "what tickets do we need".
---

# Planning and creating tickets

The output of planning here is **GitHub Issues**, not code. Planning
ends when the tickets exist with priorities and dependencies set. Do
not begin implementing in the same session — implementation is a
separate ticket-driven loop (see the `work-on-ticket` skill).

## 1. Design the plan

Break the work into tickets sized so each can be implemented in roughly
one commit and reviewed as one PR. If a ticket feels too large to
review in one sitting, split it.

Identify which tickets block which — the dependency graph matters for
prioritization.

## 2. Get user acceptance

Walk the user through the proposed tickets before writing anything to
GitHub. Capture corrections; do not create issues for a plan the user
hasn't agreed to.

## 3. Create the tickets

Once the plan is accepted, create each ticket with `gh`:

```bash
gh issue create \
  --title "Imperative-voice title here" \
  --body  "Description of what to do and why" \
  --label P2
```

Conventions:

- Title in imperative voice ("Add X", "Refactor Y", "Fix Z").
- One ticket per discrete commit-sized unit of work.

## 4. Set dependencies

Add cross-ticket dependencies so the next person (or agent) picking up
work can see what's actually unblocked. GitHub's Issue Dependencies API
is used via `gh api` if no first-class flag is available:

Docs: <https://docs.github.com/en/rest/issues/issue-dependencies?apiVersion=2026-03-10>

## 5. Assign priority labels

- `P1` — bugs or critical features.
- `P2` — default.
- `P3` — nice-to-have.

The `work-on-ticket` skill prefers higher-priority tickets first, so
get the priorities right at planning time.

## 6. Stop

Planning is done once tickets exist with priorities and dependencies
set. **Do not begin implementing in the same session.** Hand the work
back to the user; they (or a future session via `work-on-ticket`) will
pick up implementation.
