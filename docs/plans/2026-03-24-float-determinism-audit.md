# Float Determinism Audit — VIO-413

**Date:** 2026-03-24
**Status:** Audit complete, canary tests added, migration deferred

## Summary

Audited ~120 `f32`/`f64` field declarations in `sim_core/src/types.rs` and categorized them by determinism risk. Added 3 canary tests that verify full-system determinism via serialized state comparison.

**Key finding:** Current simulation IS deterministic on a single platform (canary tests pass). The risk is cross-platform divergence (x86 vs ARM vs WASM) where f32/f64 operations may produce different least-significant-bit results.

## Risk Categories

### High Risk — Float fields in tick-critical paths before/during RNG

| Field | Type | Location | Why risky |
|-------|------|----------|-----------|
| `WearState::wear` | `f32` | Accumulated every tick, compared against band thresholds | Cumulative error, feeds efficiency which affects output quantities |
| `CompositionVec` (`HashMap<ElementId, f32>`) | `f32` values | InventoryItem::Ore, Slag, AsteroidState | Iterated during processing, weights yield calculations, HashMap iteration order non-deterministic for serialization |
| `ResearchState::data_pool` (`HashMap<DataKind, f32>`) | `f32` values | Consumed by lab rolls, affects RNG boundary for tech unlock | Float comparison at probability boundary |
| `InventoryItem::*::kg` | `f32` | All inventory mass fields | Accumulated/decremented every processing tick, compared against thresholds |
| `InventoryItem::Material::quality` | `f32` | Blended during processing | Weighted average arithmetic, affects downstream recipe quality |
| `TechDef::difficulty` | `f32` | Content field, but used as RNG boundary | `rng.gen::<f32>() < threshold` — float comparison at probability edge |

### Medium Risk — Float fields used in tick calculations but not RNG-adjacent

| Field | Type | Location | Why |
|-------|------|----------|-----|
| `StationState::power_available_per_tick` | `f32` | Power budget each tick | Comparison determines module stalling |
| `ShipState::propellant_kg` | `f32` | Decremented on transit | Affects ship availability |
| `BatteryState::charge_kwh` | `f32` | Updated every tick | Charge/discharge arithmetic |
| `ProcessorState::threshold_kg` | `f32` | Compared against inventory mass | Determines processing trigger |
| `GameState::balance` | `f64` | Import/export arithmetic | Affects autopilot decisions |
| `boiloff_rate_per_day` | `f64` | Content field, cast to f32 for loss calc | f64→f32 truncation path |

### Low Risk — Float fields in content definitions (read-only at runtime)

| Field | Type | Notes |
|-------|------|-------|
| `Constants::*` (~40 fields) | `f32`/`f64` | Loaded once from JSON, never mutated during ticks |
| `ModuleDef::mass_kg`, `volume_m3`, etc. | `f32` | Static content |
| `RecipeDef::efficiency` | `f32` | Static content |
| `HullDef::mass_kg`, `cargo_capacity_m3` | `f32` | Static content |
| `ElementDef::density_kg_per_m3` | `f32` | Static content |
| `PricingItem::base_price_per_unit` | `f64` | Static content |

Low-risk fields still flow into calculations, but since they're identical on both platforms (loaded from the same JSON), they don't introduce divergence themselves.

## Existing Mitigations

1. **Sorted iteration before RNG:** All HashMap/BTreeMap iterations are sorted by key before RNG-dependent operations (documented in CLAUDE.md design rules).
2. **Thermal system uses milli-Kelvin integers:** `temp_mk: u32` — the gold standard pattern for deterministic state. No float arithmetic in the thermal tick path.
3. **ChaCha8Rng for deterministic seeding:** RNG itself is platform-independent; the risk is float operations that feed INTO rng comparisons.
4. **Canary tests (added in this PR):** 3 tests in `determinism_canary.rs` that run 200-tick full simulations twice and compare serialized state.

## Migration Plan (future work)

### Phase 1: Highest impact (recommended next)
- **`WearState::wear`** → `wear_millipct: u32` (0 = pristine, 1000 = fully worn). This is the highest-risk field: accumulated every tick, compared against 3 band thresholds, affects output efficiency.
- **`InventoryItem::*::kg`** → `mass_mg: u64` (milligrams). Eliminates float arithmetic in the most exercised code path (processing, mining, depositing).

### Phase 2: Composition and quality
- **`CompositionVec`** → `HashMap<ElementId, u32>` (parts per million, 0–1,000,000). Would require updating all yield/quality formulas.
- **`quality: f32`** → `quality_millipct: u32` (0–1000 for 0.0–1.0).

### Phase 3: Research and economy
- **`data_pool` values** → `u32` (hundredths). Small range, low frequency of use.
- **`balance: f64`** → `balance_cents: i64`. Economy values are large enough that f64 precision loss is real.

### Not recommended for migration
- **Constants:** Read-only, loaded from JSON. Float is fine — they're identical on all platforms.
- **Content definition fields** (ModuleDef, RecipeDef, etc.): Same reasoning.
- **PowerState fields:** Recomputed from scratch each tick, not accumulated. Determinism not at risk.

## Canary Tests Added

| Test | What it verifies |
|------|-----------------|
| `full_sim_deterministic_across_runs` | Two identical-seed 200-tick runs produce identical serialized state (serde_json::Value comparison handles HashMap key ordering) |
| `full_sim_state_actually_changes` | Guards against vacuously true determinism — verifies tick counter advances and inventory changes |
| `float_field_spot_check_deterministic` | Bit-identical comparison of specific high-risk fields: wear (to_bits), inventory masses (to_bits), research state |
