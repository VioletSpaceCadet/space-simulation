# Storage Enforcement Design

**Date:** 2026-02-20
**Status:** Approved

## Goal

Make storage real. Hard capacity constraints create industrial pressure:
- Module stalls when output buffer is full
- Ships wait when station can't accept deposits
- Slag becomes dangerous (fills storage, stalls production)
- No physics, no energy grid — just capacity math

## Current State

- Station capacity: `cargo_capacity_m3` (default 10,000 m3)
- `inventory_volume_m3()` calculates used volume from items
- Processors produce output directly into station inventory with **no capacity check**
- Ship deposits have **no capacity check**
- Slag accumulates in a single lot with no removal mechanism

## Design

### Approach: Pre-check Guard

Before a processor runs or a ship deposits, check if the output fits in remaining station capacity. If not, stall/block.

### Module Stall Logic

**In `station.rs` processor tick:**

1. When a processor is ready to run (enabled, interval elapsed, ore threshold met):
   - Refactor `consume_ore_fifo_with_lots()` to support dry-run mode (peek without mutation)
   - Dry-run the consumption to get weighted composition
   - Calculate expected output volume from recipe output specs + actual ore composition
   - Check: `used_volume + output_volume > cargo_capacity_m3`
2. If full:
   - Set `module_state.stalled = true`
   - Reset `ticks_since_last_run = 0` (full cycle restart penalty)
   - Emit `ModuleStalled` event (only on `false -> true` transition)
   - Skip production
3. If space available and was stalled:
   - Set `module_state.stalled = false`
   - Emit `ModuleResumed` event
   - Proceed with normal production (restart cycle from 0)

### Ship Deposit Enforcement

**In `tasks.rs` deposit handling:**

1. Before transferring items from ship to station:
   - Calculate `inventory_volume_m3(items_to_deposit)`
   - Check: `station_used + deposit_volume > station.cargo_capacity_m3`
2. If full:
   - Ship stays in deposit task, retries next tick
   - Emit `DepositBlocked` event (once, tracked by bool flag)
3. When space opens:
   - Deposit proceeds normally
   - Emit `DepositUnblocked` event

### New State Fields

```rust
// ModuleState
stalled: bool  // default false

// ShipTask::Deposit or ship-level
deposit_blocked: bool  // default false, for event dedup
```

### New Events

```rust
ModuleStalled { station_id, module_id, shortfall_m3 }
ModuleResumed { station_id, module_id }
DepositBlocked { ship_id, station_id, shortfall_m3 }
DepositUnblocked { ship_id, station_id }
```

### Output Volume Estimation

Reuse `consume_ore_fifo_with_lots()` with a dry-run parameter to avoid code duplication. From the dry-run result, compute:

- **Material volume:** For each recipe output spec with `ItemKind::Material`: `(rate_kg * yield_fraction * element_fraction) / element_density`
- **Slag volume:** `(consumed_kg - total_material_kg) / density_slag` (2500 kg/m3)

Uses actual ore composition from the lots that would be consumed — accurate, not conservative.

### Metrics

- New: `refinery_stalled_count` — number of modules currently stalled (distinct from `refinery_starved_count` which is no-input)
- Existing `station_storage_used_pct` already tracks capacity pressure

### Stall vs Starved Distinction

- **Starved:** Module ready to run but not enough ore input (existing)
- **Stalled:** Module ready to run but output won't fit in storage (new)

## Emergent Gameplay

- Slag fills storage -> refinery stalls -> ore accumulates -> ships can't deposit -> mining halts
- Players must manage slag (future: jettison, reprocess, export) to keep production flowing
- Storage modules (already defined in types) become valuable upgrades
- Natural throughput bottleneck without simulating energy, heat, or fluid dynamics

## Non-Goals

- No partial deposits (all-or-nothing)
- No output queue per module
- No energy/power enforcement (separate feature)
- No slag removal mechanism (future work)
