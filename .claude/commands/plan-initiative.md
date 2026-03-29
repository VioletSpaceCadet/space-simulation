# Plan Initiative

Turn a high-level vision into a multi-project roadmap with Linear projects, each containing a planning ticket ready for `/project-planner`.

This is for **large-scale work** spanning 3-10 projects and 30-100+ tickets. For single-project planning, use `/project-planner` instead.

disable-model-invocation: true

## Input

Argument: $ARGUMENTS (high-level vision, feature area, or strategic direction)

## Phase 1: Local Research (Parallel)

Before asking questions, understand the current state of the codebase and any prior work:

1. **Dispatch repo-research-analyst agent** — understand current architecture, content files, game state, how the area works today. Focus on: what exists, what's missing, what's the gap between current state and the vision.

2. **Dispatch learnings-researcher agent** — search `docs/solutions/`, `docs/plans/`, `docs/brainstorms/` for relevant prior work. Surface institutional knowledge, past patterns, and known pitfalls.

3. **Read design docs** — `docs/DESIGN_SPINE.md` for design philosophy, `docs/reference.md` for type reference, relevant brainstorm docs.

4. **Check Linear** — search existing projects and issues for related/overlapping work. Avoid duplicating what's planned.

Run agents in parallel. Summarize findings in 5-10 bullets before proceeding.

## Phase 2: External Research

Dispatch **best-practices-researcher agent** to study how other games and systems solve the same problems:

- Research 4-8 relevant games/systems (e.g., Factorio, Stellaris, Dwarf Fortress, KSP, RimWorld, EVE Online, Space Engineers, X4, ONI, Civilization)
- For each: first player action, progression gates, phase transitions, AI/automation handling, economic models, scoring systems
- Focus on **actionable design patterns**, not high-level descriptions
- Extract **cross-cutting patterns** (production-as-gate, resource geography, conversion chain depth, wealth-scaled challenges, transformative mid-game unlocks)

Summarize the 5-8 most applicable patterns for this initiative.

## Phase 3: Idea Refinement

Using AskUserQuestion, refine the vision through 3-6 targeted questions:

- **Scope:** What's the full arc? Where does it start, where does it end?
- **Priorities:** What's most important to get right first?
- **Constraints:** What must NOT change about existing systems?
- **AI/Automation:** How should the autopilot/AI handle this?
- **Timeline feel:** Is this 2-week sprint or 2-month roadmap?

Skip if the user's description is already detailed. Offer: "Your description is thorough. Should I proceed to analysis, or refine further?"

## Phase 4: Gap Analysis

Dispatch **spec-flow-analyzer agent** with the vision + research findings:

- Identify missing user flows and edge cases
- Find potential deadlocks (inability to progress)
- Surface scope boundary questions
- Highlight risks and failure modes

Summarize the top 5-10 gaps and risks.

## Phase 5: Write the Roadmap Plan

Write a comprehensive plan document to `docs/plans/YYYY-MM-DD-NNN-feat-<descriptive-name>-plan.md`.

### Plan structure:

```markdown
---
title: "feat: <Initiative Name>"
type: feat
status: active
date: YYYY-MM-DD
---

# <Initiative Name>

## Overview
[2-3 paragraphs: what this initiative delivers and why it matters]

## The Problem
[Current state analysis: what exists, what's missing, why it needs to change]

## The Vision / Arc
[The player/user experience from start to end. Acts or phases with fantasy, core activities, key milestones, and design pattern references]

## Projects

### Project N: <Name> (~X-Y tickets)
For each project:
- **What it delivers** — concrete deliverables
- **Why it matters** — how it fits in the arc
- **Scope** — bullet list of major work items
- **Key decisions** — table of decision/recommendation/rationale
- **Dependencies** — which projects must come first
- **Estimated tickets** — rough count
- **Critical risk** — what could go wrong
- **AI development checkpoint** — how AI/scoring improves after this project

## Dependency Graph
[ASCII or mermaid diagram showing project ordering]

## Cross-Cutting Concerns
- Codebase readiness (prerequisite refactors)
- AI development strategy (how AI improves with each project)
- Backward compatibility
- Balance validation strategy
- Event sync (FE)

## Risk Analysis
[High/Medium/Low risks with likelihood, impact, mitigation]

## Estimated Total Scope
[Table: project | tickets | key deliverable | AI component]

## Sources & References
[Internal docs + external game design research]
```

### Plan quality standards:
- Each project is independently valuable (can stop after any project and have shipped something useful)
- Dependencies form a clear ordering, not a tangled web
- Every project has an AI/measurement checkpoint
- Risks include at least one deadlock analysis
- Game design patterns cited with specific mechanics, not vague references
- Total scope estimate is realistic (not optimistic fantasy)

## Phase 6: Create Linear Projects

For each project in the plan, create a Linear project and a single planning ticket:

### Project creation (`save_project`):
- **Name:** `P<N>: <Descriptive Name>`
- **Team:** VioletSpaceCadet
- **Summary:** One-line deliverable summary (max 255 chars)
- **Description:** Full project description including:
  - Overview and plan reference
  - Scope with bullet list
  - Game design patterns to consider (from Phase 2 research)
  - Dependencies on other projects
  - Standards/quality expectations
- **Priority:** High for foundation projects, Medium for content projects, Low for end-game
- **State:** Planned

### Planning ticket creation (`save_issue`):
- **Title:** `Plan P<N>: <Project Name>`
- **Project:** The project just created
- **Description:** Planning ticket with:
  - **Objective** — what this planning session should produce
  - **Key Outcomes** — numbered list of concrete planning deliverables (schemas, ticket breakdowns, acceptance criteria)
  - **Research to Do** — codebase areas to review during planning
  - **Game Systems to Consider** — relevant patterns from Phase 2 research
  - **Standards** — checklist of quality gates (tests, sim_bench scenarios, browser tests, determinism, event sync)

### Blocking relationships:
- Wire `blocks`/`blockedBy` between planning tickets matching the dependency graph

## Phase 7: Summary & Handoff

Present to the user:
1. **Plan file location** and key stats (projects, total tickets)
2. **Linear project list** with URLs
3. **Recommended execution order**
4. **Next step:** "Run `/project-planner` on the first planning ticket (VIO-XXX) to break it into implementation tickets."

## Rules

- **Research before planning.** Never propose architecture without reading the codebase.
- **External research is mandatory.** Every initiative benefits from studying how other systems solved the same problems. Cite specific mechanics, not vibes.
- **Projects, not tickets.** This command produces project-level planning. Individual ticket breakdowns happen in `/project-planner`.
- **Each project must be independently valuable.** The initiative can be abandoned after any project without wasting the completed work.
- **AI/measurement is a first-class concern.** Every project should improve the AI's ability to handle the game. Include scoring/measurement in the earliest project.
- **One project at a time.** The plan defines the roadmap. Execution is sequential, each project getting its own detailed planning cycle.
- **Standards on every planning ticket.** Tests, sim_bench validation, browser tests (if FE), determinism, event sync — spell out what "done" means.
- **Validate with sim_bench.** Any project touching sim_core should have a sim_bench scenario validating the change across multiple seeds.
