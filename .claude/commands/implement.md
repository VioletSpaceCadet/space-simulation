# Implement

Autonomous single-ticket workflow: read ticket, implement, review, fix, compound, PR, merge into main.

## Input

Argument: $ARGUMENTS (Linear ticket identifier like VIO-123, or a brief description for a new ticket)

## Phase 1: Understand

1. **Fetch the ticket** from Linear using `get_issue`. If the argument is a description rather than a ticket ID, create a new ticket using `save_issue` first.
2. **Read ticket comments** using `list_comments` — comments often contain updated requirements, implementation hints, or design decisions made after ticket creation. Do not skip this.
3. **Check for blockers** — if the ticket has `blockedBy` relationships, verify those tickets are Done. If not, stop and inform the user.
4. **Load relevant skills** — scan `.claude/skills/` and match the task against skill triggers. Load matching skills and follow their checklists.
5. **Update Linear ticket status** to "In Progress" using `save_issue`.

## Phase 2: Branch & Plan

1. **Create a branch** from main:
   ```
   git checkout main && git pull
   git checkout -b fix/<ticket-id>-<short-name>
   ```
   Use `feat/` prefix for features, `fix/` for bugs, `chore/` for maintenance.

2. **Plan the approach** — for non-trivial tickets (new systems, multi-file changes, architectural decisions), use EnterPlanMode to think through the approach before writing code. For simple fixes or well-specified tickets, skip straight to implementation.

## Phase 3: Implement

1. **Write the code.** Follow existing project conventions (see CLAUDE.md). The `after-edit.sh` hook will auto-run `cargo fmt` + `cargo test` on every `.rs` edit.
2. **Write tests** covering the changes — not just happy path.
3. **Run the full relevant test suite** before moving on:
   - `cargo test` for Rust changes
   - `cd ui_web && npm test` for FE changes
   - `./scripts/ci_event_sync.sh` if new Event variants were added
4. **Update documentation** (reference.md, CLAUDE.md) if the change affects public APIs, types, or tick ordering.

## Phase 4: PR & Review Loop

1. **Push and create a PR** targeting main:
   ```
   git push -u origin <branch-name>
   gh pr create --base main --title "<ticket-id>: <title>" --body-file /tmp/pr-body.md
   ```
   Include a Summary (bullet points) and Test plan.

2. **Watch CI**: `gh pr checks <PR_NUMBER> --watch`
   - If CI fails: read failed logs with `gh run view <RUN_ID> --log-failed`, fix, push, watch again.

3. **Dispatch the pr-reviewer agent** using the Agent tool (subagent_type: "pr-reviewer"). Wait for the review.

4. **Fix review findings** — fix should-fix items, address nits you agree with. Commit, push, and re-run CI. Do not ask for confirmation — just fix and push (per memory: "After review: fix, commit, and push without asking").

5. **Re-review if needed** — if the reviewer found Important or Critical issues, dispatch the pr-reviewer again after fixes. Repeat until clean.

## Phase 5: Compound (auto for non-trivial)

After a clean review, assess whether the implementation involved:
- Debugging a non-obvious problem
- Establishing a new pattern or convention
- A tricky solution worth documenting
- A gotcha that someone else would hit

If yes: run `ce:compound` to document the learning in `docs/solutions/`. Use compact-safe mode if context is tight.

If the ticket was routine (simple feature, straightforward fix, well-trodden pattern): skip compound.

## Phase 6: Merge & Close

1. **Squash merge into main**:
   ```
   gh pr merge <PR_NUMBER> --squash --delete-branch
   ```
   This is allowed because CI passed and pr-reviewer is clean.

2. **Update Linear ticket status** to "Done" using `save_issue`.

3. **Clean up** — delete local branch:
   ```
   git checkout main && git pull
   git branch -d <branch-name>
   ```

4. **Compact context** — Run `/compact` to clear stale context before the next ticket.

## Rules

- **NEVER push directly to main.** All changes go through PRs.
- **Auto-merge is allowed** after CI green + pr-reviewer clean (no unresolved should-fix items).
- **Always squash merge.**
- **Always run tests** before creating a PR.
- **Update Linear** at every state change.
- **If stuck**, ask the user rather than guessing.
- **One ticket at a time.** Finish, PR, merge, then move to the next.
- **Compound selectively.** Not every fix needs a doc — only non-trivial learnings.

## When to use /project-implementation instead

Use `/project-implementation` when:
- A Linear project has 5+ tightly coupled tickets with dependency chains
- The work needs to land atomically (all tickets or none)
- You want an intermediate feature branch for integration testing before main

For most day-to-day work (1-3 tickets, independent changes), use `/implement`.
