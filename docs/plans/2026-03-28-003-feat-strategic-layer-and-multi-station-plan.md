---
title: "feat: Strategic Layer + Multi-Station Agents (Phases C+D)"
type: feat
status: active
date: 2026-03-28
origin: docs/brainstorms/hierarchical-agent-decomposition-requirements.md
---

# Strategic Layer + Multi-Station Agents (Phases C+D)

## Enhancement Summary

**Deepened on:** 2026-03-28
**Research agents used:** architecture-strategist, performance-oracle, pattern-recognition-specialist, code-simplicity-reviewer, best-practices-researcher, learnings-researcher
**Institutional learnings applied:** 14 from `docs/solutions/`

### Key Improvements from Deepening

1. **Phase D streamlined.** Reduced from 7 to 5 tickets — inter-station transfers and supply chain deferred. Core: multi-station state, ship ownership, deduplication.
2. **Simplified architecture.** Station agents read `StrategyConfig` directly — no `StationObjective` indirection layer needed for single-station. `FleetDirective` enum removed (subsumed by `fleet_size_target` field).
3. **Named struct for priority weights** instead of `BTreeMap<String, u32>`. Compile-time safety, no hash overhead, optimizer-friendly `to_vec()`/`from_vec()`.
4. **Strategy modes as multiplicative modifiers** on base weights (Stellaris pattern), not behavioral overrides.
5. **Performance gating.** Strategy evaluation every N ticks (not every tick). Cached results reused between evaluations.
6. **Utility-scored concerns** (GOAP + utility AI hybrid). Rule interpreter scores each station concern by `config_weight * state_urgency`, producing a ranked priority list.
7. **Priority halving with temporal bias** for ship assignment (DFHack labormanager pattern). Prevents starvation of low-priority tasks.
8. **Hysteresis on thresholds.** Active concerns get a small bonus (0.05-0.10) to prevent oscillation at decision boundaries.
9. **Types-first implementation ordering.** Foundation types PR → command/event PR → behavior PR → integration PR (cross-layer feature development pattern).
10. **Threshold migration merged into C1.** No separate ticket — include operational thresholds when defining `StrategyConfig`.

### New Considerations Discovered

- **Float determinism in rule interpreter:** Use integer arithmetic or milli-percent for strategy threshold comparisons. Avoid transcendental functions.
- **Global aggregates before per-station distribution:** Rule interpreter must compute fleet-wide metrics (total balance consumption, fleet utilization) before producing per-station objectives.
- **strategy.json loading lifecycle:** Provides defaults for `build_initial_state()` to seed `GameState.strategy_config`. Stored as `default_strategy` on `GameContent`.
- **GET /api/v1/strategy endpoint** missing from original plan — needed for FE and MCP reads.
- **Crisp threshold trap:** Small config weight changes can cascade across thousands of ticks. Always evaluate across multiple seeds.

---

## Overview

With Phases A+B complete (VIO-445 through VIO-454), the autopilot is now a clean hierarchy: `AutopilotController` orchestrates per-station `StationAgent`s and per-ship `ShipAgent`s. Station agents handle 9 operational sub-concerns and assign ship objectives. Ship agents handle tactical execution.

Phase C adds a **strategic layer** above station agents — a config-driven rule interpreter that reads game state and produces priority scores that influence station agent behavior. This completes the "strategy interface" from the AI progression roadmap (Phase 1), enabling: hand-tuned configs, sim_bench parameter sweeps, and eventually LLM-generated strategies — all through the same interface.

Phase D adds **multi-station support** — a second station in dev_base_state, ship-to-station assignment, cross-station asteroid deduplication, and the AI coordination logic to manage stations at different orbital bodies independently.

## Problem Statement

Station agents are currently self-directed — they make all operational decisions with hardcoded heuristics and no strategic guidance. This means:

1. **No strategy comparison.** Can't run `sim_bench --strategy expand` vs `--strategy consolidate` because there's no strategy concept.
2. **No optimization target.** The classical optimization loop (roadmap Phase 2) needs a searchable config space. Currently thresholds are scattered across `Constants` (9 `autopilot_*` fields) and `AutopilotConfig` (operational content mapping).
3. **No directional intent.** The roadmap's "expand / consolidate / optimize" modes have no representation. An optimizer or LLM has nothing to write to.

(see origin: `docs/brainstorms/hierarchical-agent-decomposition-requirements.md` — R9 hybrid strategy source, R10 directional objectives)

## Proposed Solution

Add a `StrategyConfig` struct to `GameState` (mutable, saveable) that captures strategic intent: priority weights, mode selection, resource targets, operational thresholds. A rule interpreter in `AutopilotController` evaluates config + state periodically and produces priority scores that station agents read directly — no intermediate objective type needed for single-station.

### Research Insights: Game AI Strategy Patterns

**Best practice (Stellaris model):** Three-layer config: (1) strategy modes as multiplicative modifiers on base weights, (2) per-concern priority weights as the optimizer search surface, (3) resource targets and thresholds for operational tuning. Modes are a coarse "starting point" that optimizers refine by adjusting base weights.

**Best practice (IAUS / Guild Wars 2):** All priority inputs normalized to [0.0, 1.0]. Use the geometric mean of multiple axis scores to fairly balance considerations. Response curves (linear, quadratic, logistic) transform raw inputs into utility scores.

**Best practice (DFHack labormanager):** Priority halving with temporal bias prevents starvation. After a ship is assigned to mining, mining's effective priority halves — the next ship may survey instead. Concerns not serviced recently get a temporal boost.

## Technical Approach

### Architecture (Post Phase C)

```
AutopilotController
├── strategy_config: on GameState (mutable, saveable)
├── cached_priorities: Option<ConcernPriorities>  // recomputed every N ticks
├── evaluate_strategy(state, content) → ConcernPriorities
├── station_agents: BTreeMap<StationId, StationAgent>
│   ├── reads state.strategy_config + cached_priorities
│   ├── generate() → station commands (influenced by priorities)
│   └── assign_ship_objectives() → weighted by activity priorities
└── ship_agents: BTreeMap<ShipId, ShipAgent>
    └── generate() → tactical commands (unchanged)
```

### Execution Order (Updated)

1. Sync agent lifecycle (create/remove for new/deleted entities)
2. **Evaluate strategy** (every N ticks or after SetStrategyConfig) → cache concern priorities
3. Station agents `generate()` in BTreeMap order (read `strategy_config` + priorities)
4. Station agents `assign_ship_objectives()` — weighted by `activity_weights`
5. Ship agents `generate()` in BTreeMap order

### Research Insights: Performance

**Gate strategy evaluation on interval** (not every tick). Config changes infrequently; state-derived urgency scores are stable for many consecutive ticks. Evaluate every 10-24 ticks, or immediately after `SetStrategyConfig` command. Cache `ConcernPriorities` on the controller. **Expected gain: 90-96% reduction in strategy evaluation overhead.**

**Use references, not owned values.** Pass `&StrategyConfig` to station agents rather than cloning. The config lives on `GameState` for the duration of `generate_commands`.

### Phase C: Strategic Layer

#### C1. StrategyConfig Schema + Threshold Consolidation

New types in `sim_core/src/types/`, loaded as part of `GameState`:

```rust
/// Strategic configuration — mutable game state, set by player/optimizer/LLM.
/// Lives in GameState (not GameContent) because it changes during gameplay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StrategyConfig {
    /// High-level operating mode. Multiplies base priority weights.
    pub mode: StrategyMode,

    /// Per-concern priority weights. Named struct for compile-time safety
    /// and optimizer-friendly to_vec()/from_vec().
    pub priorities: PriorityWeights,

    /// Target fleet size. Station agent builds ships when fleet is below target.
    pub fleet_size_target: u32,

    // --- Operational thresholds (consolidated from Constants.autopilot_*) ---
    pub volatile_threshold_kg: f32,
    pub refinery_threshold_kg: f32,
    pub slag_jettison_pct: f32,
    pub export_batch_size_kg: f32,
    pub export_min_revenue: f64,
    pub lh2_threshold_kg: f32,
    pub budget_cap_fraction: f64,
    pub lh2_abundant_multiplier: f32,
    pub refuel_threshold_pct: f32,
}
```

**Priority weights as a named struct** (not BTreeMap):

```rust
/// Per-concern priority weights in [0.0, 1.0].
/// Named struct for compile-time safety and optimizer-friendly interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PriorityWeights {
    pub mining: f32,
    pub survey: f32,
    pub deep_scan: f32,
    pub research: f32,
    pub maintenance: f32,
    pub export: f32,
    pub propellant: f32,
    pub fleet_expansion: f32,
}

impl PriorityWeights {
    /// Flat vector for optimizer consumption.
    pub fn to_vec(&self) -> Vec<f32> { /* ... */ }
    /// Reconstruct from flat vector.
    pub fn from_vec(v: &[f32]) -> Self { /* ... */ }
    /// Dimension names for optimizer logging.
    pub fn dimension_names() -> &'static [&'static str] { /* ... */ }
}
```

**Strategy modes as multiplicative modifiers** (Stellaris pattern):

```rust
/// High-level strategy mode — engine mechanic, not content category.
/// Each mode applies multiplicative factors to base priority weights.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum StrategyMode {
    /// Balance expansion and optimization (1.0x all weights).
    #[default]
    Balanced,
    /// Focus on resource extraction and fleet growth.
    Expand,
    /// Optimize existing operations, minimize spending.
    Consolidate,
}

impl StrategyMode {
    /// Multiplicative factors applied to base priority weights.
    pub fn multipliers(&self) -> PriorityWeights {
        match self {
            Self::Balanced => PriorityWeights::all(1.0),
            Self::Expand => PriorityWeights {
                mining: 1.3, survey: 1.5, deep_scan: 1.2,
                research: 0.7, maintenance: 0.8, export: 0.6,
                propellant: 1.0, fleet_expansion: 1.5,
            },
            Self::Consolidate => PriorityWeights {
                mining: 0.7, survey: 0.5, deep_scan: 0.8,
                research: 1.5, maintenance: 1.3, export: 1.2,
                propellant: 1.0, fleet_expansion: 0.0,
            },
        }
    }
}
```

**Effective weight = base_weight * mode_multiplier**, clamped to [0.0, 1.0].

**Why struct over BTreeMap<String, u32>:**
- Compile-time guarantees — cannot misspell a weight name
- No hashing — direct field access at ~435K TPS
- Exhaustiveness checking — `match` forces handling all concerns
- Optimizer interface — `to_vec()`/`from_vec()` for grid search and Bayesian optimization

**Loading lifecycle:** `strategy.json` in `content/` provides default values. Loaded by `sim_world` as `GameContent.default_strategy: StrategyConfig`. Used by `build_initial_state()` to seed `GameState.strategy_config`. Runtime changes go through `SetStrategyConfig` command.

### Research Insights: Backward Compatibility

Per `docs/solutions/integration-issues/backward-compatible-type-evolution.md`:
- `#[serde(default)]` on `strategy_config: StrategyConfig` field on `GameState` — existing saves without the field deserialize to defaults
- Write a backward-compat test that deserializes a JSON snapshot without the field
- `Default` impl for `StrategyConfig` returns `Balanced` mode with current `Constants.autopilot_*` values as thresholds

#### C2. Rule Interpreter (evaluate_strategy)

New method on `AutopilotController`:

```rust
/// Compute effective priority scores from strategy config + game state signals.
/// Pure function: same inputs → same outputs (deterministic).
fn evaluate_strategy(
    &self,
    state: &GameState,
    content: &GameContent,
) -> ConcernPriorities {
    let config = &state.strategy_config;
    let effective = config.priorities.apply_mode(&config.mode);

    // Score each concern: config_weight * state_urgency
    // State urgency is normalized to [0.0, 1.0]
    let mining_urgency = 1.0 - (ore_buffer_fullness(state, config));
    let maintenance_urgency = max_module_wear(state);
    let research_urgency = 1.0 - research_completion_ratio(state);
    // ... etc

    ConcernPriorities {
        mining: effective.mining * mining_urgency,
        survey: effective.survey * survey_urgency,
        // ...
    }
}
```

**Utility scoring pattern (GOAP + utility AI hybrid):**
- Config weights express "how much does the player/optimizer care about X"
- State urgency expresses "how much does the game state need X right now"
- Product of the two = final priority score
- Station agents use scores to influence sub-concern behavior

### Research Insights: Determinism in Rule Interpreter

Per `docs/solutions/logic-errors/deterministic-integer-arithmetic.md`:
- **No transcendental functions** (sin, cos) in the evaluation path — stick to arithmetic
- **BTreeMap iteration** for any per-station or per-entity scoring
- **total_cmp()** for float sorting (already established pattern)
- **game_minutes_to_ticks()** for any time horizon thresholds — never hardcode tick counts
- **Read phase then write phase** — compute all scores before delivering to agents (borrow checker + determinism)
- **Global aggregates first** — fleet utilization, total balance burn rate, etc. must be computed across ALL stations before producing per-station priorities

### Research Insights: Hysteresis and Stability

Per best practices research (IAUS / DFHack):
- **Hysteresis on active concerns:** Once mining becomes the top priority, give it a 0.05-0.10 bonus that persists until it drops well below the threshold. Prevents oscillation.
- **Temporal bias:** Concerns not serviced recently get a small bonus proportional to time since last service. Prevents starvation of low-priority tasks.
- **Priority halving for ship assignment:** After a ship is assigned to mining, mining's effective priority for the next ship assignment halves. This naturally distributes ships across concerns.

#### C3. Wire Into AutopilotController

Add to `AutopilotController`:

```rust
pub struct AutopilotController {
    station_agents: BTreeMap<StationId, StationAgent>,
    ship_agents: BTreeMap<ShipId, ShipAgent>,
    owner: PrincipalId,
    /// Cached priority scores, recomputed every N ticks.
    cached_priorities: Option<ConcernPriorities>,
    /// Last tick strategy was evaluated.
    last_strategy_tick: u64,
    /// Force re-evaluation after SetStrategyConfig.
    strategy_dirty: bool,
}
```

In `generate_commands`:
```rust
// 2. Evaluate strategy (gated)
let eval_interval = 10; // ticks between evaluations
if self.strategy_dirty
    || self.cached_priorities.is_none()
    || state.meta.tick.saturating_sub(self.last_strategy_tick) >= eval_interval
{
    self.cached_priorities = Some(self.evaluate_strategy(state, content));
    self.last_strategy_tick = state.meta.tick;
    self.strategy_dirty = false;
}
```

**`strategy_dirty` flag:** Set to `true` when `SetStrategyConfig` command is applied. This triggers immediate re-evaluation rather than waiting for the interval.

#### C4. Station Agents Consume Strategy Config

Station agents read directly from `state.strategy_config` — no intermediate `StationObjective` type needed for single-station.

Changes to station agent sub-concerns:
- `assign_ship_objectives()` — uses `activity_weights` (from cached priorities) for weighted task selection instead of hardcoded `task_priority` list. **Priority halving:** after assigning a ship to mining, halve mining's effective weight for the next assignment.
- `recruit_crew()` / `fit_ships()` — respect `fleet_size_target` (skip if fleet at target)
- `manage_propellant()` — reads `lh2_threshold_kg` from strategy config
- `export_materials()` / `jettison_slag()` — reads thresholds from strategy config
- All threshold reads migrated from `content.constants.autopilot_*` to `state.strategy_config.*`

#### C5. sim_bench Integration

Extend `apply_overrides()` to handle `strategy.*` keys using the serde serialize→patch→deserialize pattern (same as Constants overrides per `docs/solutions/patterns/batch-code-quality-refactoring.md`):

```rust
// In overrides.rs
if let Some(rest) = key.strip_prefix("strategy.") {
    strategy_overrides.push((rest, value));
} else if let Some(rest) = key.strip_prefix("module.") {
    // existing module overrides
}
```

The strategy overrides apply to `GameContent.default_strategy` before `build_initial_state()` seeds `GameState`. This means strategy overrides follow the same lifecycle as constant overrides — applied once at scenario start.

Scenario files gain `strategy.*` override keys:
```json
{
  "name": "aggressive_expansion",
  "overrides": {
    "strategy.mode": "Expand",
    "strategy.fleet_size_target": 8,
    "strategy.priorities.mining": 0.9,
    "strategy.priorities.survey": 0.7,
    "strategy.volatile_threshold_kg": 300.0
  }
}
```

### Research Insights: Optimizer Compatibility

- **Flat normalized weight vector** for grid search / Bayesian optimization: `PriorityWeights::to_vec()` provides the flat vector, `from_vec()` reconstructs
- **Always evaluate across multiple seeds** — single-seed results are noise
- **Aggregate metrics** (mean + p5 across seeds) as the optimization target
- **Constraint functions** to reject pathological configs before running them (e.g., `priorities.maintenance >= 0.2`)
- **Config stability tests:** same config on adjacent seeds should produce metrics within a reasonable band

#### C6. SetStrategyConfig Command + Daemon Endpoints

New `Command` variant:
```rust
Command::SetStrategyConfig { config: StrategyConfig }
```

Applied in `apply_commands()` — full replacement (not merge), consistent with `SetModuleEnabled` pattern. Sets `state.strategy_config = config`.

Per `docs/solutions/integration-issues/module-behavior-extensibility.md` cross-layer checklist:
1. Match arm in `apply_commands` in engine.rs
2. If emitting a `StrategyConfigChanged` event — add FE handler in applyEvents.ts
3. Run `ci_event_sync.sh` after

**Daemon endpoints:**
- `POST /api/v1/strategy` — accepts `StrategyConfig` JSON body, wraps in `SetStrategyConfig` command, pushes to command queue. Deterministic (applied at tick boundary).
- `GET /api/v1/strategy` — reads current `state.strategy_config`. Consistent with existing readable-state endpoints (`/alerts`, `/metrics`, `/snapshot`).

#### C7. MCP Advisor Strategy Tools

Two new MCP tools (following existing naming convention `verb_noun`):

1. `get_strategy_config` — thin wrapper around `GET /api/v1/strategy`
2. `suggest_strategy_change` — writes proposal to `content/advisor_proposals/` (like `suggest_parameter_change`)

Fits existing MCP workflow: recall → run → analyze → propose → test → verify → save.

## System-Wide Impact

### Interaction Graph

`sim_cli` / `sim_daemon` → `AutopilotController::generate_commands()` → [evaluate strategy (gated) → station agents generate → station agents assign ships → ship agents generate] → `Vec<CommandEnvelope>` → `sim_core::tick()` applies.

New command path: daemon `POST /api/v1/strategy` → queues `SetStrategyConfig` command → applied in `tick()` → `state.strategy_config` updated → controller `strategy_dirty = true` → re-evaluates next tick.

### Error & Failure Propagation

- Strategic config validation at deserialization (`serde(default)` for all fields). Invalid weights clamp to [0.0, 1.0].
- Default `StrategyConfig` produces identical behavior to current system (regression safety).
- `SetStrategyConfig` command applied atomically — no partial state.

### State Lifecycle Risks

- `StrategyConfig` on `GameState` → serialized/deserialized with saves. `#[serde(default)]` on all fields for backward compatibility.
- `ConcernPriorities` is transient (cached on controller, rebuilt periodically). No persistence needed.
- `strategy_dirty` flag on controller is transient — safe to lose on restart (will re-evaluate next tick).

### API Surface Parity

- `CommandSource` trait: **unchanged**.
- New `Command::SetStrategyConfig` variant.
- New daemon endpoints: `POST /api/v1/strategy`, `GET /api/v1/strategy`.
- New MCP tools: `get_strategy_config`, `suggest_strategy_change`.
- sim_bench: `strategy.*` override keys.
- FE: optional Strategy panel (not required for core value).

### Integration Test Scenarios

1. **Strategy mode comparison:** Run baseline scenario with `Expand` vs `Consolidate`, verify different fleet sizes and resource curves.
2. **Config hot-swap:** Change strategy mid-run via `SetStrategyConfig` command, verify station behavior adapts within evaluation interval.
3. **sim_bench override:** Scenario with `strategy.fleet_size_target: 8` produces more ships than default.
4. **Default equivalence:** Default `StrategyConfig` produces identical commands as current system (critical regression test).
5. **Determinism canary:** Same strategy config + same seed = identical output across 4000 ticks.
6. **Multi-seed stability:** Same config across 20 seeds produces metrics within reasonable band.
7. **Multi-station deduplication (Phase D):** 2 stations, 3 asteroids — each station only mines its claimed asteroids, no double-assignment.
8. **Multi-station independence (Phase D):** 2 stations at different orbital bodies make independent operational decisions, both get agents automatically.
9. **Ship home station (Phase D):** New ship built at station A has `home_station = A`, only station A assigns it objectives.

## Acceptance Criteria

### Functional Requirements

- [ ] `StrategyConfig` struct on `GameState`, serializable, with defaults matching current behavior
- [ ] `PriorityWeights` named struct with `to_vec()`/`from_vec()` for optimizer interface
- [ ] Three strategy modes (`Expand`, `Consolidate`, `Balanced`) with multiplicative weight modifiers
- [ ] Rule interpreter computes utility-scored concern priorities from config + state signals
- [ ] Strategy evaluation gated on interval (default 10 ticks) + dirty flag after config change
- [ ] Station agents read strategy config to influence ship assignment (weighted selection)
- [ ] Station agents read operational thresholds from strategy config (migrated from Constants)
- [ ] `SetStrategyConfig` command applied via command system (deterministic, full replacement)
- [ ] Daemon endpoints: `POST /api/v1/strategy`, `GET /api/v1/strategy`
- [ ] sim_bench supports `strategy.*` override keys (serde-patch approach)
- [ ] MCP advisor has `get_strategy_config` and `suggest_strategy_change` tools

### Phase D: Functional Requirements

- [ ] `dev_base_state.json` has 2+ stations at different orbital bodies
- [ ] `build_initial_state()` can generate multi-station starting state
- [ ] `home_station: Option<StationId>` on `ShipState` with `serde(default)`
- [ ] Ships assigned to building station at construction
- [ ] Station agents only assign objectives to their home ships
- [ ] Cross-station asteroid deduplication via strategic layer (claim map or pre-pass)
- [ ] Multi-station sim_bench scenario with regression test
- [ ] Station agents at different orbital bodies make independent decisions

### Non-Functional Requirements

- [ ] Determinism preserved: same strategy config + same seed = identical state
- [ ] No performance regression: sim_bench throughput within 5% of current
- [ ] Default `StrategyConfig` produces identical behavior to current system
- [ ] `CommandSource` trait API unchanged
- [ ] All new `GameState` fields use `#[serde(default)]` for backward compatibility

### Quality Gates

- [ ] Progression integration test passes at each ticket
- [ ] sim_bench baseline regression passes
- [ ] Behavioral equivalence test: default config = current behavior
- [ ] At least one test per strategy mode with realistic content values
- [ ] Backward-compat test: deserialize save without strategy_config field
- [ ] No new clippy warnings
- [ ] Test coverage maintained at 83%+

## Dependencies & Prerequisites

| Dependency | Status | Impact |
|-----------|--------|--------|
| Phase A+B complete | Done | Foundation for Phase C |
| Hull+slot system | In progress | Ship capabilities for aptitude-based assignment. Phase C can stub (all ships can do all tasks). |
| VIO-412 content-driven autopilot | Largely done | Most hardcoded IDs replaced. |

## Risk Analysis & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Default strategy diverges from current behavior | Medium | High | Behavioral equivalence test: default StrategyConfig must produce identical commands. |
| Config sensitivity (small weight change → cascade) | Medium | Medium | Always evaluate across multiple seeds. Add config stability tests. |
| Crisp threshold oscillation | Medium | Low | Hysteresis on active concerns. Priority halving for ship assignment. |
| StrategyConfig schema needs redesign after C | Low | Medium | `#[serde(default)]` on all fields for backward compat. Named struct fields are additive. |
| Float determinism in rule interpreter | Low | High | Integer arithmetic for threshold comparisons where possible. `total_cmp()` for sorts. No transcendentals. |
| Multi-station performance regression | Medium | Medium | Gate claim map on state changes. Pre-partition ships. Pre-compute candidate lists globally. |
| Dual-path state init divergence (Phase D) | Medium | Medium | CI test asserting both paths produce matching state. Update both paths in same PR. |
| Second station bootstrap deadlock | Low | High | Trace dependency chain. Ensure starting equipment is self-sustaining. |

## Ticket Breakdown

### Phase C: Strategic Layer (Active)

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| C1 | Define StrategyConfig + PriorityWeights + StrategyMode types | — | `sim_core` types, `strategy.json` defaults, `GameState` field with `serde(default)`, threshold consolidation |
| C2 | Build rule interpreter (evaluate_strategy) | C1 | Utility-scored concern priorities, evaluation gating, hysteresis |
| C3 | Wire strategic layer into AutopilotController + station agent consumption | C2 | Execution order update, weighted ship assignment, threshold reads from strategy config |
| C4 | sim_bench strategy overrides | C1 | `strategy.*` keys via serde-patch, multi-seed strategy comparison scenarios |
| C5 | SetStrategyConfig command + daemon endpoints | C1 | Command variant, POST + GET endpoints, strategy_dirty flag |
| C6 | MCP advisor strategy tools | C5 | get_strategy_config, suggest_strategy_change |

### Phase D: Multi-Station AI (Active)

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| D1 | Multi-station dev_base_state + world gen | C3 | 2 stations at different orbital bodies, both init paths updated |
| D2 | Ship home_station field + construction assignment | D1 | `ShipState.home_station`, assigned at build time, `serde(default)` |
| D3 | Station-scoped ship assignment + pre-partitioning | D2 | Filter by home_station, pre-partition in controller |
| D4 | Cross-station asteroid deduplication (claim map) | D3 | Strategic layer assigns asteroids to nearest station, gated rebuild |
| D5 | Multi-station sim_bench scenario | D3 | `multi_station_baseline.json`, regression test at 4000+ ticks |

### Dependency Graph

```
C1 (StrategyConfig types + thresholds) ──┬──> C2 (Rule interpreter) ──> C3 (Wire + consumption)
                                          ├──> C4 (sim_bench overrides)          │
                                          └──> C5 (Command + daemon) ──> C6      │
                                                                                  │
                                               D1 (Multi-station state) ◄─────────┘
                                                │
                                               D2 (home_station) ──> D3 (scoped assignment) ──> D4 (dedup)
                                                                      │
                                                                     D5 (bench scenario)
```

### Implementation Order (Cross-Layer Pattern)

Per `docs/solutions/patterns/cross-layer-feature-development.md`:

**Phase C:**
1. **PR 1 — Foundation types (C1):** `StrategyConfig`, `PriorityWeights`, `StrategyMode` in sim_core. `strategy.json` in content. `GameState.strategy_config` field. Backward-compat tests. Default impl matching current Constants values. **No behavior changes.**
2. **PR 2 — Command + events (C5):** `SetStrategyConfig` command variant, engine handler, daemon endpoints. `ci_event_sync.sh` if events emitted.
3. **PR 3 — Rule interpreter + wiring (C2 + C3):** `evaluate_strategy()`, gating logic, station agent reads strategy config. Weighted ship assignment. Threshold migration. Behavioral equivalence test.
4. **PR 4 — Integration (C4 + C6):** sim_bench `strategy.*` overrides, MCP tools. Multi-seed comparison scenarios.

**Phase D (after Phase C):**
5. **PR 5 — Multi-station state (D1 + D2):** Second station in dev_base_state + build_initial_state. `home_station` field on ShipState. Both init paths updated together.
6. **PR 6 — Station-scoped assignment + dedup (D3 + D4):** Filter by home_station, pre-partition ships, claim map for asteroid deduplication.
7. **PR 7 — Multi-station scenario (D5):** sim_bench scenario with 2 stations, regression test.

### Phase D: Multi-Station AI

#### D1. Multi-Station Dev Base State + World Gen

Add a second station to `dev_base_state.json` at a different orbital body (e.g., Inner Belt or Mars). Extend `build_initial_state()` to support multi-station generation.

Per `docs/solutions/patterns/multi-epic-project-execution.md`: **update both `dev_base_state.json` AND `build_initial_state()` together.** If one path has 2 stations and the other has 1, MCP-started simulations will diverge from dev testing.

Station placement uses existing `solar_system.json` resource zones:
- Station A at Earth orbit (existing) — balanced resource access
- Station B at Inner Belt — metal-rich environment, different scan sites

Each station gets appropriate starting modules for its location. Consider the bootstrap dependency chain per `docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md`: Station B needs enough starting equipment to be self-sustaining.

#### D2. Ship home_station Field

Add `home_station: Option<StationId>` to `ShipState` with `#[serde(default)]` for backward compatibility.

- Ships are assigned to their building station at construction time
- Existing ships in saves default to `None` — the controller assigns them to the nearest station on first tick
- Station agents only assign objectives to ships where `home_station == Some(self.station_id)`

This replaces the current "co-located idle ships" filter with an ownership model that works across orbital bodies.

#### D3. Station-Scoped Ship Assignment

Modify `StationAgent::assign_ship_objectives()` to filter ships by `home_station` instead of co-location. Pre-partition ships by home_station in the controller (maintain `BTreeMap<StationId, Vec<ShipId>>`  updated in lifecycle sync step) and pass the relevant subset to each station agent.

Per performance-oracle analysis: this turns O(S * K) ship scanning into O(K) for partitioning plus O(K_i) per station.

#### D4. Cross-Station Asteroid Deduplication

Solve the documented limitation at `lib.rs:86-88`: two station agents assigning the same asteroid.

**Approach:** Strategic layer builds a `BTreeMap<AsteroidId, StationId>` claim map as a pre-pass before station agents assign ship objectives. Each asteroid is claimed by the nearest station (distance-based, deterministic tiebreak by StationId).

Per performance-oracle analysis:
- **Gate claim map on state changes** — only rebuild when asteroid set or station set changes (track counts). Not every tick.
- **Pre-compute candidate lists globally, partition by claim** — compute `collect_mine_candidates` once, split by claim map, pass per-station slices to station agents. Avoids O(S * A log A) blowup.
- All distance computations use `compute_entity_absolute` — sort by distance from home station, not arbitrary order (per proximity-blind selection lesson from `docs/solutions/patterns/multi-epic-project-execution.md`).

#### D5. Multi-Station Sim Bench Scenario

Create `scenarios/multi_station_baseline.json` with 2 stations at different orbital bodies. Use as a regression test for multi-station AI behavior. Run at 4000+ ticks to catch accumulation problems.

### Research Insights: Performance for Multi-Station

Per performance-oracle analysis:
1. **Gate claim map on state changes** — only rebuild when asteroid/station set changes, not every tick
2. **Pre-compute candidate lists globally, partition by claim** — avoids O(S * A log A) blowup
3. **Pre-partition ships by home_station** — maintain `BTreeMap<StationId, Vec<ShipId>>` on controller
4. **Use references for per-station data** — pass `&[AsteroidId]` slices, not owned Vecs
5. **Consider interning entity IDs** (u64-based) to eliminate String allocation in hot paths (future optimization)

## Future Considerations

### Phase D Extensions (deferred)

- **Inter-station transfer objectives:** `TransferResource` / `RequestResource` station objectives, `Haul` ship objective for inter-station cargo transport
- **Supply chain coordination:** Multi-hop resource flow optimization
- **ReassignShip command:** Strategic ship reallocation between stations
- **Intermediate layers:** Sector/region coordinators, squadron/fleet coordinators

### Roadmap Phase 2: Classical Optimization

Once Phase C lands, the optimization loop becomes:
1. Python script generates N `StrategyConfig` variants (grid search, random search, or Bayesian optimization)
2. Each config → sim_bench scenario via `strategy.*` overrides
3. Run M seeds per config (aggregate with mean + p5)
4. Rank by `final_score` (DuckDB cross-seed analysis)
5. Best config → new default or advisor recommendation

This is the key payoff of Phase C — strategy becomes a searchable parameter space.

### Roadmap Phase 6: LLM Integration

The LLM advisor writes to `StrategyConfig` via the same interface as the optimizer. The `POST /api/v1/strategy` endpoint and `suggest_strategy_change` MCP tool are the integration surface. The LLM reasons at the fleet level; deterministic agents execute at station/ship level.

### Trait Generalization (Post Phase C)

After Phase C, evaluate whether `Agent` trait needs:
- `type Objective` associated type for type-safe layer composition
- Generic objective parameter
- Layer composition configuration

**Do not design this now.** Phase C experience will reveal what's needed.

## Documentation Plan

- Update `docs/reference.md` — new `StrategyConfig` type, `PriorityWeights`, `SetStrategyConfig` command, daemon endpoints
- Update `CLAUDE.md` — architecture section (strategic layer), tick order (strategy evaluation step)
- Update `docs/workflow.md` — sim_bench strategy override keys
- Add MCP tool documentation to `mcp_advisor/` README

## Sources & References

### Origin

- **Origin document:** [docs/brainstorms/hierarchical-agent-decomposition-requirements.md](docs/brainstorms/hierarchical-agent-decomposition-requirements.md) — Key decisions carried forward: hybrid strategy source (R9), directional objectives (R10), intermediate layer support (R11/R12), adaptive defaults (R14).

### Internal References

- Current agent system: `crates/sim_control/src/lib.rs:27` (AutopilotController)
- ShipObjective: `crates/sim_control/src/objectives.rs:8`
- StationAgent: `crates/sim_control/src/agents/station_agent.rs`
- AutopilotConfig: `crates/sim_core/src/types/content.rs:65`
- Constants autopilot fields: `crates/sim_core/src/types/constants.rs:27-139`
- sim_bench overrides: `crates/sim_bench/src/overrides.rs`
- MCP advisor: `mcp_advisor/src/index.ts`
- AI progression roadmap: `docs/plans/2026-03-23-code-quality-and-ai-progression-roadmap.md`
- Phase A+B plan: `docs/plans/2026-03-28-002-refactor-hierarchical-agent-decomposition-plan.md`
- Cross-station limitation: `crates/sim_control/src/lib.rs:86-88`

### Learnings Applied

- Global-to-per-entity scoping pitfall: `docs/solutions/patterns/hierarchical-agent-decomposition.md`
- Content-driven event engine pattern: `docs/solutions/patterns/content-driven-event-engine.md`
- Proximity-blind selection bug: `docs/solutions/patterns/multi-epic-project-execution.md`
- Deterministic integer arithmetic: `docs/solutions/logic-errors/deterministic-integer-arithmetic.md`
- Serde serialize-patch-deserialize: `docs/solutions/patterns/batch-code-quality-refactoring.md`
- Cross-layer feature development: `docs/solutions/patterns/cross-layer-feature-development.md`
- Backward-compatible type evolution: `docs/solutions/integration-issues/backward-compatible-type-evolution.md`
- Module behavior extensibility: `docs/solutions/integration-issues/module-behavior-extensibility.md`
- Stat modifier coupling: `docs/solutions/patterns/stat-modifier-tech-expansion.md`
- Balance analysis workflow: `docs/solutions/logic-errors/balance-analysis-workflow.md`
- Event sync enforcement: `docs/solutions/integration-issues/event-sync-enforcement.md`
- Gameplay deadlock from missing equipment: `docs/solutions/logic-errors/gameplay-deadlock-missing-starting-equipment.md`

### External Research

- Stellaris AI: weight-based economic plans with attitude multipliers
- FAForever (Supreme Commander): manager-based hierarchical architecture with builder priority lists
- DFHack labormanager: priority halving with temporal bias for starvation prevention
- IAUS (Guild Wars 2): infinite axis utility systems with normalized [0,1] inputs and response curves
- GOAP + utility hybrid: utility scoring for WHAT, rule interpretation for HOW
- Bayesian optimization best practices for hyperparameter tuning
