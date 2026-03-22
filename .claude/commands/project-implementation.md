# Project Implementation

Implement a Linear project end-to-end: read tickets, then for each one — branch from main, implement, review, fix, compound, merge into main, repeat.

## Input

Argument: $ARGUMENTS (Linear project name, ID, slug — or a list of ticket IDs like "VIO-217 VIO-218 VIO-219")

## Phase 1: Discovery & Queue

1. **Resolve the input:**
   - If the argument matches a project: fetch all tickets using `list_issues` filtered by project. Skip tickets already Done.
   - If the argument is ticket IDs: fetch each with `get_issue`.
   - If the argument is a description: search Linear for matching tickets.
2. **Read each ticket** with `get_issue` for full scope, acceptance criteria, and blocking relationships. **Read comments** with `list_comments` — they often contain implementation hints or updated requirements.
3. **Build execution order** — respect `blockedBy` / `blocks` relationships. Unblocked tickets first.
4. **Summarize the queue** to the user: list tickets in planned order with brief descriptions. Ask for confirmation before proceeding.
5. **Update the Linear project status** to "In Progress" if applicable.

## Phase 2: Ticket Loop

For each ticket in execution order, run this autonomous cycle:

### 2a. Start
- **Update Linear ticket status** to "In Progress".
- **Load relevant skills** — match the ticket against `.claude/skills/` triggers.
- **Create a branch** from main:
  ```
  git checkout main && git pull
  git checkout -b feat/<ticket-id>-<short-name>
  ```
  Use `fix/` for bugs, `chore/` for maintenance.

### 2b. Implement
- Read ticket description, comments, and any linked documents.
- For non-trivial tickets, use EnterPlanMode to plan the approach before writing code.
- Write the code. Follow project conventions (see CLAUDE.md).
- Write tests — not just happy path.
- Run the full relevant test suite:
  - `cargo test` for Rust changes
  - `cd ui_web && npm test` for FE changes
  - `./scripts/ci_event_sync.sh` if new Event variants were added
- Update documentation (reference.md, CLAUDE.md) if types, APIs, or tick ordering changed.

**Frontend/UI tickets:** Use the `compound-engineering:frontend-design` skill for polished UI. After implementation, run the `design-iterator` agent for screenshot→analyze→improve cycles (requires `--chrome`).

### 2c. PR & Review Loop
1. **Push and create a PR** targeting main:
   ```
   git push -u origin <branch-name>
   gh pr create --base main --title "<ticket-id>: <title>" --body-file /tmp/pr-body.md
   ```
2. **Watch CI**: `gh pr checks <PR_NUMBER> --watch`. Fix failures, push, watch again.
3. **Dispatch review agents (in parallel):**
   - **All PRs:** Dispatch `pr-reviewer` agent (subagent_type: "pr-reviewer") for correctness, tests, error handling (checklist items 1-10).
   - **Non-trivial PRs** (200+ lines, multi-file, new systems): Also dispatch `pattern-recognition-specialist` agent (subagent_type: `compound-engineering:review:pattern-recognition-specialist`) for scalability, duplication, hardcoded content (checklist items 11-14).
   - **UI tickets:** Also dispatch `design-implementation-reviewer`.
4. **Fix review findings** — fix should-fix items from all reviewers. Commit, push, re-run CI. Do not ask for confirmation.
5. **Re-review if needed** — if reviewers found Important or Critical issues, dispatch them again after fixes. Repeat until clean.

### 2d. Compound (auto for non-trivial)
If the implementation involved debugging, new patterns, or tricky solutions: run `ce:compound` to document the learning. Skip for routine changes.

### 2e. Merge & Close
1. **Squash merge into main**:
   ```
   gh pr merge <PR_NUMBER> --squash --delete-branch
   ```
2. **Update Linear ticket status** to "Done".
3. **Clean up local branch**:
   ```
   git checkout main && git pull
   git branch -d <branch-name>
   ```

### 2f. Continue
- **Check next ticket** — if it was previously blocked, verify its blockers are now Done.
- **Continue** to the next ticket.

## Phase 3: Wrap Up

After all tickets are complete:

1. **Update Linear project status** to "Completed" (if processing a full project).
2. **Run `ce:compound-refresh`** if the project touched areas with existing docs/solutions/ learnings — check for stale docs.
3. **Summarize** what was completed: list tickets merged with their PR numbers.

## Rules

- **NEVER push directly to main.** All changes go through PRs.
- **Auto-merge into main** after CI green + pr-reviewer clean (no unresolved should-fix items).
- **Always squash merge.**
- **Always run tests** before creating a PR.
- **Update Linear** at every state change.
- **If stuck on a ticket**, ask the user rather than guessing.
- **One ticket at a time.** Finish, PR, merge, compact, then next.
- **Compound selectively.** Only non-trivial learnings.
