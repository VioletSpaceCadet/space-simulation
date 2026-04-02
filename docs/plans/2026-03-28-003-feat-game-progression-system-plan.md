---
title: "feat: Game Progression System — From Ground to Solar System"
type: feat
status: active
date: 2026-03-28
---

# Game Progression System — From Ground to Solar System

## Overview

A comprehensive progression system that transforms the simulation from "start with everything, watch it run" into "earn every capability through industrial mastery." The player begins with a modest ground facility and seed funding, builds toward orbital operations, expands through the asteroid belt, and ultimately masters the solar system.

This is a **roadmap plan** — it defines ~8 major projects (~85-110 tickets total) that collectively create a full early-to-late game arc. Each project is a self-contained body of work with clear deliverables, dependencies, and success criteria. The projects are built **one at a time**, each getting its own detailed planning and implementation cycle. The ordering below reflects dependencies, not simultaneous execution.

## The Problem

The simulation currently has no progression. Analysis of the current state reveals:

| What Exists | Why It's a Problem |
|---|---|
| $1B starting balance | Money is never a constraint (200+ years of runway) |
| All 19+ modules in starting inventory | Nothing to earn or unlock |
| 11 techs, max depth 2 | Entire tree completes in ~1000 ticks (~41 game-days) |
| 1 station, no construction | No spatial expansion possible |
| Flat spatial model (ships go anywhere) | No reason to expand beyond starting location |
| Trade gated by time (1 year), not achievement | Waiting is the "strategy" |
| Autopilot handles everything | No strategic decisions to make |

The `dev_advanced_state.json` is a development convenience, not a game. A player watching this simulation sees an autopilot efficiently running a pre-built industrial complex. There is no arc of: struggle → discovery → capability → mastery.

## The Progression Arc

Inspired by proven patterns across Factorio (production-as-gate), Stellaris (parallel resource tracks), Dwarf Fortress (cascading industry chains with transformative mid-game unlock), KSP (grant/contract career mode), RimWorld (wealth-scaled difficulty), EVE Online (geographic resource gating + production chain depth), and Space Engineers (resource geography forcing orbital progression):

### Act 1: Groundwork (Ticks 0-500, ~0-20 game-days)

**Fantasy:** You are a startup space mining company with investment capital and a ground-based operations center on the homeworld. You can observe space, build basic equipment, and prove your worth to attract funding.

**Core activities:**
- Operate ground telescope arrays to discover nearby asteroids
- Build basic components in ground workshop (small-scale manufacturing)
- Launch first survey satellite (unmanned, cheap, proves orbital capability)
- Apply for grants by demonstrating milestones

**Key milestone:** First satellite deployment → funding grant + credibility

**Design pattern:** KSP career mode — grants fund the activity that earns the next grant. Factorio burner phase — manual, slow, teaches fundamentals.

### Act 2: Orbital Presence (Ticks 500-2000, ~20-80 game-days)

**Fantasy:** You've proven basic space capability. A major grant funds your first orbital station. Ships launch from ground facilities. Near-homeworld asteroids become accessible.

**Core activities:**
- Deploy first orbital outpost (small, limited slots)
- Launch first mining ship
- Begin near-space asteroid surveys and mining
- Build satellite network (survey, communication, relay)
- Research unlocks: deep scan, basic refining, solar efficiency

**Key milestone:** First ore refined at orbital station → larger grant + export license

**Design pattern:** Dwarf Fortress early game — immigration waves (capability unlocks) come from proving value. Space Engineers — reaching orbit is the transformative moment.

### Act 3: Industrial Scale (Ticks 2000-5000, ~80-200 game-days)

**Fantasy:** Your orbital station becomes a real industrial hub. Ship construction begins. Manufacturing chains deepen. Export revenue replaces grant dependence.

**Core activities:**
- Refining, smelting, assembler chains running at scale
- Ship construction (build your fleet, not just buy it)
- Research labs producing breakthroughs (mid-tier tech tree)
- Export revenue overtakes grants as primary income
- First fleet coordination (3-5 ships with different roles)

**Key milestone:** Self-sustaining economy (revenue > expenses without grants) → trade license upgrade + belt access authorization

**Design pattern:** Factorio blue science — qualitative complexity jump. X4 — transitioning from "pilot" to "fleet commander." DF magma moment — self-sustaining propellant production removes the fuel bottleneck.

### Act 4: Belt Expansion (Ticks 5000-15000, ~200-625 game-days)

**Fantasy:** With a self-sustaining orbital operation, you expand to the asteroid belt. Second station. Supply chain logistics. Specialized fleet roles. Advanced manufacturing.

**Core activities:**
- Build belt station (mining outpost or refinery hub)
- Inter-station supply chain logistics
- Specialized ship types (mining barge, transport hauler, survey scout)
- Advanced tech tree (deep scan v2, automation, propulsion efficiency)
- Manufacturing DAG depth (4-5 tier production chains)

**Key milestone:** Multi-station network operational → deep space authorization

**Design pattern:** EVE security zones — better resources in riskier (more distant) space. Stellaris mid-game — managing an empire, not just a colony. Factorio purple/yellow — massive scale required.

### Act 5: Solar System Mastery (Ticks 15000+)

**Fantasy:** The outer solar system opens. Nuclear power replaces solar. Long-range logistics. Advanced automation. The simulation becomes a supply chain optimization problem at planetary scale.

**Core activities:**
- Outer belt / Jupiter Trojans operations (volatile-rich resources)
- Nuclear/RTG power systems (solar too weak beyond the belt)
- Planetary surface bases (Mars, Luna)
- Advanced automation (reduced crew dependency)
- Deep manufacturing chains (6+ tiers for advanced products)

**Key milestone:** Self-sustaining outer system operation → "Space Industrialist" rating

**Design pattern:** Factorio infinite research — always another optimization to chase. Stellaris megastructures — transformative late-game economy. EVE capital production — months of investment for apex-tier outputs.

---

## Projects

### Project 0: Scoring & Measurement Foundation

**What it delivers:** A multi-dimensional run scoring system that measures simulation quality across every existing system. This gives us the ability to quantitatively evaluate every change that follows — progression, AI improvements, balance tuning.

**Why it's first:** You can't tune what you can't measure. The scoring system reads existing game state (metrics, events, research, economy) and computes a composite score. It requires zero engine changes — it's pure measurement on top of what already exists. Every subsequent project benefits from being able to say "this change improved score by X% across 100 seeds."

**Scope (~8-10 tickets):**

- **Run scoring dimensions** — multi-dimensional score computed every 24 ticks (1 game-day), exported in metrics:

  | Dimension | Weight | Calculation |
  |---|---|---|
  | Industrial Output | 25% | Total kg processed + components assembled, normalized by tick count |
  | Research Progress | 20% | Techs unlocked / total available + research rate trend |
  | Economic Health | 20% | Net revenue trend + balance stability + trade volume |
  | Fleet Operations | 15% | Ships built + missions completed + uptime percentage |
  | Efficiency | 10% | Avg module wear (inverted) + power utilization + storage utilization |
  | Expansion | 10% | Stations operational + zones with activity + ships in fleet |

  Named thresholds: Startup (0-200) → Contractor (200-500) → Enterprise (500-1000) → Industrial Giant (1000-2000) → Space Magnate (2000+)

- **Scoring in sim_core** — `compute_run_score(state, content, constants) -> RunScore` pure function. No side effects, no state mutation. Can be called at any tick. Returns per-dimension scores + composite.
- **Scoring in sim_bench** — score computed at metrics intervals, final score included in summary output. Cross-seed comparison: mean/min/max/stddev of composite score across seeds. New column in Parquet export.
- **Scoring in sim_daemon** — `GET /api/v1/score` endpoint returning current RunScore. Included in advisor digest. SSE events for threshold crossings ("Company Rating upgraded to Contractor").
- **ML pipeline integration** — extend `scripts/analysis/labels.py` to compute scoring dimensions from Parquet data. Cross-seed scoring comparison. Enables "which strategy config produces the highest score across 100 seeds?"
- **Scoring scenarios** — new sim_bench scenarios specifically for scoring calibration: "Does the baseline scenario reach Enterprise tier within 8760 ticks?" "What's the score distribution across 100 seeds?"
- **UI score display** — score panel showing per-dimension breakdown + composite rating + trend sparklines. Lightweight — reads from daemon API.

- **AI evaluation framework** — sim_bench scenarios that measure autopilot decision quality, not just outcomes. "Did the autopilot assign labs to the right techs?" "Did it build ships at the optimal time?" "Were export decisions revenue-positive?" Comparison framework: run same seed with two different AutopilotConfigs, diff the score trajectories, identify where decisions diverged.
- **Data gap detection** — automated analysis that identifies missing or thin metric coverage. "The scoring system has no signal for propellant management quality" → flag as gap. This runs as a Python script against Parquet output, checking for dimensions with zero variance or null coverage.
- **Baseline AutopilotConfig** — formalize the current hardcoded autopilot behavior as an explicit JSON config. This doesn't change behavior — it makes the current behavior a named, versioned, comparable baseline. Future projects can create alternative configs and measure improvement.
- **Early optimization scaffolding** — Python script that runs N configs × M seeds via sim_bench, collects scores, ranks. Doesn't need to be sophisticated — grid search over 3-5 parameters is enough. The point is proving the loop works: config → run → score → rank → better config.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| Scoring computed where | Pure function in sim_core, called by bench/daemon/CLI | Deterministic, testable, no IO dependency. Same function everywhere ensures consistency. |
| Score normalization | Per-tick normalization (score/tick) so runs of different lengths are comparable | A 10,000-tick run shouldn't automatically outscore a 5,000-tick run just by existing longer. |
| Weights fixed or configurable | Content-driven via `content/scoring.json` | Allows sim_bench scenarios to weight dimensions differently (e.g., "economy-focused" vs "research-focused" scoring profiles). |
| AutopilotConfig in this project | Baseline extraction only — formalize what exists, don't redesign | The current autopilot works. Making it measurable is the goal here. Redesign comes in later projects when the decision space is richer. |

**Dependencies:** None — reads existing game state. Can start immediately.

**Estimated tickets:** 10-14

---

### Project 1: Starting State Rework & Progression Engine

**What it delivers:** The mechanical infrastructure for progression — a milestone/grant economy, achievement-gated trade, and critically, the split of dev_advanced_state into a proper progression starting state and an advanced development state.

**Why it follows scoring:** With scoring in place, we can immediately measure whether the new starting state + progression mechanics produce better, more interesting runs than the current "start with everything" approach.

**Scope (~12-15 tickets):**

- **dev_base_state → dev_advanced_state rename** — the current `content/dev_base_state.json` (fully equipped station, all modules, $1B) becomes `content/dev_advanced_state.json`. This is the "late-game development sandbox" for testing systems in isolation. All existing references updated (sim_cli, sim_daemon defaults, sim_bench scenarios, tests). No functionality changes — just naming clarity.
- **New progression starting state** — `content/progression_start.json` with reduced starting conditions. A small orbital station with limited modules (basic refinery, basic assembler, basic solar array, sensor array, 1 lab, maintenance bay), 1 ship, modest starting balance ($50-100M), limited starting inventory (500 kg Fe, 10 repair kits, some H2O), scan sites only in near-homeworld zone. Enough to bootstrap the first mining → refining → manufacturing loop, but not enough to coast.
- **ProgressionState** in `GameState` — current phase, completed milestones, grant history, reputation score. Evaluated every tick (after research, before events). Content-driven milestone definitions in `content/milestones.json`.
- **Milestone system** — conditions checked against game state metrics (ore processed > X, techs unlocked > Y, ships built > Z, balance > $W). Uses the existing event system pattern: content-defined conditions, composable reward effects. Milestone reached → emit `Event::MilestoneReached`, apply rewards (grant money, unlock modules, open zones).
- **Grant/contract economy** — milestone grants as primary early-game income. Tiered availability based on reputation (start with "prove yourself" contracts, grow into "expand your operation" contracts). Advance payments for some contracts (KSP pattern — money to fund the mission that earns the money). Recurring contracts available mid-game (deliver X kg Fe/month).
- **Achievement-gated trade** — replace `trade_unlock_delay_minutes` (currently 525,600 = 1 year time gate) with milestone-based unlock: "First Successful Export" milestone requires ore processing + material production. Import/export gates separated (imports available earlier for bootstrapping, exports require proven production).
- **Phase tracking** — not hard gates but observable state derived from milestones completed. Phases are descriptive labels ("Startup," "Orbital," "Industrial," "Expansion," "Deep Space") driven by which milestones are complete. UI shows current phase prominently.
- **Scenario integration** — sim_bench scenarios specify starting state file (`--state progression_start.json` vs `--state dev_advanced_state.json`). New `scenarios/progression/` directory for progression-specific scenarios. Existing scenarios updated to reference `dev_advanced_state.json` explicitly.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| Hard phase gates vs. soft milestones | Soft milestones with hard capability gates on specific systems (e.g., `required_tech` on modules/recipes) | Per design spine: "No arbitrary caps." Gates should feel like natural consequences, not walls. A player who finds a creative way to accelerate should be rewarded, not blocked. |
| Grant amounts | ~$10M for first milestone, scaling to ~$100M for mid-game milestones | KSP career mode calibration: grants should fund the next 2-3 steps, not the next 20. |
| Reputation system | Simple numeric score incremented by milestone completion | Over-engineering risk: reputation is a single number that gates contract tier availability. Not a faction system. YAGNI. |
| Starting balance | $50-100M (tuned via scoring comparison with dev_advanced_state) | Enough for ~500 ticks of basic operations (crew salary, module imports, some Fe imports) but not enough to buy everything. The scoring system (Project 0) lets us compare score trajectories between starting states. |
| dev_base_state rename | `dev_base_state.json` → `dev_advanced_state.json`, all references updated | Clear naming: "advanced" = fully equipped dev sandbox. "progression" = the real starting experience. No functionality change, just naming clarity. |

**Dependencies:** Project 0 (scoring — needed to measure whether the new starting state produces better/more interesting runs).

**Estimated tickets:** 12-15

**Critical risk:** Starting state deadlock — the player MUST be able to reach the first milestone from the starting state without external help. Validate by running 100+ seed sim_bench scenarios of the first 500 ticks. (See learning: gameplay-deadlock-missing-starting-equipment.md)

**AI development checkpoint:** After this project, run scoring comparison: progression_start vs dev_advanced_state across 100 seeds. The progression start should show a rising score curve (starting low, growing as capabilities unlock). The advanced state should show a flat-high curve (everything available from tick 0). If the progression curve doesn't rise, the milestone/grant pacing needs tuning.

---

### Project 2: Ground Operations & Telescope Scanning

**What it delivers:** The early game experience — a ground-based operations center with telescopes, basic manufacturing, and the mechanics for launching payloads to orbit.

**Why it matters:** This is the "first hour" of the game. If the player starts with a fully equipped orbital station (current behavior), there is no sense of earning orbital capability. The ground phase creates the "bootstrapping from nothing" arc that makes later capabilities feel earned.

**Scope (~10-14 tickets):**

- **Ground station concept** — a `StationState` with a `base_environment: Surface` tag (new field on `StationState`). Surface stations have different constraints: cannot mine asteroids (no ships dock for mining runs), have telescope modules instead of sensor arrays, basic manufacturing only (small workshop, not full assembler). Positioned at `parent_body: "homeworld", radius_au_um: 0` (surface).
- **Telescope/observatory modules** — new `ModuleBehaviorDef` variant or specialized `SensorArray` config. Operates from surface to discover scan sites in near space. Lower data yield than orbital sensor arrays. Generates `SurveyData` at reduced rate (ground-based observation limitations). Can characterize basic asteroid properties (size, rough composition estimate) without requiring ships.
- **Ground workshop** — small-scale assembler limited to basic recipes (components, repair kits, satellite parts). Lower throughput than orbital assemblers. Sufficient for early-game bootstrapping.
- **Launch mechanics** — "launching" payloads from surface to orbit is a cost, not physics. `Command::Launch { station_id, payload }` consumes propellant + money, creates the payload at an orbital position. Satellites are cheap to launch (small mass). Station kits are expensive (large mass). The launch cost formula abstracts delta-v into a simple mass × cost_per_kg calculation.
- **Orbital transition** — first orbital outpost deployed via "station kit launch." Player manufactures a station core (assembler recipe) at ground station, then launches it to orbit. This creates a new empty `StationState` at `earth_orbit_zone`. The station starts with zero modules — player must launch or import modules to equip it.
- **Homeworld naming** — solar_system.json body names stay ("Earth", "Luna", etc.) for familiarity, but the player's company and starting base have generic names. The starting station is "Ground Operations Center," not "Earth Base."
- **Content: near-space scan sites** — initial scan sites only in `earth_orbit_zone` and `earth_neos`. Belt/outer system sites not available at start (gated by progression milestone that "authorizes" belt operations, which triggers scan site replenishment in those zones).
- **Content: early-game modules** — telescope array, ground workshop, small solar panel, ground storage (cheap but limited capacity). All with `required_tech: null` for immediate availability.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| Surface base as new entity type vs. tagged station | Tagged `StationState` with `base_environment: Surface` | KISS — avoids a parallel entity hierarchy. The engine treats it as a station with different available modules. Surface-specific constraints enforced by content (module `compatible_environments` field). |
| Ships at surface stations | Ships cannot be assigned to surface stations for mining ops. Ships spawn at orbital stations only. | This creates the key progression gate: you MUST get to orbit before you can mine. Ground operations are observation + manufacturing + launching. |
| Launch cost model | `cost = mass_kg × launch_rate_per_kg` where `launch_rate_per_kg` is a content constant ($500/kg initially, reducible by tech). Plus propellant cost. | Simple, tunable, content-driven. No physics simulation. The design spine says "no heavy physics." |
| How long should ground phase last? | ~200-500 ticks (~8-20 game-days) with active play | Long enough to learn the systems (manufacturing, research, economy) but not so long that it feels like busywork. Calibrate via sim_bench + playtesting. KSP career mode ground phase is ~2 hours of gameplay. |

**Dependencies:** Project 1 (progression engine — milestone system needed for grants that fund orbital transition). Partially parallelizable — ground station mechanics are independent of milestone conditions.

**Estimated tickets:** 10-14

**Critical risk:** Ground phase boredom — if the player can only watch telescopes scan and workshops slowly assemble, the phase drags. Mitigation: ensure multiple activities are happening in parallel (telescope discovering sites, workshop building satellite parts, research producing early unlocks). The ground phase should feel busy, not idle.

---

### Project 3: Deep Tech Tree & Research Expansion

**What it delivers:** A 60+ tech tree across 6 tiers that provides meaningful progression gating throughout the entire game. Research becomes a core strategic activity, not a trivial side-effect.

**Why it matters:** The current 11-tech tree completes in ~1000 ticks. After that, research is irrelevant. A deep tech tree makes every research decision impactful and creates genuine specialization tradeoffs (invest in mining efficiency vs. manufacturing speed vs. propulsion range).

**Scope (~10-14 tickets):**

- **Tier structure** — 6 tiers with escalating domain point requirements. Each tier roughly doubles the requirement of the previous. Tier 1 techs unlock basic capabilities (telescope operation, basic refining). Tier 6 techs are end-game (nuclear power, advanced automation, deep space operations).
- **New research domains** — expand from 4 to 5-6 domains. Add: `Engineering` (station construction, structural), `Astrobiology` (future crew/life support). Each domain has dedicated lab types and data sources.
- **Tech tree topology** — move from the current flat tree (most techs have 0-1 prereqs) to a proper DAG with depth. Prerequisites form chains: `basic_refining → advanced_refining → precision_metallurgy → exotic_alloys`. Cross-domain requirements at higher tiers force breadth (tier 4+ techs require points in 3+ domains).

  **Approximate tier distribution:**

  | Tier | Techs | Domain Pts Required | Game Phase | Example Unlocks |
  |------|-------|-------------------|------------|-----------------|
  | 1 | 8-10 | 5-20 pts/domain | Ground Ops | Telescope operation, basic manufacturing, satellite deployment |
  | 2 | 10-12 | 20-50 pts/domain | Orbital | Deep scan, refining, ship construction, solar efficiency |
  | 3 | 10-12 | 50-120 pts/domain | Industrial | Ship specialization, smelting, advanced assemblers, automation basics |
  | 4 | 10-12 | 120-250 pts/domain | Belt Expansion | Station construction, belt transit, propulsion efficiency, advanced manufacturing |
  | 5 | 8-10 | 250-500 pts/domain | Deep Space | Nuclear power, outer system ops, crew automation, closed-loop life support |
  | 6 | 5-8 | 500-1000 pts/domain | Mastery | Repeatable techs (+5% per level), exotic materials, megastructure blueprints |

- **Module/recipe gating** — every non-starter module gets a `required_tech`. Starter modules (telescope, ground workshop, basic solar) have `required_tech: null`. Advanced modules (orbital refinery, smelter, shipyard) require tier 2-3 techs. End-game modules require tier 4-5.
- **Recipe gating** — basic recipes available from start (crude refining, simple assembly). Advanced recipes (`required_tech` on `RecipeDef`) unlock better yield, lower waste, new products. Alternative recipes for the same output create meaningful choice (basic Fe smelting at 60% yield vs. advanced at 85% yield with tier 3 tech).
- **Research pacing rebalance** — data generation rates and lab throughput need rebalancing for 60+ techs. Current rate of ~0.04 points/tick means tier 1 techs (20 pts) take ~500 ticks. Tier 6 techs (1000 pts) would take ~25,000 ticks (~2.8 years game-time). This may need adjustment: more labs, better data generation at scale, or domain-specific acceleration techs.
- **Repeatable techs (tier 6)** — infinite +5% bonuses (per Stellaris pattern): mining rate, processing yield, ship speed, power efficiency, wear reduction. Cost doubles per level. Creates infinite optimization target for late-game.
- **Tech tree content file** — expand `content/techs.json` from 11 entries to 60+. Each entry follows existing schema (id, name, prereqs, domain_requirements, accepted_data, effects). No engine changes needed for adding techs — just content.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| Research pacing approach | Scale data generation with infrastructure (more labs + more sensor arrays = faster research) + add "research acceleration" techs at tier 3-4 | Per design spine: "Actions generate evidence." More infrastructure → more activity → more data → faster research. Avoids arbitrary speed boosts. |
| Tech tree designed all at once vs. incrementally | Incrementally — design tier 1-2 first, validate pacing, then tier 3-4, etc. | Per design spine: "Systems expand one at a time. Never stack 3 new entropy sources at once." Also per learning: short scenarios hide problems. |
| Domain expansion (4 → 6) | Add Engineering domain in first pass, defer Astrobiology to crew system project | Engineering is needed for station construction gating. Astrobiology is only useful once crew system has life support. |
| Specialization vs. breadth requirement | Higher tier techs require points in multiple domains (cross-domain requirements) | Per Factorio pattern: every science pack tier requires ALL previous packs. This prevents tunnel-vision and forces the player to develop all domains. |

**Dependencies:** Project 1 (milestone system for grant-funded research). Can be partially parallelized — tech content can be drafted while the progression engine is being built.

**Estimated tickets:** 10-14

**Critical risk:** Research stall at higher tiers — if data generation doesn't scale with infrastructure, the player hits a wall where they've built everything they can but can't generate enough data to unlock the next tier. Mitigation: "research acceleration" techs and infrastructure-scaling data generation. Validate with sim_bench runs at 10,000+ tick horizons. (See learning: research evidence accumulates at ~0.04 pts/tick with current setup.)

---

### Project 4: Satellite & Unmanned Operations

**What it delivers:** A new entity type (satellites) that creates the bridge between ground observation and crewed orbital operations. Satellites are cheap, unmanned, persistent orbital assets that provide passive bonuses.

**Why it matters:** Satellites fill the gap between "look at space with a telescope" and "send humans to space." They are the first things the player puts in orbit — cheaper than a station, proving capability, providing real value (scan data, communication, navigation). This is the KSP probe-before-crew progression pattern.

**Scope (~8-12 tickets):**

- **SatelliteState** — new entity in `GameState.satellites: BTreeMap<SatelliteId, SatelliteState>`. Fields: id, position, satellite_type, deployed_tick, wear (degrades over time), enabled. Satellites are persistent — they stay where deployed until they wear out or are decommissioned.
- **Satellite types** (content-defined via `content/satellite_defs.json`):

  | Type | Mechanical Effect | Game Phase |
  |------|------------------|------------|
  | Survey Satellite | Passive scan site discovery in deployed zone (like remote sensor array) | Early (Act 1-2) |
  | Communication Relay | Enables trade/command operations beyond homeworld orbit (range extender) | Early-Mid (Act 2) |
  | Navigation Beacon | Reduces travel time for ships in deployed zone (-10-20% transit ticks) | Mid (Act 3) |
  | Science Platform | Passive research data generation in zone (like orbital lab) | Mid (Act 3) |
  | Early Warning | Detects incoming events (solar flares, comets) with advance notice | Late (Act 4+) |

- **Satellite manufacturing** — satellites are assembled by the ground workshop or orbital assembler. Recipes produce satellite components, final assembly creates a deployable satellite item (`InventoryItem::Satellite`). Survey satellite recipe: simple (Fe plates + circuits + solar cell). Science platform: complex (circuits + optics + data recorder).
- **Satellite deployment** — `Command::DeploySatellite { station_id, satellite_def_id, target_position }`. From ground station: uses launch mechanics (costs money/propellant). From orbital station: direct deployment (cheaper). Creates `SatelliteState` at target position.
- **Satellite tick behavior** — new tick step after station modules, before research advancement. Each satellite executes its type-specific behavior: survey sats discover scan sites, comm relays extend command range, nav beacons apply travel modifiers, science platforms generate data, early warning monitors event conditions.
- **Satellite wear & replacement** — satellites degrade over time (slow wear accumulation). No maintenance — when worn out, they fail and need replacement. Creates ongoing manufacturing demand. Wear rate content-defined per satellite type.
- **Autopilot satellite management** — `StationAgent` gets a new sub-concern: `manage_satellites`. Evaluates coverage needs (no survey satellite in a zone with scan sites → deploy one), handles replacement of worn-out satellites, prioritizes deployment by zone importance.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| Satellite as entity vs. module | New entity type (`SatelliteState`) | Satellites are not attached to stations — they are independent orbital assets with their own position and lifecycle. Modules are station-bound. |
| Satellite deployment from ground | Via launch command with mass-based cost | Creates a meaningful progression step: "I built a satellite AND launched it to orbit." The launch is the accomplishment. |
| Communication range gating | Satellites enable operations in their zone. Without a comm relay, no trade/export commands work in a zone. | Creates a natural "claim zones by deploying infrastructure" progression. You must invest before you can exploit. Per EVE pattern: infrastructure enables activity in a region. |
| How many satellite types at launch | 3 (Survey, Communication, Navigation). Science + Early Warning in later expansion. | YAGNI — start with the types that affect core progression mechanics. Science platforms overlap with orbital sensor arrays. Early warning needs events system maturity. |

**Dependencies:** Project 1 (progression engine), Project 2 (ground ops — launch mechanics). Partially parallelizable with Project 2 — satellite entity design is independent of ground station mechanics.

**Estimated tickets:** 8-12

**Critical risk:** Satellite system feels like busywork if deploying them is tedious. Mitigation: autopilot handles deployment automatically once manufacturing is running. The player's decision is strategic (which zones to cover), not tactical (manually deploying each satellite).

---

### Project 5: Station Construction & Multi-Station Expansion

**What it delivers:** The ability to build new stations anywhere in the solar system, creating multi-station supply chains and spatial expansion gameplay. This is where the simulation becomes a logistics and supply chain management game.

**Why it matters:** The current game is limited to a single station. Without station construction, there is no spatial expansion, no supply chains, no fleet logistics — the entire mid-to-late game does not exist. Station construction is the gateway to belt operations, deep space, and solar system mastery.

**Scope (~12-16 tickets):**

- **Station frames** (from existing design: `docs/brainstorms/station-frames-requirements.md`) — `FrameId` newtype, `FrameDef` in `content/frame_defs.json`. Initial frames: Outpost (4-6 slots, cheap), Industrial Hub (8-12 slots, balanced), Research Station (frame bonus to research). Station frames use the same slot/modifier architecture as ship hulls (already designed).
- **Station kit manufacturing** — assembler recipes that produce station deployment kits (`InventoryItem::Component` with station-kit component types). Kit recipes: `station_kit_outpost` (Fe plates + structural beams + solar cells + circuits), `station_kit_industrial` (requires advanced manufacturing). Kits are large, heavy items.
- **Station deployment** — `Command::DeployStation { ship_id, frame_id, position }` or `TaskKind::DeployStation`. A ship carries a station kit to the target location and deploys it. Creates a new empty `StationState` with the specified frame. The station starts with zero modules — must be equipped by ferrying modules from another station.
- **Station module delivery** — ships can carry station modules in cargo and deliver them to another station. New task type or existing deposit mechanics extended. The autopilot needs to handle: "Station B needs a refinery module. Ship picks up module from Station A, transits to Station B, deposits."
- **Inter-station logistics** — `ShipObjective::Transfer { from_station, to_station, cargo_spec }`. Ships ferry materials between stations. Autopilot `FleetCoordinator` (new, above station agents) identifies supply/demand imbalances and assigns transfer missions. This is the biggest engine change — the current autopilot has no cross-station coordination.
- **Zone-gated resources** — scan site replenishment gated by zone access milestones. Belt scan sites only appear after "Belt Authorization" milestone (which requires comm relay deployed in belt zone). Creates the "invest infrastructure before exploiting resources" pattern.
- **Station specialization** — frame bonuses and zone resources create natural specialization. Mining outpost in the belt (close to asteroids, frame bonus to ore processing) + refinery hub in Earth orbit (close to market, better solar power) + research station anywhere (frame bonus to lab throughput).
- **Supply chain visualization** — UI panel showing material flow between stations: production rates, transfer rates, inventory levels per station, bottleneck indicators. Critical for the player to understand their supply chain.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| Station deployment requires ship or can be launched from ground | Ship deployment for remote stations, ground launch for Earth orbit stations | Earth orbit stations can be launched (costly but possible). Belt stations require ships carrying kits. This creates the natural progression: ground → orbit (launch) → belt (ship deployment). |
| Fleet coordination architecture | New `FleetCoordinator` agent above `StationAgent` layer in sim_control | The current per-station agent model has no mechanism for cross-station decisions. A coordinator agent evaluates global supply/demand and assigns inter-station transfer objectives to ships. |
| Empty station bootstrapping | Station starts with zero modules and zero crew. Player must equip via ship deliveries. | This makes station construction a meaningful multi-step process, not a one-click operation. It creates logistics gameplay: "I need to ship 8 modules and a crew team to my new belt outpost." |
| How many stations should be typical late-game | 2-4 stations (not 20+) | Per design spine: complexity should create strategic tradeoffs, not busywork. Managing 2-4 specialized stations is interesting. Managing 20 identical ones is tedious. |

**Dependencies:** Project 1 (milestones for zone access), Project 3 (techs for station construction), Project 2 (launch mechanics for first orbital station). This is a mid-game system — it can be developed in parallel with earlier projects but tested end-to-end only after Projects 1-3 deliver.

**Estimated tickets:** 12-16

**Critical risk:** Inter-station logistics deadlock — building a belt station requires materials FROM the belt (or ferrying from Earth orbit, which requires long transit times and enough propellant for round trips). Mitigation: "station seed kit" includes enough starting supplies for ~500 ticks of operation. First few module deliveries funded by the "belt expansion" grant. Validate with 5000+ tick sim_bench scenarios.

---

### Project 6: AI Intelligence & Optimization

**What it delivers:** An autopilot that can navigate the full progression arc — making strategic decisions about what to research, what to build, when to expand. A classical optimization loop that discovers better strategies than hand-tuned defaults. Trained scoring models running at tick speed in Rust.

**Why it matters:** The game is observed, not played. The autopilot IS the player. By this point in the roadmap, the game has scoring (Project 0), progression gating (Project 1), a deep tech tree (Project 3), satellites (Project 4), and multi-station operations (Project 5). The AI needs to handle all of this intelligently. This project is the capstone that ties the AI development checkpoints from every previous project into a coherent, optimizable system.

**Scope (~12-16 tickets):**

- **Full AutopilotConfig schema** — evolves the baseline config from Project 0 into a comprehensive strategy specification: priority weights (mining vs. manufacturing vs. research), expansion thresholds (when to build new station), fleet composition targets (miners:haulers:scouts ratio), research domain priorities, manufacturing recipe preferences. sim_bench can override config per scenario.
- **Progression-aware agent behaviors** — `StationAgent` and `ShipAgent` updated to handle progression phases:
  - Startup phase: optimize for first milestones, manage limited resources carefully
  - Orbital phase: install modules, begin mining loop, manage first fleet
  - Industrial phase: scale production, begin exports, diversify fleet
  - Expansion phase: evaluate station construction, assign logistics routes
  - Each phase activates different sub-concerns. Phase transition detected by reading `ProgressionState`.
- **Classical optimization loop** — Python script that generates N `AutopilotConfig` variants, runs each across M seeds via sim_bench, ranks by composite score. Builds on the early scaffolding from Project 0 but with full decision space: `scipy.optimize.minimize` or Bayesian optimization over 15-20 config parameters.
- **Trained scoring models in Rust** — XGBoost/LightGBM trained on scoring data from Projects 0-5, weights exported for Rust inference. Decision tree evaluation in `sim_core` for microsecond-speed tactical scoring (which asteroid to mine, which recipe to run, when to expand). Per existing AI roadmap Phase 4.
- **Knowledge system maturity** — extend MCP advisor to understand progression state. `get_metrics_digest` includes progression phase, milestones, score trajectory. `query_knowledge` can filter by game phase. `save_run_journal` captures phase-specific observations. `update_playbook` accumulates phase-specific strategies.
- **Progression regression tests** — sim_bench scenarios that serve as AI quality gates: "Can the autopilot reach Industrial phase from progression_start within 5000 ticks?" "What's the optimal research domain priority for reaching tier 3 techs fastest?" These run in CI and prevent AI regressions.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| AutopilotConfig complexity | Start with ~10-15 key parameters, expand as decision space grows | Per existing roadmap: "Design for scipy.optimize first — the LLM interface will be a superset later." Keep config small enough for grid search. |
| Scoring granularity | Compute every 24 ticks (1 game-day), persist rolling window | Fine enough for trend detection, coarse enough for performance. Aligns with existing metrics_every=24. |
| Phase-specific autopilot behaviors | Phase detected from ProgressionState, enables/disables sub-concerns | The autopilot does not need to handle ground-phase logic during expansion-phase. Phase awareness simplifies per-phase behavior design. |
| When to train first real ML model | After Project 1 and Project 3 deliver (enough decision space) | Per existing roadmap: "Content depth is the rate limiter." Without 60+ techs and multiple phases, the decision space is too flat for ML to find interesting patterns. |

**Dependencies:** Project 1 (progression state for phase detection), Project 3 (deep tech tree for decision space). The scoring system can be built early (Project 1 timeframe) since it just reads game state. AI behavior improvements are incremental — each project adds new autopilot capabilities.

**Estimated tickets:** 12-16

**Critical risk:** The AutopilotConfig schema is "the crux" (per existing roadmap). Too rigid → optimizer can't explore. Too loose → search space explodes. Mitigation: start with priority weights and thresholds (well-understood optimization targets), expand to structural decisions (template selection, expansion timing) only after the optimization loop is proven.

---

### Project 7: Planetary Bases & Deep Space Operations

**What it delivers:** End-game content depth — planetary surface bases, nuclear/RTG power for the outer solar system, long-range fleet logistics, advanced manufacturing chains, and the final milestones of solar system mastery.

**Why it matters:** Without end-game content, the simulation "finishes" when the player has 2-3 stations in the belt. Planetary bases (Mars, Luna) and deep space operations (Jupiter Trojans) provide the aspirational goals that keep the simulation interesting after the initial progression is complete.

**Scope (~8-12 tickets):**

- **Planetary surface bases** — reuse the `base_environment: Surface` tag from Project 2. New planetary surface zones in solar_system.json for Mars and Luna. Each planet has unique resource availability and environmental constraints (Mars: CO2 atmosphere → carbon extraction, Luna: Helium-3 → fusion fuel candidate, low gravity → cheaper launches).
- **Nuclear/RTG power modules** — new module types unlocked by tier 5 tech. Required for operations beyond the belt where solar intensity drops below useful levels (Jupiter at 4% solar). Content: `module_nuclear_reactor` (high power, fuel consumption, requires enriched uranium from belt mining), `module_rtg` (low power but zero fuel, expensive to manufacture, for satellites/outposts).
- **Deep space logistics** — long-range fleet operations with multi-hop transit. Ships refuel at relay stations. The propellant system already creates natural range limits — nuclear-powered ships (tier 5 tech) have extended range. Communication relay satellites (Project 4) required for command authority in deep space zones.
- **Advanced manufacturing chains** — tier 4-5 products requiring 5-6 production steps. Examples: nuclear fuel rods (uranium ore → enriched uranium → fuel pellet → fuel rod assembly), advanced ship hulls (exotic alloys → precision components → hull segment → hull assembly). Creates the deep production planning gameplay.
- **End-game milestones** — "Mars Landing," "Lunar Base Operational," "Jupiter Trojan Survey," "Self-Sustaining Outer System," "Solar System Industrialist" (all 5 bodies with permanent bases).
- **Solar system map expansion** — add Mars orbit zone, Luna surface zone, Jupiter orbit zone to solar_system.json. Add nav graph nodes and edges. Each zone has unique resource classes and environmental constraints.

**Key decisions:**

| Decision | Recommendation | Rationale |
|---|---|---|
| Planetary bases as new vs. reused system | Reuse surface station from Project 2 with planet-specific content | KISS — a Mars surface base is mechanically identical to the homeworld ground station, just with different available resources and environmental modifiers (solar intensity, gravity). |
| Nuclear power complexity | Simple fuel consumption model (fuel rods per tick, like repair kits for maintenance) | Per design spine: "No heavy physics." Nuclear is a powerful, expensive power source. It's not a reactor simulation. |
| How far into the solar system | Jupiter Trojans as the edge (5.2 AU). No Saturn/Uranus/Neptune. | Diminishing returns — each additional planet is more content work for less gameplay variety. Jupiter Trojans are already in solar_system.json. |
| End-game "win condition" | No hard win. Infinite repeatable techs + "Space Magnate" rating. The sim runs forever with increasing optimization challenge. | Per design spine: "Replayability emerges from procgen variation, research variance, different scaling strategies." A win condition would end the simulation. |

**Dependencies:** Project 3 (tier 5 techs), Project 5 (station construction — planetary bases ARE stations), Project 4 (comm relays for deep space command). This is the final project chronologically.

**Estimated tickets:** 8-12

**Critical risk:** End-game content that nobody reaches. If the progression arc to this point takes 30,000+ ticks (~3.4 years game-time at mpt=60), only dedicated players will see this content. Mitigation: sim_bench scenarios that fast-forward to late-game state, allowing balance tuning of end-game content independently. Also: the time scale may need compression for later phases (more game-minutes per tick, or faster transit).

---

## Dependency Graph & Execution Order

**Recommended execution order:** 0 → 1 → 3 → 2 → 4 → 5 → 6 → 7

Each project is built one at a time with its own planning cycle. The order above reflects dependencies AND the principle that measurement and gating mechanics (scoring, starting state, tech tree) should precede new content (ground ops, satellites, stations).

```
Project 0: Scoring & Measurement ─────────────────────────────────┐
    │                                                              │
    Project 1: Starting State & Progression Engine                 │
    │   (needs: 0 for measuring starting state quality)            │
    │                                                              │
    ├── Project 3: Deep Tech Tree                                  │
    │       (needs: 1 for milestone gating)                        │
    │                                                              │
    ├── Project 2: Ground Operations & Telescopes                  │
    │       │   (needs: 1 for milestones, 3 for tech gating)      │
    │       │                                                      │
    │       ├── Project 4: Satellite & Unmanned Operations         │
    │       │       (needs: 2 for launch mechanics)                │
    │       │                                                      │
    │       Project 5: Station Construction & Multi-Station         │
    │       │   (needs: 1, 2-launch, 3-techs, 4-comms)            │
    │       │                                                      │
    │       └── Project 7: Planetary Bases & Deep Space            │
    │              (needs: 3-tier5 techs, 4-comms, 5-stations)    │
    │                                                              │
    └── Project 6: AI Intelligence & Optimization                  │
           (needs: 0-scoring, evolves with each project)           │
```

**AI development is continuous, not a single project.** Every project has an "AI checkpoint" — after shipping, run scoring comparison to measure AI decision quality on the new content. Project 6 is the capstone that ties the AI learning loop together (AutopilotConfig, optimization, phase-aware decisions), but AI measurement happens from Project 0 onward.

**Critical path:** 0 → 1 → 3 → 5 → 7 (scoring → starting state → tech gating → station construction → end-game)

## Cross-Cutting Concerns

### Codebase Readiness

The existing roadmap (`docs/plans/2026-03-23-code-quality-and-ai-progression-roadmap.md`) identified 18 code quality tickets (VIO-402 through VIO-419) that directly impact progression work:

| Ticket | Impact on Progression |
|---|---|
| **VIO-402** (Metrics derive macro) | Every new system (satellites, milestones, scoring) needs metrics. Currently 12 edits per field. Blocking for Project 0 scoring and all subsequent projects. |
| **VIO-403** (Test fixture builders) | Every project adds tests. Currently 30-50 lines of boilerplate per test. Blocking for velocity. |
| **VIO-407** (ModuleBehaviorDef factory) | Projects 2 and 7 add new module types (telescope, nuclear reactor). Currently 12 files / 150 lines per new module type. |
| **VIO-412** (Content-driven autopilot) | Project 6 needs configurable autopilot. Currently 30+ hardcoded content IDs. Blocking for AI intelligence work. |
| **VIO-406** (Constants override via serde) | Progression tuning requires rapid constant adjustment. Currently 200-line manual match. |

**Recommendation:** Complete VIO-402, VIO-403, VIO-407, and VIO-412 before starting Project 2. These refactors cut the "add a new system" cost from ~2000 lines to ~800 lines and directly enable the content-driven progression model. VIO-406 should be done before serious balance tuning begins.

### AI Development Strategy

The AI develops **in parallel with the sim**, not as a separate late-stage effort. Each project has both a sim-side deliverable and an AI-side checkpoint:

| Project | Sim Deliverable | AI Development |
|---|---|---|
| **0: Scoring** | Scoring dimensions, UI display | Baseline AutopilotConfig extraction, optimization scaffolding, data gap detection, AI evaluation framework |
| **1: Starting State** | progression_start.json, milestones, grants | Score curve comparison: progression vs advanced start. First data on "can the AI bootstrap from nothing?" |
| **2: Ground Ops** | Telescopes, launch, surface base | Autopilot ground-phase behavior. "Does the AI efficiently transition to orbit?" |
| **3: Tech Tree** | 60+ techs, research pacing | Research prioritization quality. "Does the AI pick the right techs?" Optimization over research domain weights. |
| **4: Satellites** | SatelliteState, deployment | Satellite strategy. "Does the AI deploy satellites to the right zones?" |
| **5: Stations** | Multi-station, logistics | FleetCoordinator, supply chain optimization. "Does the AI build stations at the right time/place?" |
| **6: AI Capstone** | — | Full optimization loop, trained Rust models, phase-aware autopilot, CI regression tests |
| **7: Deep Space** | Planetary bases, nuclear power | Deep-space expansion strategy. End-game optimization. |

**The key insight:** by the time we reach Project 6, the optimization loop has been running informally since Project 0 — comparing configs, measuring scores, identifying AI deficiencies. Project 6 formalizes and automates what we've been doing manually. The trained models reflect 5 projects worth of accumulated training data and scoring calibration.

**Data pipeline grows with each project:**
- Project 0: scoring dimensions in Parquet, basic cross-seed comparison
- Project 1: milestone completion timing in Parquet, phase transition tracking
- Project 3: research path analysis (which domain priorities produce fastest progression)
- Project 5: logistics metrics (transfer throughput, supply chain efficiency)
- Project 6: full feature set for ML training (50+ columns from all systems)

### Backward Compatibility & State File Strategy

Two starting states, clearly named:

| File | Purpose | When to use |
|---|---|---|
| `content/dev_advanced_state.json` | Fully equipped station, all modules, $1B balance. **Renamed from dev_base_state.json** in Project 1. | Development, debugging, testing individual systems in isolation |
| `content/progression_start.json` | Minimal station, limited modules, $50-100M balance. **New in Project 1.** | Progression gameplay, sim_bench scoring scenarios, AI evaluation |

- `sim_cli` default: `--state progression_start.json` (progression is the real game)
- `sim_cli --state dev_advanced_state.json` for dev sandbox
- sim_bench scenarios explicitly specify which state file
- `build_initial_state()` in sim_world stays as-is (used by scenarios that don't specify `--state`)
- Existing test fixtures unaffected (they construct their own state)

### Balance Validation Strategy

Each project must validate its progression arc via sim_bench before merging:

| Project | Validation Scenario | Key Metric |
|---|---|---|
| 1 | Fresh start → first milestone within 200 ticks | Milestone completion tick |
| 2 | Ground phase → orbital station within 500 ticks | Phase transition tick |
| 3 | Full tech tree completion within 30,000 ticks | Research pacing curve |
| 4 | Satellite network → full zone coverage within 1000 ticks | Coverage percentage |
| 5 | Dual-station supply chain stable for 5000 ticks | Transfer throughput |
| 6 | Optimized config vs. default: >15% score improvement | Composite score delta |
| 7 | Outer system self-sustaining within 20,000 ticks | Revenue/expense ratio |

### Event Sync (FE)

Per CLAUDE.md: every new `Event` variant in `sim_core/src/types.rs` must have a handler in `ui_web/src/hooks/applyEvents.ts`. New events from this work:
- `MilestoneReached { milestone_id, rewards }`
- `PhaseAdvanced { from, to }`
- `GrantAwarded { milestone_id, amount }`
- `SatelliteDeployed { satellite_id, position }`
- `SatelliteFailed { satellite_id, reason }`
- `StationDeployed { station_id, frame_id, position }`
- `PayloadLaunched { station_id, payload, cost }`

## Risk Analysis

### High Risk

**Progression deadlock** — player cannot reach a milestone because the starting state lacks something required.
- *Likelihood:* High (this has happened before — see `docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md`)
- *Impact:* Game-breaking
- *Mitigation:* 100-seed sim_bench validation for each project. Trace every dependency chain from milestone condition back to starting state. Automated regression test: "can the autopilot reach milestone N from fresh start?"

**Ground phase boredom** — the ground phase is too slow/limited, players disengage.
- *Likelihood:* Medium
- *Impact:* High (if the first 30 minutes are boring, nobody continues)
- *Mitigation:* Multiple parallel activities during ground phase. Quick first milestone (~100-200 ticks). Visual feedback (telescope discoveries, manufacturing progress). Calibrate via Chrome-based playtesting.

**Balance at scale** — 60+ techs, 7 phases, 4+ station types, satellites, grants — the parameter space is enormous.
- *Likelihood:* High
- *Impact:* Medium (fixable with tuning, but time-consuming)
- *Mitigation:* Classical optimization loop (Project 6) automates balance exploration. Start with tier 1-2 only, validate, expand. Per design spine: "introduce one pressure system, observe, tune, introduce next."

### Medium Risk

**Scope creep** — 80-100 tickets is a massive body of work.
- *Likelihood:* High
- *Impact:* Medium (each project delivers standalone value)
- *Mitigation:* Each project is independently valuable. Project 1 alone (progression engine + new starting state) transforms the game. Projects can be deprioritized without invalidating completed work.

**AI cannot progress** — the autopilot fails to navigate the progression system.
- *Likelihood:* Medium (the autopilot currently handles a fully-equipped station; a fresh start with limited resources is a different problem)
- *Impact:* High (the game is unplayable if the AI cannot progress)
- *Mitigation:* Phase-specific autopilot behaviors (Project 6). Each phase has simplified decision-making. Validate with sim_bench: "autopilot reaches Phase N within X ticks." If the autopilot struggles, add grant-funded "jumpstart" packages at each phase.

**Performance at scale** — multi-station, 10+ ships, satellites, deep tech tree may slow tick rate.
- *Likelihood:* Low-Medium (current: ~435K TPS)
- *Impact:* Medium (ML training needs fast ticks)
- *Mitigation:* Profile after Project 5 (multi-station is the biggest perf risk). Current perf profiling infrastructure exists (samply, TickTimings).

### Low Risk

**Content staleness** — 60+ techs and multiple phase-specific content files become maintenance burden.
- *Likelihood:* Low (content-driven architecture is well-established)
- *Mitigation:* All content is JSON. Schema validation catches errors. sim_bench regression tests catch gameplay impact.

## Estimated Total Scope

| Project | Tickets | Key Deliverable | AI Component |
|---|---|---|---|
| 0. Scoring & Measurement | 10-14 | Run scoring, AI evaluation framework, baseline config, optimization scaffolding | Foundation — scoring + data pipeline + first optimization loop |
| 1. Starting State & Progression | 12-15 | dev_advanced_state split, milestone grants, achievement-gated trade | Checkpoint — score curve comparison between starting states |
| 2. Ground Operations & Telescopes | 10-14 | Surface base, telescopes, launch mechanics | Checkpoint — autopilot handles ground-phase decisions |
| 3. Deep Tech Tree | 10-14 | 60+ techs, 6 tiers, module/recipe gating | Checkpoint — autopilot research prioritization quality |
| 4. Satellite System | 8-12 | SatelliteState entity, 3 types, deployment | Checkpoint — autopilot satellite deployment strategy |
| 5. Station Construction | 12-16 | Station frames, kits, inter-station logistics | Checkpoint — autopilot multi-station coordination |
| 6. AI Intelligence & Optimization | 12-16 | Full AutopilotConfig, phase-aware AI, trained Rust models | Capstone — optimization loop, trained models, regression tests |
| 7. Planetary Bases & Deep Space | 8-12 | Mars/Luna bases, nuclear power, end-game content | Checkpoint — autopilot deep-space expansion strategy |
| **Total** | **82-113** | **Full early-to-late game progression** | **AI develops continuously across all projects** |

**Prerequisite refactors** (from existing code quality tickets): VIO-402, VIO-403, VIO-407, VIO-412, VIO-406 (~5 tickets, already in Linear).

**Timeline context:** At the current velocity (~2-3 tickets/day with AI-assisted development), the full roadmap is ~5-10 weeks of focused work. Projects 0-1-3 (scoring + starting state + tech gating) could deliver in ~3-4 weeks and fundamentally transform the game.

## Sources & References

### Internal
- `docs/DESIGN_SPINE.md` — authoritative design philosophy
- `docs/plans/2026-03-23-code-quality-and-ai-progression-roadmap.md` — AI progression phases, code quality audit
- `docs/brainstorms/entity-depth-requirements.md` — hull+slot architecture, station frames
- `docs/brainstorms/manufacturing-dag-requirements.md` — production chain depth
- `docs/brainstorms/ai-knowledge-system-requirements.md` — ML pipeline, 3-layer AI architecture
- `docs/brainstorms/station-frames-requirements.md` — station frame + slot system
- `docs/brainstorms/population-workers-requirements.md` — crew system, automation progression
- `docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md` — critical learning on starting state validation
- `docs/solutions/patterns/stat-modifier-tech-expansion.md` — tech effect implementation pattern
- `docs/solutions/patterns/content-driven-event-engine.md` — event system architecture for milestones
- `docs/BALANCE.md` — baseline progression timing (first asteroid 2.1 days, first tech 2.5 days)
- `content/knowledge/playbook.md` — gameplay strategy patterns

### External (Game Design Research)
- **Factorio** — Science pack production-as-gate system, qualitative complexity jumps per tier
- **Stellaris** — Parallel resource tracks (minerals → alloys), influence as expansion bottleneck, tech card weighted draws
- **Dwarf Fortress** — Cascading industry chains, magma as transformative mid-game unlock, immigration scaling with wealth
- **KSP Career Mode** — Grant/contract system with advance payments, reputation gating, building tier upgrades
- **RimWorld** — Wealth-scaled difficulty, emergent progression without formal milestones
- **EVE Online** — Real-time skill training gates, security-space geographic progression, T2 production chain depth
- **Space Engineers** — Resource geography forcing orbital progression (uranium only in space), block unlock tiers
- **X4: Foundations** — Economic empire phases (pilot → fleet → station → empire), station blueprint research
- **ONI** — Biome gating, survival pressure forcing innovation, tiered research buildings
- **Civilization** — Multi-dimensional scoring, named achievement thresholds
