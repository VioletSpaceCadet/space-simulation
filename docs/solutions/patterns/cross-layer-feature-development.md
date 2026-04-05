---
title: "Cross-layer feature development pattern"
category: patterns
date: 2026-03-01
last_refreshed: 2026-04-05
tags: [cross-layer, feature-development, sim-core, daemon, ui, content, command-event]
derived_from:
  - integration-issues/event-sync-enforcement.md
  - integration-issues/backward-compatible-type-evolution.md
  - integration-issues/module-behavior-extensibility.md
  - logic-errors/deterministic-integer-arithmetic.md
  - patterns/autopilot-strategic-layer-foundation-patterns.md
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
- Starting state updates in `dev_advanced_state.json` and `build_initial_state()`
- sim_bench scenarios to validate balance at 30-day and 90-day horizons

## Checklist: Adding a `Command` + `Event` Pair (VIO-483)

When a new feature needs an external entry point (runtime config change, user
action) that the sim applies and the FE must observe, the `Command`+`Event` pair
spans **five layers**. Miss any one and the feature silently no-ops somewhere.

1. **`sim_core/src/types/commands.rs`** — add the `Command::Foo { ... }` variant.
2. **`sim_core/src/types/events.rs`** — add the `Event::FooChanged {}` variant.
   **Use empty struct form `{}` even with no fields** (see
   [event sync enforcement — unit-variant gotcha](../integration-issues/event-sync-enforcement.md#gotcha-unit-variant-serde-trap-vio-483)).
3. **`sim_core/src/engine.rs`** — handle the command in `apply_commands`, mutate
   state, `emit` the event through `state.counters` so the `EventEnvelope` gets
   a monotonic ID.
4. **`sim_daemon/src/routes.rs`** — add the HTTP route (`POST /api/v1/...`),
   enqueue the command via the shared command queue, and dirty any agent caches
   that depend on the changed state (e.g., `controller.mark_strategy_dirty()`).
5. **`ui_web/src/hooks/applyEvents.ts`** — add the handler entry. Use `noOp` if
   the event is informational and doesn't mutate UI state (e.g., a refetch is
   triggered by a separate channel).
6. **`scripts/ci_event_sync.sh`** — run locally after adding the event. Confirm
   the variant count increases. If you used a unit variant by mistake, CI will
   pass green while the FE silently drops the event.
7. **Tests** — cover the command handler in `sim_core` (state mutation + event
   emitted), the daemon route (status code + enqueue), and the FE handler if
   it's non-trivial. At least one test should dispatch through the full stack.

See [autopilot strategic layer foundation patterns](./autopilot-strategic-layer-foundation-patterns.md)
for the full 15-item migration checklist and worked examples, including the
cache-dirty-flag pattern for runtime config changes.

## Supporting Learnings

- [Event sync enforcement](../integration-issues/event-sync-enforcement.md) — CI catches missing FE handlers (with unit-variant gotcha)
- [Backward-compatible type evolution](../integration-issues/backward-compatible-type-evolution.md) — `serde(default)` on every new field
- [Module behavior extensibility](../integration-issues/module-behavior-extensibility.md) — Detailed checklist for new module types
- [Deterministic integer arithmetic](../logic-errors/deterministic-integer-arithmetic.md) — Integer state, clamped casts, sorted iteration
- [Balance analysis workflow](../logic-errors/balance-analysis-workflow.md) — Extended scenarios catch long-term issues
- [Autopilot strategic layer foundation patterns](./autopilot-strategic-layer-foundation-patterns.md) — Command+Event 5-layer checklist, cache gating, cross-layer scenario overrides

## When to apply

Any feature that touches simulation state. If it only touches UI or daemon (no sim_core changes), this pattern is overkill.
