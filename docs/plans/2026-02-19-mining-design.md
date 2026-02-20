# Mining Feature Design

## Goal

Add asteroid mining to the simulation: ships autonomously mine asteroids, haul ore back to a station, and deposit it into the station's inventory. Physical realism (mass, volume, element density) is modelled from the start to support future fuel-cost mechanics.

## Architecture

Cargo is tracked in **kg per element** with a **volume (m³) capacity constraint**. Volume used = Σ(cargo[element] / density[element]). This keeps mass as a first-class value for the future fuel-weight feature while correctly modelling that dense materials (iron) take less space than lighter ones (helium dominates volume even at small mass fractions).

The autopilot extends its existing survey → deep-scan pipeline with a third looping phase: mine → transit to station → deposit → mine next target.

---

## Section 1: Data Model

### Asteroid mass

- `AsteroidState` gains `mass_kg: f32` — assigned at survey time from a uniform random range.
- `Constants` gains `asteroid_mass_min_kg: f32` (100.0) and `asteroid_mass_max_kg: f32` (100_000.0).
- `mass_kg` is the **remaining** mass; it decreases as the asteroid is mined.
- Asteroids with `mass_kg == 0` are removed from game state.

### Element definitions

- New `content/elements.json` — physical properties per element:
  ```json
  { "elements": [
    { "id": "Fe", "density_kg_per_m3": 7874.0, "display_name": "Iron" },
    { "id": "Si", "density_kg_per_m3": 2329.0, "display_name": "Silicon" },
    { "id": "He", "density_kg_per_m3": 0.164,  "display_name": "Helium" }
  ]}
  ```
- New `ElementDef { id: String, density_kg_per_m3: f32, display_name: String }` struct loaded into `GameContent`.
- Element density is used for all cargo volume calculations throughout the simulation.

### Ship cargo hold

- `ShipState` gains:
  - `cargo: HashMap<ElementId, f32>` — mass in kg per element
  - `cargo_capacity_m3: f32` — maximum hold volume
- Volume in use = Σ(cargo[el] / density[el]) across all elements
- `Constants` gains `ship_cargo_capacity_m3: f32` (default 20.0)

### Station inventory

- `StationState` gains:
  - `cargo: HashMap<ElementId, f32>` — mass in kg per element
  - `cargo_capacity_m3: f32` — maximum storage volume
- `Constants` gains `station_cargo_capacity_m3: f32` (default 10_000.0)

---

## Section 2: Tasks

### Mine task

- `TaskKind::Mine { asteroid: AsteroidId }`
- Duration computed at assignment using mixed-composition volume rate:
  ```
  effective_m3_per_kg = Σ (fraction[el] / density[el])   // over all elements
  ticks_to_fill_hold  = free_volume_m3 / (mining_rate_kg_per_tick × effective_m3_per_kg)
  ticks_to_deplete    = asteroid.mass_kg / mining_rate_kg_per_tick
  duration            = min(ticks_to_fill_hold, ticks_to_deplete)
  ```
- `Constants` gains `mining_rate_kg_per_tick: f32` (default 50.0 — filling a 20 m³ hold of pure iron ≈ 2.2 hours game time; a He-bearing asteroid fills much faster)
- At resolution:
  - Extract `mining_rate_kg_per_tick × duration × fraction[el]` kg per element
  - Add extracted kg to ship cargo
  - Subtract total extracted kg from `asteroid.mass_kg`
  - Remove asteroid from state if `mass_kg == 0`
- Emits `OreMined { asteroid_id, extracted: HashMap<ElementId, f32>, asteroid_remaining_kg: f32 }`

### Deposit task

- `TaskKind::Deposit { station: StationId }`
- Ship must already be at the station node (autopilot ensures this via Transit)
- Fixed duration: `Constants.deposit_ticks` (default 60 — 1 hour game time)
- At resolution: move all ship cargo into station inventory (capped at station remaining capacity)
- Emits `OreDeposited { station_id: StationId, deposited: HashMap<ElementId, f32> }`

---

## Section 3: Autopilot Mining Loop

Priority order for idle ships (after existing survey/deep-scan phases):

1. **Ship has cargo** → Transit to nearest station (fewest hops), then Deposit
2. **Ship has no cargo, deep-scanned asteroids with mass remaining exist** → Transit to best target, then Mine
3. **No targets yet** → wait for survey/deep-scan to complete

**"Best mining target"** = highest `mass_kg × known_Fe_fraction` (valuable mass remaining), break ties by hop count from ship's current node. Uses `knowledge.composition` (set by deep scan) — only deep-scanned asteroids are mined.

The loop is fully autonomous and perpetual. After depositing, the autopilot immediately assigns the next mine run on the same tick.

---

## Section 4: Frontend

### Layout fix (prerequisite)

The existing three-panel layout clips content with no way to reveal it. Fix before adding columns:
- **Resizable panel widths** — drag handles between panels
- **Horizontal scroll within table panels** — columns never clip silently

### Mining data additions

- **Asteroid table**: add `mass_kg` column (e.g. "12,450 kg"); dim/remove fully depleted asteroids
- **Ship panel** (new or expanded): cargo bar showing `X.X / 20.0 m³` used, breakdown by element
- **Station panel**: inventory table (kg per element), total volume used vs capacity
- **Events feed**: `OreMined` and `OreDeposited` events flow through existing feed

---

## Phased Implementation Steps

1. **Asteroid mass** — add `mass_kg` to `AsteroidState`, assigned at survey time from constants range
2. **Element definitions** — new `elements.json`, `ElementDef` struct, load into `GameContent`
3. **Ship & station cargo holds** — add cargo fields to `ShipState` and `StationState`, wire constants
4. **Mine task** — `TaskKind::Mine`, engine resolution, asteroid depletion, `OreMined` event
5. **Deposit task** — `TaskKind::Deposit`, station inventory fill, `OreDeposited` event
6. **Autopilot mining loop** — extend autopilot to chain Mine → Transit → Deposit → repeat
7. **FE: resizable panels + table scroll** — fix layout before adding columns
8. **FE: mining data** — asteroid mass column, ship cargo bar, station inventory panel
