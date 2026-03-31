---
title: "Multi-Project Planning & Consolidation at Scale"
category: patterns
date: 2026-03-30
tags: [planning, linear, project-management, architecture-review, consolidation]
module: all
symptom: "Planning 7 interdependent projects with existing standalone projects that overlap"
root_cause: "Standalone projects designed before the P0-P7 initiative existed needed absorption into the numbered sequence"
---

# Multi-Project Planning & Consolidation at Scale

## Context

Planned 7 projects (P0-P6) with 112 tickets in a single session, while absorbing 2 existing standalone projects (Station Frame+Slot, Strategic Layer + Multi-Station AI) that overlapped with the new project structure.

## Key Patterns

### 1. Architecture Prerequisites Surface During Planning, Not Before

The P2 plan's `/deepen-plan` step (5 parallel review agents) discovered 3 critical prerequisites that weren't in the original design:

- **FacilityCore shared abstraction** (VIO-543) — without this, GroundFacility would duplicate the entire module dispatch pipeline from StationState
- **DataKind/ResearchDomain string migration** (VIO-544) — CLAUDE.md convention said these should be content-driven strings, but they were hardcoded enums. Adding sensor data kinds as more enums would compound the violation.
- **FacilityId command polymorphism** (VIO-545) — without this, every station command would need a duplicate variant for ground facilities

**Lesson:** Architecture review agents on the first complex project (P2) are worth the time. Skip them on simpler projects (P3, P4) where the patterns are established.

### 2. Project Absorption Pattern

Two standalone projects needed to merge into the P-numbered sequence:

| Standalone | Disposition | Why |
|---|---|---|
| Station Frame+Slot (7 tickets) | All 7 → P5 Milestone 1 | Frames ARE station construction foundation |
| Strategic Layer (11 tickets) | Phase C (6) → P6 M1, Phase D split: 2 → P5 M4, 3 → P6 M2 | Strategy = AI, multi-station split by responsibility |

**Pattern:**
1. Move tickets to the absorbing project (change `project` field)
2. Mark standalone project as Completed with summary: "ABSORBED INTO P5/P6. Tickets moved."
3. Update the P-project plan to note which tickets were absorbed and from where

### 3. Content-Driven Convention Enforcement

The DataKind/ResearchDomain audit revealed a gap between documentation (CLAUDE.md says "content-driven strings") and reality (hardcoded Rust enums). This was caught because:
- P2 needed new sensor data kinds → would add more enum variants
- Architecture review agent flagged the convention violation
- Code grep confirmed: DataKind matched in ~30 places, domain_to_data_kind() hardcoded 1:1 mapping

**Lesson:** When planning features that add new content-category types, always grep for the existing type definition first. If it's an enum when CLAUDE.md says string, add a migration prerequisite ticket.

### 4. Planning Efficiency Curve

| Project | Time | Tickets | Notes |
|---|---|---|---|
| P0 (Scoring) | Full research + plan + tickets | 11 | First project — most research needed |
| P1 (Progression) | Reused P0 research, focused plan | 12 | Much faster — context from P0 carried over |
| P2 (Ground Ops) | Full research + deepening + questions | 24 | Complex redesign — worth the extra time |
| P4 (Satellites) | Minimal research, fast plan | 12 | Well-defined scope, patterns established |
| P3 (Tech Tree) | No research, content-focused plan | 8 | Mostly content work, minimal engine changes |
| P5 (Stations) | Absorption + new tickets | 24 | Complex but leveraged existing detailed tickets |
| P6 (AI) | Absorption + new tickets | 19 | Leveraged Strategic Layer's detailed tickets |

**Lesson:** First 2-3 projects need full research. Later projects reuse patterns and go faster. Spend the depth budget on the architecturally novel projects (P2 was the biggest departure from the original roadmap).

### 5. Cross-Project Dependency Tracking

Two cross-cutting tickets (VIO-579, VIO-580) were created after noticing gaps:
- **VIO-579:** Full progression arc regression test (50K ticks ground→orbit→belt) — no per-project scenario covered the full chain
- **VIO-580:** Tick performance benchmark — cumulative overhead of P2+P4 tick steps wasn't specified

**Lesson:** After planning all projects, do a gap analysis: "What tests does NO project cover?" The answer is usually cross-project integration and cumulative performance.

## Prevention / Application

When planning a multi-project initiative:

1. **Plan in dependency order** — each project informs the next (P0→P1→P2→P4→P3→P5→P6)
2. **Deepen the architecturally novel project** — not every project needs 5 review agents
3. **Grep before assuming conventions** — verify that documented conventions match code reality
4. **Absorb, don't duplicate** — existing projects with overlapping scope get absorbed, not paralleled
5. **Add cross-cutting tickets** — integration test + performance benchmark for the full arc
6. **Ask design questions early** — P2 had 3 rounds of questions that fundamentally changed the design. Better to discover "new entity type, not tagged StationState" during planning than during implementation.
