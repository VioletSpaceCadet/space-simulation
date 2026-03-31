---
title: "feat: P3 — Tech Tree Expansion for P0-P4 Systems"
type: feat
status: active
date: 2026-03-30
origin: docs/plans/2026-03-28-003-feat-game-progression-system-plan.md
---

# P3: Tech Tree Expansion for P0-P4 Systems

## Overview

Expand the tech tree from 11 flat techs to ~25-30 techs across 3 tiers, providing meaningful progression gating for the systems built in P0-P4: ground operations (sensors, manufacturing), rocketry (launch vehicles, reusability), and satellites (survey, communication, navigation, science). No future-state techs (nuclear, deep space, planetary bases) — those come with the projects that need them.

**Scoping principle:** Every tech in this tree gates functionality that already exists or is being built in P0-P4. No speculative techs for systems that don't exist yet. When P5+ add new systems, they add their own techs.

**Key design goals:**
- **Maximum content flexibility** — domain point requirements, research rates, tier thresholds all configurable via content JSON, not hardcoded
- **Progression speed tunable via constants** — a single `research_speed_multiplier` or per-domain rate scaling so the entire tech tree can be compressed or stretched without touching individual tech entries
- **This runs AFTER P4** — satellites and launch systems exist before the tech tree is deepened. Existing 11 techs continue working; new techs layer on top.

## Problem Statement

The current 11-tech tree completes in ~1000 ticks (~42 game-days at mpt=60). After that, research is irrelevant. With P2 adding ground operations, rocketry, and diverse sensors, and P4 adding satellites, there are many new capabilities that should be gated behind research — but the tech tree has no structure for it.

| Problem | Impact |
|---|---|
| 11 techs, max depth 2 | Tree completes trivially fast, no strategic decisions |
| No tiers | All techs accessible from start — no progression gating |
| 4 research domains only (Survey, Materials, Manufacturing, Propulsion) | No Engineering domain for rocketry/structural techs |
| No rocketry techs | P2 rockets ungated — all available immediately |
| No satellite techs | P4 satellites ungated |
| No sensor techs | P2 sensor types ungated |
| Fixed research rates | Can't tune progression speed without editing individual techs |

## Proposed Solution

### Tier Structure (content-driven)

Add a `tier: u32` field to TechDef. Tiers are organizational (UI grouping, prerequisite validation) and optionally affect research speed (higher tiers could have a configurable speed penalty).

| Tier | Domain Pts Range | Game Phase | Gating For |
|---|---|---|---|
| 1 | 5-20 | Ground Ops (P2 early) | Ground sensors, basic manufacturing, basic rocketry |
| 2 | 20-60 | Early Orbital (P2 late + P4) | Medium rocketry, satellites, deep scan, refining |
| 3 | 60-150 | Industrial (P4 late) | Heavy rocketry, reusability, advanced manufacturing, automation |

**No tiers 4-6 in this scope.** Those arrive with P5 (stations), P6 (AI), P7 (planetary bases).

### Configurable Research Pacing

All research speed parameters in content, not hardcoded:

```json
// content/constants.json additions
{
  "research_speed_multiplier": 1.0,        // Global speed dial (2.0 = 2x faster)
  "research_domain_rates": {               // Per-domain speed multipliers
    "Survey": 1.0,
    "Materials": 1.0,
    "Manufacturing": 1.0,
    "Propulsion": 1.0,
    "Engineering": 1.0
  },
  "research_tier_scaling": [1.0, 1.0, 1.0], // Per-tier speed multiplier (1.0 = no penalty)
  "research_infrastructure_scaling": true,   // More labs + sensors = faster research
  "research_lab_diminishing_returns": 0.7    // Each additional lab gives 70% of previous
}
```

**How it works:** When a lab produces domain points:
```
effective_points = base_points
  × research_speed_multiplier
  × research_domain_rates[domain]
  × research_tier_scaling[tech.tier]
  × infrastructure_factor(lab_count)
```

This lets you tune the entire progression timeline via one constant (`research_speed_multiplier`) or fine-tune per-domain/per-tier.

### Engineering Domain (via VIO-544)

With DataKind and ResearchDomain migrated to content-driven strings (VIO-544), adding an Engineering domain is a content-only change:
- New data kind: `"EngineeringData"` produced by manufacturing activities and structural assembly
- New research domain: `"Engineering"` consuming EngineeringData
- Rocketry and structural techs require Engineering domain points
- Engineering labs accept EngineeringData

### Tech Tree Design (Scoped to P0-P4)

#### Tier 1: Ground Phase (~8 techs, 5-20 domain pts)

| Tech | Prereqs | Domain Requirements | Effects | Gating For |
|---|---|---|---|---|
| tech_ground_observation | none | Survey: 5 | Unlocks ground optical telescope purchase | P2 ground sensors |
| tech_radio_astronomy | ground_observation | Survey: 10, Materials: 5 | Unlocks radio telescope | P2 radio sensor |
| tech_basic_rocketry | ground_observation | Engineering: 15 | Unlocks sounding + light launcher recipes | P2 launch system |
| tech_satellite_deployment | basic_rocketry | Engineering: 10, Survey: 5 | Unlocks satellite manufacturing recipes | P4 satellites |
| tech_deep_scan_v1 | (EXISTING) | Survey: 20 | EnableDeepScan | Existing |
| tech_solar_efficiency | (EXISTING, retiered) | Mfg: 10, Survey: 5 | +50% solar output | Existing |
| tech_battery_storage | (EXISTING, retiered) | Mfg: 15 | +100% battery capacity | Existing |
| tech_electrolysis_efficiency | (EXISTING, retiered) | Materials: 15, Mfg: 5 | -40% electrolysis power | Existing |

#### Tier 2: Early Orbital (~10 techs, 20-60 domain pts)

| Tech | Prereqs | Domain Requirements | Effects | Gating For |
|---|---|---|---|---|
| tech_medium_rocketry | basic_rocketry | Engineering: 40, Propulsion: 20 | Unlocks medium launcher | P2 medium rockets |
| tech_navigation_systems | satellite_deployment | Survey: 30, Engineering: 15 | Unlocks nav beacon satellites | P4 nav sats |
| tech_orbital_science | satellite_deployment | Survey: 25, Materials: 20 | Unlocks science platform satellites | P4 science sats |
| tech_advanced_comm | satellite_deployment | Engineering: 30 | Unlocks advanced comm relay (tier upgrade) | P4 adv comm |
| tech_spectroscopy | radio_astronomy | Materials: 25, Survey: 15 | Unlocks spectroscopy sensor purchase | P2 future sensors |
| tech_infrared_sensing | ground_observation | Survey: 20, Materials: 15 | Unlocks IR sensor purchase | P2 future sensors |
| tech_advanced_refining | (EXISTING, retiered) | Materials: 30, Mfg: 10 | +15% processing yield | Existing |
| tech_ship_construction | (EXISTING, retiered) | Mfg: 40 | EnableShipConstruction | Existing |
| tech_cryo_insulation | (EXISTING, retiered) | Materials: 20, Mfg: 5 | -75% boiloff rate | Existing |
| tech_advanced_manufacturing | (EXISTING, retiered) | Mfg: 50 | (gates automation_basic) | Existing |

#### Tier 3: Industrial (~7-8 techs, 60-150 domain pts)

| Tech | Prereqs | Domain Requirements | Effects | Gating For |
|---|---|---|---|---|
| tech_heavy_rocketry | medium_rocketry | Engineering: 80, Propulsion: 40 | Unlocks heavy launcher | P2 heavy rockets |
| tech_partial_recovery | medium_rocketry | Engineering: 60, Materials: 40 | Unlocks booster recovery | P2 reusability |
| tech_full_reuse | partial_recovery | Engineering: 120, Materials: 60 | Unlocks full reusability | P2 full reuse |
| tech_advanced_propulsion | efficient_propulsion | Propulsion: 80, Engineering: 30 | Reduced fuel consumption | P2 launch cost |
| tech_efficient_transit | (EXISTING, retiered) | Propulsion: 30 | +10% ship speed | Existing |
| tech_efficient_propulsion | (EXISTING, retiered) | Propulsion: 20, Survey: 10 | -33% fuel consumption | Existing |
| tech_automation_basic | (EXISTING, retiered) | Mfg: 100, Materials: 40 | (gates future automation) | Existing |
| tech_research_acceleration | automation_basic | Mfg: 80, Survey: 40 | +25% research speed (stat modifier) | Research pacing |

**Total: ~25 techs** (11 existing retiered + ~14 new). Focused, not bloated.

### Module/Recipe Gating

Assign `required_tech` to modules and recipes that should be progression-gated:

| Module/Recipe | Required Tech | Rationale |
|---|---|---|
| Ground optical telescope | tech_ground_observation | First research target |
| Radio telescope | tech_radio_astronomy | Tier 1 progression |
| Sounding rocket recipe | tech_basic_rocketry | First launch capability |
| Light launcher recipe | tech_basic_rocketry | Early orbital access |
| Medium launcher recipe | tech_medium_rocketry | Heavier payloads |
| Heavy launcher recipe | tech_heavy_rocketry | Station kit launches |
| Survey satellite recipe | tech_satellite_deployment | First satellite |
| Comm relay recipe | tech_satellite_deployment | Zone enablement |
| Nav beacon recipe | tech_navigation_systems | Transit optimization |
| Science platform recipe | tech_orbital_science | Orbital research |
| Advanced comm relay | tech_advanced_comm | Full speed zones |
| IR sensor | tech_infrared_sensing | Advanced classification |
| Spectrograph | tech_spectroscopy | Composition analysis |
| Reusable rocket variant | tech_partial_recovery | Cost reduction |
| Fully reusable rocket | tech_full_reuse | Major cost reduction |
| Starter modules (solar, basic assembler, maintenance, sensor_array) | None | Always available |

## Technical Approach

### Engine Changes (Minimal)

Most of P3 is content. Engine changes needed:

1. **TechDef.tier field** — `tier: u32` (serde default = 1). Organizational + optional speed scaling. Content validation: tier > 0, tier sequential.

2. **Research pacing constants** — `research_speed_multiplier`, `research_domain_rates`, `research_tier_scaling`, `research_lab_diminishing_returns` in Constants. Applied in `generate_data()` and `advance_research()`.

3. **Infrastructure scaling** — lab count modifies data generation rate. Each additional lab gives diminishing returns (configurable factor). Already partially exists (each lab ticks independently) but no diminishing returns applied.

4. **Engineering domain** — content-only after VIO-544 (DataKind/ResearchDomain string migration). Add `"Engineering"` domain, `"EngineeringData"` data kind, engineering labs to module_defs.json.

5. **Research acceleration tech effect** — new TechEffect variant `ResearchSpeedBonus { multiplier }` that adds to `research_speed_multiplier`. Applied via stat modifier system.

### Content Changes (Bulk)

- `content/techs.json` — expand from 11 to ~25 entries with tier field, reorganized prerequisites
- `content/module_defs.json` — add `required_tech` to modules that should be gated
- `content/recipes.json` — add `required_tech` to recipes for rockets, satellites, advanced components
- `content/constants.json` — add research pacing constants
- `content/module_defs.json` — add engineering lab module definition

## Implementation Tickets

### Ticket 1: TechDef tier field + research pacing constants

**What:** Add `tier: u32` to TechDef. Add research pacing constants to Constants. All content-configurable.

**Details:**
- TechDef gets `tier: u32` (serde default = 1, backward compatible)
- Constants additions:
  - `research_speed_multiplier: f64` (default 1.0)
  - `research_domain_rates: HashMap<String, f64>` (default all 1.0, keyed by domain string after VIO-544)
  - `research_tier_scaling: Vec<f64>` (default [1.0, 1.0, 1.0], per-tier)
  - `research_lab_diminishing_returns: f64` (default 1.0 = no diminishing returns)
- Content validation: tier values valid, scaling arrays match tier count
- Existing techs get tier = 1 or 2 based on current domain requirements

**Acceptance criteria:**
- [ ] TechDef has tier field, backward compatible
- [ ] 5 new research pacing constants in Constants
- [ ] All tests pass with defaults (behavior unchanged)
- [ ] Content validation catches invalid tier/scaling values
- [ ] Schema documented in docs/reference.md

**Dependencies:** VIO-544 (DataKind/ResearchDomain strings — for domain_rates keys)
**Size:** Medium

---

### Ticket 2: Research pacing engine — apply speed multipliers + diminishing returns

**What:** Apply research_speed_multiplier, domain rates, tier scaling, and lab diminishing returns in the research pipeline.

**Details:**
- `generate_data()` applies: `base_amount × research_speed_multiplier × domain_rate`
- `advance_research()` applies tier scaling when computing progress toward tech unlock
- Lab diminishing returns: Nth lab produces `base × diminishing_returns^(N-1)` (1st lab = 100%, 2nd = 70%, 3rd = 49% at 0.7 factor)
- All configurable via Constants (no hardcoded values)
- sim_bench scenarios can override these via scenario overrides

**Acceptance criteria:**
- [ ] research_speed_multiplier = 2.0 produces 2x faster research
- [ ] Per-domain rates affect only that domain
- [ ] Tier scaling affects only techs in that tier
- [ ] Lab diminishing returns reduce marginal lab output
- [ ] Unit test: each multiplier independently validated
- [ ] Integration test: 2x speed completes tech tree in half the ticks
- [ ] Existing behavior unchanged with all defaults = 1.0

**Dependencies:** Ticket 1
**Size:** Medium

---

### Ticket 3: Engineering domain + engineering lab module

**What:** Add Engineering research domain and EngineeringData data kind via content (requires VIO-544). Add engineering lab module definition.

**Details:**
- After VIO-544: "Engineering" is just a string in content, no Rust change
- New data kind: "EngineeringData" — produced by structural assembly activities and a dedicated engineering lab
- New module: `module_engineering_lab` — accepts EngineeringData, produces Engineering domain points
- Assembler activity generates EngineeringData (like it generates ManufacturingData now)
- Rocketry techs require Engineering domain points

**Acceptance criteria:**
- [ ] "Engineering" domain and "EngineeringData" kind work in content
- [ ] Engineering lab module defined and loadable
- [ ] Assembler generates EngineeringData alongside ManufacturingData
- [ ] Rocketry techs can require Engineering domain
- [ ] Unit test: engineering lab processes EngineeringData

**Dependencies:** VIO-544 (content-driven strings)
**Size:** Small-Medium

---

### Ticket 4: Tier 1 tech content — ground phase techs

**What:** Define ~8 Tier 1 techs in techs.json. Retier existing starter techs. Gate ground sensors and basic rocketry.

**Details:**
- New techs: tech_ground_observation, tech_radio_astronomy, tech_basic_rocketry, tech_satellite_deployment
- Retier existing: tech_deep_scan_v1 (tier 1), tech_solar_efficiency (tier 1), tech_battery_storage (tier 1), tech_electrolysis_efficiency (tier 1)
- Domain requirements: 5-20 points (achievable within ground phase)
- Prerequisites form a sensible DAG (ground_observation → radio_astronomy, ground_observation → basic_rocketry → satellite_deployment)

**Acceptance criteria:**
- [ ] 8 tier 1 techs in techs.json (4 new + 4 retiered)
- [ ] Prereq DAG is valid (no cycles, all refs exist)
- [ ] Domain requirements achievable with ground sensors in reasonable time
- [ ] Integration test with real content: tier 1 techs unlock within expected ticks
- [ ] Existing behavior preserved for retiered techs

**Dependencies:** Ticket 1 (tier field), Ticket 3 (Engineering domain for basic_rocketry)
**Size:** Medium

---

### Ticket 5: Tier 2 tech content — early orbital techs

**What:** Define ~10 Tier 2 techs. Gate medium rocketry, satellites, advanced refining.

**Details:**
- New techs: tech_medium_rocketry, tech_navigation_systems, tech_orbital_science, tech_advanced_comm, tech_spectroscopy, tech_infrared_sensing
- Retier existing: tech_advanced_refining (tier 2), tech_ship_construction (tier 2), tech_cryo_insulation (tier 2), tech_advanced_manufacturing (tier 2)
- Domain requirements: 20-60 points (requires ground + early orbital research)
- Cross-domain requirements on some techs (forces breadth)

**Acceptance criteria:**
- [ ] 10 tier 2 techs in techs.json (6 new + 4 retiered)
- [ ] Prereq DAG valid
- [ ] Cross-domain requirements force research breadth
- [ ] Integration test: tier 2 techs unlock after tier 1 within expected ticks

**Dependencies:** Ticket 4 (tier 1 techs as prerequisites)
**Size:** Medium

---

### Ticket 6: Tier 3 tech content — industrial techs

**What:** Define ~8 Tier 3 techs. Gate heavy rocketry, reusability, advanced propulsion, research acceleration.

**Details:**
- New techs: tech_heavy_rocketry, tech_partial_recovery, tech_full_reuse, tech_advanced_propulsion, tech_research_acceleration
- Retier existing: tech_efficient_transit (tier 3), tech_efficient_propulsion (tier 3), tech_automation_basic (tier 3)
- Domain requirements: 60-150 points
- tech_research_acceleration: new TechEffect::ResearchSpeedBonus { multiplier: 0.25 } (adds 25% to global speed)

**Acceptance criteria:**
- [ ] 8 tier 3 techs in techs.json (5 new + 3 retiered)
- [ ] Prereq DAG valid (chains from tier 1 → 2 → 3)
- [ ] tech_research_acceleration applies speed bonus via stat modifier
- [ ] Integration test: tier 3 techs unlock at expected pacing

**Dependencies:** Ticket 5 (tier 2 techs as prerequisites)
**Size:** Medium

---

### Ticket 7: Module/recipe gating — assign required_tech to P2/P4 content

**What:** Assign `required_tech` values to modules and recipes that should be progression-gated. Starter modules remain ungated.

**Details:**
- Ground sensors: optical (tech_ground_observation), radio (tech_radio_astronomy), IR (tech_infrared_sensing), spectroscopy (tech_spectroscopy)
- Rocket recipes: sounding/light (tech_basic_rocketry), medium (tech_medium_rocketry), heavy (tech_heavy_rocketry)
- Satellite recipes: survey/comm (tech_satellite_deployment), nav (tech_navigation_systems), science (tech_orbital_science)
- Reusable rocket variants: partial (tech_partial_recovery), full (tech_full_reuse)
- Advanced comm relay: tech_advanced_comm
- Starter modules (basic solar, basic assembler, maintenance, sensor_array): required_tech = None

**Acceptance criteria:**
- [ ] All P2/P4 modules and recipes have appropriate required_tech
- [ ] Starter modules remain ungated
- [ ] Autopilot respects tech gates (doesn't try to build what isn't unlocked)
- [ ] Integration test: can't manufacture medium rocket without tech_medium_rocketry

**Dependencies:** Tickets 4-6 (techs must exist to reference), P2/P4 content (modules/recipes must exist)
**Size:** Medium

---

### Ticket 8: Research pacing validation — sim_bench scenarios

**What:** sim_bench scenarios validating that the tech tree pacing works at different speed settings.

**Details:**
- `scenarios/tech_tree_baseline.json` — ground_start, default research_speed_multiplier (1.0), 20000 ticks, 10 seeds. Expected: tier 1 complete by ~2000 ticks, tier 2 by ~8000, tier 3 by ~18000.
- `scenarios/tech_tree_fast.json` — same but research_speed_multiplier = 3.0 via overrides. Expected: ~3x faster completion.
- `scenarios/tech_tree_slow.json` — research_speed_multiplier = 0.5. Expected: ~2x slower.
- Validates: all techs reachable, no deadlocks, pacing scales linearly with multiplier.

**Acceptance criteria:**
- [ ] Baseline pacing matches expected tier completion windows
- [ ] Speed multiplier produces proportional pacing change
- [ ] No tech deadlocks (all techs reachable from ground_start)
- [ ] Cross-seed variance reasonable (stddev < 20% of mean)

**Dependencies:** Tickets 4-6 (tech content), Ticket 2 (pacing engine)
**Size:** Medium

---

### Ticket 9: Tech tree visualization data for FE

**What:** Expose tech tree structure (tiers, prereqs, unlock status) via daemon API for future FE visualization. No FE panel in this ticket — just the API.

**Details:**
- `GET /api/v1/tech-tree` — returns full tech tree structure: tiers, techs per tier, prerequisite edges, unlock status, current research progress per domain
- Extends existing `GET /api/v1/content` response with tech tree metadata
- ResearchPanel already exists — this provides data for enhancing it with tier grouping and prereq visualization

**Acceptance criteria:**
- [ ] API returns tech tree with tier grouping
- [ ] Prereq edges included for DAG visualization
- [ ] Unlock status per tech
- [ ] Current domain point progress included

**Dependencies:** Ticket 1 (tier field on TechDef)
**Size:** Small-Medium

---

## Dependency Graph

```
VIO-544 (DataKind/ResearchDomain strings) ──→ Ticket 3 (Engineering domain)
                                                       │
Ticket 1 (tier field + pacing constants) ──→ Ticket 2 (pacing engine)
       │                                            │
       └──→ Ticket 4 (tier 1 techs) ←── Ticket 3   │
                    │                                │
              Ticket 5 (tier 2 techs)                │
                    │                                │
              Ticket 6 (tier 3 techs) ←── Ticket 2 (research_acceleration effect)
                    │
              Ticket 7 (module/recipe gating)
                    │
              Ticket 8 (pacing validation scenarios)

Ticket 1 ──→ Ticket 9 (API for FE)
```

**Critical path:** VIO-544 → 3 → 4 → 5 → 6 → 7 → 8

**Parallel:** Tickets 1-2 (engine) can parallel with VIO-544 → 3 (content)

## Risk Analysis

### Medium Risk

**Research pacing imbalance** — with configurable rates, a bad multiplier setting could make the tree trivial or impossibly slow.
- *Mitigation:* sim_bench scenarios validate at 3 speed settings. Defaults produce reasonable pacing. Content-tunable means quick fixes.

**Existing tech behavior changes** — retiering existing techs could change when they unlock relative to current gameplay.
- *Mitigation:* Existing techs keep their current domain_requirements. Tier assignment is organizational only (no speed penalty by default). Behavior only changes if research_tier_scaling is modified from defaults.

### Low Risk

**Engineering domain data generation** — new domain needs a data source. If no module generates EngineeringData, Engineering techs are unreachable.
- *Mitigation:* Assembler activity generates EngineeringData. Engineering lab module accepts it. Both exist from ground facility startup.

## Sources & References

### Origin

- **Origin document:** [docs/plans/2026-03-28-003-feat-game-progression-system-plan.md](../../docs/plans/2026-03-28-003-feat-game-progression-system-plan.md) — Project 3 section. **Scoped down:** 25 techs (not 60+), 3 tiers (not 6), no nuclear/deep space. Focused on P0-P4 systems only.

### Internal References

- `content/techs.json` — current 11 techs (all being retiered or preserved)
- `crates/sim_core/src/research.rs` — generate_data(), advance_research() (pacing changes here)
- `crates/sim_core/src/types/content.rs:620` — ModuleDef.required_tech (already exists)
- `crates/sim_core/src/types/content.rs:906` — RecipeDef.required_tech (already exists)
- VIO-544 — DataKind/ResearchDomain string migration (prerequisite for Engineering domain)
- P2 plan (VIO-543-566) — rocketry techs referenced
- P4 plan (VIO-567-578) — satellite techs referenced

### Related Linear

- VIO-505: Plan P3 (planning ticket — mark done)
