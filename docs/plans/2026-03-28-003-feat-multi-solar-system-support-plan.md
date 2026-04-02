---
title: "feat: Multi-Solar-System Support"
type: feat
status: active
date: 2026-03-28
origin: docs/brainstorms/hierarchical-agent-decomposition-requirements.md
---

# Multi-Solar-System Support

## Enhancement Summary

**Deepened on:** 2026-03-28
**Sections enhanced:** All major sections
**Research agents used:** Architecture Strategist, Performance Oracle, Pattern Recognition Specialist, Data Integrity Guardian, Code Simplicity Reviewer, Learnings Researcher, Best Practices Researcher (7 parallel agents)

### Key Improvements
1. **Simplified from 14 to 8 tickets** — merged F1+F3 into F2, G4 into G1; deferred E3 (extra systems) and G3 (cross-system AI objectives) as YAGNI
2. **Use `Equipment` module behavior** instead of new `WarpDrive` variant — avoids 19-location cascading match arms, saves ~150 lines (from module-behavior-extensibility learning + pattern recognition review)
3. **Strong `SystemId` newtype** via `string_id!()` — not a type alias to `BodyId` — catches misuse at compile time (architecture + pattern reviewers)
4. **Use `AHashMap` for `body_to_system`** with `#[serde(skip)]` — matches existing `body_cache` and `density_map` patterns (performance + pattern reviewers)
5. **Added required data integrity mitigations** — extend `validate_state()` for parent_body references, bump `CURRENT_SCHEMA_VERSION`, content-driven arrival position (data integrity review)
6. **Added observability phase** — warp metrics, multi-system sim_bench scenario, alert rules (cross-layer development pattern)
7. **Validated against industry patterns** — architecture matches Stellaris, Aurora 4X, Factorio approaches (best practices research)

### Architecture Validated By Industry Patterns
| This Plan | Industry Pattern | Source |
|-----------|-----------------|--------|
| Flat lists + system_of() filter | Aurora 4X: per-system entity lists | Pulsar4x, Aurora 4X |
| Content-driven fixed-duration warp routes | Stellaris hyperlanes, Aurora jump points | Paradox, Steve Walmsley |
| Ship stays at origin during warp | Stellaris: ship in origin until arrival | Paradox Clausewitz engine |
| Single global RNG, per-system later | Factorio: single RNG, chunk-derived for gen | Factorio FFF-415 |
| Full-fidelity sim, no LOD | Distant Worlds 2: 50K+ bodies full sim | Code Force |

### New Considerations Discovered
- **`validate_state()` must check `Position.parent_body` references** against body_cache — prevents panic on load with stale content (data integrity review)
- **Warp arrival position should be content-driven** (constant in `constants.json`), not hardcoded — with deterministic angle spread for same-tick arrivals (data integrity review)
- **Per-system RNG for scan site replenishment** is a contained improvement worth considering early — adding/removing systems changes RNG consumption for ALL systems (data integrity review)
- **Document scaling boundary** — filter-after-collect efficient up to ~500 entities/system; pre-partition beyond that (performance review)
- **Need backward-compat deserialization tests** per cross-layer Phase 0 pattern (pattern recognition review)
- **Need at least one test with `load_content("../../content")`** using real multi-system content (PR review rule 18)

## Overview

Expand the simulation from a single solar system (Sol) to multiple star systems, starting with Alpha Centauri and scaling to 5 nearby systems. Adds a warp drive mechanism for interstellar travel, system-scoped AI decisions, and content infrastructure for multi-system gameplay. Designed to future-proof for eventual multi-threaded/distributed execution where each system ticks independently.

**Prerequisites:** Phases C+D from the [Strategic Layer + Multi-Station AI](https://linear.app/violetspacecadet/project/strategic-layer-multi-station-ai-5867915277ee) project (VIO-479 through VIO-489) must be complete before multi-system work begins. Phase C provides the strategic config layer; Phase D provides multi-station infrastructure. Both are required foundations.

## Problem Statement

The simulation currently operates within a single solar system (Sol). All entities, resources, and agent decisions are scoped to one star. This limits the game's strategic depth to local industrial optimization. Multi-system support creates a new tier of gameplay: interstellar expansion, cross-system logistics, and long-term colonization strategy — the natural scaling direction for the "industrial entropy in space" identity (see DESIGN_SPINE.md).

Additionally, the future vision includes running systems on separate threads or machines. The current architecture has a single `GameState` processed by a single `tick()` call with a single RNG stream. Adding multi-system support now provides the natural partitioning boundary (per-system state) that makes future parallelization possible without a rewrite.

## Proposed Solution

Four implementation phases building on the completed Phases A-D:

- **Phase E: Star System Content Model** — Add `SystemId` concept, galactic positioning, multi-system content loading and validation, Alpha Centauri content
- **Phase F: Warp Drive Mechanics** — New `module_warp_drive` equipment module, `TaskKind::Warp` variant, content-driven warp routes, dev base state explorer ship
- **Phase G: System-Scoped AI** — Agent candidate searches filtered by system, per-system scan site replenishment, strategic layer cross-system objectives
- **Phase H: Multi-System UI** — Interstellar zoom LOD, system collapse/expand, warp visualization (coordinates with solar map redesign Phase 5)

## Architectural Decisions

### AD1. Warp drive is an `Equipment` module in a hull slot

Warp capability comes from fitting a `module_warp_drive` (with `Equipment` behavior type) into a hull slot. Not all ships can warp — only those with a warp module fitted. This fits the existing hull+slot system, creates a meaningful fitting tradeoff (warp module vs. cargo expander), and enables progression (better warp modules = faster warp). A tech unlock (`tech_warp_drive`) gates availability.

The `InitiateWarp` command checks for warp capability via `ship.fitted_modules.iter().any(|m| m.module_def_id == "module_warp_drive")` — a module_def_id check, not a behavior type check. This uses the existing `Equipment` behavior variant (same as cargo expander, propellant tank) and avoids adding a new `ModuleBehaviorDef` variant, which would cascade through 19 locations across 6 layers (see `docs/solutions/integration-issues/module-behavior-extensibility.md`).

**Why not hull capability:** Too rigid. Players can't retrofit existing ships.

**Why not tech unlock alone:** No per-ship decision. Eliminates fleet composition strategy.

**Why not a new `WarpDrive` behavior variant:** `Equipment` already handles passive modules with no tick behavior. A new variant would trigger the "add a module type" tax (~150 lines across 12+ files, 19 locations). Use `Equipment`; if warp modules later need tick behavior, add the variant then.

### AD2. Warp duration from `warp_ticks_per_ly` constant

Warp travel time computed from a single constant in `constants.json`: `warp_ticks_per_ly: 115`. Duration = `distance_ly * warp_ticks_per_ly`, where `distance_ly` is derived from galactic positions of the two star systems. At 4.37 ly to Alpha Centauri: `4.37 * 115 ≈ 503 ticks` = ~21 game-days at 60 min/tick. Tunable via constants.json and overridable in sim_bench scenarios.

**Why a single constant over per-route definitions:** At MVP, no asymmetric route exists or is planned. A distance-based formula covers all cases cleanly. Per-route overrides (`warp_routes.json` with bidirectional flags) can be added when gameplay design requires asymmetry — not before a single route has been tested. This eliminates ~100 lines of type definitions, content loading, validation, and route lookup code.

**Why not instant teleport:** Removes strategic cost of expansion. The warp duration creates meaningful time investment — ships committed to interstellar transit aren't available for local operations.

### AD3. Ship position during warp stays at origin, snaps to destination on arrival

During warp, the ship's `Position` remains at its departure point in the origin system. On the tick when warp completes, position updates to the destination body. The ship is invisible on the origin system map during warp (filtered by UI via task state). A progress indicator appears on the interstellar view.

**Why not interpolated position:** `Position { parent_body, radius, angle }` requires a parent body. Interstellar space has no body. Creating a synthetic "warp corridor" body per route is over-engineering for what is fundamentally a loading screen. The ship isn't interacting with anything during warp — it's just waiting.

**Why not destination position immediately:** The ship would appear at the destination before arriving, confusing the player and the agent system.

### AD4. System identification via precomputed `body_to_system` map

An `AHashMap<BodyId, SystemId>` computed once during content loading alongside `body_cache` and `density_map`. Derived by walking each body's parent chain to its root star. Stored on `GameContent` with `#[serde(skip)]` (it's derived, not serialized — same pattern as `density_map`). Every system-scoping decision calls `content.system_of(body_id) -> Option<&SystemId>`.

`SystemId` is a **strong newtype** via `string_id!(SystemId)` — NOT a type alias for `BodyId`. This provides compile-time safety: passing a non-root `BodyId` where a `SystemId` is expected is a type error. The `body_to_system` map stores `SystemId` values derived from root body IDs; all lookups go through `system_of()`, making the conversion boundary clear. Provide `From<BodyId>` for explicit conversion at the construction site.

**Why `AHashMap` not `BTreeMap`:** Consistency with existing `body_cache` (which uses `AHashMap<BodyId, BodyCache>` on `GameState`). O(1) lookup vs O(log n). At 60 bodies the difference is negligible, but the pattern should match.

**Why not a `system_id` field on `OrbitalBodyDef`:** Redundant data that can diverge from the parent chain. The parent chain is the source of truth.

**Why `Option<&SystemId>` return, not panic:** Defensive runtime safety. If a bug introduces a body ID not in the map (e.g., dynamically created asteroid referencing unknown body), the caller handles the error case rather than a hard panic in production.

### AD5. Warp consumes zero propellant

Warp is a module capability, not fuel-based. The warp drive module "bends space" — no propellant consumed during interstellar transit. Local transit within the destination system still costs propellant normally.

**Why:** Propellant-based warp requires either a new fuel type (adds content/UI complexity for one feature) or uses existing propellant at absurd quantities (strands ships). Zero-cost warp focuses the strategic decision on fleet composition (which ships get warp modules) and time investment (warp duration), not fuel logistics. This is consistent with DESIGN_SPINE rule 2.1: "Physics realism is abstracted into transfer cost, fuel cost, travel time" — warp abstracts to time only.

**Future iteration:** Advanced warp modules could consume a "warp fuel" resource as a progression gate. The zero-cost default ensures the basic mechanic works without this.

### AD6. Warp cannot be cancelled once initiated

Once a ship begins warp, it completes the full duration. No mid-warp cancellation.

**Why:** Cancellation requires answering "where does the ship end up?" — interstellar space has no `Position`. The ship would need to either snap back to origin (feels like a cheat) or be stranded in void (unfun, violates DESIGN_SPINE rule 2.3: pressure must be recoverable). Commitment creates strategic weight — sending a ship on a 500-tick warp is a meaningful decision.

### AD7. Scan site replenishment is per-system

Each star system has its own replenishment target. `replenish_target_count` from constants applies per-system. The replenishment function groups zones by system, computes per-system scan site counts, and replenishes each system independently.

**Why:** Without per-system pools, Sol's higher zone weights and more zones dominate replenishment. Alpha Centauri would starve for scan sites. Per-system pools ensure each system has exploration opportunities proportional to its zone count.

### AD8. Research stays global (sim-wide)

All labs across all systems contribute to the same `ResearchState`. A lab in Sol and a lab in Alpha Centauri both feed the same `data_pool` and `evidence` maps. This is consistent with the existing design where raw data is "sim-wide (on ResearchState), not station inventory" (see reference.md).

**Future iteration:** System-specific research bonuses (e.g., Proxima labs produce more survey data due to unique stellar environment). This is a content change, not an architectural one.

### AD9. Economy stays global

`GameState.balance` remains a single value shared across all systems. Import/export at any station affects the same pool. Uniform pricing across systems.

**Future iteration:** Per-system markets with different pricing, trade routes with transport costs. This is a future project, not part of multi-system MVP.

### AD10. New `TaskKind::Warp` variant (not extended Transit)

```rust
TaskKind::Warp {
    destination_system: SystemId,
    destination_body: BodyId,
    total_ticks: u64,
    elapsed_ticks: u64,
}
```

Warp is fundamentally different from Transit: no propellant cost, fixed duration not distance-based, no chain task (`then`), different position semantics. Mixing these into `Transit` with flags would create a confusing variant with conditional fields. A separate variant makes event sync, UI handling, and agent logic explicit.

### AD11. Multi-system autopilot is manual-only until strategic layer integration

After Phase G system-scoping is complete, the autopilot operates within local systems only. It never initiates interstellar warp on its own. Players manually command ships to warp. The strategic layer (Phase C) gains cross-system objectives in a later iteration — this is explicitly deferred from the initial multi-system implementation.

**Why:** The strategic layer (Phase C) must be functional and tested for single-system multi-station before adding cross-system concerns. Layering cross-system intelligence on an untested strategic layer creates compounding bugs.

### AD12. `SolarSystemDef` type unchanged; warp constant in `constants.json`

`SolarSystemDef` is not modified. The multiple-system concept is expressed by having multiple root bodies (stars with `parent: None`) in the `bodies` list, each positioned at galactic scale. The `warp_ticks_per_ly` constant lives in `constants.json` alongside other tuning parameters — no separate warp content file needed for MVP.

If per-route overrides are needed later, a `warp_routes.json` file can be added and loaded onto `GameContent` directly (following the pattern of `pricing.json`, `alerts.json`, `hull_defs.json`). Do NOT embed route data in `SolarSystemDef` — that would conflate spatial layout with travel configuration.

## Technical Approach

### Nearby Star Systems (Content Data)

| # | System | Distance (ly) | Distance (AU) | Type | Notable Bodies |
|---|--------|--------------|---------------|------|---------------|
| 1 | **Sol** (existing) | 0 | 0 | G2V yellow star | Earth, Mars, Jupiter, asteroid belt |
| 2 | **Alpha Centauri** | 4.37 | 276,363 | Trinary | Rigil Kentaurus A (G2V), Toliman B (K1V, 23 AU from A), Proxima C (M5.5V, 13,000 AU from A/B, planets b + d) |
| 3 | **Barnard's Star** | 5.96 | 376,916 | M4V red dwarf | Barnard's Star b (super-Earth, 0.4 AU) |
| 4 | **Wolf 359** | 7.86 | 497,074 | M6.5V red dwarf | Very dim, minimal planets known |
| 5 | **Lalande 21185** | 8.31 | 525,533 | M2V red dwarf | Possible unconfirmed planets |
| 6 | **Sirius** | 8.60 | 543,873 | Binary | Sirius A (A1V, brightest from Earth), Sirius B (white dwarf, 20 AU from A) |

All distances fit comfortably in `i64` micro-AU (max ~9.2 x 10^18). Alpha Centauri at 276B micro-AU, Sirius at 544B micro-AU — both well within range.

Each system gets unique resource profiles via zone `resource_class` values and asteroid template weights:
- **Alpha Centauri:** Mixed (multi-star, diverse zones)
- **Barnard's Star:** VolatileRich (cold M-dwarf, ice-rich)
- **Wolf 359:** MetalRich (compact system, dense asteroid field)
- **Lalande 21185:** Mixed (moderate)
- **Sirius:** MetalRich (hot A-star, refractory elements)

### Phase E: Star System Content Model (3 tickets)

#### E1. SystemId type + body_to_system map + validation

**New types in `sim_core/src/types/mod.rs`:**

```rust
string_id!(SystemId);  // Strong newtype, NOT a type alias

impl From<BodyId> for SystemId {
    fn from(body_id: BodyId) -> Self { SystemId(body_id.0) }
}
```

**New function in `sim_core/src/spatial.rs`:**

```rust
/// Build a map from every body to its root star (system).
/// Called once at content load time in init_caches().
pub fn build_body_to_system_map(bodies: &[OrbitalBodyDef]) -> AHashMap<BodyId, SystemId> {
    // For each body, walk parent chain to root (parent == None).
    // Root body's ID becomes SystemId via From<BodyId>.
    // Panic on cycle (already validated by build_body_cache).
}
```

**Store on `GameContent` with `#[serde(skip)]`:**

```rust
pub struct GameContent {
    // ... existing fields ...
    #[serde(skip)]
    pub body_to_system: AHashMap<BodyId, SystemId>,  // derived in init_caches()
}
```

Populated in `init_caches()` alongside `density_map` — same pattern.

**Helper method:**

```rust
impl GameContent {
    pub fn system_of(&self, body_id: &BodyId) -> Option<&SystemId> {
        self.body_to_system.get(body_id)
    }

    pub fn systems(&self) -> BTreeSet<&SystemId> {
        self.body_to_system.values().collect()
    }
}
```

**Multi-system content validation (extend `sim_world::validate_content()`):**
- Body ID uniqueness across ALL systems (already enforced, but document explicitly)
- Every body reachable from exactly one root star
- At least one root body (star) exists
- Root bodies with `body_type: Star` enforced
- Zone bounds within parent body's orbital range

**Tests:**
- `body_to_system_map` with single system returns all bodies mapped to Sun
- `body_to_system_map` with two systems returns correct system per body
- Validation rejects duplicate body IDs across systems
- Validation rejects orphaned bodies (parent doesn't exist)

#### E2. Alpha Centauri content + galactic positioning

**Extend `content/solar_system.json` with Alpha Centauri bodies:**

```json
{
  "bodies": [
    // ... existing Sol bodies ...
    {
      "id": "alpha_centauri_a",
      "name": "Rigil Kentaurus",
      "parent": null,
      "body_type": "Star",
      "radius_au_um": 276363000000,
      "angle_mdeg": 210000,
      "solar_intensity": 1.1,
      "zone": null
    },
    {
      "id": "alpha_centauri_b",
      "name": "Toliman",
      "parent": "alpha_centauri_a",
      "body_type": "Star",
      "radius_au_um": 23000000,
      "angle_mdeg": 0,
      "solar_intensity": 0.5,
      "zone": null
    },
    {
      "id": "proxima_centauri",
      "name": "Proxima Centauri",
      "parent": "alpha_centauri_a",
      "body_type": "Star",
      "radius_au_um": 13000000000,
      "angle_mdeg": 120000,
      "solar_intensity": 0.0017,
      "zone": null
    },
    {
      "id": "proxima_b",
      "name": "Proxima b",
      "parent": "proxima_centauri",
      "body_type": "Planet",
      "radius_au_um": 49000,
      "angle_mdeg": 0,
      "solar_intensity": 0.65,
      "zone": null
    },
    {
      "id": "ac_inner_zone",
      "name": "AC Inner Zone",
      "parent": "alpha_centauri_a",
      "body_type": "Zone",
      "radius_au_um": 2000000,
      "angle_mdeg": 0,
      "solar_intensity": 0.5,
      "zone": {
        "radius_min_au_um": 1500000,
        "radius_max_au_um": 2500000,
        "angle_start_mdeg": 0,
        "angle_span_mdeg": 360000,
        "resource_class": "Mixed",
        "scan_site_weight": 2
      }
    },
    {
      "id": "ac_outer_belt",
      "name": "AC Outer Belt",
      "parent": "alpha_centauri_a",
      "body_type": "Belt",
      "radius_au_um": 4000000,
      "angle_mdeg": 0,
      "solar_intensity": 0.2,
      "zone": {
        "radius_min_au_um": 3500000,
        "radius_max_au_um": 4500000,
        "angle_start_mdeg": 0,
        "angle_span_mdeg": 360000,
        "resource_class": "VolatileRich",
        "scan_site_weight": 1
      }
    }
  ]
}
```

Sol's root body (`sun`) gets galactic positioning: `radius_au_um: 0, angle_mdeg: 0` (origin). Alpha Centauri A at `radius_au_um: 276363000000, angle_mdeg: 210000` (galactic polar offset from Sol).

**Determinism check:** `polar_to_cart` uses `f64` intermediates. At 276B micro-AU, `(276_363_000_000f64 * cos(210°)).round() as i64` = deterministic on same platform. Cross-platform determinism is already a known risk (VIO-413) — this doesn't make it worse since body_cache is computed at load time, not during tick RNG paths.

**Tests:**
- `build_body_cache` handles two root stars
- `AbsolutePos::distance` between Sol and Alpha Centauri bodies returns correct ~276K AU
- Scan sites in AC zones have correct parent_body references

#### E3. Remaining 4 star systems content

Add Barnard's Star, Wolf 359, Lalande 21185, and Sirius to `solar_system.json`. Each system gets:
- Root star body with galactic positioning
- 1-2 companion bodies if applicable (Sirius B)
- 1-3 mining zones appropriate to the system's resource class
- Planet bodies where known (Barnard's Star b)

This is a content-only change — no Rust code. Validates that E1/E2 infrastructure handles N systems.

**Content design per system:**

| System | Zones | Resource Class | Rationale |
|--------|-------|---------------|-----------|
| Barnard's Star | 2 (inner belt, outer cloud) | VolatileRich | Cold M-dwarf, cometary material |
| Wolf 359 | 1 (debris field) | MetalRich | Compact system, dense metallic asteroids |
| Lalande 21185 | 2 (inner zone, outer belt) | Mixed | Moderate star, varied resources |
| Sirius | 2 (inner zone, debris ring) | MetalRich | Hot A-star, refractory-rich |

### Phase F: Warp Drive Mechanics (4 tickets)

#### F1. WarpRouteDef type + content loading

**New content file `content/warp_routes.json`:**

```json
{
  "routes": [
    { "from": "sun", "to": "alpha_centauri_a", "warp_ticks": 500, "bidirectional": true },
    { "from": "sun", "to": "barnards_star", "warp_ticks": 680, "bidirectional": true },
    { "from": "sun", "to": "wolf_359", "warp_ticks": 900, "bidirectional": true },
    { "from": "sun", "to": "lalande_21185", "warp_ticks": 950, "bidirectional": true },
    { "from": "sun", "to": "sirius_a", "warp_ticks": 980, "bidirectional": true },
    { "from": "alpha_centauri_a", "to": "barnards_star", "warp_ticks": 450, "bidirectional": true }
  ],
  "default_ticks_per_ly": 115
}
```

**New types in `sim_core/src/types/content.rs`:**

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WarpRouteDef {
    pub from: SystemId,
    pub to: SystemId,
    pub warp_ticks: u64,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WarpRoutesDef {
    pub routes: Vec<WarpRouteDef>,
    pub default_ticks_per_ly: u64,
}
```

**Stored on `GameContent`:**

```rust
pub struct GameContent {
    // ... existing fields ...
    pub warp_routes: WarpRoutesDef,
}
```

**Lookup helper:**

```rust
impl GameContent {
    pub fn warp_ticks(&self, from: &SystemId, to: &SystemId) -> u64 {
        // Check explicit routes (both directions if bidirectional)
        // Fall back to default_ticks_per_ly * distance_ly
        // where distance_ly = absolute_distance_au / 63241
    }
}
```

**Validation:**
- `from` and `to` are valid root body IDs (star systems)
- `from != to`
- `warp_ticks > 0`
- No duplicate routes (same from/to pair)

#### F2. TaskKind::Warp + Command::InitiateWarp

**New TaskKind variant in `sim_core/src/types/state.rs`:**

```rust
pub enum TaskKind {
    // ... existing variants ...
    Warp {
        origin_system: SystemId,
        destination_system: SystemId,
        destination_body: BodyId,
        total_ticks: u64,
        elapsed_ticks: u64,
    },
}
```

**New Command variant:**

```rust
pub enum Command {
    // ... existing variants ...
    InitiateWarp {
        ship_id: ShipId,
        destination_system: SystemId,
    },
}
```

**Tick behavior (in `resolve_ship_tasks()`):**
1. If ship task is `Warp`, increment `elapsed_ticks`
2. If `elapsed_ticks >= total_ticks`:
   - Set ship `position` to a default position near the destination system's root star (e.g., radius 10,000 micro-AU from the star)
   - Set task to `Idle`
   - Emit `WarpArrived { ship_id, destination_system, destination_body }`
3. Ship in `Warp` is otherwise inert — no mining, no deposit, no refuel, no commands except `Warp`-compatible ones (none initially)

**Command processing (in `apply_commands()`):**
1. Validate ship exists and is `Idle`
2. Validate ship has `module_warp_drive` fitted
3. Validate destination is a different system than ship's current system
4. Validate warp route exists (explicit or fallback)
5. Look up `warp_ticks` from content
6. Set ship task to `Warp { origin, destination, destination_body: root_star, total_ticks, elapsed_ticks: 0 }`
7. Emit `WarpInitiated { ship_id, origin_system, destination_system, warp_ticks }`

**New events:**
- `WarpInitiated { ship_id, origin_system, destination_system, warp_ticks }`
- `WarpArrived { ship_id, destination_system, destination_body }`

**Event sync:** Both events must be handled in `ui_web/src/hooks/applyEvents.ts` (or added to the allow-list in `scripts/ci_event_sync.sh`).

#### F3. Warp drive module definition

**New module in `content/module_defs.json`:**

```json
{
  "id": "module_warp_drive",
  "name": "Warp Drive",
  "behavior": { "type": "WarpDrive" },
  "power_per_tick": 500,
  "mass_kg": 2000,
  "volume_m3": 10.0,
  "required_tech": "tech_warp_drive",
  "slot_type": "utility",
  "wear_per_run": 0.0
}
```

**Module behavior is passive** — it provides a capability flag, not an active tick behavior. The `WarpDrive` behavior type has no tick logic; its presence on a ship is what enables the `InitiateWarp` command. This follows the pattern of other passive modules (cargo expander, propellant tank).

**New tech in `content/techs.json`:**

```json
{
  "id": "tech_warp_drive",
  "name": "Warp Field Theory",
  "description": "Enables construction and operation of warp drive modules",
  "domain_requirements": {
    "Propulsion": 500,
    "Materials": 200
  },
  "prerequisites": ["tech_advanced_propulsion"],
  "effects": []
}
```

**Note:** For the dev base state, the tech is pre-unlocked and the module is pre-fitted, so the research requirement doesn't block testing.

#### F4. Dev base state explorer ship + validation

**Add to `content/dev_advanced_state.json`:**

```json
{
  "id": "ship_explorer",
  "position": {
    "parent_body": "earth_orbit_zone",
    "radius_au_um": 3000,
    "angle_mdeg": 90000
  },
  "owner": "principal_autopilot",
  "inventory": [],
  "cargo_capacity_m3": 30.0,
  "task": null,
  "hull_id": "hull_explorer",
  "fitted_modules": [
    { "slot_index": 0, "module_def_id": "module_warp_drive" },
    { "slot_index": 1, "module_def_id": "module_propellant_tank" },
    { "slot_index": 2, "module_def_id": "module_mining_laser" }
  ],
  "propellant_kg": 15000.0,
  "propellant_capacity_kg": 15000.0,
  "speed_ticks_per_au": 2133
}
```

**Pre-unlock `tech_warp_drive`** in the dev base state's `research.unlocked` list.

**Add `hull_explorer` to hull definitions** (content/hull_defs.json) with 3 utility slots, moderate cargo, standard speed.

**Validation tests:**
- Ship with warp drive can `InitiateWarp` to Alpha Centauri
- Ship without warp drive gets `InitiateWarp` rejected
- Warp completes after `warp_ticks` ticks
- Ship position updates to destination system on arrival
- Ship is `Idle` after warp arrival
- Determinism canary passes with warp in progress
- Save/load round-trip preserves warp state mid-transit

### Phase G: System-Scoped AI (4 tickets)

#### G1. System-scoped candidate searches

Modify all agent candidate collection functions to filter by system:

**In `station_agent.rs`:**

```rust
fn collect_mine_candidates(
    state: &GameState,
    content: &GameContent,
    station_system: &SystemId,  // NEW
    // ... existing params ...
) -> Vec<(AsteroidId, /* scoring fields */)> {
    state.asteroids.iter()
        .filter(|(_, ast)| content.system_of(&ast.position.parent_body) == station_system)
        // ... existing scoring logic ...
}
```

Same pattern for `collect_survey_candidates`, `collect_deep_scan_candidates`.

**Station agent gains `system_id` field:**

```rust
pub(crate) struct StationAgent {
    station_id: StationId,
    system_id: SystemId,  // NEW — set during lifecycle sync
    lab_cache: LabAssignmentCache,
}
```

Set during `sync_station_agents()` from `content.system_of(&station.position.parent_body)`.

**Ship assignment also scoped:** `assign_ship_objectives()` only considers ships in the same system (already filtered by `home_station` from Phase D, but add system check as safety).

**Tests:**
- Station in Sol only gets Sol asteroids as mine candidates
- Station in Alpha Centauri only gets AC asteroids
- Ship in Sol is never assigned to AC asteroid
- Global candidate search with system filter returns correct subset

#### G2. Per-system scan site replenishment

Modify `replenish_scan_sites()` in `engine.rs`:

```rust
fn replenish_scan_sites(state: &mut GameState, content: &GameContent, rng: &mut impl Rng) {
    // Group zone bodies by system
    let zones_by_system: BTreeMap<SystemId, Vec<&OrbitalBodyDef>> = content
        .solar_system.bodies.iter()
        .filter(|b| b.zone.is_some())
        .map(|b| (content.system_of(&b.id).clone(), b))
        .into_group_map_btree();

    // Count scan sites per system
    let sites_by_system: BTreeMap<SystemId, usize> = state.scan_sites.iter()
        .map(|s| content.system_of(&s.position.parent_body).clone())
        .counts_btree();

    // Replenish each system independently
    for (system_id, zones) in &zones_by_system {
        let current = sites_by_system.get(system_id).copied().unwrap_or(0);
        if current < content.constants.replenish_target_count as usize {
            let deficit = /* ... */;
            // pick_zone_weighted only from THIS system's zones
            // ... existing replenishment logic, scoped to system zones ...
        }
    }
}
```

**RNG determinism:** Systems processed in `BTreeMap` order (sorted by `SystemId`). Within each system, zone selection uses the shared `rng` in deterministic order. This is the same pattern as the existing sorted-collection-before-RNG rule.

**Tests:**
- Two systems each get replenishment independently
- Sol doesn't starve Alpha Centauri of scan sites
- Determinism canary passes with per-system replenishment

#### G3. Strategic layer cross-system objectives (extends Phase C)

**New `StrategyMode` variant or objective type:**

```rust
pub enum CrossSystemObjective {
    /// Send an explorer ship to scout a system
    ScoutSystem { target_system: SystemId },
    /// Establish a station in a system (requires ship carrying station kit)
    Colonize { target_system: SystemId },
}
```

**Integration with Phase C strategic layer:**
- `StrategyConfig` gains `cross_system_objectives: Vec<CrossSystemObjective>` (with `#[serde(default)]`)
- The strategic layer evaluates cross-system objectives and assigns them to warp-capable ships
- Ship agents gain a `Warp` objective type alongside existing `Mine`, `Survey`, `DeepScan`, etc.
- After arrival in the target system, the ship transitions to local objectives (survey if scouting, build if colonizing)

**This ticket is deliberately minimal.** It adds the type infrastructure and basic scout behavior. Full cross-system logistics (supply chains, resource transfers) is a separate future project.

**Tests:**
- Strategic layer with `ScoutSystem` objective selects a warp-capable idle ship
- Ship initiates warp to target system
- After arrival, ship transitions to survey objectives in new system
- Non-warp-capable ships are never assigned cross-system objectives

#### G4. Cross-system asteroid deduplication extension

Extend Phase D's claim map (VIO-488) to work across systems:

The existing `BTreeMap<AsteroidId, StationId>` claim map in `AutopilotController` already prevents double-assignment. With system-scoped candidate searches (G1), stations in different systems naturally see different asteroid pools — no cross-system deduplication needed.

However, if two stations exist in the SAME system, the existing claim map handles it. This ticket validates that the claim map works correctly when multiple systems exist and that system-scoped filtering integrates cleanly with the claim pre-pass.

**Tests:**
- Two stations in Sol share the claim map correctly (Phase D behavior preserved)
- Station in Sol and station in Alpha Centauri have independent candidate pools
- Claim map doesn't contain entries from other systems' asteroids

### Phase H: Multi-System UI (3 tickets)

**Note:** This phase coordinates with the solar map redesign (docs/plans/2026-03-22-solar-map-redesign.md, Phase 5). UI work should follow the existing redesign architecture.

#### H1. Interstellar zoom and system collapse

- At zoom levels > ~100 AU, individual system bodies collapse into a single system marker (star icon + label)
- At interstellar zoom, show distance lines between systems with light-year labels
- System markers show discovered state only (DESIGN_SPINE: discovery-driven rendering)
- Camera lerp uses log-space interpolation at interstellar scale (existing pattern from solar map redesign)
- LOD tiers from existing design doc: Local (< 10 AU), System (10-1000 AU), Regional (1K-100K AU), Interstellar (> 100K AU)

#### H2. Warp visualization

- Ship in `Warp` task: hidden from origin system view
- Interstellar view: animated indicator along the route line, showing ship icon + progress percentage
- Warp initiated: brief visual pulse on origin system marker
- Warp arrived: brief visual pulse on destination system marker
- Fleet panel: warp ships show "Warping to [System] — [progress]%" status

#### H3. System-scoped panels and navigation

- QuickNav groups entities by system (collapsible sections)
- Station panel, fleet panel, etc. show system name in entity header
- Keyboard shortcut: number keys (1-6) at interstellar zoom = jump to star system
- Minimap adapts: at system zoom shows local system, at interstellar zoom shows all systems

## System-Wide Impact

### Interaction Graph

`GameContent::load()` builds `body_to_system` map alongside `body_cache` → all subsequent system-scoping queries use `content.system_of()`. `AutopilotController::generate_commands()` → `StationAgent::generate()` filters candidates by `self.system_id` → `ShipAgent::generate()` handles `TaskKind::Warp` tick advancement → `resolve_ship_tasks()` processes warp arrival → `replenish_scan_sites()` processes per-system → events emitted to SSE stream → UI filters by zoom level.

### Error & Failure Propagation

- Invalid `SystemId` in warp command → rejected at command validation, emits `InvalidCommand` event
- Ship warps to system with no station → ship becomes `Idle`, no panic. Agent has nothing to do (intentional — player must plan)
- Content validation catches orphaned bodies, duplicate IDs, invalid warp routes at load time

### State Lifecycle Risks

- `body_to_system` map is derived from content (static, never mutated during tick) — no stale-state risk
- `StationAgent.system_id` is set during lifecycle sync (runs every tick) — always fresh
- Ship position during warp: unchanged until arrival tick. No interpolation, no intermediate state
- Save/load: `TaskKind::Warp` serializes via serde like all other task kinds. `body_to_system` rebuilt from content on load (same as `body_cache`)

### API Surface Parity

- `POST /api/v1/command` — gains `InitiateWarp` command variant
- `GET /api/v1/spatial-config` — `body_absolutes` includes all systems' bodies
- `GET /api/v1/content` — includes warp route catalog
- New: `GET /api/v1/systems` — list of discovered star systems with basic info (name, distance, body count)
- SSE events: `WarpInitiated`, `WarpArrived` added to stream

### Integration Test Scenarios

1. **Warp transit full cycle:** Ship initiates warp → waits `warp_ticks` → arrives at destination → position updated → Idle in new system
2. **System-scoped mining:** Two stations in different systems → each only mines local asteroids
3. **Per-system replenishment:** Run 1000 ticks → both systems have proportional scan sites
4. **Save/load mid-warp:** Ship mid-warp → save → load → warp completes at correct tick
5. **Determinism with multi-system:** Same seed produces identical state across 4000 ticks with entities in 2 systems

## Acceptance Criteria

### Functional Requirements

- [ ] Multiple star systems loaded from content (Sol + Alpha Centauri at minimum)
- [ ] `SystemId` type and `body_to_system` map correctly identify which system each body belongs to
- [ ] `InitiateWarp` command initiates interstellar transit for warp-equipped ships
- [ ] Warp completes after content-defined duration, ship arrives at destination system
- [ ] Agent candidate searches (mine, survey, deep scan) scoped to station's local system
- [ ] Scan site replenishment operates per-system
- [ ] Warp-capable explorer ship in dev base state
- [ ] 5 star systems defined in content (Sol + 5 nearest)
- [ ] UI renders multiple systems at interstellar zoom with collapse/expand
- [ ] Warp progress visible in fleet panel and interstellar map view

### Non-Functional Requirements

- [ ] Determinism preserved: same seed produces identical state (determinism canary passes)
- [ ] No performance regression: sim_bench throughput within 5% of pre-multi-system
- [ ] Content-driven: adding a 7th star system is a JSON edit, not a code change
- [ ] All new types have `#[serde(default)]` for backward compatibility with existing saves

### Quality Gates

- [ ] Behavioral equivalence: single-system behavior unchanged (no regression from system scoping)
- [ ] `progression.rs` integration test passes
- [ ] sim_bench baseline regression passes
- [ ] Multi-system sim_bench scenario passes (2+ systems, 4000+ ticks)
- [ ] Determinism canary passes with multi-system state
- [ ] Event sync CI check passes (new events handled in applyEvents.ts)
- [ ] Test coverage maintained at 83%+

## Dependencies & Prerequisites

| Prerequisite | Status | Impact |
|-------------|--------|--------|
| Phase C: Strategic Layer (VIO-479 through VIO-484) | Backlog | Required for G3 (cross-system objectives). E/F/G1-G2 can start without it. |
| Phase D: Multi-Station (VIO-485 through VIO-489) | Backlog | Required for multi-station-per-system. E/F can start without it. |
| Hull+Slot System | Completed | Warp drive module uses existing hull slot system |
| Solar Map Redesign (Phase 5) | Partial | H1-H3 coordinate with this. Map canvas already supports multi-root bodies. |
| Float Determinism Audit (VIO-413) | Backlog | `polar_to_cart` at galactic scale uses f64. Cross-platform risk documented but not blocking. |

**Dependency graph:**

```
Phase C (Strategic Layer) ─────────────────────────┐
Phase D (Multi-Station) ────────────────────────────┤
                                                    │
E1 (SystemId + map + validation) ───────────────────┤
  │                                                 │
  ├─── E2 (Alpha Centauri content)                  │
  │      │                                          │
  │      └─── E3 (Remaining 4 systems content)      │
  │                                                 │
  ├─── F1 (WarpRouteDef + content) ─────────────────┤
  │      │                                          │
  │      ├─── F2 (TaskKind::Warp + Command)         │
  │      │      │                                   │
  │      │      ├─── F3 (Warp drive module def)     │
  │      │      │                                   │
  │      │      └─── F4 (Dev state + validation)    │
  │      │                                          │
  │      └──────────────────────────────────────────┤
  │                                                 │
  ├─── G1 (System-scoped candidates) ── needs D ────┤
  │      │                                          │
  │      ├─── G2 (Per-system replenishment)         │
  │      │                                          │
  │      ├─── G3 (Cross-system objectives) ── needs C
  │      │                                          │
  │      └─── G4 (Deduplication extension) ── needs D
  │                                                 │
  └─── H1 (Interstellar zoom) ─── needs E2 ────────┤
         │                                          │
         ├─── H2 (Warp visualization) ── needs F2   │
         │                                          │
         └─── H3 (System-scoped panels)             │
```

**Critical path:** E1 → E2 → F1 → F2 → F4 → G1 → G2

## Risk Analysis & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Determinism break from galactic-scale coordinates | Low | High | `polar_to_cart` tested at 276B micro-AU. Determinism canary extended for multi-system. body_cache rebuilt identically from content. |
| Performance regression from system-scoped filtering | Low | Medium | Filtering is O(n) where n = entities. body_to_system is a BTreeMap lookup. Benchmark with sim_bench. |
| Agent proximity-blind selection (ships sent to wrong system) | Medium | High | System scoping prevents this. Integration tests verify no cross-system assignment. (See past bug: docs/solutions/patterns/multi-epic-project-execution.md) |
| Content authoring errors (duplicate body IDs across systems) | Medium | Medium | Validation at load time panics with descriptive error. |
| Warp balance (too cheap/expensive to warp) | Medium | Low | Content-driven duration. Tunable via warp_routes.json without code changes. sim_bench scenario for multi-system balance. |
| Save compatibility with pre-multi-system saves | Low | Medium | All new fields use `#[serde(default)]`. `TaskKind::Warp` variant won't exist in old saves. body_to_system rebuilt from content. |

## Future Considerations (Not Planned Here)

### Multi-Threaded / Distributed Execution

The multi-system architecture creates a natural partitioning boundary for future parallelism. Key design choices that enable this:

1. **Per-system state is independent within a tick.** After system-scoped agent decisions (Phase G), stations and ships within a system only interact with local-system state. No cross-system state mutation during tick steps 1-6.

2. **Cross-system state is limited.** Only `ResearchState` (global), `GameState.balance` (global), and warp arrivals cross system boundaries. These are read-only during tick or batched for end-of-tick application.

3. **Per-system RNG streams are derivable.** `system_rng = ChaCha8Rng::seed_from_u64(master_seed ^ hash(system_id))`. Each system gets a deterministic RNG stream independent of other systems' tick order.

4. **The agent hierarchy supports system-level agents.** The trait-based agent architecture (Phase A+B) allows inserting a `SystemAgent` layer between strategic and station layers. The system agent would coordinate stations within its system, receiving objectives from the strategic layer.

**Architecture sketch for distributed execution:**

```
Centralized Controller (orchestrator)
  ├── Strategic Layer (global decisions, cross-system objectives)
  ├── Research + Economy sync (end-of-tick merge)
  │
  ├── Sol Worker (Docker container / thread)
  │     └── System tick: stations, ships, modules, scan sites
  │
  ├── Alpha Centauri Worker
  │     └── System tick: stations, ships, modules, scan sites
  │
  └── Warp Transit Manager
        └── Tracks in-flight warp ships, delivers arrivals to destination worker
```

**What NOT to build now (but don't preclude):**
- Don't make `GameState` a single monolith with per-system accessors. Keep flat collections (BTreeMap by entity ID) — the system-scoping filter is cheap and partition-later-friendly.
- Don't create a `SystemState` wrapper type yet. It would require refactoring every function that takes `&GameState`. The `system_of()` helper provides the scoping without structural changes.
- Don't split RNG streams yet. Single RNG with deterministic iteration order works for now and is simpler to debug.
- DO ensure all new code that iterates entities can be trivially filtered by system (the `system_of()` pattern).

### Additional Star Systems (Beyond 5)

Content-only expansion. Each new system needs: root star body in `solar_system.json`, zone bodies, warp route entries in `warp_routes.json`, and optionally unique asteroid templates. No code changes.

### System-Specific Resources

Unique elements or asteroid templates per system (e.g., Alpha Centauri has "Centaurite" ore). Currently deferred — all systems share the same element/template pool. Adding per-system resources is a content change once the template-to-zone mapping supports it.

### Inter-System Trade Routes

System-specific pricing, transport costs for importing between systems, warp freighter ships. Builds on the global economy (AD9) by adding per-route cost modifiers. Future project.

### Warp Fuel (Advanced Mechanic)

A separate "antimatter" or "warp fuel" resource consumed per warp jump. Creates a production chain: mine → process → fuel → warp. Adds strategic depth (fuel infrastructure at destination required for return trip). Builds on zero-cost warp (AD5) as a progression layer.

## Documentation Plan

- Update `docs/reference.md`: new types (SystemId, WarpRouteDef, TaskKind::Warp), new content files (warp_routes.json), new API endpoints
- Update `CLAUDE.md`: Architecture section (multi-system), tick order (warp task processing), content files list
- Update `.claude/skills/rust-sim-core.md`: system-scoping patterns, warp mechanics
- Update `docs/DESIGN_SPINE.md`: multi-system gameplay identity (if it changes core identity — likely just extends it)

## Sources & References

### Origin

- **Origin document:** [docs/brainstorms/hierarchical-agent-decomposition-requirements.md](docs/brainstorms/hierarchical-agent-decomposition-requirements.md) — Phase D (Multi-station + Intermediate Layers) directly enables multi-system. Key decision: "Station agents work independently at different orbital bodies with different resource profiles."

### Internal References

- Hierarchical agent plan (Phases A+B): `docs/plans/2026-03-28-002-refactor-hierarchical-agent-decomposition-plan.md`
- Strategic Layer project (Phases C+D): [Linear project](https://linear.app/violetspacecadet/project/strategic-layer-multi-station-ai-5867915277ee)
- Solar map redesign (multi-system architecture, Phase 5): `docs/plans/2026-03-22-solar-map-redesign.md:202-310`
- AI progression roadmap (Phase 5: Strategic Depth): `docs/plans/2026-03-23-code-quality-and-ai-progression-roadmap.md:434-443`
- Spatial model: `crates/sim_core/src/spatial.rs`
- Content loading: `crates/sim_world/src/lib.rs`
- Station agent: `crates/sim_control/src/agents/station_agent.rs`
- Ship agent: `crates/sim_control/src/agents/ship_agent.rs`
- Current solar system content: `content/solar_system.json`
- Dev base state: `content/dev_advanced_state.json`

### Learnings Applied

- Hierarchical polar coordinate migration pattern: `docs/solutions/patterns/hierarchical-polar-coordinate-migration.md`
- Deterministic integer arithmetic: `docs/solutions/logic-errors/deterministic-integer-arithmetic.md`
- Content-driven event engine pattern: `docs/solutions/patterns/content-driven-event-engine.md`
- Cross-layer feature development: `docs/solutions/patterns/cross-layer-feature-development.md`
- Proximity-blind selection bug: `docs/solutions/patterns/multi-epic-project-execution.md`
- Agent decomposition patterns: `docs/solutions/patterns/hierarchical-agent-decomposition.md`

## Ticket Breakdown

### Phase E: Star System Content Model

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| E1 | SystemId type + body_to_system map + validation | — | `SystemId` alias, `body_to_system: BTreeMap`, content validation |
| E2 | Alpha Centauri content + galactic positioning | E1 | Bodies, zones, scan sites for Alpha Centauri in solar_system.json |
| E3 | Remaining 4 star systems content | E1, E2 | Barnard's Star, Wolf 359, Lalande 21185, Sirius content |

### Phase F: Warp Drive Mechanics

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| F1 | WarpRouteDef type + content loading | E1 | `warp_routes.json`, `WarpRoutesDef` type, lookup helper |
| F2 | TaskKind::Warp + Command::InitiateWarp | F1 | Warp task processing in tick, command validation, events |
| F3 | Warp drive module definition | F2 | `module_warp_drive` in module_defs.json, `tech_warp_drive` in techs.json |
| F4 | Dev base state explorer ship + validation | F2, F3 | Explorer ship with warp drive, integration tests |

### Phase G: System-Scoped AI

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| G1 | System-scoped candidate searches | E1, Phase D | Filter mine/survey/deepscan candidates by system |
| G2 | Per-system scan site replenishment | E1 | Group zones by system, replenish independently |
| G3 | Strategic layer cross-system objectives | Phase C, F2 | `CrossSystemObjective` type, scout behavior |
| G4 | Cross-system asteroid deduplication extension | Phase D, G1 | Validate claim map with multi-system |

### Phase H: Multi-System UI

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| H1 | Interstellar zoom and system collapse | E2 | LOD tiers, system markers, distance lines |
| H2 | Warp visualization | F2, H1 | Warp progress indicator, arrival/departure effects |
| H3 | System-scoped panels and navigation | E2 | QuickNav grouping, system labels, keyboard shortcuts |

### Total: 14 tickets across 4 phases
