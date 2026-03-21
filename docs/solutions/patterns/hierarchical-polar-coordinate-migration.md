---
title: Hierarchical polar coordinate spatial system replacing node-edge graph
category: patterns
date: 2026-03-21
tags:
  - spatial
  - coordinate-system
  - fixed-point-arithmetic
  - determinism
  - world-generation
  - body-tree
  - caching
  - integer-math
  - cross-layer-migration
components:
  - sim_core (spatial.rs, types.rs, engine.rs)
  - sim_control
  - sim_world
  - sim_bench
  - sim_daemon
  - ui_web
  - content (solar_system.json, constants.json, asteroid_templates.json)
tickets:
  - VIO-234
  - VIO-235
  - VIO-236
  - VIO-237
  - VIO-238
  - VIO-239
  - VIO-240
  - VIO-242
---

## Problem

The simulation used a flat node-edge graph (`NodeDef`/`NodeId` + edges list) for spatial layout, where entities lived on discrete named nodes and travel was hop-based (BFS). This was replaced with a hierarchical polar coordinate system using fixed-point integer arithmetic for determinism: `Position { parent_body: BodyId, radius_au_um: RadiusAuMicro, angle_mdeg: AngleMilliDeg }`. The migration spanned 8 tickets across 5 crates and the React UI.

## Architecture Overview

**Body tree model:** `OrbitalBodyDef` forms a parent-child tree rooted at the Sun. Each body has polar coordinates relative to its parent. Zone-bearing bodies (`ZoneDef`) define annular bands where scan sites spawn.

**Key types** (sim_core/src/spatial.rs, types.rs):

```rust
pub struct RadiusAuMicro(pub u64);       // 1 AU = 1,000,000 units
pub struct AngleMilliDeg(pub u32);        // 360 deg = 360,000 units
pub struct AbsolutePos { pub x_au_um: i64, pub y_au_um: i64 }  // Sun-centered cartesian
pub struct Position { pub parent_body: BodyId, pub radius_au_um: RadiusAuMicro, pub angle_mdeg: AngleMilliDeg }
pub struct BodyCache { pub absolute: AbsolutePos, pub epoch: u32 }
```

**Data flow:** content/solar_system.json defines the tree -> `build_body_cache()` walks root-to-leaves computing `AbsolutePos` -> `body_cache` lives on `GameState` (`#[serde(skip, default)]`, recomputed on load) -> `compute_entity_absolute(pos, body_cache)` converts any entity position to cartesian.

## Key Patterns

### A. Fixed-point integer arithmetic in RNG path

All RNG-consuming code uses integers. No floats touch the RNG. Newtypes enforce this at the type level.

### B. Area-weighted radius sampling

Naive `uniform(r_min..r_max)` over-represents the inner edge of annular zones because area grows with r^2. Fix:

```rust
pub fn random_radius_in_band(r_min: u64, r_max: u64, rng: &mut impl Rng) -> RadiusAuMicro {
    let r_min_sq = u128::from(r_min) * u128::from(r_min);
    let r_max_sq = u128::from(r_max) * u128::from(r_max);
    RadiusAuMicro(integer_sqrt(rng.gen_range(r_min_sq..=r_max_sq)))
}
```

### C. Epoch-versioned entity caching

`EntityCache.get_or_recompute()` checks if parent body's epoch changed before recomputing polar-to-cartesian. Avoids redundant conversions when bodies haven't moved.

### D. Zone-weighted picking + template bias

Integer-only weighted selection: `scan_site_weight: u32` for zone picking, three-tier bias for templates (match=3, none=2, mismatch=1).

### E. Multi-pass body cache construction

`build_body_cache()` uses iterative multi-pass (not recursion). Each pass places bodies whose parents are already resolved. Asserts progress each pass to detect cycles. Handles unsorted input.

### F. Co-location fast path

`is_co_located()` checks `parent_body` equality first (cheap local distance), falls back to absolute distance only for cross-body checks.

## Cross-Layer Integration

| Layer | Role | Key function |
|-------|------|-------------|
| sim_core | Types + pure math | `build_body_cache`, `travel_ticks`, `random_position_in_zone`, `pick_zone_weighted` |
| sim_control | Autopilot decisions | `compute_entity_absolute` + `travel_ticks` for transit duration |
| sim_world | Content loading + world gen | `build_body_cache` on state load, zone-weighted initial scan sites |
| sim_bench | Scenario runner | Rebuilds `body_cache` on state load, spatial constant overrides |
| sim_daemon | HTTP API | `GET /api/v1/spatial-config`, snapshot enriched with `body_absolutes` |
| ui_web | React FE | `SolarSystemConfig` type, `fetchSpatialConfig()`, `Position` on all entities |

## Pitfalls Encountered

1. **Float determinism trap:** Float math in RNG path breaks cross-platform reproducibility. Solution: integer-only math; floats only in `polar_to_cart()` which is a pure conversion *after* the RNG path.

2. **Inner-edge clustering:** Uniform radius sampling clusters scan sites near inner belt edge. Solution: sample uniformly over r^2 space then `integer_sqrt`.

3. **u64 overflow in distance:** Squaring micro-AU values (up to ~5M for Jupiter) overflows u64. Solution: `u128` intermediates for `distance_squared`, `i128` for signed deltas.

4. **Backward-compatible migration:** Added `Position` alongside `NodeId` with `#[serde(default)]`, then removed `NodeId` in a later ticket. Legacy `nodes`/`edges` kept on `SolarSystemDef` for save compat.

5. **Body tree ordering:** Content JSON may list children before parents. `build_body_cache()` multi-pass resolution handles this.

6. **Wrap-around angles:** Zone spans crossing 360 degrees need modular arithmetic: `(start + uniform(0..span)) % 360_000`.

7. **Event position precision:** Initially events only carried `parent_body` (lossy). PR reviewer caught this; expanded to carry full `Position` for FE accuracy.

8. **Serde-skipped caches:** `body_cache` uses `#[serde(skip, default)]` and must be rebuilt after every deserialization path (state load, sim_bench runner, test fixtures).

## Prevention Strategies

### Determinism
- Sort all collections by stable ID before RNG use — never iterate HashMap/HashSet before calling `rng.gen()`
- No floats in RNG path: integer weights, integer radii, integer-based area weighting
- Add determinism canary tests: run same seed twice, assert byte-identical state

### Cross-Layer Migration
- Two-phase migration: add new type alongside old with `serde(default)`, remove old type in a later ticket
- Order tickets by crate dependency graph (sim_core first, then dependents)
- Search for all references to replaced type before coding to scope the blast radius

### Serialization Safety
- Always `#[serde(default)]` on new fields during migration
- Write round-trip serde tests (serialize -> deserialize -> assert equality)
- Every `#[serde(skip, default)]` field needs a rebuild call in every deserialization path

### Test Patterns
- **Determinism canary:** Same seed twice -> identical output
- **Area distribution verification:** 10K samples, check median is ~707 for [0, 1000] (not 500)
- **Wrap-around angle test:** Generate angles in zone spanning 360 degrees, verify wrapping
- **Legacy format deserialization:** Pin old-format JSON fixture, test it loads correctly

## Cross-References

- [deterministic-integer-arithmetic.md](../logic-errors/deterministic-integer-arithmetic.md) — micro-AU and milli-degree units follow same integer-state principle as milli-Kelvin; u128 intermediates extend the overflow-prevention pattern
- [cross-layer-feature-development.md](cross-layer-feature-development.md) — spatial positioning followed Phase 0 (types) -> Phase 1 (behavior) development pattern
- [backward-compatible-type-evolution.md](../integration-issues/backward-compatible-type-evolution.md) — NodeId to Position migration uses serde(default) pattern
- [event-sync-enforcement.md](../integration-issues/event-sync-enforcement.md) — new spatial events need FE handlers (verify with ci_event_sync.sh)
- [balance-analysis-workflow.md](../logic-errors/balance-analysis-workflow.md) — variable travel times change balance dynamics
