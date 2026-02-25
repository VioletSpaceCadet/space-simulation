# Wear + Maintenance (Phase 1) — Design

## Goal

Station modules accumulate wear while operating, suffer efficiency penalties in bands, and eventually auto-disable. A Maintenance Bay module consumes RepairKits to gradually restore modules. No RNG disasters — gradual suffocation.

Designed to be **generic**: wear math is portable to ships/ship-modules in future phases without refactoring.

## Wear System

### WearState struct

Standalone struct embedded wherever wear applies (station modules now, ships later):

```rust
pub struct WearState {
    pub wear: f32, // 0.0 (pristine) to 1.0 (broken)
}
```

Added to `ModuleState`. Storage modules don't accumulate wear (only processors and maintenance bays tick).

### Accumulation

- Wear increments by `wear_per_run` (defined per `ModuleDef`) each time a processor fires
- Idle/starved modules don't degrade
- `wear_per_run` example: 0.01 per run → 100 runs to reach 1.0

### Efficiency bands

Pure function `wear_efficiency(wear: f32, constants: &Constants) -> f32`:

| Wear range | Efficiency | Label |
|---|---|---|
| 0.0–0.5 | 1.0 (100%) | Nominal |
| 0.5–0.8 | 0.75 (75%) | Degraded |
| 0.8–1.0 | 0.50 (50%) | Critical |

Thresholds and multipliers stored in `constants.json`:

```json
{
  "wear_band_degraded_threshold": 0.5,
  "wear_band_critical_threshold": 0.8,
  "wear_band_degraded_efficiency": 0.75,
  "wear_band_critical_efficiency": 0.5
}
```

### Efficiency application

Multiplier applies to **recipe output quantities only** — ore consumption stays constant. A degraded refinery wastes ore, creating economic pressure.

### Auto-disable

When `wear >= 1.0`, module sets `enabled = false` and emits `ModuleAutoDisabled` event. Player must repair before re-enabling.

## Maintenance Bay Module

### Module definition

New `ModuleBehaviorDef::Maintenance` variant (not a Processor):

```rust
pub enum ModuleBehaviorDef {
    Processor(ProcessorDef),
    Storage { capacity_m3: f32 },
    Maintenance(MaintenanceDef),
}

pub struct MaintenanceDef {
    pub repair_interval_ticks: u64,    // e.g. 30
    pub wear_reduction_per_run: f32,   // e.g. 0.2
    pub repair_kit_cost: u32,          // e.g. 1
}
```

### Module kind state

```rust
pub enum ModuleKindState {
    Processor(ProcessorState),
    Storage,
    Maintenance(MaintenanceState),
}

pub struct MaintenanceState {
    pub ticks_since_last_run: u64,
}
```

### Behavior

- Runs on `repair_interval_ticks` cycle
- Each run: find the most worn module on station (highest `wear`, ties broken by ID sort for determinism)
- Skip if no module has `wear > 0.0`
- Consume 1 RepairKit from station inventory; skip if none available
- Reduce target module's wear by `wear_reduction_per_run`, clamped to 0.0
- Emit `MaintenanceRan` event

### Target selection

Most worn first. Deterministic: sort modules by `(wear DESC, id ASC)`, pick first with `wear > 0.0`. Maintenance Bay skips itself.

## RepairKit Component

- Uses existing `InventoryItem::Component { component_id, count, quality }`
- `component_id: "repair_kit"`, quality always 1.0
- New content file `content/component_defs.json`:

```json
[
  {
    "id": "repair_kit",
    "name": "Repair Kit",
    "mass_kg": 50.0,
    "volume_m3": 0.1
  }
]
```

- Starting inventory includes 5 RepairKits for initial runway
- No crafting in Phase 1 — importable only (future: fabrication recipe)

## Tick Integration

Station module tick (engine step 3) becomes:

1. For each processor module: run existing logic, apply `wear_efficiency()` to outputs, increment wear by `wear_per_run`, check auto-disable
2. For each maintenance module: run repair cycle (interval check, target selection, RepairKit consumption, wear reduction)

Order matters: processors wear first, then maintenance repairs. Within a single tick, a module can wear and then get repaired.

## Events

- `WearAccumulated { station_id, module_id, wear_before, wear_after }` — every processor run
- `ModuleAutoDisabled { station_id, module_id }` — wear hit 1.0
- `MaintenanceRan { station_id, target_module_id, wear_before, wear_after, repair_kits_remaining }` — maintenance bay fired

## Metrics

New fields in `MetricsSnapshot`:

- `avg_module_wear: f32` — average wear across enabled processor modules
- `max_module_wear: f32` — worst-case wear value
- `maintenance_runs: u64` — maintenance bay runs this snapshot period
- `repair_kits_remaining: u64` — total RepairKits across all stations

## Autopilot

- Auto-installs Maintenance Bay if present in inventory
- Auto-enables Maintenance Bay
- No auto-RepairKit management in Phase 1

## Future extensibility (not in Phase 1)

- **Ship wear**: embed `WearState` in `ShipState`, accumulate per-trip, `wear_efficiency()` applies to travel speed or mining yield
- **RepairTarget enum**: `Module(ModuleInstanceId) | Ship(ShipId)` for maintenance bay targeting
- **RepairKit fabrication**: processor recipe consuming iron → RepairKits
- **Wear-per-quality**: lower quality ore = higher wear rate
