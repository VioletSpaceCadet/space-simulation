# Spatial Positioning System Design

**Date:** 2026-02-26
**Status:** Approved
**Scope:** Replace node-edge graph with hierarchical polar coordinate system

## Overview

Replace the current 4-node linear graph (Earth Orbit → Inner Belt → Mid Belt → Outer Belt) with a hierarchical polar coordinate system. Every entity gets a real position in 2D space. Travel time is distance-based. The FE map renders actual orbital positions with pan/zoom.

**Explicit constraint:** This model is a 2D ecliptic plane. No orbital inclinations. Documented here so future work doesn't assume 3D was promised.

## 1. Data Model

### Fixed-Point Types

All spatial values use integer fixed-point for determinism. No IEEE 754 in the hot path.

```rust
/// Micro-AU. 1 AU = 1_000_000 µAU. u64 range = ~18.4T AU.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct RadiusAuMicro(u64);

/// Milli-degrees. 360° = 360_000 m°. u32 wraps naturally.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct AngleMilliDeg(u32);

/// Sun-centered cartesian, signed micro-AU. Precomputed cache.
#[derive(Clone, Copy, PartialEq, Eq)]
struct AbsolutePos {
    x_au_um: i64,
    y_au_um: i64,
}
```

Strong newtypes everywhere — never raw `u32`/`u64` for angles or distances.

### Unit Conversion Constant

```rust
/// 1 AU = 149,597,870,700 meters (IAU 2012 exact definition)
const METERS_PER_AU: u64 = 149_597_870_700;

/// 1 micro-AU ≈ 149.6 km
/// Defined now for future thrust/fuel calculations.
const METERS_PER_MICRO_AU: f64 = 149_597.870_7;
```

### Canonical Angle Operations

Methods on `AngleMilliDeg`:

```rust
const FULL_CIRCLE: u32 = 360_000;
const HALF_CIRCLE: u32 = 180_000;

impl AngleMilliDeg {
    /// Wrapping addition mod 360_000
    fn add(self, other: AngleMilliDeg) -> AngleMilliDeg {
        AngleMilliDeg((self.0 + other.0) % FULL_CIRCLE)
    }

    /// Smallest signed delta in [-180_000, +180_000]
    fn signed_delta(self, other: AngleMilliDeg) -> i32 {
        let raw = other.0 as i32 - self.0 as i32;
        if raw > HALF_CIRCLE as i32 { raw - FULL_CIRCLE as i32 }
        else if raw < -(HALF_CIRCLE as i32) { raw + FULL_CIRCLE as i32 }
        else { raw }
    }

    /// Is this angle within a (start, span) zone? Handles wrap-around.
    /// Rule: offset = (angle - start + 360_000) % 360_000; contained = offset < span
    fn within_span(self, start: AngleMilliDeg, span: AngleMilliDeg) -> bool {
        let offset = (self.0 + FULL_CIRCLE - start.0) % FULL_CIRCLE;
        offset < span.0
    }
}
```

FE must mirror these exactly (see Section 4).

### Orbital Bodies (Content-Defined Tree)

The solar system is a tree of orbital bodies. Each body has a position relative to its parent. Stations, ships, asteroids are NOT orbital bodies — they are entities with positions that reference a body.

```rust
struct OrbitalBodyDef {
    id: BodyId,
    name: String,
    parent: Option<BodyId>,           // None = Sun (root)
    body_type: BodyType,              // Star, Planet, Moon, Belt, Zone
    radius_au_um: RadiusAuMicro,      // Distance from parent
    angle_mdeg: AngleMilliDeg,        // Starting angle around parent
    solar_intensity: f32,             // For solar arrays (display-only, ok as float)
    zone: Option<ZoneDef>,            // If this body is a spawnable zone
}

struct ZoneDef {
    radius_min_au_um: RadiusAuMicro,
    radius_max_au_um: RadiusAuMicro,
    angle_start_mdeg: AngleMilliDeg,  // Default 0
    angle_span_mdeg: AngleMilliDeg,   // Default 360_000 (full ring)
    resource_class: ResourceClass,
    scan_site_weight: u32,            // Integer weight. 0 = never spawn.
}

enum BodyType { Star, Planet, Moon, Belt, Zone }
enum ResourceClass { MetalRich, Mixed, VolatileRich }
```

**Rendering rule:** Bodies render based on `zone.is_some()`, not `body_type`. Zone present → ring/arc. Zone absent → point/icon. A `Belt` with a zone and a `Zone` with a zone render identically.

### Entity Positioning

Every entity gains a `Position` replacing `location_node: NodeId`:

```rust
struct Position {
    parent_body: BodyId,          // Can be any body, including a Zone
    radius_au_um: RadiusAuMicro,  // Distance from parent body
    angle_mdeg: AngleMilliDeg,    // Angle around parent body
}
```

### Epoch-Versioned Absolute Position Cache

Avoids walking the tree on every distance calculation. No per-body dependent entity lists.

```rust
/// On each orbital body (side table, not serialized)
struct BodyCache {
    absolute: AbsolutePos,
    epoch: u32,                   // Incremented when body's absolute changes
}

/// On each entity (not serialized)
struct EntityCache {
    absolute: AbsolutePos,
    cached_parent_epoch: u32,     // Compared to parent body's epoch
}
```

**Cache protocol:**
1. At startup (and per-tick when orbits exist later): walk tree root→leaves, compute each body's `absolute`, increment `epoch` if changed.
2. On entity access: compare `entity.cached_parent_epoch` with `body_cache[parent].epoch`. If stale, recompute `absolute = body.absolute + polar_to_cart(entity.radius, entity.angle)`, store new epoch.
3. Distance between two entities = integer math on cached absolute coords.

### Example Solar System Content

```
Sun (root)
├── Earth         (r=1,000,000 µAU, Planet)
│   ├── Luna      (r=2,570 µAU from Earth, Moon)
│   ├── Earth Orbit Zone (r=500 µAU, zone: 1–50 µAU band)
│   └── Earth NEOs (r=15,000 µAU, zone: 5,000–30,000 µAU band)
├── Mars          (r=1,524,000 µAU, Planet)
│   └── Mars NEOs (zone)
├── Inner Belt    (r=2,350,000 µAU, Belt, zone: 2,100,000–2,800,000 µAU)
├── Outer Belt    (r=3,050,000 µAU, Belt, zone: 2,800,000–3,300,000 µAU)
└── Jupiter       (r=5,203,000 µAU, Planet)
```

Adding moons, trojans, or outer planets is purely content — no code changes.

### Migration

| Current `NodeId` | New `parent_body` | Position |
|---|---|---|
| `node_earth_orbit` | `EarthOrbitZone` | Random within zone band |
| `node_belt_inner` | `InnerBeltZone` | Random within zone band |
| `node_belt_mid` | `MidBeltZone` | Random within zone band |
| `node_belt_outer` | `OuterBeltZone` | Random within zone band |

## 2. Travel Calculation

### Distance Functions

Squared distance for comparisons (no sqrt). Actual distance only for final travel time.

```rust
/// Use for comparisons, nearest-entity selection, range checks.
fn distance_squared_au_um2(a: AbsolutePos, b: AbsolutePos) -> u128 {
    let dx = (a.x_au_um - b.x_au_um) as i128;
    let dy = (a.y_au_um - b.y_au_um) as i128;
    (dx * dx + dy * dy) as u128
}

/// Use ONLY for final travel time calculation. Single sqrt per travel command.
fn distance_au_um(a: AbsolutePos, b: AbsolutePos) -> u64 {
    integer_sqrt(distance_squared_au_um2(a, b)) as u64
}
```

**Usage rule:** Autopilot selecting nearest asteroid/station uses `distance_squared`. Only the final "issue transit command" path calls `distance_au_um`. Sqrt is called once per travel command, never per candidate.

### Travel Time

Linear scaling. Two new constants replace `travel_ticks_per_hop`:

```rust
/// constants.json
ticks_per_au: u64,         // e.g., 2133 (calibrated: Earth→Inner Belt ≈ 2880 ticks)
min_transit_ticks: u64,    // e.g., 1 — floor for very short trips

fn travel_ticks(from: AbsolutePos, to: AbsolutePos, constants: &Constants) -> u64 {
    let dist = distance_au_um(from, to);  // single sqrt
    let ticks = dist * constants.ticks_per_au / 1_000_000;
    ticks.max(constants.min_transit_ticks)
}
```

### Transit Task

```rust
TaskKind::Transit {
    origin_pos: Position,
    destination_pos: Position,
    origin_abs: AbsolutePos,          // Precomputed at command time
    destination_abs: AbsolutePos,     // Precomputed at command time
    total_ticks: u64,
    then: Box<TaskKind>,
}
```

**Travel time is locked at departure.** Distance computed once when command issued, `total_ticks` set, never changes mid-flight. `origin_abs` and `destination_abs` cached for FE interpolation.

### Ship Position During Transit

Ship keeps `origin_pos` in `ShipState.position` until arrival. On completion, position set to `destination_pos`. FE interpolates visually using task's absolute positions and progress fraction.

### What Gets Removed

- `graph.rs` (BFS pathfinding) — deleted
- `SolarSystemDef.edges` — removed
- `travel_ticks_per_hop` constant — replaced by `ticks_per_au` + `min_transit_ticks`
- `shortest_hop_count()` — gone
- `maybe_transit()` — simplified to distance calc

## 3. World Generation & Zone Spawning

### Zone-Weighted Site Distribution

Integer weights, no floats in the RNG path.

```rust
fn weighted_pick_zone<'a>(
    zones: &'a [(&OrbitalBodyDef, &ZoneDef)],
    total_weight: u32,
    rng: &mut impl Rng,
) -> &'a (&'a OrbitalBodyDef, &'a ZoneDef) {
    let roll = rng.gen_range(0..total_weight);
    let mut acc = 0u32;
    for entry in zones {
        acc += entry.1.scan_site_weight;
        if roll < acc { return entry; }
    }
    zones.last().unwrap()
}
```

### Area-Weighted Radius Sampling (Integer)

Naive `uniform(r_min, r_max)` clusters entities toward inner edge. Correct formula samples uniformly by area:

```rust
fn random_radius_in_band(
    r_min: RadiusAuMicro,
    r_max: RadiusAuMicro,
    rng: &mut impl Rng,
) -> RadiusAuMicro {
    let min2 = (r_min.0 as u128) * (r_min.0 as u128);
    let max2 = (r_max.0 as u128) * (r_max.0 as u128);
    let sample = rng.gen_range(min2..max2);
    RadiusAuMicro(integer_sqrt(sample) as u64)
}
```

All integer. Same `integer_sqrt` used for distance calc.

### Angle Generation (Wrap-Safe)

```rust
fn random_angle_in_span(
    start: AngleMilliDeg,
    span: AngleMilliDeg,
    rng: &mut impl Rng,
) -> AngleMilliDeg {
    let offset = rng.gen_range(0..span.0);
    AngleMilliDeg((start.0 + offset) % FULL_CIRCLE)
}
```

### Template Resource Class Bias

Each asteroid template gains `preferred_class: Option<ResourceClass>`. Spawning weight multiplier:

| Match | Multiplier |
|---|---|
| `preferred_class == zone.resource_class` | 3 |
| `preferred_class == None` | 2 |
| `preferred_class != zone.resource_class` | 1 |

Clustering without exclusivity — iron-rich templates appear in VolatileRich zones, just 3x less likely.

### Replenish Gating

Replace "every tick" with interval check:

```rust
/// constants.json
replenish_check_interval_ticks: u64,  // e.g., 24 (once per game-day at 1 tick/hr)
replenish_target_count: u32,

// In engine tick:
if state.meta.tick % constants.replenish_check_interval_ticks == 0 {
    let deficit = constants.replenish_target_count
        .saturating_sub(state.scan_sites.len() as u32);
    for _ in 0..deficit.min(REPLENISH_BATCH_SIZE) {
        // spawn using zone-weighted, area-sampled positioning
    }
}
```

### Station and Ship Placement

Stations placed with explicit positions in `build_initial_state`:

```rust
// Station in Earth Orbit Zone
// 3 µAU ≈ 450 km (LEO altitude). 1 µAU ≈ 149.6 km.
position: Position {
    parent_body: BodyId("earth_orbit_zone"),
    radius_au_um: RadiusAuMicro(3),
    angle_mdeg: AngleMilliDeg(0),
}
```

Ships start co-located with their home station (same position).

## 4. Frontend Map

### Data Pipeline

**Daemon → FE on connect (static until orbits exist):**

```typescript
interface SolarSystemConfig {
  bodies: OrbitalBodyDef[];
  body_absolutes: Record<BodyId, AbsolutePos>;
  ticks_per_au: number;
  min_transit_ticks: number;
}
```

**Entity state (streamed via SSE, gains position):**

```typescript
interface ShipState {
  position: Position;
  task: Task;
}

// Discriminated union — Transit abs fields are required, not optional
type Task =
  | { kind: "Idle" }
  | { kind: "Survey"; started_tick: number; eta_tick: number }
  | { kind: "DeepScan"; started_tick: number; eta_tick: number }
  | { kind: "Mine"; started_tick: number; eta_tick: number }
  | { kind: "Deposit"; started_tick: number; eta_tick: number }
  | { kind: "Transit"; started_tick: number; eta_tick: number;
      origin_abs: AbsolutePos; destination_abs: AbsolutePos };
```

### FE Angle Helpers (Must Mirror Rust Exactly)

```typescript
const FULL_CIRCLE_MDEG = 360_000;
const HALF_CIRCLE_MDEG = 180_000;

function mdegToRad(mdeg: number): number {
  return (mdeg / 1000) * (Math.PI / 180);
}

function addAngleMdeg(a: number, b: number): number {
  return ((a + b) % FULL_CIRCLE_MDEG + FULL_CIRCLE_MDEG) % FULL_CIRCLE_MDEG;
}

function signedDeltaMdeg(from: number, to: number): number {
  const raw = ((to - from) % FULL_CIRCLE_MDEG + FULL_CIRCLE_MDEG) % FULL_CIRCLE_MDEG;
  return raw > HALF_CIRCLE_MDEG ? raw - FULL_CIRCLE_MDEG : raw;
}

function withinSpanMdeg(angle: number, start: number, span: number): boolean {
  const offset = ((angle - start) % FULL_CIRCLE_MDEG + FULL_CIRCLE_MDEG) % FULL_CIRCLE_MDEG;
  return offset < span;
}
```

### Map Camera

```typescript
interface MapView {
  centerX_au_um: number;
  centerY_au_um: number;
  scale: number;           // Pixels per µAU
}

function auUmToScreen(pos: AbsolutePos, view: MapView): { x: number; y: number } {
  return {
    x: (pos.x_au_um - view.centerX_au_um) * view.scale + screenWidth / 2,
    y: (pos.y_au_um - view.centerY_au_um) * view.scale + screenHeight / 2,
  };
}
```

Pan shifts center. Zoom changes scale.

### Rendering Strategy: Hybrid Canvas + SVG

```
Canvas layer (bottom): Zone arcs, entity dots, transit trail lines
SVG overlay (top):     Selection highlight, hover tooltip anchor, labels at high zoom
```

Canvas handles volume (thousands of asteroids). SVG handles interaction (click targets, labels).

### Visibility Thresholds (Pixel-Radius Based)

| Element | Show when |
|---|---|
| Zone ring | `outerPx - innerPx >= 1` OR `outerPx >= 2` |
| Body icon | Always (fixed screen-size) |
| Entity dot | Entity count in viewport < 500 |
| Entity label | Zoom level makes dots separable (high zoom) |

**Density guard:** FE precomputes `Map<BodyId, EntityId[]>` from entity `parent_body` on each state update. If a zone's entity count in viewport > 500, don't render individual dots — increase zone fill opacity proportionally to entity count (density tint). Zooming in reduces count and individual dots reappear.

### Zone Rendering

Zone arcs colored by `resource_class`:
- MetalRich: warm amber (semi-transparent)
- Mixed: neutral gray
- VolatileRich: cool blue

Full-ring zones (span=360°) render as concentric circle bands. Partial-span zones render as arc segments. Wrap-around spans (e.g., 350°–20°) handled by the angle helpers above.

### Ship Transit Rendering

Linear interpolation between origin and destination absolute positions:

```typescript
function shipTransitPosition(ship: ShipState, displayTick: number): AbsolutePos {
  const task = ship.task;
  if (task.kind !== 'Transit') return entityAbsolute(ship.position);

  const total = task.eta_tick - task.started_tick;
  const elapsed = Math.min(displayTick - task.started_tick, total);
  const t = elapsed / total;

  return {
    x_au_um: task.origin_abs.x_au_um +
      (task.destination_abs.x_au_um - task.origin_abs.x_au_um) * t,
    y_au_um: task.origin_abs.y_au_um +
      (task.destination_abs.y_au_um - task.origin_abs.y_au_um) * t,
  };
}
```

Straight-line on the map. Curved paths deferred to gravity/propellant epic.

### Entity Visual Style

| Entity | Shape | Size | Color |
|---|---|---|---|
| Star | Circle (glow) | Fixed large | Yellow/white |
| Planet | Circle | Fixed medium | Per-planet palette |
| Moon | Circle | Fixed small | Gray |
| Station | Rotated square | Fixed | Teal |
| Ship (idle) | Triangle | Fixed small | White |
| Ship (transit) | Triangle + trail | Fixed small | Blue |
| Ship (mining) | Triangle + pulse | Fixed small | Orange |
| Asteroid | Circle | `log(mass_kg)` scaled | Amber/Blue by class |
| Scan site | Circle + "?" | Fixed tiny | Gray |

### Tooltip

Uses sim constants for consistency:

```typescript
function tooltipForEntity(entity, selected, config) {
  if (!selected) return { name: entity.name };

  const dx = entity.absolute.x_au_um - selected.absolute.x_au_um;
  const dy = entity.absolute.y_au_um - selected.absolute.y_au_um;
  const dist_au_um = Math.hypot(dx, dy);  // Numerically safe
  const dist_au = dist_au_um / 1_000_000;
  const est_ticks = Math.max(
    Math.round(dist_au * config.ticks_per_au),
    config.min_transit_ticks,
  );

  return { name: entity.name, distance_au: dist_au, est_travel_ticks: est_ticks };
}
```

### Interaction

- Click entity → select, show detail panel
- Click zone → filter entity list to that zone
- Hover entity → tooltip (name, distance from selected, estimated travel ticks)
- Double-click planet/zone → zoom to that reference frame

### Migration from Current Map

Delete `SolarSystemMap.tsx`, `layout.ts`, `ringRadiusForNode()`, `angleFromId()`. Positions come from real data. `useAnimatedTick` hook unchanged. Panel layout unchanged. SSE architecture unchanged.

## 5. Future Extensions (Designed For, Not Implemented)

### Orbital Motion

Add `orbital_period_ticks: Option<u64>` to `OrbitalBodyDef`. Per tick, body angle increments by `360_000 / orbital_period_ticks`. Cascades down tree via epoch versioning.

**Launch windows:** Matter because destination position changes between planning and departure — not because in-flight distance is recomputed (travel time locked at departure).

### Comets / Transient Objects

Entity with scripted trajectory: sequence of `(tick, Position)` waypoints. Sim interpolates between waypoints, updating comet position. Ships target comet's current position at departure. Gameplay: rare volatiles, time-limited access window.

### Elliptical Orbits

Add `eccentricity` and `semi_major_axis_au_um` to `OrbitalBodyDef`. Polar equation: `r = a(1-e²)/(1+e·cos(θ))`. Tree structure and caching unaffected.

### 3D / Inclinations

`Position` gains `inclination_mdeg`. `AbsolutePos` gains `z_au_um: i64`. Distance becomes 3D euclidean. **Not promised — noting extension path.**

### Multi-Star Systems

Second star is another body in the tree. No code changes, just content.
