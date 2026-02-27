# Project Implementation

Implement a Linear project end-to-end: read tickets, create branches, write code, review, merge, and deliver a final PR for owner approval.

## Input

Argument: $ARGUMENTS (Linear project name, ID, or slug)

## Phase 1: Project Discovery

1. **Fetch the project** from Linear using `list_projects` or `get_project` with the argument provided.
2. **Fetch all tickets** in the project using `list_issues` filtered by project. Include sub-issues.
3. **Read each ticket** with `get_issue` to understand full scope, descriptions, acceptance criteria, and blocking relationships.
4. **Identify ticket order** — respect `blockedBy` / `blocks` relationships. Build a dependency graph and determine execution order (unblocked tickets first).
5. **Summarize the plan** to the user: list tickets in planned execution order with brief descriptions. Ask for confirmation before proceeding.

## Phase 2: Branch Setup

1. **Create the feature branch** from main:
   ```
   git checkout main && git pull
   git checkout -b feat/<project-slug>
   git push -u origin feat/<project-slug>
   ```
2. **Update the Linear project status** to "In Progress" if not already.

## Phase 3: Ticket Implementation Loop

For each ticket in execution order:

### 3a. Start the ticket
- **Update Linear ticket status** to "In Progress" using `save_issue`.
- **Create a ticket branch** off the feature branch:
  ```
  git checkout feat/<project-slug>
  git checkout -b feat/<project-slug>/<ticket-id>-<short-name>
  ```

### 3b. Implement the ticket
- Read the ticket description and any linked documents for requirements.
- If the ticket needs design exploration, use EnterPlanMode to plan the approach and get user approval before writing code.
- Write the code. Follow existing project conventions (see CLAUDE.md).
- Write tests covering the changes — not just happy path.
- Confirm all tests pass before moving on. Run the relevant test commands from CLAUDE.md.
- Update documentation (reference.md, CLAUDE.md) if the change affects public APIs, types, or tick ordering.

### 3c. Create a PR into the feature branch
- Push the ticket branch.
- Create a PR targeting the feature branch:
  ```
  gh pr create --base feat/<project-slug> --title "<ticket-id>: <title>" --body "..."
  ```
  Include a Summary (bullet points of what changed) and Test plan.

### 3d. CI + Review
- Watch CI: `gh pr checks <PR_NUMBER> --watch`
- If CI fails: read the failed logs with `gh run view <RUN_ID> --log-failed`, fix the issue, push, and watch again.
- Once CI passes: **dispatch the pr-reviewer agent** using the Task tool (subagent_type: "pr-reviewer") to perform a thorough code review.
- If the reviewer finds Important or Critical issues: fix them, push, wait for CI, and request another review.
- Once the review is clean: squash merge the PR.
  ```
  gh pr merge <PR_NUMBER> --squash --delete-branch
  ```

### 3e. Close the ticket
- **Update Linear ticket status** to "Done" using `save_issue`.
- **Return to the feature branch** and pull the merged changes:
  ```
  git checkout feat/<project-slug> && git pull
  ```

### 3f. Repeat
- Continue to the next unblocked ticket. If a ticket was previously blocked, check whether its blockers are now resolved.

## Phase 4: Final Integration

1. **Merge main into the feature branch** to resolve any conflicts:
   ```
   git checkout feat/<project-slug>
   git merge main
   ```
   Resolve conflicts if any. Run full test suite to confirm nothing broke.

2. **Create the final PR** into main:
   ```
   gh pr create --base main --title "feat: <project-name>" --body "..."
   ```
   The body should summarize all tickets completed with their identifiers.

3. **Watch CI** on the final PR: `gh pr checks <PR_NUMBER> --watch`

4. **Dispatch the pr-reviewer agent** for a final comprehensive review of the full feature branch diff.

5. **Fix any issues** found in the final review. Push and re-review until clean.

6. **Notify the user** that the PR is ready for their approval. Do NOT merge PRs into main — that requires owner (@VioletSpaceCadet) approval.

## Phase 5: Cleanup

After the owner merges the final PR:

1. Delete the feature branch: `git push origin --delete feat/<project-slug>`
2. Update the Linear project status to "Completed".
3. Clean up local branches.

## Rules

- **NEVER push directly to main.** All changes go through PRs.
- **NEVER merge into main.** Only the owner does that.
- **Always squash merge** ticket PRs into the feature branch.
- **Always run tests** before creating a PR. Don't PR broken code.
- **Update Linear** at every state change — tickets should reflect real-time progress.
- **If stuck on a ticket**, ask the user rather than guessing. Blocked is better than wrong.
- **One ticket at a time.** Finish, PR, merge, then move to the next. Don't batch unrelated changes.
