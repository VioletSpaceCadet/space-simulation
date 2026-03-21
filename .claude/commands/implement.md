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

6. **For ambiguous tickets** — if the ticket lacks clear acceptance criteria, is tagged "needs design", or the scope is unclear, run `ce:brainstorm` to clarify requirements before implementation.

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

### Frontend/UI tickets: Design iteration

If the ticket involves UI changes (components, panels, layout, styling):

1. **Use the `compound-engineering:frontend-design` skill** for implementation — it produces polished, distinctive UI code rather than generic defaults.
2. **After initial implementation, run the `design-iterator` agent** (subagent_type: `compound-engineering:design:design-iterator`) to iteratively refine the UI through screenshot→analyze→improve cycles. This requires `--chrome` flag and a running Vite dev server.
3. If no `--chrome` flag is available, skip the design iterator and rely on vitest + manual review.

## Phase 4: PR & Review Loop

1. **Push and create a PR** targeting main:
   ```
   git push -u origin <branch-name>
   gh pr create --base main --title "<ticket-id>: <title>" --body-file /tmp/pr-body.md
   ```
   Include a Summary (bullet points) and Test plan.

2. **Watch CI**: `gh pr checks <PR_NUMBER> --watch`
   - If CI fails: read failed logs with `gh run view <RUN_ID> --log-failed`, fix, push, watch again.

3. **Dispatch review agents:**
   - **All PRs:** Dispatch the `pr-reviewer` agent (subagent_type: "pr-reviewer").
   - **Non-trivial PRs** (multi-file, new systems, 200+ lines): Also run `ce:review` for deeper multi-agent analysis.
   - **UI tickets:** Also dispatch `design-implementation-reviewer` (subagent_type: `compound-engineering:design:design-implementation-reviewer`) for visual quality review.

4. **Fix review findings** — fix should-fix items from all reviewers. Commit, push, and re-run CI. Do not ask for confirmation — just fix and push.

5. **Re-review if needed** — if reviewers found Important or Critical issues, dispatch them again after fixes. Repeat until clean.

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

## Multiple tickets

For batching multiple tickets, use `/project-implementation` instead — it accepts a project name or list of ticket IDs, processes them in dependency order, compacts between each, and merges each one into main independently.
