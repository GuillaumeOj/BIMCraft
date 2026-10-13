# Project management

BIMCraft tracks work in GitHub Issues and plans it on a single GitHub Project board. Everything is public and free, contributors need nothing beyond a GitHub account, and pull requests close issues automatically. This page describes the board, the workflow and the one-time setup.

## Principles

- **One issue, one outcome.** An issue describes a user-visible result or a well-bounded technical task, small enough for one pull request. Larger work is a parent issue with sub-issues.
- **Link the specification.** When a requirement applies, put its ID in the issue title, e.g. `[STR-07] Rebar cover check`, and in the pull request (`Requirements:` line of the template).
- **The board is the plan, issues are the truth.** Discussion, decisions and acceptance criteria live in the issue; the board only holds planning fields.
- **Pull requests close issues.** Write `Closes #123` in the pull request so merging moves the issue to *Done*.

## Labels

Labels say *what kind* of work and *where* it is. Planning information (priority, size, cycle) lives in board fields, not labels.

| Group | Labels | Use |
| --- | --- | --- |
| Type | `bug`, `enhancement`, `spec`, `docs`, `chore` | Exactly one per issue. The issue forms set `bug`, `enhancement` and `spec`. |
| Area | `area:core`, `area:geometry`, `area:georef`, `area:architecture`, `area:structure`, `area:analysis`, `area:documentation`, `area:interop`, `area:api`, `area:ui`, `area:tooling` | One or more. Matches the sections of the [specification](../specifications.md). |
| Status helpers | `blocked`, `good first issue`, `help wanted`, `no-user-docs` | `no-user-docs` is required by CI when a pull request has no user documentation. |

Issue *types* (Bug, Feature, Task) are only available to organisation-owned repositories. While the repository is owned by a personal account, the type labels play that role.

## Milestones

One milestone per roadmap phase of the [specification](../specifications.md#15-roadmap-and-phases): `Phase 0 – Foundations`, `Phase 1 – MVP`, `Phase 2 – V1`, `Phase 3 – Platform`. The milestone description repeats the phase gate, so “done” is unambiguous. Issues without a milestone are not planned yet.

## The board

A single user-level project, **BIMCraft**, linked to the repository.

### Fields

| Field | Type | Values |
| --- | --- | --- |
| Status | Single select (built in) | `Triage`, `Backlog`, `Todo`, `In progress`, `In review`, `Done` |
| Priority | Single select | `Urgent`, `High`, `Medium`, `Low` |
| Size | Single select | `XS` (< 2 h), `S` (½ day), `M` (1–2 days), `L` (≤ 1 week), `XL` (split it) |
| Cycle | Iteration | 2-week iterations |
| Discipline | Single select | `Architecture`, `Structure`, `Documentation`, `Platform` |

Milestone, labels, assignees and sub-issue progress come from the issues and can be shown as columns.

### Views

| View | Layout | Filter / grouping | Purpose |
| --- | --- | --- | --- |
| Triage | Table | `status:Triage` | New issues waiting for labels, priority and size |
| Current cycle | Board by Status | `cycle:@current` | Daily work |
| My work | Board by Status | `assignee:@me -status:Done` | Personal queue |
| Backlog | Table grouped by Priority | `status:Backlog,Todo` | Planning the next cycle |
| Roadmap | Roadmap by Milestone | `-status:Done`, date field: Cycle | Phase progress |

### Automations

Enable these built-in workflows (project → ⋯ → Workflows):

- **Item added to project** → set Status to `Triage`.
- **Auto-add to project** → filter `is:issue,pr is:open` on the repository, so every new issue and pull request lands in Triage.
- **Item closed** and **Pull request merged** → set Status to `Done`.
- **Pull request linked to issue** → set Status to `In review`.
- **Auto-archive items** → `is:closed updated:<@today-30d`.

## Workflow

1. **Triage** (at least weekly): for each item in *Triage*, check it is clear and not a duplicate, add type and area labels, Priority, Size and Discipline, then move it to *Backlog*. Close what will not be done, with the reason. Split anything sized `XL` into sub-issues.
2. **Plan a cycle**: at the start of each 2-week cycle, pull items from *Backlog* into the cycle and set them to *Todo*. Prefer finishing a parent issue's sub-issues over starting new ones.
3. **Work**: assign yourself and move the item to *In progress*. Branch from `main`, follow [AGENTS.md](../../AGENTS.md), and open the pull request with `Closes #<issue>`.
4. **Review and merge**: the automations move the issue to *In review* and then *Done*.

Coding agents follow the same workflow; they can read and update issues with the `gh` CLI, but never create, close or comment on issues or pull requests without the user asking (AGENTS.md §1).

## Keyboard and terminal

- On github.com, `Ctrl/Cmd+K` opens the command palette and `?` lists shortcuts. On the board, arrow keys move between cells, `Enter` edits a field and `Space` opens an item.
- [`gh dash`](https://github.com/dlvhdr/gh-dash) gives a terminal dashboard of issues and pull requests.
- Common commands: `gh issue list --label area:structure`, `gh issue create --label bug`, `gh issue develop 123 --checkout` (creates a branch linked to the issue).

## One-time setup

Run these once, as a maintainer, with a `gh` token that has the `project` scope (`gh auth refresh -s project`). They change the GitHub repository, so coding agents must not run them on their own.

```bash
REPO=GuillaumeOj/BIMCraft

# Labels (--force updates existing ones)
gh label create bug              -R "$REPO" --force --color d73a4a --description "Something does not work as expected"
gh label create enhancement      -R "$REPO" --force --color a2eeef --description "New capability or improvement"
gh label create spec             -R "$REPO" --force --color 5319e7 --description "Change to the specification"
gh label create docs             -R "$REPO" --force --color 0075ca --description "Documentation only"
gh label create chore            -R "$REPO" --force --color cfd3d7 --description "Tooling, CI, maintenance"
for area in core geometry georef architecture structure analysis documentation interop api ui tooling; do
  gh label create "area:$area" -R "$REPO" --force --color c5def5
done
gh label create blocked          -R "$REPO" --force --color b60205 --description "Waiting on something else"
gh label create "good first issue" -R "$REPO" --force --color 7057ff --description "Good for newcomers"
gh label create "help wanted"    -R "$REPO" --force --color 008672 --description "Extra attention is needed"
gh label create no-user-docs     -R "$REPO" --force --color ededed --description "Pull request without user documentation"

# Milestones
for title in "Phase 0 – Foundations" "Phase 1 – MVP" "Phase 2 – V1" "Phase 3 – Platform"; do
  gh api "repos/$REPO/milestones" -f title="$title"
done

# Project, linked to the repository; note the number printed by the first command
gh project create --owner @me --title BIMCraft
NUMBER=1
gh project link "$NUMBER" --owner @me --repo "$REPO"
gh project field-create "$NUMBER" --owner @me --name Priority   --data-type SINGLE_SELECT --single-select-options "Urgent,High,Medium,Low"
gh project field-create "$NUMBER" --owner @me --name Size       --data-type SINGLE_SELECT --single-select-options "XS,S,M,L,XL"
gh project field-create "$NUMBER" --owner @me --name Discipline --data-type SINGLE_SELECT --single-select-options "Architecture,Structure,Documentation,Platform"
```

The CLI cannot do the rest; finish in the project settings on github.com:

1. Edit the built-in **Status** field options to `Triage`, `Backlog`, `Todo`, `In progress`, `In review`, `Done`.
2. Add an **Iteration** field named `Cycle` with 2-week iterations.
3. Create the views and enable the automations listed above.
4. Make the project public (Settings → Visibility) so contributors can see the plan.
