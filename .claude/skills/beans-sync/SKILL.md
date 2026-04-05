---
name: beans-sync
description: "Sync beans with GitHub issues. TRIGGER when: user says 'sync beans', 'sync issues', 'update github issues', or when a bean status changes and needs to reflect on GitHub."
---

# Beans ↔ GitHub Issue Sync

Keeps beans and GitHub issues in sync. Beans are the source of truth for status; GitHub issues mirror that status.

## Usage

```
/beans-sync              # Full sync: push bean statuses to GitHub
/beans-sync pull         # Pull: create beans for new GitHub issues not yet tracked
/beans-sync push         # Push: update GitHub issues to match bean statuses
/beans-sync <bean-id>    # Sync a single bean with its linked GitHub issue
```

## How beans link to GitHub issues

Each bean's body contains a line like `GitHub issue #118`. This is the link. Extract the issue number with a regex: `GitHub issue #(\d+)`.

## Push sync (bean → GitHub)

For each bean that has a linked GitHub issue:

1. Read the bean status
2. Read the GitHub issue state via `gh issue view <number> --json state,stateReason`
3. Apply the mapping:

| Bean status   | GitHub action                                       |
|---------------|-----------------------------------------------------|
| completed     | `gh issue close <number> --reason completed`        |
| scrapped      | `gh issue close <number> --reason "not planned"`    |
| in-progress   | Add "in-progress" label if not present              |
| todo          | Remove "in-progress" label if present               |
| draft         | No action (issue stays open)                        |

4. If the bean priority changed, update GitHub labels:

| Bean priority | GitHub label    |
|---------------|-----------------|
| critical      | p0-critical     |
| high          | p1-important    |
| normal        | p2-normal       |
| low           | p2-normal       |
| deferred      | (remove all p-labels) |

5. Report what changed.

## Pull sync (GitHub → beans)

1. List all open GitHub issues: `gh issue list --state open --json number,title,labels,body`
2. List all beans: `beans list --json`
3. For each GitHub issue not linked to any bean (no bean body contains `GitHub issue #<number>`):
   - Create a new bean using the same type/priority mapping from the original import:
     - Labels `bug` → type bug, `feature` → type feature, `chore` → type task, default task
     - Labels `p0-critical` → critical, `p1-important` → high, `p2-normal` → normal, default normal
   - Include `GitHub issue #<number>` in the description
4. For each closed GitHub issue that has a bean still in `todo` or `in-progress`:
   - Update the bean to `completed` (if closed as completed) or `scrapped` (if closed as not planned)
5. Report what changed.

## Full sync (default)

Run push first, then pull. Report a summary table of all changes.

## Single bean sync

When given a bean ID:
1. `beans show <id> --json` to get status and body
2. Extract the GitHub issue number from the body
3. Run push sync for just that bean
4. Report what changed

## Important

- Never close a GitHub issue without the bean being `completed` or `scrapped`
- Never change a bean status based on GitHub — beans are the source of truth for status
- The pull direction only creates new beans and marks beans done when GitHub issues close externally
- Always report changes as a table: bean ID, GitHub issue, action taken
