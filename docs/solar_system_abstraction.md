# Solar System Abstraction Specification

> Status: Design phase. No code changes yet.
> Companion docs: `energy_and_propellant_design.md`, `movement_cost_model.md`

## 1. Goals

Expand the spatial model from a linear 4-node chain to a richer graph that:
- Makes **location matter** — solar intensity, resource availability, and travel cost vary by region
- Supports **volatile-rich vs metal-rich** asteroid distribution
- Provides **natural trade routes** — inner belt has power but needs fuel; outer belt has ice but needs power
- Is **simple enough** to not require orbital mechanics
- Is **extensible** for future real dataset ingestion (JPL small body database, etc.)

---

## 2. Current Model

```
Earth Orbit ── Inner Belt ── Mid Belt ── Outer Belt
```

4 nodes, 3 edges. Linear chain. No properties on nodes besides name. Travel cost is uniform: 2,880 ticks per hop.

---

## 3. Proposed Model

### 3.1 Node Properties

Each node gains:

```rust
pub struct NodeDef {
    pub id: NodeId,
    pub name: String,
    pub solar_intensity: f32,       // 0.0–1.0. Power output multiplier.
    pub resource_class: ResourceClass,  // What asteroid types spawn here
    pub hop_cost_dv: f32,           // Delta-v cost TO this node (from neighbors)
}

pub enum ResourceClass {
    MetalRich,      // Mostly S-type: Fe, Si, Ni
    Mixed,          // Both metal and volatile templates
    VolatileRich,   // Mostly C-type: H2O, carbonaceous
}
```

### 3.2 Expanded Node Graph

```
                    ┌── NEO Zone ──┐
                    │              │
Earth Orbit ── Lunar Orbit ── Inner Belt ── Mid Belt ── Outer Belt
                                                │
                                           Trojan Zone
```

| Node | Solar Intensity | Resource Class | Notes |
|---|---|---|---|
| `node_earth_orbit` | 1.00 | — | Starting location. No asteroids. Trade hub. |
| `node_lunar_orbit` | 0.95 | Mixed | Near-Earth. Some volatiles, some metal. |
| `node_neo_zone` | 0.90 | Mixed | Near-Earth Objects. Fast access from Lunar. |
| `node_belt_inner` | 0.40 | MetalRich | S-type asteroids dominate. Good solar. |
| `node_belt_mid` | 0.20 | Mixed | Transition zone. Both types present. |
| `node_belt_outer` | 0.08 | VolatileRich | C-type asteroids. Ice-rich. Poor solar. |
| `node_trojan` | 0.06 | VolatileRich | Jupiter Trojans. Very ice-rich, very far. |

### 3.3 Edge Definition

Edges now carry a `hop_dv` cost (delta-v in m/s, abstracted) that determines both travel time and propellant consumption:

```json
{
  "edges": [
    { "from": "node_earth_orbit", "to": "node_lunar_orbit", "hop_dv": 3000 },
    { "from": "node_lunar_orbit", "to": "node_neo_zone", "hop_dv": 4000 },
    { "from": "node_lunar_orbit", "to": "node_belt_inner", "hop_dv": 5000 },
    { "from": "node_belt_inner", "to": "node_belt_mid", "hop_dv": 2000 },
    { "from": "node_belt_mid", "to": "node_belt_outer", "hop_dv": 3000 },
    { "from": "node_belt_mid", "to": "node_trojan", "hop_dv": 6000 }
  ]
}
```

**Travel time** is derived from `hop_dv`:
```
travel_ticks = hop_dv * ticks_per_dv_unit
```

Where `ticks_per_dv_unit` is a constant (e.g., 1.0 → 3000 dv = 3000 ticks ≈ 2 days). This replaces the uniform `travel_ticks_per_hop`.

**Propellant cost** is also derived from `hop_dv` (see `movement_cost_model.md`).

### 3.4 Backward Compatibility

The old `travel_ticks_per_hop` constant becomes a fallback for edges without explicit `hop_dv`. During the transition:

1. If edge has `hop_dv`: use it for both time and propellant cost
2. If edge lacks `hop_dv`: use `travel_ticks_per_hop` for time, zero propellant cost

This allows Epic 1–2 to land before propellant-based movement (Epic 4).

---

## 4. Solar Intensity Model

### 4.1 Physics Basis (Simplified)

Real solar intensity follows the inverse-square law: `I = I₀ / r²` where `r` is distance from the Sun in AU.

| Location | Real AU | Real Intensity | Our Value |
|---|---|---|---|
| Earth orbit | 1.0 | 1.00 | 1.00 |
| Lunar orbit | 1.0 | 1.00 | 0.95 (minor penalty for distance from Earth infrastructure) |
| NEO zone | 1.0–1.5 | 0.44–1.00 | 0.90 |
| Inner belt | 2.0–2.5 | 0.16–0.25 | 0.40 (gameplay concession — real would be ~0.20) |
| Mid belt | 2.5–3.0 | 0.11–0.16 | 0.20 |
| Outer belt | 3.0–3.5 | 0.08–0.11 | 0.08 |
| Trojan (L4/L5) | 5.2 | 0.04 | 0.06 |

Values are **gameplay-tuned**, not strict inverse-square. Inner belt is boosted to make solar viable (barely) as a primary power source. Outer belt and beyond are intentionally punishing — you need batteries, nuclear (future), or massive solar arrays.

### 4.2 Impact on Power Generation

A solar array module with `base_output_kw: 50.0` at different locations:

| Location | Solar Intensity | Effective Output |
|---|---|---|
| Earth Orbit | 1.00 | 50.0 kW |
| Inner Belt | 0.40 | 20.0 kW |
| Mid Belt | 0.20 | 10.0 kW |
| Outer Belt | 0.08 | 4.0 kW |
| Trojan | 0.06 | 3.0 kW |

Running a basic refinery (10 kW) at the outer belt requires 3 solar arrays. This creates **infrastructure scaling pressure** — the design spine's core tension.

---

## 5. Resource Distribution Model

### 5.1 Asteroid Template Assignment

Each `ResourceClass` determines which asteroid templates spawn at scan sites in that node:

| Resource Class | Templates | Probability |
|---|---|---|
| MetalRich | `tmpl_iron_rich` (80%), `tmpl_silicate` (20%) | No volatiles |
| Mixed | `tmpl_iron_rich` (40%), `tmpl_silicate` (30%), `tmpl_volatile_rich` (30%) | Balanced |
| VolatileRich | `tmpl_volatile_rich` (60%), `tmpl_carbonaceous` (30%), `tmpl_silicate` (10%) | Ice-dominant |

### 5.2 New Asteroid Templates

```json
{
  "id": "tmpl_volatile_rich",
  "anomaly_tags": ["VolatileRich"],
  "composition_ranges": {
    "H2O": [0.30, 0.60],
    "Fe": [0.05, 0.15],
    "Si": [0.10, 0.25],
    "He": [0.05, 0.20]
  }
}
```

```json
{
  "id": "tmpl_carbonaceous",
  "anomaly_tags": ["Carbonaceous"],
  "composition_ranges": {
    "H2O": [0.15, 0.35],
    "Fe": [0.10, 0.20],
    "Si": [0.20, 0.35],
    "He": [0.10, 0.25]
  }
}
```

**Key insight:** Volatile-rich asteroids have lower density (~1,500 kg/m³ vs 3,000 for metal ore), so the same mass takes more volume. This interacts with ship cargo capacity — a full load of ice ore weighs less but fills the hold just as fast.

### 5.3 Scan Site Generation

Currently: scan sites are assigned to random belt nodes. Proposed change:

- Each node has a `scan_site_weight` (0.0–1.0) controlling how many sites spawn there.
- When sites replenish (count < 5), new sites preferentially spawn at nodes with higher weight.
- Template selection uses the node's `ResourceClass` probabilities.

This gives players a reason to explore specific regions.

---

## 6. Content File Changes

### 6.1 solar_system.json (Updated Format)

```json
{
  "nodes": [
    {
      "id": "node_earth_orbit",
      "name": "Earth Orbit",
      "solar_intensity": 1.0,
      "resource_class": null,
      "scan_site_weight": 0.0
    },
    {
      "id": "node_lunar_orbit",
      "name": "Lunar Orbit",
      "solar_intensity": 0.95,
      "resource_class": "Mixed",
      "scan_site_weight": 0.1
    },
    {
      "id": "node_belt_inner",
      "name": "Inner Belt",
      "solar_intensity": 0.40,
      "resource_class": "MetalRich",
      "scan_site_weight": 0.3
    },
    {
      "id": "node_belt_mid",
      "name": "Mid Belt",
      "solar_intensity": 0.20,
      "resource_class": "Mixed",
      "scan_site_weight": 0.25
    },
    {
      "id": "node_belt_outer",
      "name": "Outer Belt",
      "solar_intensity": 0.08,
      "resource_class": "VolatileRich",
      "scan_site_weight": 0.25
    },
    {
      "id": "node_trojan",
      "name": "Trojan Zone",
      "solar_intensity": 0.06,
      "resource_class": "VolatileRich",
      "scan_site_weight": 0.1
    }
  ],
  "edges": [
    { "from": "node_earth_orbit", "to": "node_lunar_orbit", "hop_dv": 3000 },
    { "from": "node_lunar_orbit", "to": "node_belt_inner", "hop_dv": 5000 },
    { "from": "node_belt_inner", "to": "node_belt_mid", "hop_dv": 2000 },
    { "from": "node_belt_mid", "to": "node_belt_outer", "hop_dv": 3000 },
    { "from": "node_belt_mid", "to": "node_trojan", "hop_dv": 6000 }
  ]
}
```

Note: `node_neo_zone` omitted from MVP. Can be added later without breaking anything.

### 6.2 Backward Compatibility

- `solar_intensity` defaults to `1.0` if missing (all existing nodes get full solar)
- `resource_class` defaults to `null` (existing template assignment logic unchanged)
- `hop_dv` defaults to `null` (falls back to `travel_ticks_per_hop` constant)
- `scan_site_weight` defaults to `1.0` (uniform distribution, current behavior)

---

## 7. Future Extension Points

### 7.1 Real Dataset Ingestion

The node model is designed so that a future "realistic mode" could:
- Import JPL Small Body Database for real asteroid catalogs
- Map real asteroid families to our ResourceClass enum
- Use real AU distances to compute solar_intensity via inverse-square
- Use real delta-v tables for hop costs between orbital regions

This requires NO architectural changes — just different content in `solar_system.json` and `asteroid_templates.json`.

### 7.2 Additional Nodes

Future nodes that fit the model:
- `node_neo_zone` — Near-Earth Objects (easy access, mixed resources)
- `node_mars_orbit` — Mars vicinity (mid solar, gateway to outer system)
- `node_ceres` — Dwarf planet (volatile-rich, possible permanent base)
- `node_jupiter_orbit` — Radiation hazard, massive ice moons (very future)

### 7.3 Station Construction at New Nodes

Currently there's one station at Earth Orbit. The spatial model supports multiple stations at different nodes. Station construction is a natural future feature that builds on:
- Energy system (new station needs power infrastructure)
- Propellant system (supply chain to new station)
- The node graph (strategic placement decisions)

---

## 8. Out of Scope

| Topic | Reason |
|---|---|
| Orbital periods / time-varying positions | Design spine prohibits. Nodes are static. |
| Continuous coordinate space | Graph-based abstraction is sufficient and deterministic. |
| Gravity wells / escape velocity | Abstracted into hop_dv costs on edges. |
| Atmospheric drag | No atmospheres in the model. |
| Radiation zones | Future entropy source. Not in this phase. |
| Planet surfaces / landing | Space-only simulation. |
