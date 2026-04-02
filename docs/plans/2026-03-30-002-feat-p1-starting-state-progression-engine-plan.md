---
title: "feat: P1 — Starting State & Progression Engine"
type: feat
status: active
date: 2026-03-30
origin: docs/plans/2026-03-28-003-feat-game-progression-system-plan.md
---

# P1: Starting State & Progression Engine

## Overview

The mechanical infrastructure for progression — a milestone/grant economy, achievement-gated trade, phase tracking, and critically, the split of `dev_base_state.json` into a proper progression starting state and an advanced development sandbox. This transforms the simulation from "start with everything, watch it run" into "earn every capability through industrial milestones."

**What changes for the player (autopilot):**
- Starts with $50-100M instead of $1B
- Starts with 6 basic modules instead of 21
- Starts with limited inventory and near-homeworld scan sites only
- Earns money through milestone grants, not just trade
- Unlocks trade through achievement, not waiting 365 game-days
- Sees phase progression: Startup → Orbital → Industrial → Expansion → Deep Space

**What changes for the measurement loop (from P0):**
- P0 scoring (VIO-519 through VIO-529) measures run quality
- P1 creates two starting states to compare with that scoring
- `progression_start.json` should show a **rising score curve** (starting low, growing as milestones unlock capabilities)
- `dev_advanced_state.json` shows a **flat-high curve** (everything available from tick 0)
- If the progression curve doesn't rise, the milestone/grant pacing needs tuning — the scoring system (P0) provides the signal

## Problem Statement

The simulation currently has no progression. The starting state (formerly `dev_base_state.json`, now `dev_advanced_state.json`) provides:

| Resource | Current Value | Problem |
|---|---|---|
| Balance | $1,000,000,000 | 200+ years of runway at current crew costs. Money is never a constraint. |
| Modules | 21 installed + 19 spare | Nothing to earn or unlock — full industrial capability from tick 0 |
| Crew | 21 (11 operators, 2 pilots, 4 technicians, 4 scientists) | Full staffing, no recruitment decisions |
| Ships | 1 (fully fitted) | Fleet expansion is trivial with $1B |
| Scan sites | 10 across all zones (belt, NEOs, Jupiter) | Full solar system access from tick 0 |
| Trade | Unlocked after 8760 ticks (365 game-days of waiting) | Waiting is the "strategy" — not achievement |

**Result:** The autopilot efficiently runs a pre-built industrial complex. There is no arc of: struggle → discovery → capability → mastery. The scoring system (P0) will measure this flat-high pattern and confirm what we already know — the current start isn't a game.

## Proposed Solution

### Milestone System

Content-driven milestones with conditions checked against game state. Each milestone has:
- **Conditions** — predicates evaluated against `GameState` + `MetricsSnapshot` (e.g., `techs_unlocked >= 1`, `total_material_kg >= 500.0`)
- **Rewards** — grants (money), module unlocks, zone access, trade permissions
- **Events** — `MilestoneReached`, `GrantAwarded` emitted for UI and logging

### First 8 Milestones (Draft)

| # | Milestone | Condition | Grant | Unlock Effect |
|---|-----------|-----------|-------|---------------|
| 1 | First Survey | asteroids_discovered >= 1 | $5M | — |
| 2 | First Ore | total_ore_kg >= 100 | $10M | — |
| 3 | First Refined Material | total_material_kg >= 50 | $15M | Enable basic imports |
| 4 | First Component | assembler runs >= 1 (from counters) | $20M | — |
| 5 | First Tech Unlocked | techs_unlocked >= 1 | $25M | — |
| 6 | First Export | export_count >= 1 | $30M | — |
| 7 | First Ship Constructed | ships built (from counters) >= 2 | $50M | — |
| 8 | Self-Sustaining Economy | export_revenue_total > crew_salary_cumulative for 100 ticks | $100M | Full trade + belt zone access |

**Pacing target:** Milestone 1 within ~100 ticks. Milestone 5 within ~500 ticks. Milestone 8 within ~2000 ticks. Calibrated via sim_bench.

**Design patterns:**
- **KSP Career Mode** — grants fund the mission that earns the next grant. Advance payments for expensive operations.
- **Dwarf Fortress Embark** — limited budget forces meaningful starting choices. Not punishing, just focused.
- **X4: Foundations** — starting credits fund first mining ship, income funds first station.

### Progression Starting State

`content/progression_start.json` — minimal but functional:

| Resource | Progression Start | Rationale |
|---|---|---|
| Balance | $75,000,000 | ~500 ticks of crew salary + basic imports. Not enough to coast. |
| Station modules | 6: basic_solar_array (x2), sensor_array, exploration_lab, basic_iron_refinery, maintenance_bay | Minimum for: power → scan → research → refine → maintain loop |
| Crew | 8: 4 operators, 1 pilot, 2 technicians, 1 scientist | Minimum for module operation + 1 ship |
| Ships | 1: general_purpose hull with mining_laser + cargo_expander + propellant_tank | Same as current — can mine from tick 0 |
| Inventory | 300 kg Fe, 3000 kg H2O, 5 repair_kits | Bootstrap materials + propellant + initial maintenance |
| Scan sites | 3-4 in earth_neos + earth_orbit_zone only | Near-homeworld only. Belt/outer sites gated by milestone |

**Crew salary burn rate at progression start:**
- 4 operators × $25/hr + 1 pilot × $40/hr + 2 technicians × $37.50/hr + 1 scientist × $50/hr = $265/tick (at mpt=60)
- $75M / $265 = ~283,019 ticks runway from salary alone (~32 years game-time)
- But imports, recruitment, and ship operations also cost money — real runway is shorter
- First grant ($5M at ~100 ticks) extends runway before costs ramp up

**Critical validation:** Every dependency chain from milestone conditions back to starting state must be traceable:

```
Milestone 1 (First Survey):
  asteroids_discovered >= 1
  ← ship surveys scan site
  ← scan sites exist in starting state ✓
  ← ship exists in starting state ✓
  ← sensor_array installed (autopilot does this from inventory) ✓

Milestone 3 (First Refined Material):
  total_material_kg >= 50
  ← refinery processes ore
  ← basic_iron_refinery in starting inventory ✓
  ← ore deposited from mining
  ← ship mined asteroid (needs composition from deep scan OR survey)
  ← deep scan needs tech_deep_scan_v1
  ← tech needs exploration domain points
  ← exploration_lab in starting inventory ✓
  ← lab needs raw data from sensor_array ✓

Milestone 6 (First Export):
  export_count >= 1
  ← export command accepted
  ← trade unlocked (milestone-gated, not time-gated)
  ← milestone 3 (First Refined Material) must unlock basic trade ✓
```

(See `docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md` for past deadlock incident)

### Achievement-Gated Trade

Replace the current time-based trade gate (`trade_unlock_delay_minutes: 525600` = 365 game-days) with milestone-based gating:

| Trade Type | Current Gate | New Gate |
|---|---|---|
| Basic imports | tick >= 8760 | Milestone 3 (First Refined Material) — "proved you can process, now you can buy" |
| Exports | tick >= 8760 | Milestone 3 (First Refined Material) — same gate, split later if needed |
| Full trade (all items) | tick >= 8760 | Milestone 8 (Self-Sustaining Economy) — "proved economic viability" |

**Implementation:** Replace `trade_unlock_tick()` check with `progression_state.is_milestone_completed("first_refined_material")` (or equivalent). The `StationContext.trade_unlocked` flag still exists — its computation changes from tick-based to milestone-based.

**Backward compatibility:** `dev_advanced_state.json` starts with all milestones completed (or trade_unlocked = true by default), so existing behavior is preserved for development.

### Phase Tracking

Phases are **descriptive labels**, not hard gates. Derived from which milestones are completed:

| Phase | Derived From | UI Display |
|---|---|---|
| Startup | Default (no milestones) | "Startup Phase" |
| Orbital | Milestone 2 (First Ore) completed | "Orbital Operations" |
| Industrial | Milestone 5 (First Tech) completed | "Industrial Phase" |
| Expansion | Milestone 8 (Self-Sustaining) completed | "Expansion Phase" |
| Deep Space | (P2+ milestones) | "Deep Space Operations" |

Phases are observable state, not gatekeepers. They drive UI display, autopilot behavior hints, and scoring context. Per design spine: "No arbitrary caps."

## Technical Approach

### Architecture

```
content/milestones.json (NEW)
  ├── milestones: [{id, name, conditions, rewards, phase_advance}]
  └── trade_gates: {basic_import: "milestone_id", export: "milestone_id", full: "milestone_id"}

sim_core/src/types/progression.rs (NEW)
  ├── ProgressionState {completed_milestones, phase, grant_history, reputation}
  ├── MilestoneDef {id, name, conditions, rewards}
  ├── MilestoneCondition enum {MetricThreshold, CounterThreshold, ...}
  ├── MilestoneReward {grant_amount, unlock_modules, unlock_zones, unlock_trade_tier}
  └── Phase enum {Startup, Orbital, Industrial, Expansion, DeepSpace}

sim_core/src/progression.rs (NEW)
  ├── evaluate_milestones(&GameState, &[MilestoneDef], &MetricsSnapshot) -> Vec<MilestoneId>
  └── apply_milestone_rewards(&mut GameState, &MilestoneDef) -> Vec<Event>

sim_world/src/lib.rs (EXTEND)
  ├── load milestones.json into GameContent
  └── build_initial_state() unchanged (used by scenarios without --state)

content/dev_advanced_state.json (RENAMED from dev_base_state.json)
content/progression_start.json (NEW)
```

### Tick Integration

Milestone evaluation slots into the existing tick cycle after research advancement (step 4) and before scan site replenishment (step 5):

```
1. Apply commands
2. Resolve ship tasks
3. Tick station modules (processors, assemblers, sensors, labs, maintenance, thermal, boiloff)
4. Advance research
4.5 Evaluate milestones (NEW) — check conditions, apply rewards, emit events
5. Replenish scan sites
6. Increment tick
```

**Why after research:** Milestones may check `techs_unlocked`, which changes in step 4. Evaluating after research ensures the milestone fires on the same tick the tech unlocks.

### Trade Gating Refactor

Current enforcement points (6 locations):

| Location | Current Check | New Check |
|---|---|---|
| `sim_core/src/commands.rs:347` | `current_tick < trade_unlock_tick()` | `!progression_state.trade_tier_unlocked(ImportBasic)` |
| `sim_core/src/commands.rs:436` | `current_tick < trade_unlock_tick()` | `!progression_state.trade_tier_unlocked(Export)` |
| `sim_control/src/agents/station_agent.rs:969` | `state.meta.tick >= trade_unlock_tick()` | `state.progression.trade_tier_unlocked(ImportBasic)` |
| `sim_control/src/agents/station_agent.rs:1102` | Same | Same pattern |
| `sim_control/src/agents/station_agent.rs:445` | `ctx.trade_unlocked` | Unchanged (ctx computation changes) |
| `sim_control/src/agents/station_agent.rs:559,711` | `ctx.trade_unlocked` | Unchanged |

**StationContext.trade_unlocked** computation changes from:
```rust
let trade_unlocked = state.meta.tick >= trade_unlock_tick(constants);
```
to:
```rust
let trade_unlocked = state.progression.trade_tier_unlocked(TradeTier::Basic);
```

**Backward compatibility:** When `ProgressionState` is `None` (legacy states, dev_advanced_state), default to trade_unlocked = true. The `trade_unlock_delay_minutes` constant remains as a fallback for states without progression.

## Implementation Tickets

### Ticket 1: dev_base_state → dev_advanced_state rename

**What:** Rename `content/dev_base_state.json` to `content/dev_advanced_state.json`. Update ALL references across the codebase.

**Details:**
- Rename the file
- Update all scenario JSON files that reference `"state": "./content/dev_base_state.json"`
- Update all Rust test code that references the path
- Update CLI default state path (if hardcoded)
- Update all docs that reference dev_base_state
- Git mv for clean history

**Files to update (from research — 53 references found):**
- `scenarios/*.json` (state field)
- `crates/sim_bench/src/runner.rs`
- `crates/sim_control/tests/progression.rs`
- `crates/sim_bench/tests/*.rs`
- `crates/sim_world/tests/content_validation.rs`
- `docs/plans/*.md`, `docs/reference.md`, `CLAUDE.md`

**Acceptance criteria:**
- [ ] `content/dev_base_state.json` no longer exists
- [ ] `content/dev_advanced_state.json` has identical content
- [ ] Zero references to "dev_base_state" in codebase (`grep -r "dev_base_state"` returns empty)
- [ ] All tests pass (`cargo test`)
- [ ] All scenarios still runnable
- [ ] CI passes

**Dependencies:** None — can start immediately
**Estimated size:** Small (mechanical rename, no logic changes)

---

### Ticket 2: Milestone content schema and MilestoneDef types

**What:** Define `content/milestones.json` schema and corresponding Rust types. Load milestone definitions as part of `GameContent` in sim_world.

**Details:**
- `MilestoneDef` struct: id, name, description, conditions (Vec<MilestoneCondition>), rewards (MilestoneReward), phase_advance (Option<Phase>)
- `MilestoneCondition` enum: `MetricAbove { field, threshold }`, `CounterAbove { counter, threshold }`, `MilestoneCompleted { milestone_id }` (for chaining)
- `MilestoneReward` struct: grant_amount (f64), unlock_trade_tier (Option<TradeTier>), unlock_zone_ids (Vec<String>), unlock_module_ids (Vec<String>)
- `TradeTier` enum: `None`, `BasicImport`, `Export`, `Full`
- Content loading in `sim_world::load_content()` — add `milestones: Vec<MilestoneDef>` to `GameContent`
- Validation: milestone IDs unique, condition fields reference valid metric names, chained milestones reference existing IDs
- Draft first 8 milestones in `content/milestones.json`

**Acceptance criteria:**
- [ ] `content/milestones.json` exists with 8 milestones
- [ ] `MilestoneDef`, `MilestoneCondition`, `MilestoneReward` types compile and serde correctly
- [ ] `load_content()` loads milestones without error
- [ ] Unit test: valid milestones load successfully
- [ ] Unit test: invalid milestone (unknown condition field) rejected
- [ ] Schema documented in `docs/reference.md`

**Dependencies:** None — can start immediately (parallel with ticket 1)
**Estimated size:** Medium

---

### Ticket 3: ProgressionState in GameState + phase tracking

**What:** Add `ProgressionState` struct to `GameState`. Track completed milestones, current phase, grant history, and trade tier.

**Details:**
- `ProgressionState` struct:
  ```rust
  pub struct ProgressionState {
      pub completed_milestones: BTreeSet<String>,  // milestone IDs
      pub phase: Phase,
      pub grant_history: Vec<GrantRecord>,  // {milestone_id, amount, tick}
      pub reputation: f64,
      pub trade_tier: TradeTier,
  }
  ```
- `Phase` enum: `Startup`, `Orbital`, `Industrial`, `Expansion`, `DeepSpace`
- Add `pub progression: ProgressionState` to `GameState` (not Option — always present, defaults to empty/Startup)
- Serialization: `ProgressionState` must round-trip through JSON (for save/load and state files)
- `dev_advanced_state.json` gets a progression block with all milestones completed + Full trade tier (preserving current behavior)
- `ProgressionState::default()` = empty milestones, Startup phase, None trade tier
- Helper methods: `is_milestone_completed(&str) -> bool`, `trade_tier_unlocked(TradeTier) -> bool`

**Acceptance criteria:**
- [ ] `ProgressionState` added to `GameState` with default
- [ ] Serializes/deserializes correctly (existing state files load with default progression)
- [ ] `dev_advanced_state.json` includes progression block (all milestones, full trade)
- [ ] Helper methods work correctly
- [ ] Unit test: default state has no milestones, Startup phase
- [ ] Unit test: state with completed milestones round-trips through JSON
- [ ] All existing tests pass (backward compatibility)

**Dependencies:** None — can start immediately (parallel with tickets 1, 2)
**Estimated size:** Medium

---

### Ticket 4: Milestone evaluation engine

**What:** Pure function that evaluates milestone conditions against current game state. Called after research advancement in the tick cycle. Returns newly-completed milestone IDs.

**Details:**
- `evaluate_milestones(state: &GameState, milestones: &[MilestoneDef], metrics: &MetricsSnapshot) -> Vec<MilestoneId>`
- For each uncompleted milestone: check all conditions. If all conditions met, milestone is newly completed.
- Conditions evaluate against `MetricsSnapshot` fields (using `get_field_f64()`) and `GameState` counters
- `MilestoneCondition::MilestoneCompleted` checks `state.progression.completed_milestones`
- Evaluation order: milestones sorted by ID for determinism (important for RNG-dependent side effects)
- Integration into tick cycle: new step 4.5 between `advance_research()` and `replenish_scan_sites()`
- Newly completed milestones added to `state.progression.completed_milestones`
- Phase updated based on milestone definitions with `phase_advance` field

**Acceptance criteria:**
- [ ] `evaluate_milestones()` is a pure function (no side effects beyond return value)
- [ ] Milestones only fire once (completed_milestones set prevents re-triggering)
- [ ] Multiple milestones can complete on the same tick
- [ ] Chained milestones (condition: MilestoneCompleted) work within same tick if dependency completes first
- [ ] Unit test: known state with metrics meeting conditions triggers milestone
- [ ] Unit test: already-completed milestone does not re-trigger
- [ ] Unit test: chained milestone triggers when dependency completes
- [ ] Integration test: milestone fires during tick cycle at correct step (after research, before replenish)

**Dependencies:** Ticket 2 (MilestoneDef types), Ticket 3 (ProgressionState)
**Estimated size:** Medium

---

### Ticket 5: Milestone rewards + grant economy

**What:** Apply milestone rewards when milestones complete. Grant money to balance, unlock trade tiers, unlock modules, unlock zones.

**Details:**
- `apply_milestone_rewards(state: &mut GameState, milestone: &MilestoneDef) -> Vec<Event>`
- **Grant:** Add `reward.grant_amount` to `state.balance`. Record in `state.progression.grant_history`.
- **Trade tier:** Set `state.progression.trade_tier` to `max(current, reward.unlock_trade_tier)`
- **Module unlock:** Add module IDs to an unlock set (future: gating module installation by unlock status)
- **Zone unlock:** Trigger scan site replenishment in newly unlocked zones (belt, outer system)
- **Reputation:** Increment `state.progression.reputation` by milestone-specific amount
- Emit `Event::GrantAwarded { milestone_id, amount }` for each grant

**Acceptance criteria:**
- [ ] Grant money added to balance when milestone completes
- [ ] Grant history recorded with milestone_id, amount, tick
- [ ] Trade tier advances correctly (only upgrades, never downgrades)
- [ ] Zone unlock triggers scan site replenishment in the unlocked zone
- [ ] Unit test: grant adds correct amount to balance
- [ ] Unit test: trade tier advances from None → BasicImport → Export → Full
- [ ] Integration test with real content: milestone 1 fires and grants $5M

**Dependencies:** Ticket 4 (evaluation engine)
**Estimated size:** Medium

---

### Ticket 6: Achievement-gated trade — replace time gate

**What:** Replace `trade_unlock_delay_minutes` time-based trade gate with milestone-based gating from `ProgressionState.trade_tier`.

**Details:**
- **6 enforcement points** to update:
  - `sim_core/src/commands.rs:347` (import command rejection)
  - `sim_core/src/commands.rs:436` (export command rejection)
  - `sim_control/src/agents/station_agent.rs:969` (StationContext.trade_unlocked computation)
  - `sim_control/src/agents/station_agent.rs:1102` (same)
  - Concern guards at station_agent.rs:445, 559, 711 (unchanged — they read ctx.trade_unlocked)
- `StationContext.trade_unlocked` computation: `state.progression.trade_tier_unlocked(TradeTier::BasicImport)`
- **Backward compatibility:** If progression state has no milestones but state loaded from `dev_advanced_state.json` (which has full trade tier), trade is unlocked. If loaded from old format without progression block, default to checking `trade_unlock_delay_minutes` as fallback.
- `trade_unlock_delay_minutes` constant remains in constants.json as fallback (set to 0 in progression mode, or removed later)
- Update existing trade tests to use milestone-based gating

**Acceptance criteria:**
- [ ] Import commands rejected when trade tier is None, accepted when BasicImport+
- [ ] Export commands rejected when trade tier below Export
- [ ] `dev_advanced_state.json` (full trade tier) allows trade from tick 0
- [ ] `progression_start.json` (no trade tier) blocks trade until milestone unlocks it
- [ ] All existing trade tests updated and passing
- [ ] New test: milestone completion unlocks trade within same tick
- [ ] StationAgent concerns correctly gated by new trade_unlocked computation

**Dependencies:** Ticket 3 (ProgressionState with trade_tier), Ticket 5 (rewards set trade_tier)
**Estimated size:** Medium

---

### Ticket 7: progression_start.json — minimal starting state

**What:** Create the progression starting state file with minimal but functional resources. Validate every dependency chain from milestones back to starting equipment.

**Details:**
- **Station:** `station_earth_orbit` with 6 modules in inventory:
  - `module_basic_solar_array` (x2) — power
  - `module_sensor_array` — asteroid discovery + data generation
  - `module_exploration_lab` — research domain points for deep scan tech
  - `module_basic_iron_refinery` — ore processing
  - `module_maintenance_bay` — wear management
- **Crew:** 8 total — 4 operators, 1 pilot, 2 technicians, 1 scientist
- **Ship:** 1x `ship_0001` with hull_general_purpose, mining_laser, cargo_expander, propellant_tank (same as current)
- **Balance:** $75,000,000
- **Inventory:** 300 kg Fe (quality 0.7), 3000 kg H2O (quality 1.0), 5x repair_kit
- **Scan sites:** 3-4 in earth_neos + earth_orbit_zone only (belt/outer gated by milestones)
- **Progression:** Default (no milestones, Startup phase, no trade tier)
- **Dependency chain validation:** Document every milestone's dependency chain traced back to starting state (as shown in Proposed Solution section)

**Missing from progression start vs dev_advanced_state:**
- No assembler (must be imported or unlocked) — OR include basic_assembler for component manufacturing
- No shipyard (must unlock via tech + trade)
- No extra labs (only exploration — materials/engineering earned later)
- No extra ships (must construct via tech_ship_construction)
- Limited scan sites (no belt access)

**Decision needed during implementation:** Include basic_assembler or not? Including it enables Milestone 4 (First Component) without trade. Excluding it creates a trade dependency for assembler import, which requires Milestone 3 first. **Recommendation:** Include it — avoids potential deadlock and the assembler is a basic capability.

**Acceptance criteria:**
- [ ] `content/progression_start.json` is valid and loads without error
- [ ] Autopilot installs all 6 (or 7) modules within first 10 ticks
- [ ] sim_cli runs from progression_start without crash for 2000+ ticks
- [ ] Every milestone dependency chain documented and validated
- [ ] No gameplay deadlock: autopilot can reach milestone 1 from fresh start (tested with 20 seeds)
- [ ] Balance doesn't hit zero before first grant (validated with sim_bench)

**Dependencies:** Ticket 1 (rename done first — clear naming), Ticket 5 (grants exist for testing)
**Estimated size:** Medium

---

### Ticket 8: Progression events + FE handlers

**What:** New Event variants for milestone system. Add handlers in `applyEvents.ts` per event sync rule.

**Details:**
- New Event variants in `sim_core/src/types/events.rs`:
  - `MilestoneReached { milestone_id: String, milestone_name: String }`
  - `PhaseAdvanced { from_phase: String, to_phase: String }`
  - `GrantAwarded { milestone_id: String, amount: f64 }`
- Emitted from `apply_milestone_rewards()` (Ticket 5)
- **FE handlers** in `ui_web/src/hooks/applyEvents.ts`:
  - `MilestoneReached` — could display notification toast, update progression panel
  - `PhaseAdvanced` — update phase display
  - `GrantAwarded` — could flash balance change
- Update `scripts/ci_event_sync.sh` to include new variants
- **Note:** Full UI progression panel is NOT in P1 scope — just the event handlers. A dedicated progression panel comes with P2+ or as a separate UI ticket.

**Acceptance criteria:**
- [ ] 3 new Event variants compile and serialize
- [ ] Events emitted at correct tick step (after milestone evaluation)
- [ ] `applyEvents.ts` handles all 3 variants (at minimum: console log + state update)
- [ ] `scripts/ci_event_sync.sh` passes
- [ ] SSE stream includes milestone events
- [ ] Unit test: milestone completion emits correct events

**Dependencies:** Ticket 5 (reward application emits events)
**Estimated size:** Small-Medium

---

### Ticket 9: Autopilot progression awareness

**What:** Update `StationAgent` and `ShipAgent` to handle limited starting conditions in progression mode. The autopilot must work with 6-7 modules and constrained resources, not just the 21-module fully-equipped station.

**Details:**
- **Current assumption:** StationAgent assumes all module types are available. With progression start, many modules (shipyard, extra labs, smelter, assembler variants) are absent.
- **Concerns to audit:**
  - `ModuleConcern` — already handles missing modules gracefully (skips if not in inventory)
  - `LabConcern` — only assigns labs that exist. With 1 lab, assigns to highest-priority domain.
  - `ShipFittingConcern` — needs `tech_ship_construction` which won't be available early. Already gated by tech check.
  - `ThrusterImport` / `ImportConcern` — gated by `trade_unlocked`. Won't fire until milestone unlocks trade.
  - `ExportConcern` — same trade gate.
  - `PropellantConcern` — needs H2O. Starting inventory has 3000 kg. Must ensure ship can refuel.
- **Key risk:** Ship runs out of propellant before first mining cycle completes (especially if scan sites are far). Validate transit distances for near-homeworld scan sites.
- **Phase-specific hints (lightweight):** Read `state.progression.phase` to adjust priorities. During Startup: prioritize mining + research over exports. During Industrial: begin export focus. **Keep this minimal in P1** — full phase-aware autopilot is P6.

**Acceptance criteria:**
- [ ] Autopilot runs from progression_start.json without panics or infinite loops
- [ ] Autopilot installs available modules, doesn't crash on missing module types
- [ ] Ship successfully mines and deposits ore within first 100 ticks
- [ ] Lab assigns to exploration domain (only lab available)
- [ ] No import/export attempts before trade is unlocked
- [ ] Integration test: autopilot + progression_start reaches milestone 1 within 200 ticks

**Dependencies:** Ticket 7 (progression_start.json exists to test against)
**Estimated size:** Medium (mostly validation + targeted fixes)

---

### Ticket 10: sim_bench progression scenarios

**What:** sim_bench scenarios that validate the progression system works end-to-end. Run as part of CI.

**Details:**
- **`scenarios/progression_bootstrap.json`** — runs progression_start.json for 500 ticks across 20 seeds. Validates: autopilot reaches milestone 1 (First Survey) in all seeds, reaches milestone 2 (First Ore) in 90%+ seeds, balance never hits zero.
- **`scenarios/progression_economy.json`** — runs progression_start.json for 2000 ticks across 20 seeds. Validates: milestone 3-5 reached, grants received, trade unlocked, balance trend positive after grants.
- **`scenarios/progression_deadlock.json`** — runs progression_start.json for 5000 ticks across 100 seeds. Validates: NO seed has a deadlocked state (defined as: zero ore mined + zero techs unlocked after 1000 ticks). This is the anti-deadlock regression test.
- Update `ci_bench_smoke.sh` to include a quick progression check (5 seeds, 500 ticks)

**Acceptance criteria:**
- [ ] `progression_bootstrap.json` passes (milestone 1 in 100% of seeds)
- [ ] `progression_economy.json` passes (milestones 3-5, positive balance)
- [ ] `progression_deadlock.json` passes (zero deadlocked seeds across 100 seeds)
- [ ] CI smoke includes progression validation
- [ ] Scenarios documented in `docs/reference.md`

**Dependencies:** Ticket 7 (progression_start.json), Ticket 4 (milestone evaluation), Ticket 9 (autopilot works with progression start)
**Estimated size:** Medium

---

### Ticket 11: Score curve comparison — progression vs advanced

**What:** Use P0 scoring infrastructure to compare score trajectories between `progression_start.json` and `dev_advanced_state.json`. Validate that progression produces the expected rising curve.

**Details:**
- Use `sim_bench compare` (VIO-526) or regular sim_bench runs to generate score data for both starting states
- **Expected patterns:**
  - `progression_start`: Score starts low (Startup), rises as milestones unlock capabilities, should reach Contractor within 2000 ticks
  - `dev_advanced_state`: Score starts high (Enterprise+), stays relatively flat
- Create `scenarios/scoring_progression_compare.json` that runs both starting states
- Analysis script: plot score trajectories, identify inflection points where milestones fire
- This validates the core premise: "progression creates a more interesting score curve"

**Acceptance criteria:**
- [ ] Progression start produces rising score curve (monotonically increasing composite after tick 100)
- [ ] Dev advanced produces flat-high score curve
- [ ] Progression score at tick 2000 > progression score at tick 100 (growth confirmed)
- [ ] Score inflection points correlate with milestone completion ticks
- [ ] Comparison documented as baseline for P2+ tuning

**Dependencies:** P0 scoring (VIO-522 sim_bench scoring), Ticket 7 (progression_start.json), Ticket 10 (progression scenarios)
**Estimated size:** Small-Medium

---

### Ticket 12: 100-seed autopilot progression regression test

**What:** Automated regression test ensuring the autopilot can navigate the full progression from `progression_start.json` through all 8 milestones. Catches AI regressions and starting state deadlocks.

**Details:**
- Extend `crates/sim_control/tests/progression.rs` with a new test:
  - Loads real content + `progression_start.json`
  - Runs 100 seeds for 5000 ticks each (can be run with `--ignored` for CI due to runtime)
  - Asserts: every seed reaches milestone 1 by tick 200, milestone 3 by tick 500, milestone 5 by tick 1000
  - Reports: per-seed milestone completion ticks, mean/stddev, any failed seeds
- Separate quick test (5 seeds, 2000 ticks) for regular CI
- This test is the **primary defense against gameplay deadlock regressions**

**Acceptance criteria:**
- [ ] Quick test (5 seeds): passes in regular `cargo test`
- [ ] Full test (100 seeds): passes with `--ignored` flag
- [ ] 100% of seeds reach milestone 1 by tick 200
- [ ] 95%+ of seeds reach milestone 3 by tick 500
- [ ] 90%+ of seeds reach milestone 5 by tick 1000
- [ ] Test reports per-seed statistics for debugging failures

**Dependencies:** Ticket 9 (autopilot progression awareness), Ticket 10 (scenarios validate the same things at sim_bench level)
**Estimated size:** Medium

---

## Dependency Graph

```
Ticket 1: dev_base_state rename ─────────────────────────────────────┐
                                                                      │
Ticket 2: Milestone schema + types ──────────────────────────────┐   │
                                                                  │   │
Ticket 3: ProgressionState + phase ──────────────────────────┐   │   │
                                                              │   │   │
                                            Ticket 4: Evaluation engine
                                                 │                    │
                                            Ticket 5: Rewards + grants│
                                                 │         │          │
                                            Ticket 6:  Ticket 8:     │
                                            Trade gate  Events + FE  │
                                                 │                    │
                                            Ticket 7: progression_start.json
                                                 │
                                            Ticket 9: Autopilot awareness
                                                 │
                                            Ticket 10: Progression scenarios
                                                 │           │
                                            Ticket 11:  Ticket 12:
                                            Score compare  Regression test
```

**Critical path:** 2 → 4 → 5 → 7 → 9 → 10 → 12

**Parallelizable:**
- Stream A: 1 (rename) — independent, start immediately
- Stream B: 2 (schema) + 3 (state) — independent, start immediately
- Stream C: After 4 completes → 5, 6, 8 can all run in parallel
- Stream D: After 7 → 9, then 10, 11, 12

**Recommended execution order (single-threaded):**
1. Ticket 1 (rename — quick, clears the namespace)
2. Ticket 2 (milestone schema)
3. Ticket 3 (ProgressionState)
4. Ticket 4 (evaluation engine)
5. Ticket 5 (rewards + grants)
6. Ticket 6 (trade gate refactor)
7. Ticket 8 (events + FE handlers)
8. Ticket 7 (progression_start.json)
9. Ticket 9 (autopilot awareness)
10. Ticket 10 (progression scenarios)
11. Ticket 12 (regression test)
12. Ticket 11 (score comparison — needs P0 scoring to be complete)

## System-Wide Impact

### Interaction Graph

- `evaluate_milestones()` runs each tick (step 4.5) → reads GameState + MetricsSnapshot → writes ProgressionState → emits Events
- `apply_milestone_rewards()` → mutates `state.balance` (grants), `state.progression.trade_tier`, may trigger `replenish_scan_sites()` for zone unlocks
- Trade gating refactor → changes `StationContext.trade_unlocked` computation → affects 3 concerns (ThrusterImport, MaterialExport, ShipFitting)
- Events flow through SSE → `applyEvents.ts` → UI state updates
- ProgressionState persisted in save files → loaded by sim_cli, sim_daemon, sim_bench

### Error Propagation

- Milestone evaluation failure (invalid condition field): Should not happen if content validation catches it at load time. If it does, log warning and skip milestone (don't crash the tick).
- Grant application to negative balance: Balance can go negative (crew salary already allows this). Grant pulls balance positive. No special handling needed.
- Missing milestone definitions (old content dir): `milestones.json` is optional. If absent, `Vec::new()` — no milestones, no progression. Backward compatible.

### State Lifecycle Risks

- **Partial milestone completion:** Milestone evaluation + reward application happen in the same tick step. If the process is interrupted between evaluation and reward, the milestone would be marked complete but rewards not applied. **Mitigation:** Evaluate and apply atomically — rewards applied immediately after condition check, before moving to next milestone.
- **Save file migration:** Old save files without `progression` field will deserialize with `ProgressionState::default()`. This is safe — defaults to Startup phase, no milestones, no trade tier. For `dev_advanced_state.json`, explicitly set full progression in the file.

### API Surface Parity

- `GET /api/v1/snapshot` — includes `progression` field in GameState (additive)
- `GET /api/v1/advisor/digest` — could include phase + milestones (optional, not required for P1)
- SSE stream — 3 new event types (additive)
- MCP advisor — `get_metrics_digest` could include progression context (optional)
- sim_bench — scenarios specify state file explicitly (no API change)

### Integration Test Scenarios

1. **Bootstrap to first milestone:** Start from progression_start → autopilot runs → milestone 1 fires → grant received → balance increased
2. **Trade unlock via milestone:** Start with no trade → complete milestone 3 → trade becomes available → autopilot imports thrusters → ship built
3. **Save/load with progression:** Start run → complete 3 milestones → save → load → milestones still completed, phase correct, trade tier preserved
4. **Dev advanced backward compatibility:** Load dev_advanced_state → all trade unlocked → existing behavior identical
5. **Deadlock prevention:** 100-seed run from progression_start → every seed reaches milestone 3 within 500 ticks

## P0 ↔ P1 Integration

| P0 Delivers (VIO-519 through VIO-529) | P1 Uses It For |
|---|---|
| `compute_run_score()` | Comparing progression_start vs dev_advanced_state score curves (Ticket 11) |
| sim_bench scoring columns | Progression scenario validation includes score trajectory data |
| `GET /api/v1/score` | UI can show score alongside phase progression |
| Data gap detection | After P1: check if milestone/grant metrics create scoring blind spots |
| Optimization scaffolding | Tune milestone grant amounts, starting balance, scan site count via grid search |

**P1 adds scoring signals for P2+:**
- Milestone completion timing as a scoring dimension hint (Expansion dimension gets richer)
- Phase advancement rate
- Grant efficiency (grants received vs time)

## Acceptance Criteria

### Functional Requirements

- [ ] `progression_start.json` exists with minimal viable starting state
- [ ] `dev_advanced_state.json` preserves current behavior exactly
- [ ] 8 milestones defined in `content/milestones.json` with conditions and rewards
- [ ] Milestone evaluation runs each tick, fires when conditions met
- [ ] Grants add money to balance, recorded in history
- [ ] Trade gated by milestone achievement, not time
- [ ] Phase tracking reflects milestone completion
- [ ] 3 new Event variants emitted and handled in FE
- [ ] Autopilot navigates from progression_start through milestones

### Non-Functional Requirements

- [ ] Milestone evaluation < 0.1% tick overhead (simple condition checks)
- [ ] 100% of seeds reach milestone 1 from progression_start within 200 ticks
- [ ] Zero gameplay deadlocks across 100 seeds / 5000 ticks
- [ ] Backward compatible: old state files load correctly with default progression

### Quality Gates

- [ ] Every milestone dependency chain traced and documented
- [ ] Integration test with `load_content("../../content")` real content (not zero fixtures)
- [ ] sim_bench progression scenarios in CI (bootstrap + economy + deadlock)
- [ ] Score curve comparison validates rising progression pattern
- [ ] Event sync CI passes with 3 new variants

## Risk Analysis

### High Risk

**Gameplay deadlock** — autopilot cannot reach a milestone because starting state lacks something required.
- *Likelihood:* High (this has happened before — VIO-5/VIO-7, documented in `docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md`)
- *Impact:* Game-breaking
- *Mitigation:* Trace every dependency chain. 100-seed regression test (Ticket 12). sim_bench deadlock scenario (Ticket 10). Include basic_assembler in starting state to avoid import-dependency deadlock.

**Starting balance too low → bankruptcy before first grant**
- *Likelihood:* Medium
- *Impact:* High (simulation collapses)
- *Mitigation:* $75M gives ~283K ticks of salary-only runway. First grant at ~100 ticks adds $5M. sim_bench validates balance never hits zero in first 500 ticks. Balance is tunable via content.

### Medium Risk

**Trade gate refactor breaks existing behavior**
- *Likelihood:* Low-Medium (6 enforcement points, well-understood)
- *Impact:* High (trade is core economic activity)
- *Mitigation:* Backward compatibility via `ProgressionState::default()` + fallback to time gate. Existing trade tests updated.

**Milestone pacing wrong** — too fast (trivializes progression) or too slow (frustrating)
- *Likelihood:* Medium
- *Impact:* Medium (tunable via content, not code changes)
- *Mitigation:* All thresholds in content/milestones.json (not hardcoded). sim_bench scenarios measure pacing. P0 scoring provides quantitative feedback.

### Low Risk

**Performance impact of milestone evaluation**
- *Likelihood:* Very Low (8 milestones × simple condition checks)
- *Impact:* Negligible
- *Mitigation:* Skip evaluation for already-completed milestones. O(n) where n = uncompleted milestones.

## Sources & References

### Origin

- **Origin document:** [docs/plans/2026-03-28-003-feat-game-progression-system-plan.md](../../docs/plans/2026-03-28-003-feat-game-progression-system-plan.md) — Project 1 section (lines 162-197). Key decisions: soft milestones with hard capability gates, $50-100M starting balance, KSP-style grant economy, simple numeric reputation.

### Internal References

- `content/dev_base_state.json` (→ `dev_advanced_state.json`) — current starting state (21 modules, $1B, 21 crew)
- `crates/sim_core/src/engine.rs:9` — `trade_unlock_tick()` function (to be replaced)
- `crates/sim_core/src/commands.rs:347,436` — import/export trade gating (to be updated)
- `crates/sim_control/src/agents/station_agent.rs:969,1102` — StationContext.trade_unlocked (to be updated)
- `crates/sim_core/src/types/state.rs:21-52` — GameState struct (add ProgressionState)
- `crates/sim_core/src/types/events.rs` — Event enum (add 3 variants)
- `crates/sim_core/src/lib.rs:95` — `emit()` helper for events
- `crates/sim_world/src/lib.rs:578-677` — `load_content()` pattern for adding milestones.json
- `crates/sim_control/tests/progression.rs` — existing progression test (extend with milestone validation)
- `content/crew_roles.json` — salary rates (operator $25, technician $37.50, scientist $50, pilot $40)
- `docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md` — critical learning on deadlock prevention

### P0 Scoring Dependencies

- VIO-519: Scoring content schema and types
- VIO-521: compute_run_score() function
- VIO-522: sim_bench scoring integration (needed for Ticket 11 score comparison)
- VIO-526: AI evaluation framework (config comparison)

### Related Work

- VIO-503: Plan P1: Starting State & Progression Engine (this ticket's parent)
- VIO-502 design review comment: recommended iterative AutopilotConfig evolution across P0-P5
- VIO-403: Test fixture builders (recommended prerequisite — reduces test boilerplate)
- P0 plan: `docs/plans/2026-03-30-001-feat-p0-scoring-measurement-foundation-plan.md`
