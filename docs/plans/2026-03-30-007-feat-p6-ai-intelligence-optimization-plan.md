---
title: "feat: P6 — AI Intelligence & Optimization"
type: feat
status: active
date: 2026-03-30
origin: docs/plans/2026-03-28-003-feat-game-progression-system-plan.md
---

# P6: AI Intelligence & Optimization

## Overview

The AI capstone — an autopilot that can navigate the full progression arc from ground facility through multi-station empire. Absorbs the **Strategic Layer + Multi-Station AI** project (Phase C: StrategyConfig, Phase D remnants). Builds on P0's baseline AutopilotConfig and evaluation framework to create a full optimization loop: config → run → score → rank → better config.

**What P6 delivers:**
1. **StrategyConfig** — full strategic configuration layer controlling autopilot priorities, expansion timing, fleet composition targets (absorbs Strategic Layer Phase C)
2. **Progression-aware behaviors** — autopilot detects current phase and adjusts strategy (ground → orbital → industrial → expansion)
3. **Multi-station AI** — station-scoped assignment, multi-station dev state, scenarios (absorbs Phase D remnants)
4. **Classical optimization loop** — Bayesian optimization over StrategyConfig parameters via sim_bench
5. **Progression regression tests** — CI gates ensuring the autopilot can complete the full P0-P5 progression arc

**Absorbed project:** Strategic Layer + Multi-Station AI (VIO-479 to VIO-484 Phase C + VIO-485, VIO-487, VIO-489 Phase D). Mark standalone project as absorbed.

**NOT in this scope (deferred):** Trained ML models in Rust (XGBoost weight export). The original roadmap included this but it requires significantly more training data from P0-P5 runs. Defer to a future project when the optimization loop has generated enough data.

## Problem Statement

By P5, the simulation has: scoring (P0), milestones (P1), ground facilities + launch (P2), tech tree (P3), satellites (P4), and multi-station operations (P5). The autopilot must handle all of this — but currently it has no strategic layer. Each concern operates independently with hardcoded priorities. There's no way to:

- Tell the autopilot "prioritize research over manufacturing"
- Compare two strategies quantitatively across 100 seeds
- Detect that we're in the ground phase and should focus on sensors over mining
- Optimize expansion timing (when to build second station)
- Coordinate fleet composition targets (how many miners vs haulers vs construction ships)

## Proposed Solution

### 3 Milestones

| Milestone | Scope | Tickets |
|---|---|---|
| **M1: StrategyConfig** | Strategic layer types, rule interpreter, consumption by agents, sim_bench overrides, daemon/MCP integration (absorbs Phase C) | ~8 |
| **M2: Progression-Aware AI** | Phase detection, phase-specific priorities, multi-station state/scenarios (absorbs Phase D remnants) | ~6 |
| **M3: Optimization Loop** | Bayesian optimization, config comparison at scale, regression tests | ~5 |

### M1: StrategyConfig (absorbs Strategic Layer Phase C)

The strategic configuration layer from the existing Strategic Layer project, with 6 detailed tickets already designed (VIO-479 to VIO-484):

- **StrategyConfig struct** — mode (Balanced/Mining/Research/Expansion), priority weights for each concern, fleet_size_target, operational thresholds
- **Rule interpreter** — `evaluate_strategy()` pure function producing ConcernPriorities from config + game state
- **Agent consumption** — station agents read strategy priorities, FleetCoordinator uses fleet targets
- **sim_bench overrides** — `strategy.*` keys in scenario JSON
- **Daemon endpoints** — GET/POST /api/v1/strategy for runtime changes
- **MCP tools** — get_strategy_config, suggest_strategy_change

This is the foundation P0's AutopilotConfig baseline (VIO-520) evolves into. The baseline extracted hardcoded behavior; StrategyConfig makes it configurable and optimizable.

### M2: Progression-Aware AI

The autopilot detects which phase of the game it's in and adjusts strategy automatically:

| Phase | Detection | Strategy Adjustments |
|---|---|---|
| Ground | Only ground facilities, no orbital stations | Prioritize sensors → research → rocketry manufacturing → launch |
| Early Orbital | First orbital station exists, < 2 stations | Prioritize mining → refining → export revenue → satellite deployment |
| Industrial | 2+ stations, active logistics | Prioritize manufacturing → fleet expansion → supply chain optimization |
| Expansion | 3+ stations, belt operations active | Prioritize zone coverage → station specialization → efficiency |

**Phase detection** reads `ProgressionState.phase` (from P1) and game state signals (station count, fleet size, logistics activity). Strategy mode auto-adjusts unless manually overridden.

**Multi-station state + scenarios** (absorbs Phase D remnants):
- VIO-485: Multi-station dev_advanced_state (for testing multi-station AI)
- VIO-487: Station-scoped ship assignment + pre-partitioning
- VIO-489: Multi-station sim_bench regression scenarios

### M3: Optimization Loop

Builds on P0's evaluation framework (VIO-526, VIO-528) with more sophistication:

- **Bayesian optimization** — `scipy.optimize.minimize` or `optuna` over 15-20 StrategyConfig parameters
- **Parameter space definition** — which StrategyConfig fields are optimizable, their bounds, constraints
- **Large-scale comparison** — run 50-100 configs × 20 seeds each, rank by composite score
- **Progression regression tests** — CI gates: "autopilot reaches Industrial phase from ground_start within N ticks across 100 seeds"
- **Knowledge integration** — best configs saved to knowledge system, optimization history tracked

## Implementation Tickets

### Milestone 1: StrategyConfig (absorbs Phase C: VIO-479 to VIO-484)

#### Ticket 1 (VIO-479): StrategyConfig + PriorityWeights types

Already detailed. StrategyConfig on GameState, StrategyMode enum, PriorityWeights named struct, 9 operational thresholds.

**Existing ticket: VIO-479** → move to P6

---

#### Ticket 2 (VIO-480): Rule interpreter (evaluate_strategy)

Already detailed. Pure function: config + state → ConcernPriorities. State urgency normalized to [0, 1].

**Existing ticket: VIO-480** → move to P6

---

#### Ticket 3 (VIO-481): Wire strategic layer into AutopilotController

Already detailed. Updated execution order: evaluate strategy → cache priorities → station agents consume.

**Existing ticket: VIO-481** → move to P6

---

#### Ticket 4 (VIO-482): sim_bench strategy overrides

Already detailed. `strategy.*` keys in scenario JSON via serde patch.

**Existing ticket: VIO-482** → move to P6

---

#### Ticket 5 (VIO-483): SetStrategyConfig command + daemon endpoints

Already detailed. Command variant, POST/GET /api/v1/strategy endpoints.

**Existing ticket: VIO-483** → move to P6

---

#### Ticket 6 (VIO-484): MCP advisor strategy tools

Already detailed. get_strategy_config, suggest_strategy_change MCP tools.

**Existing ticket: VIO-484** → move to P6

---

#### Ticket 7: Full AutopilotConfig schema — evolve P0 baseline into StrategyConfig

**What:** Merge P0's baseline AutopilotConfig (VIO-520) with StrategyConfig. The baseline extracted hardcoded behavior; now make all those parameters part of the configurable strategy layer.

**Details:**
- P0's AutopilotConfig has ~10 behavioral parameters (mining priorities, lab strategy, export thresholds, etc.)
- StrategyConfig (VIO-479) adds priority weights, fleet targets, mode
- Merge: StrategyConfig becomes the superset. AutopilotConfig fields become StrategyConfig fields.
- content/autopilot.json (content ID mappings) stays separate from StrategyConfig (behavioral tuning)
- Versioned: "strategy-v2" (v1 was P0 baseline)

**Acceptance criteria:**
- [ ] StrategyConfig contains all P0 AutopilotConfig behavioral parameters
- [ ] P0 baseline config loadable as StrategyConfig (backward compatible)
- [ ] sim_bench can override any StrategyConfig parameter
- [ ] All existing behavior preserved with default config

**Dependencies:** P0 VIO-520 (baseline config), VIO-479 (StrategyConfig types)
**Size:** Medium

---

#### Ticket 8: StrategyConfig integration tests — strategy affects autopilot behavior

**What:** Integration tests proving StrategyConfig actually changes autopilot behavior. Different configs produce different outcomes.

**Details:**
- Test: "mining-focused" config produces more ore than "research-focused" config
- Test: fleet_size_target = 5 builds more ships than target = 2
- Test: expansion mode builds second station sooner than balanced mode
- All use sim_bench with strategy overrides, 10+ seeds per config

**Acceptance criteria:**
- [ ] Different strategies produce measurably different outcomes
- [ ] Tests validate causal link between config and behavior
- [ ] Results statistically significant across seeds

**Dependencies:** Tickets 1-4 (strategy layer functional)
**Size:** Medium

---

### Milestone 2: Progression-Aware AI

#### Ticket 9: Phase detection + automatic strategy mode switching

**What:** Autopilot detects current game phase from ProgressionState and game signals. Automatically switches StrategyMode unless manually overridden.

**Details:**
- Phase detection logic in AutopilotController:
  - Ground: only ground facilities exist (no stations)
  - Early Orbital: 1 station, < 5 ships
  - Industrial: 2+ stations, active logistics
  - Expansion: 3+ stations, belt operations
- On phase change: update StrategyConfig.mode (unless user has set manual override)
- Phase-specific default priority weights defined in content
- Emit Event::StrategyModeChanged { from, to, reason }

**Acceptance criteria:**
- [ ] Phase correctly detected from game state
- [ ] Strategy mode auto-switches on phase transitions
- [ ] Manual override prevents auto-switching
- [ ] Integration test: ground_start → phase transitions detected through progression

**Dependencies:** P1 ProgressionState (VIO-532), Ticket 3 (strategy wired into controller)
**Size:** Medium

---

#### Ticket 10: Phase-specific autopilot priorities

**What:** Each phase has tuned default priority weights. Ground phase prioritizes sensors and research. Orbital prioritizes mining and exports. Industrial prioritizes manufacturing and fleet.

**Details:**
- Content: strategy_defaults.json with per-phase priority presets
- Ground phase: sensors=1.0, research=0.9, manufacturing=0.8, launch=0.7, mining=0.1
- Orbital phase: mining=1.0, refining=0.9, exports=0.8, fleet=0.7, research=0.5
- Industrial phase: manufacturing=1.0, logistics=0.9, fleet=0.8, expansion=0.7
- Expansion phase: zone_coverage=1.0, specialization=0.9, efficiency=0.8
- Content-driven: tunable without code changes

**Acceptance criteria:**
- [ ] Per-phase priority presets in content
- [ ] Autopilot behavior visibly different per phase
- [ ] sim_bench: ground_start with phase-aware AI progresses faster than default

**Dependencies:** Ticket 9 (phase detection)
**Size:** Medium

---

#### Ticket 11 (VIO-485): Multi-station dev_advanced_state + world gen

Already detailed. Second station in dev state, extended build_initial_state(). Needed for testing multi-station AI.

**Existing ticket: VIO-485** → move to P6

---

#### Ticket 12 (VIO-487): Station-scoped ship assignment + pre-partitioning

Already detailed. Station agents only assign home ships. Pre-partition in AutopilotController.

**Existing ticket: VIO-487** → move to P6

---

#### Ticket 13 (VIO-489): Multi-station sim_bench scenario + regression test

Already detailed. 2-station scenario, determinism canary.

**Existing ticket: VIO-489** → move to P6

---

#### Ticket 14: GroundFacilityAgent strategy consumption

**What:** GroundFacilityAgent (from P2) reads StrategyConfig priorities. Sensor purchases, manufacturing decisions, launch scheduling all influenced by strategy weights.

**Details:**
- GroundFacilityAgent concerns read ConcernPriorities from StrategyConfig evaluation
- Budget allocation across sensors/manufacturing/launches driven by priority weights
- Phase-aware: ground phase allocates more to sensors, later phases allocate more to launches
- Integration with FleetCoordinator (P5) for construction ship assignment

**Acceptance criteria:**
- [ ] GroundFacilityAgent reads strategy priorities
- [ ] Different strategy configs produce different ground facility behavior
- [ ] Integration test: mining-focused strategy reduces sensor spending, increases manufacturing

**Dependencies:** P2 GroundFacilityAgent (VIO-554), Ticket 3 (strategy wired in)
**Size:** Medium

---

### Milestone 3: Optimization Loop

#### Ticket 15: Bayesian optimization over StrategyConfig

**What:** Python script that uses Bayesian optimization (optuna or scipy) to search over StrategyConfig parameters, finding configs that maximize composite score.

**Details:**
- Script: scripts/analysis/optimize_strategy.py
- Evolves P0's grid search (VIO-528) into proper Bayesian optimization
- Parameter space: 15-20 StrategyConfig fields with bounds and constraints
- Objective: maximize mean composite score across M seeds
- Uses sim_bench compare infrastructure for statistical rigor
- Optimization history saved to knowledge system (journals)
- Best config exported as content/strategy_optimized.json

**Acceptance criteria:**
- [ ] Bayesian optimization runs end-to-end
- [ ] Finds config that outperforms default by measurable margin
- [ ] History tracked, best config exportable
- [ ] ruff/mypy/pytest pass

**Dependencies:** P0 comparison framework (VIO-526), Ticket 4 (sim_bench overrides)
**Size:** Large

---

#### Ticket 16: Full progression regression test — ground to multi-station

**What:** CI gate ensuring the autopilot can complete the full progression arc from ground_start through multi-station operations. The ultimate "does the AI work" test.

**Details:**
- Quick test (5 seeds, 30000 ticks): ground → orbit → first satellite → first station milestone
- Full test (50 seeds, 50000 ticks, ignored): ground → orbit → belt expansion → multi-station logistics
- Uses optimized strategy config (from Ticket 15 or default)
- Assertion thresholds per phase
- Extends VIO-579 (cross-project integration scenario) with strategy-aware AI

**Acceptance criteria:**
- [ ] Quick test passes in regular cargo test
- [ ] Full test passes with --ignored
- [ ] 90%+ seeds reach Industrial phase
- [ ] 80%+ seeds achieve multi-station operations
- [ ] Failure mode analysis for non-completing seeds

**Dependencies:** Ticket 9 (phase-aware AI), P5 complete
**Size:** Medium

---

#### Ticket 17: Knowledge system maturity — optimization insights

**What:** Extend MCP advisor to understand strategy and progression. Optimization results feed back into knowledge system.

**Details:**
- get_metrics_digest includes: current strategy mode, phase, score trajectory
- query_knowledge filters by game phase, strategy mode
- save_run_journal captures: strategy config used, phase transitions, score milestones
- update_playbook accumulates: optimal strategy patterns per phase
- Optimization history from Ticket 15 automatically journaled

**Acceptance criteria:**
- [ ] Advisor digest includes strategy context
- [ ] Knowledge queryable by phase
- [ ] Optimization results captured in journals

**Dependencies:** MCP advisor (existing), Ticket 15
**Size:** Medium

---

#### Ticket 18: Strategy comparison dashboard data

**What:** Daemon API endpoint providing strategy comparison data — multiple configs ranked by score, with per-dimension breakdowns.

**Details:**
- GET /api/v1/strategy/leaderboard — returns ranked strategies from optimization history
- Per-config: composite score, per-dimension scores, phase completion timing
- Integrates with P0 scoring and optimization history
- Data only — no FE panel (per testing focus preference)

**Acceptance criteria:**
- [ ] Leaderboard endpoint returns ranked strategies
- [ ] Per-dimension breakdown included
- [ ] Integrates with optimization history

**Dependencies:** Ticket 15, P0 scoring
**Size:** Small-Medium

---

#### Ticket 19: P6 sim_bench scenarios — strategy optimization validation

**What:** Scenarios validating that strategy optimization produces measurably better outcomes than default config.

**Details:**
- strategy_default.json: ground_start, 40000 ticks, 20 seeds with default StrategyConfig
- strategy_optimized.json: same but with optimized config from Ticket 15
- strategy_regression.json: ensures optimized config doesn't regress on any dimension
- Comparison: optimized should outperform default by >10% composite score

**Acceptance criteria:**
- [ ] Optimized config outperforms default measurably
- [ ] No dimension regresses in optimized config
- [ ] Statistical significance across 20 seeds

**Dependencies:** Ticket 15 (optimized config exists)
**Size:** Medium

---

## Dependency Graph

```
Milestone 1: StrategyConfig (absorbs Phase C)
  VIO-479 (types) → VIO-480 (interpreter) → VIO-481 (wiring) → VIO-482 (sim_bench)
                                                    │                      │
                                              VIO-483 (command/API)  Ticket 8 (integration tests)
                                                    │
                                              VIO-484 (MCP tools)
                                                    │
                                              Ticket 7 (merge with P0 baseline)

Milestone 2: Progression-Aware AI
  Ticket 9 (phase detection) → Ticket 10 (phase priorities) → Ticket 14 (ground agent)
  VIO-485 (multi-station state) → VIO-487 (scoped assignment) → VIO-489 (scenarios)

Milestone 3: Optimization Loop
  Ticket 15 (Bayesian optimization) → Ticket 16 (regression test)
                                   → Ticket 17 (knowledge maturity)
                                   → Ticket 18 (leaderboard API)
                                   → Ticket 19 (validation scenarios)
```

## Absorbed Project Disposition

**Strategic Layer + Multi-Station AI:**
- Phase C (VIO-479 to VIO-484): → P6 Milestone 1 (all 6 tickets)
- Phase D remnants:
  - VIO-485 (multi-station state) → P6 Milestone 2
  - VIO-487 (scoped assignment) → P6 Milestone 2
  - VIO-489 (multi-station scenario) → P6 Milestone 2
- VIO-486 (home_station) → already in P5
- VIO-488 (claim map) → already in P5
- **Mark standalone project as Completed/Absorbed**

## Sources & References

### Origin
- **Origin document:** [docs/plans/2026-03-28-003-feat-game-progression-system-plan.md](../../docs/plans/2026-03-28-003-feat-game-progression-system-plan.md) — Project 6 section
- **Strategic Layer:** [docs/brainstorms/hierarchical-agent-decomposition-requirements.md](../../docs/brainstorms/hierarchical-agent-decomposition-requirements.md)

### P0-P5 Dependencies
- P0: Scoring (VIO-519-529), AutopilotConfig baseline (VIO-520), evaluation framework (VIO-526), optimization scaffolding (VIO-528)
- P1: ProgressionState (VIO-532) — phase detection
- P2: GroundFacilityAgent (VIO-554) — strategy consumption
- P5: FleetCoordinator (VIO-598) — strategy-driven logistics

### Related Linear
- VIO-508: Plan P6 (planning ticket — mark done)
- Strategic Layer project: VIO-479 to VIO-489
