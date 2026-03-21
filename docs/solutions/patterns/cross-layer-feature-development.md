---
title: "Cross-layer feature development pattern"
category: patterns
date: 2026-03-01
tags: [cross-layer, feature-development, sim-core, daemon, ui, content]
derived_from:
  - integration-issues/event-sync-enforcement.md
  - integration-issues/backward-compatible-type-evolution.md
  - integration-issues/module-behavior-extensibility.md
  - logic-errors/deterministic-integer-arithmetic.md
---

## Pattern

New simulation features (Heat System, Energy System, Economy) span all layers: sim_core types, station tick logic, daemon metrics/alerts, content JSON, and React UI. A consistent development pattern has emerged across completed projects.

## The Pattern

### Phase 0: Foundation Types
Add types to `sim_core/src/types.rs` with `#[serde(default)]` on every new field. Write backward-compat deserialization tests. Update all test fixtures. No behavior yet — just types that compile and serialize cleanly.

**Why first**: Types propagate to every layer. Getting them right (integer units, serde compat, naming) before writing behavior avoids costly refactors.

### Phase 1: Core Behavior
Implement the tick-level behavior in sim_core. Use integer arithmetic for state, floats only at conversion boundaries. Sorted collections for determinism. Write unit tests for each behavior function in isolation.

**Key rule**: sim_core has no IO, no async. Pure deterministic state mutation.

### Phase 2: Integration (Overheating, Alerts, Metrics)
Add consequences that cross system boundaries:
- Overheat zones, wear multipliers, stall conditions (sim_core)
- MetricsSnapshot fields (sim_core, surfaced by daemon)
- AlertEngine rules (sim_daemon)

### Phase 3: Observability
- sim_bench scenario overrides for the new system's tunable parameters
- Daemon trend tracking for new metrics
- MCP advisor digest includes new fields

### Phase 4: Frontend
- Event handlers in `applyEvents.ts` (handler map + noOp for informational events)
- TypeScript type updates in `types.ts`
- Panel rendering for new state
- Run `ci_event_sync.sh` to verify exhaustiveness

### Phase 5: Content & Balance
- Module definitions in `module_defs.json`
- Starting state updates in `dev_base_state.json` and `build_initial_state()`
- sim_bench scenarios to validate balance at 30-day and 90-day horizons

## Supporting Learnings

- [Event sync enforcement](../integration-issues/event-sync-enforcement.md) — CI catches missing FE handlers
- [Backward-compatible type evolution](../integration-issues/backward-compatible-type-evolution.md) — `serde(default)` on every new field
- [Module behavior extensibility](../integration-issues/module-behavior-extensibility.md) — Detailed checklist for new module types
- [Deterministic integer arithmetic](../logic-errors/deterministic-integer-arithmetic.md) — Integer state, clamped casts, sorted iteration
- [Balance analysis workflow](../logic-errors/balance-analysis-workflow.md) — Extended scenarios catch long-term issues

## When to apply

Any feature that touches simulation state. If it only touches UI or daemon (no sim_core changes), this pattern is overkill.
