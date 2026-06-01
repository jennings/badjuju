---
name: work-on-ticket
description: |
  Use when the user asks to work on a ticket, find the next ticket, pick
  up unblocked work, implement an issue, or claim a GitHub issue in this
  repo's ticket-driven workflow. Triggers: "work on a ticket", "what
  should I work on", "implement issue #N", "next ticket", "start a unit
  of work", "find unblocked work".
---

# Working on a ticket

Units of work in this repo are GitHub issues. Each unit follows this
loop: pick an unblocked ticket, claim it, implement it, verify it,
commit, and label it implemented. Merging the commit into `main` closes
the issue automatically — do not close issues manually.

## 1. Find a ticket

List candidates with `gh`. The filter is: open, not blocked, not already
in progress, not already implemented:

```bash
gh issue list \
  --state open \
  --search "-is:blocked -label:\"in progress\" -label:implemented" \
  --json number,title,labels,url
```

Prefer higher-priority tickets: `P1` > `P2` > `P3`. If multiple tickets
share the top priority, pick whichever has the clearest scope.

If no ticket exists yet for the unit of work the user is describing,
create one first — do not start without a ticket.

## 2. Claim the ticket

Add the `in progress` label so others know it's taken:

```bash
gh issue edit <n> --add-label "in progress"
```

## 3. Implement

One ticket per unit of work. A single ticket may span multiple commits,
but a single commit must not span multiple tickets.

**Do not push.** The user pushes manually.

## 4. Verify

Code must compile and tests must pass before the unit of work is done.
Per `BUILD.md`:

```bash
redo           # build
redo test      # build + run all unit tests
```

## 5. Commit message format

Follow the project's style. Example:

```
feat(area): Short descriptive title here in imperative voice

Write a longer description here of the changes that were made and why.
Include lists, diagrams, tables, etc. if they help describe why this
change was made.

Resolves #123
```

Last line is either:

- `Resolves #123` — this commit completely finishes the ticket.
- `Progresses: #123` — there's more work to do on the ticket.

## 6. Finish

When the ticket is **completely implemented**:

```bash
gh issue edit <n> --add-label implemented --remove-label "in progress"
```

**Do not close the ticket.** Landing the commit on `main` closes it
automatically.
