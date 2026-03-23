---
title: "Content-driven sim events engine with composable effects and temporal modifiers"
category: patterns
date: 2026-03-23
tags:
  - sim-events
  - content-driven
  - determinism
  - modifiers
  - effects
  - weighted-random
  - cross-layer
components:
  - sim_core/src/sim_events.rs
  - sim_core/src/engine.rs
  - sim_core/src/types.rs
  - sim_core/src/modifiers.rs
  - sim_world/src/lib.rs
  - sim_daemon/src/routes.rs
  - ui_web/src/components/EventsFeed.tsx
  - content/events.json
tickets:
  - VIO-352
  - VIO-353
  - VIO-354
  - VIO-355
  - VIO-356
  - VIO-357
---

# Content-Driven Event Engine

Learnings from implementing the Sim Events System project (6 tickets, VIO-352 through VIO-357). A content-driven event engine with composable effects, deterministic evaluation, and temporal modifiers.

## Architecture Patterns

### Content-driven definitions over compile-time enums

Event definitions live in `content/events.json`, not as Rust enum variants. Each `SimEventDef` has: id, name, category, rarity (mapped to base weight at load time), cooldown_ticks, conditions, weight_modifiers, targeting, effects, and description_template. Adding new events is a JSON-only change.

This follows the project's established pattern (CLAUDE.md checklist item 14): enums are reserved for engine mechanics (Command, Event, TaskKind), while content categories use strings loaded from JSON.

**Trade-off:** Runtime validation (`validate_event_defs`) replaces compile-time type safety. Validation catches authoring errors at load time, not at compile time.

### Composable effects via enum dispatch

`EffectDef` is a tagged enum with 6 variants: `DamageModule`, `AddInventory`, `AddResearchData`, `SpawnScanSite`, `ApplyModifier`, `TriggerAlert`. Each event definition carries a `Vec<EffectDef>` — effects compose freely. The effect types ARE compile-time enums (not content-driven strings) because they interact with engine internals (state mutation, event emission).

### Deterministic evaluation with sorted iteration + seeded RNG

All collection iteration is sorted by ID before any RNG use — event defs, station IDs, ship IDs, module IDs. The weighted random selection uses cumulative u64 sums with `rng.gen_range(0..total_weight)`. No floating-point anywhere in the selection path.

Integer arithmetic for weights: Rarity maps to integer base weights (Common=100, Uncommon=25, Rare=5, Legendary=1). Weight modifiers use `weight_multiplier_pct` as integer percentage (300 = 3x). Effective weight: `weight = weight * multiplier_pct / 100`.

### Dual emission contract

When an event fires, it emits BOTH:
- `SimEventFired` — narrative event for the UI event log (what happened, to whom, with what effects)
- Individual mechanical events per effect (`WearAccumulated`, `DataGenerated`, `ScanSiteSpawned`, `AlertRaised`)

Existing UI handlers for wear/data/alerts continue to work without modification. The narrative event enables the dedicated event display. Both are emitted; deduplicating would break things.

### Temporal modifier lifecycle

`ApplyModifier` effects create a `Modifier` with `ModifierSource::Event(id)` on the target's `ModifierSet` and register an `ActiveEffect` with an expiry tick. Each tick, `sweep_expired_effects` partitions active effects into expired/remaining, removes modifiers by source, and emits `SimEventExpired`. The sweep runs at the START of evaluation (before selecting new events) so expired modifiers don't influence condition checks.

**Critical:** The sweep runs even when `events_enabled=false`. Without this, disabling events mid-game would leave stale modifiers permanently active.

## Implementation Decisions

| Decision | Rationale |
|----------|-----------|
| `BTreeMap` for cooldowns | Deterministic serialization and iteration order |
| `VecDeque` for history | O(1) append + O(1) eviction as ring buffer |
| Integer weights, not floats | No floating-point non-determinism in selection |
| Single event per tick | Simplicity; global + per-event cooldowns prevent spam |
| Tick phase 4.5 | After research (conditions reflect current research), before scan replenishment |
| `events_enabled: false` in test fixtures | Prevents random events from interfering with unrelated test assertions |

## Gotchas & Pitfalls

### CI clippy is stricter than local

CI runs `clippy -D warnings`. Specific catches: backtick formatting in doc comments (`sim_core` not `sim_core`), format arg inlining (`"{variable}"` not `"{}", variable`), function size warnings. Always run `cargo clippy -- -D warnings` locally before pushing.

### Adding fields to Constants/GameState/GameContent is high-churn

When `events_enabled`, `event_global_cooldown_ticks`, and `event_history_capacity` were added to `Constants`, every test file constructing the struct directly had to be updated. `#[serde(default)]` only helps deserialization, not `Struct { ... }` literals. A subagent was dispatched to fix all ~20 affected files in parallel.

**Mitigation:** Use `..Default::default()` struct update syntax where possible. Consider builder/factory functions for test fixtures.

### Event sync CI enforces FE handlers

`scripts/ci_event_sync.sh` checks that every `Event` variant in `types.rs` has a handler in `applyEvents.ts`. New variants need at minimum a `noOp` handler entry. This caught `SimEventFired` and `SimEventExpired` immediately.

### sim_bench override system needs explicit entries

The `overrides.rs` match in `sim_bench` must have entries for each overridable constant. Three new match arms were needed. Without them, scenario files using `event_global_cooldown_ticks` as an override key fail with "unknown override key".

### Borrow checker: read phase then write phase

Can't pass `&state` and `&mut state` simultaneously. The solution: compute all read-only data (conditions, weight computation, candidate filtering) BEFORE starting mutations (effect application, cooldown updates, history insertion). This creates a clear read phase → write phase separation in `evaluate_events`.

### Function size limits require decomposition

`apply_effects` was decomposed into `apply_single_effect` (match dispatcher) + per-effect helpers (`apply_damage_module`, `apply_add_inventory`, etc.) to satisfy clippy's function size limit. New effect types should follow this pattern: one helper function per effect type.

## Testing Strategy

### Test isolation via events_enabled flag

Test fixtures set `events_enabled: false` by default. Event-specific tests explicitly enable it and reduce `event_global_cooldown_ticks` for faster execution.

### Unit tests cover each layer independently

- Rarity/weight resolution, condition evaluation, serde roundtrips
- Validation rejects: duplicate IDs, zero cooldowns, effect-targeting incompatibility
- Cooldown blocking, history capacity enforcement
- Effect application for each variant, modifier lifecycle (apply → expire → removed)

### Integration tests with real content

4 tests in `sim_control/tests/sim_events_integration.rs` using `load_content("../../content")`:
- **Determinism regression:** Same seed = identical event sequences + final state (serialized comparison)
- **Seed divergence:** Different seeds produce different event sequences
- **State mutation:** Events fire and populate history/cooldowns
- **Modifier expiry:** `SimEventExpired` events emitted after temporal modifiers expire

### sim_bench scenario

`scenarios/events_test.json`: 3 seeds x 5000 ticks with low global cooldown (10 ticks) for high-frequency event observation. Verifies no crashes/panics with events enabled.

## Checklist: Adding New Effect Types

1. Add variant to `EffectDef` enum
2. Add `apply_{effect_name}()` helper function (keep dispatch minimal)
3. Add validation in `validate_event_defs()` — use exhaustive match, no wildcard
4. Add effect-targeting coherence check in `validate_effect_targeting()`
5. Add value range validation in `validate_effect_values()` if applicable
6. Add at least one event definition in test content
7. Add unit test for the helper function
8. If it produces a new `Event` variant, add handler in `applyEvents.ts` + schema in `eventSchemas.ts`
9. Run `./scripts/ci_event_sync.sh` to verify

## Related Documentation

- [Cross-layer feature development](cross-layer-feature-development.md) — The event engine tickets map to this pattern's phases
- [Event sync enforcement](../integration-issues/event-sync-enforcement.md) — CI mechanism for Rust/FE event sync
- [Deterministic integer arithmetic](../logic-errors/deterministic-integer-arithmetic.md) — Patterns used in weight computation
- [Backward compatible type evolution](../integration-issues/backward-compatible-type-evolution.md) — `#[serde(default)]` pattern for new fields
