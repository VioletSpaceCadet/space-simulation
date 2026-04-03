---
title: "Progression System Implementation — P1 Project Patterns"
category: patterns
date: 2026-04-02
tags: [progression, milestones, game-phase, trade-gating, starting-state, multi-ticket, tick-integration]
component: sim_core, sim_world, sim_control, ui_web
severity: info
---

# Progression System Implementation — P1 Project Patterns

## Overview

P1 "Starting State & Progression Engine" implemented a complete progression system across 12 Linear tickets (VIO-530 through VIO-541) in 8 PRs (#384-391). This document captures reusable patterns, gotchas, and prevention strategies discovered during execution.

## Pattern 1: Bulk File Rename Across Codebase

**Context:** Renaming `dev_base_state.json` to `dev_advanced_state.json` required updating 55 references across scenarios, tests, docs, agents, and source files.

**Approach:** `git mv` + `git grep -l "old_name" | xargs sed -i '' 's/old_name/new_name/g'`

**Gotcha:** Blind find-replace created tautological text in plan documents that described the rename itself. Sentences like "dev_base_state -> dev_advanced_state rename" became "dev_advanced_state -> dev_advanced_state rename".

**Prevention:**
- After bulk rename, grep for the new name appearing in "before -> after" patterns
- Review plan/design docs where the old name appears in a "source -> target" description
- These need the old name preserved on the "before" side

## Pattern 2: Adding a New Field to GameState

**Context:** Adding `progression: ProgressionState` to `GameState` required updating ~18 struct literal constructors across sim_core tests, sim_world, and sim_control.

**How to find all constructors:**
```bash
grep -rn "body_cache: AHashMap::default()" crates/sim_core/src/
```
The `body_cache` field (with `#[serde(skip)]`) is always last in GameState. Any new field goes before it.

**Clippy gotcha:** `Default::default()` works in test files but CI clippy `default_trait_access` lint requires `Type::default()` in non-test (library) code. Use `Type::default()` everywhere to be safe.

**Backward compat:** Always add `#[serde(default)]` on new GameState fields so existing state JSON files deserialize without breaking.

## Pattern 3: New Tick Step Integration

**Context:** Adding milestone evaluation at tick step 4.5 required coordinated changes across multiple crates.

**Checklist when adding a tick step:**
1. Add `Duration` field to `TickTimings` in `instrumentation.rs`
2. Update `iter_fields()` to include the new field
3. Update doc comment field count ("N duration fields")
4. Wrap the new step with `timed!(timings, field_name, expr)` in `engine.rs`
5. Update doc comment on `tick()` in `engine.rs`
6. Update these "field count" assertions:
   - `crates/sim_core/src/instrumentation.rs` — `tick_timings_has_N_fields` and `compute_step_stats_returns_N_entries` (2 tests)
   - `crates/sim_bench/src/runner.rs` — `test_run_seed_produces_output` ("N step timing entries")
   - `crates/sim_daemon/src/main.rs` — `test_perf_returns_stats_after_ticks` ("N step entries")
7. Update `CLAUDE.md` tick order documentation

## Pattern 4: Event Variant Addition

**Context:** Adding `MilestoneReached`, `PhaseAdvanced`, `GrantAwarded` event variants.

**Checklist:**
1. Add variant(s) to `Event` enum in `sim_core/src/types/events.rs`
2. Add handler in `ui_web/src/hooks/applyEvents.ts` (use `noOp` for info-only events)
3. Run `./scripts/ci_event_sync.sh` to verify exhaustiveness
4. When renaming a variant, grep for ALL references to the old name — the condition enum `MilestoneCondition::MilestoneCompleted` is a different type from `Event::MilestoneCompleted`

## Pattern 5: Replacing Boolean Gate with Tiered System

**Context:** Replacing `trade_unlock_tick()` time-based gate with `TradeTier`-based milestone gate.

**Key insight:** A single boolean `trade_unlocked` doesn't distinguish import vs export tiers. When the old code used one boolean, the new tiered system needed two:
- `trade_import_unlocked` (requires `BasicImport` tier)
- `trade_export_unlocked` (requires `Export` tier)

**Audit pattern:** When replacing a boolean with a tiered enum:
1. Find ALL consumers of the boolean (`grep trade_unlocked`)
2. For each consumer, determine which tier it actually needs
3. Split the context field if different consumers need different tiers
4. Update tests: change `state.meta.tick = trade_unlock_tick(...)` to `state.progression.trade_tier = TradeTier::Full`

## Pattern 6: Progression Starting State Design

**Context:** Creating `progression_start.json` — the minimal starting state that avoids gameplay deadlock.

**Minimum viable module set (7 modules):**
- 2x `basic_solar_array` (power)
- `sensor_array` (scanning)
- `exploration_lab` (research)
- `basic_iron_refinery` (ore processing)
- `basic_assembler` (component manufacturing)
- `maintenance_bay` (repair)

**Critical validation:** Every dependency chain from milestones back to starting equipment must work:
- Milestone 1 (first_survey) <- ship surveys <- scan sites exist + ship exists
- Milestone 2 (first_ore) <- ship mines <- asteroid discovered (sensor + deep scan chain)
- Milestone 3 (first_material) <- refinery processes <- ore deposited
- Milestone 4 (first_component) <- assembler runs <- `basic_assembler` in starting inventory

**Prevention:** Multi-seed integration test is mandatory:
```rust
#[test]
fn progression_start_multi_seed_validation() {
    let seeds = [1, 42, 123, 456, 789];
    for seed in seeds {
        // Load state, run 500 ticks with autopilot
        // Assert: first_survey reached, first_tech reached, balance > 0
    }
}
```

## Pattern 7: Ticket Absorption

**Context:** VIO-534 (milestone rewards) was naturally implemented inside VIO-533 (milestone evaluation engine) because the evaluation function already needed to apply grants, advance phases, and update trade tiers.

**When to absorb:**
- Ticket M's scope is fully covered by ticket N's implementation
- No meaningful separate deliverable for M
- The combined change is cleaner as one PR

**How to absorb:**
- Mark the absorbed ticket as Done in Linear
- Add a note: "Absorbed into VIO-NNN — all reward logic implemented in evaluate_milestones()"
- Do NOT create a hollow PR just to close the ticket

## Clippy CI Strictness Reference

The CI runs `cargo clippy` with `-D warnings`. Common surprises for new code:

| Lint | Trigger | Fix |
|------|---------|-----|
| `doc_markdown` | Type names in doc comments without backticks (`MetricsSnapshot`) | Wrap in backticks: `` `MetricsSnapshot` `` |
| `unnecessary_map_or` | `.map_or(false, \|v\| v >= x)` | Use `.is_some_and(\|v\| v >= x)` |
| `default_trait_access` | `Default::default()` in non-test code | Use `TypeName::default()` |
| `too_many_lines` | Function exceeds ~100 lines | Extract helper functions |

**Prevention:** Run `cargo clippy` locally before pushing. The after-edit hook runs `cargo test` but not clippy.

## Related Documentation

- [Multi-epic project execution](multi-epic-project-execution.md) — Prior patterns for cross-epic work
- [Gameplay deadlock: missing starting equipment](../logic-errors/gameplay-deadlock-missing-starting-equipment.md) — The incident that motivated P1's dependency chain validation
- [Cross-layer feature development](cross-layer-feature-development.md) — Patterns for changes spanning Rust + TypeScript

## Project Summary

| Ticket | Title | PR | Key Pattern |
|--------|-------|-----|-------------|
| VIO-530 | dev_base_state rename | #384 | Bulk rename |
| VIO-531 | Milestone schema + types | #385 | Content-driven types |
| VIO-532 | ProgressionState in GameState | #386 | New field propagation |
| VIO-533 | Milestone evaluation engine | #387 | Tick step integration |
| VIO-534 | Milestone rewards | — | Absorbed into VIO-533 |
| VIO-535 | Achievement-gated trade | #388 | Boolean → tiered gate |
| VIO-537 | Progression events | #389 | Event variant addition |
| VIO-536 | progression_start.json | #390 | Starting state validation |
| VIO-538 | Autopilot progression | #391 | Multi-seed validation |
