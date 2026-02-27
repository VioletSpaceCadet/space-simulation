# Project Planner

Turn a vague idea into a concrete design doc and Linear project with tickets, ready for `/project-implementation`.

disable-model-invocation: true

## Input

Argument: $ARGUMENTS (topic, idea, or feature description)

## Phase 1: Research & Context Gathering

Before asking any questions, build a thorough understanding of the problem space:

1. **Read design docs** — `docs/DESIGN_SPINE.md`, `docs/reference.md`, and any relevant files in `docs/plans/`.
2. **Explore the codebase** — Use the Explore agent (Task tool, subagent_type: "Explore") to understand relevant architecture, patterns, and constraints. Target areas the topic likely touches (sim_core types, tick ordering, UI components, content files, etc.).
3. **Check Linear for duplicates** — Search for existing tickets and projects related to the topic using `list_issues` and `list_projects`. Avoid duplicating work that's already planned or in progress.
4. **Gather baseline metrics** (if sim-related) — If the topic touches sim_core, sim_control, or game state, optionally dispatch the sim-e2e-tester agent (Task tool, subagent_type: "sim-e2e-tester") to gather baseline metrics before any changes. This gives a comparison point for the testing plan.

Summarize what you found in 3-5 bullet points before moving to Phase 2.

## Phase 2: Clarifying Questions

Refine the idea into a concrete spec through targeted questions:

1. **Ask questions one at a time** — Do NOT present a wall of questions. Ask one, wait for the answer, then ask the next. Each question should build on previous answers.
2. **Use AskUserQuestion** with multiple-choice options where possible. Include a recommended option as the first choice with "(Recommended)" suffix.
3. **Focus areas in order:**
   - Purpose and success criteria (What does "done" look like?)
   - Scope boundaries (What's explicitly out of scope?)
   - Approach preferences (2-3 approaches with trade-offs and a recommendation)
   - Key constraints (performance, determinism, backwards compatibility, content format)
4. **Stop when scope is clear** — Don't ask questions for the sake of asking. 3-6 questions is typical. Move on once you have enough to design.

## Phase 3: Design

Present the design incrementally. Get user approval on each section before proceeding to the next.

1. **Scale detail to complexity** — Simple sections (e.g., "add a field to a struct") get 2-3 sentences. Complex sections (e.g., new tick phase, new UI panel) get thorough treatment with code sketches.
2. **Sections to cover** (skip any that don't apply):
   - **Data model** — New types, modified types, content file changes. Show struct definitions.
   - **Tick ordering** — Where new logic fits in the tick order. Impact on determinism.
   - **API changes** — New endpoints, modified responses, SSE event changes.
   - **State changes** — What's added to GameState, how it's initialized, serialization impact.
   - **sim_control changes** — Autopilot behavior changes, new command types.
   - **Frontend components** — New panels, modified panels, new hooks, SSE subscription changes.
   - **Content files** — New JSON files, schema changes to existing content.
   - **Error handling** — Failure modes, fallback behavior, alert conditions.
   - **Migration / backwards compatibility** — Impact on existing save states, scenarios, bench configs.
3. **Present each section** with AskUserQuestion offering: "Approve", "Modify" (with notes), "Skip this section".
4. **After all sections approved**, present a full summary of the design for final confirmation.

## Phase 4: Testing Plan

Generate testing tickets based on what the feature touches. Testing is NOT optional.

### If the feature touches sim_core / sim_control / game state:

- **Unit tests** — `cargo test` coverage for new logic. Test edge cases and determinism (same seed = same result).
- **sim_bench scenario** — Update or create scenario JSON if behavior changes affect balance metrics.
- **sim-e2e-tester verification** — Bulk simulation runs comparing before/after metrics. Balance regression checks.
- **Mutation testing** — `cargo-mutants` ticket for critical logic paths (state transitions, RNG-dependent code, resource calculations).

### If the feature touches ui_web:

- **vitest unit tests** — New components, hooks, and utility functions.
- **Playwright E2E** — Only for critical-path smoke tests (SSE connection, basic panel render). Keep minimal per project convention.
- **Chrome-based testing** — fe-chrome-tester agent for visual verification, SSE streaming, panel interactions. Not a formal test suite — ad-hoc verification after implementation.

### Always:

- **Integration test ticket** — End-to-end verification that the full stack works (daemon + UI + new feature).

## Phase 5: Write Design Doc & Create Linear Project

### 5a. Write the design doc

Save to `docs/plans/YYYY-MM-DD-<topic-slug>-design.md` with this structure:

```markdown
# <Feature Name> Design

**Goal:** <One sentence>
**Status:** Planned
**Linear Project:** <link, added after project creation>

## Overview
<2-3 paragraph summary of what this builds and why>

## Design
<All approved sections from Phase 3, consolidated>

## Testing Plan
<Summary from Phase 4>

## Ticket Breakdown
<List of planned tickets with brief descriptions, in dependency order>

## Open Questions
<Any deferred decisions or future considerations>
```

Commit the design doc:
```
git add docs/plans/<filename>.md
git commit -m "docs: add <topic> design doc"
```

### 5b. Create the Linear project

1. **Create the project** using `save_project`:
   - Name: descriptive project name
   - Team: VioletSpaceCadet
   - Description: 1-2 sentence summary
   - State: "Planned"

2. **Create all tickets** using `save_issue`:
   - Title format: `<group-prefix>: <description>` (e.g., "H1-01: Add HeatState to GameState")
   - Include full description with acceptance criteria
   - Set labels: `sim_core`, `fe`, `test`, `docs`, `balance` as appropriate
   - Set priority: Urgent (1) or High (2) for blockers, Medium (3) for standard work, Low (4) for polish/testing
   - Assign to the project

3. **Wire dependencies** using `save_issue` with `blocks` / `blockedBy`:
   - Data model tickets block implementation tickets
   - Implementation tickets block testing tickets
   - Core logic blocks UI that depends on it

4. **Update the design doc** with the Linear project link.

5. **Summarize** — Present the final ticket list to the user with identifiers, titles, and dependency order.

## Handoff

After the project is created, tell the user:

> Project ready for implementation. Run `/project-implementation <project-name>` to begin the ticket-by-ticket implementation workflow.

## Rules

- **Research before asking.** Never ask a question you could answer by reading the codebase.
- **One question at a time.** Let the user think about each question individually.
- **Recommend, don't just list.** When presenting approaches, always indicate which you'd recommend and why.
- **Design section by section.** Don't dump the entire design at once. Get incremental approval.
- **Testing is mandatory.** Every project gets testing tickets appropriate to what it touches.
- **Labels and priorities on every ticket.** No unlabeled tickets.
- **Dependencies must be explicit.** Use Linear's blocks/blockedBy — don't just mention order in descriptions.
- **Don't over-ticket.** Group related small changes into single tickets. A ticket should be a meaningful unit of work (1-4 hours), not a single line change.
