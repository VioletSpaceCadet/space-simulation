# Movement Cost Model Specification

> Status: Design phase. No code changes yet.
> Companion docs: `energy_and_propellant_design.md`, `solar_system_abstraction.md`

## 1. Goals

Replace free travel with propellant-consuming movement that:
- Creates **logistics pressure** — ships need fuel, fuel needs infrastructure
- Is **deterministic and simple** — no orbital mechanics, no n-body physics
- Interacts with **ship mass** — heavier ships cost more fuel (the rocket equation tradeoff)
- Is **tunable** via content files — delta-v costs on edges, exhaust velocity on ship types
- Does NOT change the fundamental graph-based travel model

---

## 2. Current Movement Model

```rust
TaskKind::Transit {
    destination: NodeId,
    total_ticks: u64,  // hop_count * travel_ticks_per_hop (2880)
    then: Box<TaskKind>,
}
```

- Travel time is uniform per hop: 2,880 ticks (2 days)
- No propellant consumed
- Ship mass irrelevant
- All edges equal cost

---

## 3. Proposed Movement Model

### 3.1 Travel Time

Travel time becomes **variable per edge**, derived from the edge's `hop_dv`:

```
travel_ticks_for_edge = hop_dv * ticks_per_dv_unit
```

Where `ticks_per_dv_unit` is a constant (default: `1.0`). This means:
- Earth → Lunar (3,000 dv) = 3,000 ticks ≈ 2.1 days
- Lunar → Inner Belt (5,000 dv) = 5,000 ticks ≈ 3.5 days
- Inner → Mid Belt (2,000 dv) = 2,000 ticks ≈ 1.4 days
- Mid → Outer Belt (3,000 dv) = 3,000 ticks ≈ 2.1 days
- Mid → Trojan (6,000 dv) = 6,000 ticks ≈ 4.2 days

Multi-hop routes sum their edge times. Earth → Outer Belt = 3000 + 5000 + 2000 + 3000 = 13,000 ticks ≈ 9 days.

### 3.2 Propellant Consumption

**Simplified Tsiolkovsky rocket equation:**

For each hop, the ship consumes propellant:

```
mass_ratio = exp(hop_dv / exhaust_velocity)
propellant_needed = ship_total_mass * (1.0 - 1.0 / mass_ratio)
```

Where:
- `ship_total_mass = dry_mass + cargo_mass + propellant_mass` (at start of hop)
- `exhaust_velocity` is a property of the ship's engine type (default: 30,000 m/s for starter chemical engine — roughly LH2/LOX performance)
- `hop_dv` comes from the edge definition

**Example calculation:**

Ship with 5,000 kg dry mass, 150,000 kg ore cargo, 20,000 kg propellant. Hop of 3,000 dv, exhaust velocity 30,000 m/s:

```
total_mass = 5000 + 150000 + 20000 = 175,000 kg
mass_ratio = exp(3000 / 30000) = exp(0.1) ≈ 1.1052
propellant_needed = 175000 * (1 - 1/1.1052) = 175000 * 0.0952 ≈ 16,660 kg
```

That's 16.7 tonnes of LH2 for one hop. This creates real logistics pressure.

### 3.3 Multi-Hop Planning

For multi-hop routes, propellant must be computed **per-hop sequentially** because each hop reduces propellant mass, changing the total mass for subsequent hops.

```rust
fn compute_route_propellant(
    ship: &ShipState,
    route: &[EdgeDef],  // ordered hops
    exhaust_velocity: f32,
) -> Result<f32, InsufficientPropellant> {
    let mut remaining_propellant = ship.propellant_kg;
    let dry_mass = ship.dry_mass_kg;
    let cargo_mass = inventory_mass(ship);

    for edge in route {
        let total_mass = dry_mass + cargo_mass + remaining_propellant;
        let mass_ratio = (edge.hop_dv / exhaust_velocity).exp();
        let consumed = total_mass * (1.0 - 1.0 / mass_ratio);

        if consumed > remaining_propellant {
            return Err(InsufficientPropellant { shortfall: consumed - remaining_propellant });
        }
        remaining_propellant -= consumed;
    }

    Ok(ship.propellant_kg - remaining_propellant) // total consumed
}
```

### 3.4 Ship State Changes

```rust
pub struct ShipState {
    // ... existing fields ...
    pub dry_mass_kg: f32,             // New: empty ship mass
    pub propellant_kg: f32,           // New: current propellant
    pub propellant_capacity_kg: f32,  // New: max propellant
    pub exhaust_velocity: f32,        // New: engine performance (m/s)
}
```

**Starter ship values:**
- `dry_mass_kg`: 5,000 (light mining shuttle)
- `propellant_capacity_kg`: 50,000 (enough for ~3 inner-belt round trips empty, fewer when loaded)
- `exhaust_velocity`: 30,000 (LH2/LOX chemical engine)

### 3.5 Refueling

New `TaskKind::Refuel`:

```rust
TaskKind::Refuel {
    station_id: StationId,
    target_kg: f32,  // How much LH2 to load (up to capacity)
}
```

- Duration: proportional to amount loaded (e.g., 1 tick per 100 kg = 500 ticks for full tank)
- Consumes LH2 from station inventory
- Blocked if station has insufficient LH2 (similar to deposit blocking)

**Autopilot integration:**
- Before issuing a Transit command, autopilot checks if ship has enough propellant for the round trip (to target and back)
- If not, issues Refuel command first
- If station has no LH2, ship remains idle (new fleet_idle sub-reason: `NoFuel`)

---

## 4. Determinism Guarantees

- **Propellant consumed at transit start**, not over time. When Transit task begins, full propellant for all hops is deducted atomically. This avoids mid-flight floating-point drift.
- **Route computed at command issue time**, not during flight. If graph changes mid-flight (it won't, but defensively), the pre-computed cost stands.
- **`exp()` determinism**: Rust's `f32::exp()` is deterministic for same input on same platform. Cross-platform determinism is already a non-goal (the sim uses ChaCha8 RNG, not float bit-exactness).
- **Mass calculations use f32 consistently** — no mixed f32/f64 intermediate values.

---

## 5. Interaction with Existing Systems

### 5.1 Mining Loop Impact

Current loop: Transit → Mine → Transit → Deposit → repeat.

New loop: **Refuel** → Transit → Mine → Transit → Deposit → repeat.

The refuel step adds time and creates propellant as a consumable constraint. A ship mining at the outer belt needs significantly more propellant per trip than one mining at the inner belt.

### 5.2 Storage Pressure

LH2 has very low density (71 kg/m³). Storing 50,000 kg of LH2 requires ~704 m³ of station storage. This is 35% of the 2,000 m³ station capacity. Propellant storage competes with ore, materials, and components for station volume.

This creates a genuine strategic decision: how much station capacity to dedicate to propellant reserves vs production materials.

### 5.3 Fleet Scaling

More ships = more propellant consumption. Fleet expansion is no longer free after the initial ship construction cost. This aligns with the design spine: "scaling multiplies both output and fragility."

### 5.4 Economy Interaction

LH2 should be importable/exportable via the economy system. Players can:
- Buy propellant at Earth Orbit (expensive but immediate)
- Manufacture it from volatile-rich asteroids (cheaper but requires infrastructure)
- Export surplus propellant (revenue source for volatile-mining operations)

---

## 6. Balance Considerations

### 6.1 Key Tuning Knobs

| Parameter | Default | Effect |
|---|---|---|
| `exhaust_velocity` | 30,000 m/s | Higher = less propellant per hop. Tech upgrades increase this. |
| `hop_dv` per edge | 2,000–6,000 | Higher = more expensive hops. Controls region accessibility. |
| `ticks_per_dv_unit` | 1.0 | Scales travel time. Higher = slower travel. |
| `propellant_capacity_kg` | 50,000 | Limits range. Larger tanks = more hops but less cargo space. |
| `ship_dry_mass_kg` | 5,000 | Lower = more efficient. Affects propellant cost linearly. |
| `lh2_boiloff_rate` | 0.00001 | Loss per tick. Creates urgency to use propellant before it evaporates. |
| `refuel_kg_per_tick` | 100 | Refueling speed. Affects turnaround time. |

### 6.2 Pacing Targets

| Route | Loaded Ship (150t ore) | Empty Ship | Notes |
|---|---|---|---|
| Earth ↔ Inner Belt | ~35,000 kg LH2 round trip | ~8,000 kg | Primary mining route |
| Earth ↔ Mid Belt | ~50,000 kg LH2 round trip | ~12,000 kg | Requires careful planning |
| Earth ↔ Outer Belt | ~70,000 kg LH2 round trip | ~18,000 kg | Needs refueling station |
| Earth ↔ Trojan | Cannot round-trip on one tank | ~25,000 kg | Requires waypoint refueling |

These numbers create natural "range rings" — you can easily reach the inner belt, but outer belt requires propellant infrastructure.

### 6.3 Research Progression

| Tech | Effect | Domain |
|---|---|---|
| Starter (no tech) | exhaust_velocity: 30,000 | — |
| `tech_efficient_propulsion` | exhaust_velocity: 45,000 (+50%) | Engineering |
| `tech_ion_drive` | exhaust_velocity: 100,000 (efficient but slow thrust) | Engineering + Materials |
| `tech_cryo_insulation` | boiloff_rate * 0.25 | Materials |

---

## 7. Implementation Phases

### Phase 1: Variable Travel Time (Epic 2 prerequisite)
- Add `hop_dv` to edge definitions
- Compute travel time from `hop_dv * ticks_per_dv_unit`
- No propellant consumption yet — travel just takes different time per edge
- Backward compatible: edges without `hop_dv` use `travel_ticks_per_hop`

### Phase 2: Propellant Consumption (Epic 4)
- Add propellant fields to ShipState
- Deduct propellant on transit
- Add Refuel task
- Autopilot checks propellant before transit
- Ships with no fuel become idle

### Phase 3: Propellant Economy (Epic 4 + Economy)
- LH2 in pricing.json
- Import/export via economy system
- Propellant cost metrics in sim_bench

---

## 8. Out of Scope

| Topic | Reason |
|---|---|
| Hohmann transfers / real orbital mechanics | Design spine prohibits. Delta-v is abstracted. |
| Continuous thrust trajectories | Ships "hop" between nodes. No in-flight simulation. |
| Gravity assists | Would require orbital position tracking. |
| Aerobraking | No atmospheres in the model. |
| Variable exhaust velocity by thrust level | One engine stat per ship type. Simple. |
| Relativistic effects | Speeds are far below c. |
| Ship-to-ship docking / transfer | Ships interact with stations only. |
| Mid-flight abort / rerouting | Transit is atomic. Ship commits to full route. |
