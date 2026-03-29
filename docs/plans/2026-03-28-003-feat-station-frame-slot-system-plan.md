---
title: Station Frame+Slot System
type: feat
status: active
date: 2026-03-28
origin: docs/brainstorms/entity-depth-requirements.md
linear_project: https://linear.app/violetspacecadet/project/station-frameslot-system-fda7008d3151
---

# Station Frame+Slot System — Phase 1 Plan

## Context

Ships have a fully implemented hull+slot system: `HullDef` with typed `SlotDef` entries, `FittedModule` tracking, slot validation on `FitShipModule`, `recompute_ship_stats()` via the modifier pipeline, autopilot fitting templates, and content-driven hull classes in `hull_defs.json`.

Stations have **none of this**. Modules are a flat unbounded `Vec<ModuleState>` with no frame type, no slot constraints, no capacity limits, and no frame bonuses. `InstallModule` only checks tech gating. The autopilot blindly installs every inventory module.

This plan introduces station frames — the station-side mirror of ship hulls — to create structural constraints, frame bonuses, and the foundation for station diversity (Outpost vs Industrial Hub vs Research Station).

**Origin:** `docs/brainstorms/entity-depth-requirements.md` (R7-R12), `docs/brainstorms/station-frames-requirements.md` (stub)

## Design Decisions

1. **Shared SlotType**: Reuse the `SlotType` string newtype. Station slots use `"industrial"`, `"utility"`, `"research"`, `"structural"`. Ships already use `"industrial"`, `"utility"`, `"propulsion"`. Cross-fitting prevented at command level.

2. **Advisory enforcement**: Existing modules with `slot_index: None` still function. Slot validation only on new installations. Strict enforcement deferred to Phase 2.

3. **FrameDef mirrors HullDef**: Same pattern — id, name, base stats, `Vec<SlotDef>`, bonuses (`Vec<Modifier>`), optional tech gate, tags.

4. **slot_index on ModuleState**: `Option<usize>` with `#[serde(default)]`. Simpler than external mapping. Legacy modules deserialize as `None`.

5. **Frame bonuses via ModifierSource::Frame**: New variant. `recompute_station_stats()` mirrors `recompute_ship_stats()`.

6. **Base stats from frame**: `cargo_capacity_m3` derived from `FrameDef.base_cargo_capacity_m3` + modifiers. Power stays solar-array-driven in Phase 1 (frame defines a `base_power_capacity_kw` for the future but doesn't replace solar generation yet).

## Ticket Breakdown

### SF-01 (VIO-490): Data model — types + newtypes

**Files:**
- `crates/sim_core/src/types/mod.rs` — add `string_id!(FrameId)`
- `crates/sim_core/src/types/content.rs` — add `FrameDef` struct, add `frames: BTreeMap<FrameId, FrameDef>` to `GameContent`
- `crates/sim_core/src/types/state.rs` — add `frame_id: Option<FrameId>` to `StationState`, add `slot_index: Option<usize>` to `ModuleState`
- `crates/sim_core/src/modifiers.rs` — add `ModifierSource::Frame(FrameId)` variant

**Details:**
```rust
// types/mod.rs
string_id!(FrameId);

// types/content.rs
pub struct FrameDef {
    pub id: FrameId,
    pub name: String,
    pub base_cargo_capacity_m3: f32,
    pub base_power_capacity_kw: f32,  // future use
    pub slots: Vec<SlotDef>,          // reuse existing SlotDef
    #[serde(default)]
    pub bonuses: Vec<crate::modifiers::Modifier>,
    #[serde(default)]
    pub required_tech: Option<TechId>,
    #[serde(default)]
    pub tags: Vec<String>,
}

// GameContent addition:
pub frames: BTreeMap<FrameId, FrameDef>,

// StationState addition (serde(default)):
pub frame_id: Option<FrameId>,

// ModuleState addition (serde(default)):
pub slot_index: Option<usize>,
```

All new fields use `#[serde(default)]` for backward compatibility. Existing test fixtures need `frame_id: None` and `slot_index: None` added to struct literals.

**Reuse:** `SlotDef` (content.rs:548-551), `string_id!` macro (mod.rs:72-83), `Modifier` type (modifiers.rs)

**Tests:** FrameDef serde roundtrip, ModifierSource::Frame serde, backward-compat deserialization (old saves without frame_id)

**Dependencies:** None

---

### SF-02 (VIO-491): Content — frame_defs.json + module compatible_slots

**Files:**
- `content/frame_defs.json` (new)
- `content/module_defs.json` (modify — add compatible_slots to station modules)

**Frame definitions (3 initial frames):**

| Frame | Cargo (m3) | Power (kW) | Slots | Bonuses |
|-------|-----------|------------|-------|---------|
| `frame_outpost` | 500 | 30 | 2 utility, 2 industrial, 1 research, 1 structural = 6 | None |
| `frame_industrial_hub` | 2000 | 100 | 10 utility, 10 industrial, 5 research, 5 structural = 30 | None |
| `frame_research_station` | 1000 | 80 | 4 utility, 2 industrial, 6 research, 2 structural = 14 | +15% ResearchSpeed (PctAdditive) |

Industrial Hub sized for current 21-module initial loadout (9 utility + 8 industrial + 4 research) with growth headroom.

**Module compatible_slots assignments (all 21 station modules):**

| Category | Modules | Slot Type |
|----------|---------|-----------|
| Processors | basic_iron_refinery, basic_smelter, heating_unit, electrolysis_unit, plate_press | `["industrial"]` |
| Assemblers | basic_assembler, structural_assembler, shipyard | `["industrial"]` |
| Labs | materials_lab, exploration_lab, engineering_lab, propulsion_lab | `["research"]` |
| Utility | basic_solar_array, basic_battery, basic_radiator, sensor_array, maintenance_bay | `["utility"]` |
| Thermal | crucible, casting_mold | `["industrial"]` |
| Automated | automated_refinery, automated_assembler | `["industrial"]` |

Ship equipment modules (mining_laser, cargo_expander, etc.) keep their existing compatible_slots unchanged.

**Dependencies:** SF-01

---

### SF-03 (VIO-492): Content loading + validation

**Files:**
- `crates/sim_world/src/lib.rs` — add `load_frame_defs()`, update `validate_content()`, update `build_initial_state()`

**Details:**
- `load_frame_defs()` mirrors `load_hull_defs()` (lib.rs:510-529): read `frame_defs.json`, parse `Vec<FrameDef>`, build `BTreeMap<FrameId, FrameDef>`, panic on duplicate IDs
- Add to `load_content()` pipeline alongside hull loading
- Validation (mirrors hull validation at lib.rs:292-364):
  - Station modules with non-empty compatible_slots: warn if slot type not in any frame
  - Frame slot types: warn if no station module is compatible
  - Station modules with active behavior (non-Equipment): warn if compatible_slots is empty (content authoring reminder)
- `build_initial_state()`: set `station.frame_id = Some(FrameId("frame_industrial_hub".into()))` on initial station

**Reuse:** `load_hull_defs()` pattern, `validate_hull_defs()` pattern

**Tests:** Content loading with valid/invalid frames, validation warnings, build_initial_state includes frame_id

**Dependencies:** SF-01, SF-02

---

### SF-04 (VIO-493): recompute_station_stats + frame bonus application

**Files:**
- `crates/sim_core/src/commands.rs` — add `recompute_station_stats()` function

**Details:**
```rust
pub fn recompute_station_stats(station: &mut StationState, content: &GameContent) {
    use crate::modifiers::{ModifierSource, StatId};

    station.modifiers.remove_where(|s| matches!(s, ModifierSource::Frame(_)));

    if let Some(ref frame_id) = station.frame_id {
        if let Some(frame) = content.frames.get(frame_id) {
            for bonus in &frame.bonuses {
                let mut modifier = bonus.clone();
                modifier.source = ModifierSource::Frame(frame_id.clone());
                station.modifiers.add(modifier);
            }
            station.cargo_capacity_m3 = station
                .modifiers
                .resolve_f32(StatId::CargoCapacity, frame.base_cargo_capacity_m3);
        }
    }
}
```

Call sites: after `build_initial_state()`, after loading saved state (if frame_id is set).

**Reuse:** `recompute_ship_stats()` (commands.rs:683-734) as direct template, `ModifierSet::remove_where()` (modifiers.rs:178-184)

**Tests:** Frame with bonuses -> ResearchSpeed modifier applied, cargo_capacity resolved from frame base + modifiers, no-frame station unchanged

**Dependencies:** SF-01, SF-03

---

### SF-05 (VIO-494): InstallModule slot validation

**Files:**
- `crates/sim_core/src/types/commands.rs` — add `slot_index: Option<usize>` to `Command::InstallModule`
- `crates/sim_core/src/commands.rs` — update `handle_install_module()` (lines 60-152)
- `crates/sim_core/src/types/events.rs` — add `slot_index: Option<usize>` to `Event::ModuleInstalled`

**Updated InstallModule command:**
```rust
Command::InstallModule {
    station_id: StationId,
    module_item_id: ModuleItemId,
    slot_index: Option<usize>,  // NEW — None = auto-find
}
```

**Validation logic in handle_install_module (when station has frame):**
1. Look up frame from `station.frame_id` + content
2. If `slot_index` is `Some(idx)`:
   - Validate `idx < frame.slots.len()`
   - Validate `frame.slots[idx].slot_type` is in module's `compatible_slots`
   - Validate no existing module has `module.slot_index == Some(idx)`
3. If `slot_index` is `None`:
   - Find first unoccupied slot where `frame.slots[i].slot_type` is in module's `compatible_slots`
   - If none found, reject with new event `ModuleNoCompatibleSlot`
4. Set `module_state.slot_index = Some(resolved_idx)` on the new ModuleState
5. If station has no frame (`frame_id.is_none()`): legacy behavior, no slot assignment

**New event:** `ModuleNoCompatibleSlot { station_id, module_def_id }` — autopilot uses this to know installation failed.

**Tests:** Install with valid explicit slot, wrong slot type rejected, occupied slot rejected, auto-find picks first compatible, auto-find fails when full, frameless station legacy behavior, backward compat (old commands without slot_index)

**Dependencies:** SF-01, SF-03

---

### SF-06 (VIO-495): Autopilot slot awareness

**Files:**
- `crates/sim_control/src/agents/station_agent.rs` — update `manage_modules()` (lines 60-119)

**Changes to manage_modules():**
- Before issuing `InstallModule`, check if the station has a frame
- If framed: for each inventory module, check if any compatible slot is unoccupied
  - Build occupied set: `station.modules.iter().filter_map(|m| m.slot_index).collect::<HashSet<_>>()`
  - For each frame slot, check: `!occupied.contains(&idx) && module_def.compatible_slots.contains(&slot.slot_type)`
  - If no compatible slot free, skip this module
- Use `slot_index: None` (auto-find) in the command — let the handler resolve the exact slot
- If frameless: existing behavior (install all)

**Reuse:** `StationAgent::fit_ships()` (station_agent.rs:625-693) as pattern reference for inventory checking + deterministic ordering

**Tests:** Autopilot skips install when all compatible slots full, installs when slot available, frameless station unchanged

**Dependencies:** SF-05

---

### SF-07 (VIO-496): API + FE data layer + event sync

**Files:**
- `crates/sim_daemon/src/routes.rs` — add `frames` to `ContentResponse`
- `ui_web/src/types.ts` — add `FrameDef` interface, update `StationState` and `ModuleState` types, update `ContentResponse`
- `ui_web/src/hooks/applyEvents.ts` — update `ModuleInstalled` handler for `slot_index` field
- `ui_web/src/hooks/eventSchemas.ts` — update `ModuleInstalled` Zod schema
- `scripts/ci_event_sync.sh` — add `ModuleNoCompatibleSlot` to allow-list (if not handled in FE)

**ContentResponse addition:**
```rust
pub frames: std::collections::BTreeMap<sim_core::FrameId, sim_core::FrameDef>,
```

**FE type additions:**
```typescript
interface FrameDef {
  id: string;
  name: string;
  base_cargo_capacity_m3: number;
  base_power_capacity_kw: number;
  slots: SlotDef[];
  bonuses: Modifier[];
  tags: string[];
}

// StationState additions:
frame_id?: string;

// ModuleState additions:
slot_index?: number;
```

**No new UI components in Phase 1** — just the data layer. Frame-aware station detail view is a future FE ticket.

**Tests:** FE type compilation, event sync CI passes, Zod schema roundtrip for updated ModuleInstalled

**Dependencies:** SF-01, SF-05

---

## Dependency Graph

```
SF-01 (types) ──┬── SF-02 (content JSON)
                │
                ├── SF-03 (loading/validation) <- SF-02
                │
                ├── SF-04 (modifier recompute) <- SF-03
                │
                ├── SF-05 (install validation) <- SF-03
                │        │
                │        ├── SF-06 (autopilot)
                │        │
                │        └── SF-07 (API + FE)
                │
                └── (SF-02 + SF-03 can run after SF-01)
```

**Parallelizable:** SF-02 and SF-04 can proceed in parallel after SF-01. SF-06 and SF-07 can proceed in parallel after SF-05.

## Critical Existing Code to Reuse

| Pattern | Source | Target |
|---------|--------|--------|
| `HullDef` struct | content.rs:532-546 | `FrameDef` struct |
| `load_hull_defs()` | sim_world/lib.rs:510-529 | `load_frame_defs()` |
| Hull validation | sim_world/lib.rs:292-364 | Frame validation |
| `recompute_ship_stats()` | commands.rs:683-734 | `recompute_station_stats()` |
| `handle_fit_ship_module()` validation | commands.rs:737-820 | `handle_install_module()` slot validation |
| `ModifierSource::Hull(HullId)` | modifiers.rs:85 | `ModifierSource::Frame(FrameId)` |
| `StationAgent::fit_ships()` | station_agent.rs:625-693 | `manage_modules()` slot awareness |

## Migration Strategy

- `StationState.frame_id`: `Option<FrameId>` with `#[serde(default)]` -> old saves get `None`
- `ModuleState.slot_index`: `Option<usize>` with `#[serde(default)]` -> old modules get `None`
- `build_initial_state()` assigns `frame_industrial_hub` to the starting station
- Advisory enforcement: unslotted modules (`slot_index: None`) continue to function
- No save-breaking changes

## Out of Scope (Phase 2+)

- Frame tier upgrades (Mk1 -> Mk2 -> Mk3)
- Station construction from kits (`OutputSpec::Station { frame_id }`)
- Station templates (frame + module loadout blueprints)
- Strict slot enforcement (modules must have valid slot to tick)
- Expansion modules (bolt-on modules that add slots)
- Frame-aware station detail UI panel

## Verification

1. `cargo test -p sim_core` — all existing tests pass with new default fields
2. `cargo test -p sim_world` — content loading includes frames, validation passes
3. `cargo test -p sim_control` — autopilot respects slot availability
4. `cargo clippy` — no warnings from new code
5. `cd ui_web && npm test` — FE types compile, event schemas valid
6. `./scripts/ci_event_sync.sh` — event exhaustiveness check passes
7. Manual: run daemon + UI, verify station shows frame_id in state, content endpoint includes frames
8. sim_bench: run baseline scenario, verify no regression (modules still install, production unchanged)
