---
title: "refactor: Hierarchical Agent Decomposition (Phases A+B)"
type: refactor
status: active
date: 2026-03-28
origin: docs/brainstorms/hierarchical-agent-decomposition-requirements.md
---

# Hierarchical Agent Decomposition (Phases A+B)

## Overview

Decompose the flat 10-behavior autopilot into a hierarchical agent system: ship agents handle tactical autonomy (transit, refuel, mine, deposit), station agents handle operational decisions (module management, resource needs, ship assignment), and the existing `CommandSource` API remains unchanged. Phases A+B are an internal refactor — same behavior, better structure.

## Problem Statement

The autopilot is a flat `Vec<Box<dyn AutopilotBehavior>>` of 10 peer behaviors that all see the full `GameState`. This creates: (1) no scope isolation — every behavior reasons about everything, (2) no coordination — ship scheduling has no awareness of station-level resource needs, (3) can't scale to multi-station — behaviors don't know which station they serve. See origin doc for full analysis (see origin: `docs/brainstorms/hierarchical-agent-decomposition-requirements.md`).

## Proposed Solution

Two-phase internal refactor:

**Phase A** — Define objective types and extract `ShipTaskScheduler` into ship agents that receive objectives. Ship agents handle tactical details (transit, refuel, mine, deposit) autonomously. Remaining 9 flat behaviors continue running unchanged.

**Phase B** — Create per-station agent instances that absorb the 9 remaining flat behaviors as sub-concerns. Station agents perform capability-aware ship assignment, emitting ship objectives. Flat behaviors are fully retired.

### Architectural Decisions (resolved from SpecFlow analysis)

**AD1. Assignment happens at the station layer, not ship layer.**
The current `ShipTaskScheduler` uses a shared-iterator pattern to prevent double-assignment (two ships sent to same asteroid). This is fundamentally a coordination problem. The station agent owns assignment: it decides which ship gets which objective, then ship agents execute. Ship agents have tactical autonomy (how to get there, when to refuel) but don't pick their own targets. This preserves the deduplication pattern and aligns with R7. (see origin: R7, capability-aware ship assignment)

**AD2. Deterministic execution order: station agents sorted by StationId, then ship agents sorted by ShipId.**
Matches current `BTreeMap` iteration order. Station agents run first (emitting ship objectives + station commands), then ship agents run (converting objectives to commands). Command IDs are assigned sequentially via the shared `next_id: &mut u64` counter. This preserves the determinism canary.

**AD3. Objectives persist in ship agent state until completed or invalidated.**
Station agents only issue new objectives to idle ships (no mid-task reassignment in Phase A+B). Ship agents hold their current objective as `Option<ShipObjective>` in `&mut self` state. When the objective completes (task done) or becomes invalid (asteroid depleted, ship arrives and finds nothing), the ship goes idle and the station agent assigns a new objective next tick. This mirrors how `TaskKind` already works — ships have a task until it resolves.

**AD4. During Phase A, `ShipFitting` stays as a flat behavior running before ship agents.**
Ship agents need fitted modules to evaluate capabilities. Since `ShipFitting` fits modules to idle ships as commands (applied next tick), and ship agents also operate on the same snapshot, there's no conflict — both see the same state. `ShipFitting` moves to the station agent in Phase B.

**AD5. Station sub-concerns run in the same order as current flat behaviors.**
Within a station agent, sub-concerns execute in the same sequence as `default_behaviors()`: module management → lab assignment → crew → propellant → fitting (Phase B) → ship objective emission. PropellantPipeline/StationModuleManager coordination preserved via role-based module ownership (modules with `propellant_role` are managed exclusively by the propellant sub-concern).

**AD6. LabAssignment cache is per-station agent instance.**
Research state is sim-wide, but the cache is cheap (small HashMap). Each station agent maintains its own cache with the same invalidation logic (`unlocked.len()` check). No shared mutable state needed.

**AD7. Station agents have built-in housekeeping behaviors (no objectives needed from above).**
Until Phase C (strategic layer), station agents operate autonomously with sensible defaults. Module management, crew assignment, slag jettison, lab assignment, trade — all are internal housekeeping that doesn't need strategic direction. Strategic objectives (Phase C) will only affect resource allocation priorities, not operational details. (see origin: R14, adaptive defaults)

## Technical Approach

### Phase A: Ship Agent Extraction (4 tickets)

#### A1. Define objective and agent types

New file: `crates/sim_control/src/objectives.rs`

```rust
/// Objective issued by a station agent to a ship agent.
#[derive(Debug, Clone)]
pub(crate) enum ShipObjective {
    Mine { asteroid_id: AsteroidId },
    DeepScan { asteroid_id: AsteroidId },
    Survey { site_id: SiteId },
    Deposit { station_id: StationId },
    Idle,
}
```

New file: `crates/sim_control/src/agents/mod.rs` (module structure)

```rust
/// A decision-making agent that receives context and emits commands.
/// Agents are scoped — they see relevant state but only act within their domain.
pub(crate) trait Agent: Send {
    /// Human-readable name for logging/debugging.
    fn name(&self) -> &'static str;

    /// Generate commands for this tick.
    fn generate(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}
```

The `Agent` trait intentionally has the same signature as `AutopilotBehavior` — this allows incremental migration. The difference is conceptual: agents are scoped, hierarchical, and objective-driven.

**Keep it simple:** No generic objective type parameter, no associated types, no layer composition framework yet. The trait is just `AutopilotBehavior` with a new name and new file location. Generalization happens after we learn from Phase A+B what the trait actually needs. (see origin: brainstorm discussion — "start simple, learn, generalize")

#### A2. Implement ShipAgent

New file: `crates/sim_control/src/agents/ship_agent.rs`

```rust
pub(crate) struct ShipAgent {
    ship_id: ShipId,
    objective: Option<ShipObjective>,
}
```

The ship agent:
1. Checks if current objective is still valid (asteroid exists & has mass, site exists, station exists)
2. If objective is invalid or completed → sets `objective = None` (ship becomes idle)
3. If objective is `Some` and ship task is `Idle` → converts objective to `AssignShipTask` command with transit/refuel logic
4. Handles opportunistic refuel check (existing `should_opportunistic_refuel` logic)

Logic extracted from current `ShipTaskScheduler` helpers: `maybe_transit`, `try_refuel`, `maybe_assign_refuel`, `try_mine`, `try_deep_scan`, `try_survey`.

The deposit-priority logic (`deposit_priority()`) stays: if a ship has cargo, depositing takes priority over the current objective (ship fulfills deposit, then resumes objective).

#### A3. Ship assignment bridge (temporary)

During Phase A, a temporary "assignment bridge" replaces `ShipTaskScheduler`:

New file: `crates/sim_control/src/agents/ship_assignment.rs`

```rust
/// Temporary bridge: performs the station-layer logic of assigning objectives
/// to idle ships. Replaced by StationAgent in Phase B.
pub(crate) struct ShipAssignmentBridge;
```

This contains the existing `ShipTaskScheduler` selection logic:
1. Collect idle ships (no objective, task is Idle)
2. Pre-compute sorted candidate lists (mine candidates by value, deep scan candidates, survey sites by distance) — existing Schwartzian transforms
3. Iterate idle ships in sorted order, assign objectives using the shared-iterator pattern (prevents double-assignment)
4. Store assigned objectives into each ship agent's `objective` field

The bridge runs **before** ship agents in the execution order. It's a flat behavior that coordinates assignment, then ship agents execute.

#### A4. Wire up and validate

Modify `AutopilotController` to use the new system:

```rust
pub struct AutopilotController {
    /// Phase A: flat behaviors minus ShipTaskScheduler
    behaviors: Vec<Box<dyn AutopilotBehavior>>,
    /// Phase A: ship assignment bridge + ship agents
    ship_assignment: ShipAssignmentBridge,
    ship_agents: BTreeMap<ShipId, ShipAgent>,
    owner: PrincipalId,
}
```

Execution order in `generate_commands`:
1. Run flat behaviors (9 remaining, in order — includes `ShipFitting`)
2. Run `ShipAssignmentBridge` (assigns objectives to idle ship agents)
3. Run ship agents in `BTreeMap` order (each converts objective to commands)

**Ship agent lifecycle:** When a new ship appears in `state.ships` that isn't in `ship_agents`, create a `ShipAgent` for it. When a ship disappears, remove its agent.

**Validation:**
- Behavioral equivalence test: run old system and new system with same inputs, assert identical commands
- Determinism canary must pass
- `progression.rs` integration test must pass
- sim_bench regression with baseline scenario

### Phase B: Station Agent Consolidation (5 tickets)

#### B1. Define StationAgent

New file: `crates/sim_control/src/agents/station_agent.rs`

```rust
pub(crate) struct StationAgent {
    station_id: StationId,
    /// LabAssignment cross-tick cache (per-station instance)
    lab_cache: LabAssignmentCache,
}

struct LabAssignmentCache {
    cached_eligible: HashMap<ResearchDomain, Vec<TechId>>,
    last_unlocked_count: usize,
    initialized: bool,
}
```

The station agent has ordered sub-concerns (methods, not separate trait objects):
1. `manage_modules()` — from `StationModuleManager` (skips propellant-role modules)
2. `assign_labs()` — from `LabAssignment` (uses `lab_cache`)
3. `assign_crew()` — from `CrewAssignment`
4. `recruit_crew()` — from `CrewRecruitment`
5. `import_components()` — from `ThrusterImport`
6. `jettison_slag()` — from `SlagJettison`
7. `export_materials()` — from `MaterialExport`
8. `manage_propellant()` — from `PropellantPipeline`
9. `fit_ships()` — from `ShipFitting` (moved from flat behaviors)
10. `assign_ship_objectives()` — from `ShipAssignmentBridge` (absorbed)

Each method receives `&self` or `&mut self` (for cache), `&GameState`, `&GameContent`, station-specific state reference, and `&mut next_id`. Each returns `Vec<CommandEnvelope>`.

#### B2. Migrate station-scoped behaviors (3 groups)

**Group 1 — Module management:** `StationModuleManager`, `PropellantPipeline` → `manage_modules()` + `manage_propellant()`. Preserve the role-based ownership coordination.

**Group 2 — Research + Crew:** `LabAssignment`, `CrewAssignment`, `CrewRecruitment` → `assign_labs()` + `assign_crew()` + `recruit_crew()`. Lab cache moves to station agent state.

**Group 3 — Economy + Fleet:** `ThrusterImport`, `SlagJettison`, `MaterialExport`, `ShipFitting` → respective methods. Trade-unlock gating (`state.meta.tick < trade_unlock_tick()`) applied once at the station agent level, gating all economy methods.

#### B3. Absorb ShipAssignmentBridge into StationAgent

The `assign_ship_objectives()` method replaces the bridge. The station agent knows its assigned ships and performs capability-aware assignment:
- Ship capabilities derived from hull + fitted modules (e.g., has mining equipment = can mine)
- Assignment logic: iterate priorities, find best-matching idle ship, emit objective
- The shared-iterator deduplication pattern moves here (station assigns one asteroid per ship)

#### B4. Update AutopilotController

```rust
pub struct AutopilotController {
    station_agents: BTreeMap<StationId, StationAgent>,
    ship_agents: BTreeMap<ShipId, ShipAgent>,
    owner: PrincipalId,
}
```

`generate_commands`:
1. Sync agent maps with state (create/remove agents for new/removed stations/ships)
2. Run station agents in `BTreeMap` order → collects station commands + ship objectives
3. Deliver ship objectives to ship agents
4. Run ship agents in `BTreeMap` order → collects ship commands
5. Return all commands

No flat behaviors remain. `default_behaviors()` is removed.

#### B5. Remove legacy code and validate

- Remove `AutopilotBehavior` trait (or keep as alias if useful)
- Remove all standalone behavior structs
- Remove `ShipAssignmentBridge`
- Remove `default_behaviors()`
- Full test suite: behavioral equivalence, determinism canary, progression, sim_bench regression

## System-Wide Impact

### Interaction Graph

`sim_cli` / `sim_daemon` call `AutopilotController::generate_commands()` (via `CommandSource` trait) → internally: station agents iterate modules/ships/inventory → emit `CommandEnvelope`s + `ShipObjective`s → ship agents convert objectives to `AssignShipTask` commands → all commands returned as flat `Vec` → `sim_core::tick()` applies them.

No change to callers. `CommandSource` trait signature unchanged. Command types unchanged. `tick()` unchanged.

### Error & Failure Propagation

Ship agents validate objectives before acting. Invalid objective (asteroid depleted, station removed) → objective cleared, ship goes idle, station reassigns next tick. No panics on stale references.

### State Lifecycle Risks

Agent maps (`station_agents`, `ship_agents`) are transient — rebuilt/synced from `GameState` every tick. `LabAssignmentCache` is the only cross-tick state, and it's self-rebuilding (initialized from scratch on first access). No serialization needed — agent state is reconstructable.

### API Surface Parity

`CommandSource` trait: unchanged. `AutopilotController::new()`: unchanged API, different internals. sim_bench scenario overrides: unchanged (override constants/modules, not agent internals). Phase C will add strategic config overrides.

### Integration Test Scenarios

1. **Multi-ship deduplication:** 3 idle ships, 2 minable asteroids → only 2 ships assigned mining, third surveys or goes idle
2. **Objective invalidation:** Ship mining asteroid, asteroid depletes → ship goes idle next tick, gets new objective
3. **Ship construction:** Shipyard produces new ship mid-run → ship agent created, station assigns objective
4. **Propellant coordination:** Propellant modules managed exclusively by propellant sub-concern, module manager skips them
5. **Progression regression:** 4000+ tick run produces same final state as current system

## Acceptance Criteria

### Functional Requirements

- [ ] Ship agents convert objectives to commands autonomously (transit, refuel, mine, deposit, survey, deep scan)
- [ ] Station agents consolidate all 9 remaining behaviors as ordered sub-concerns
- [ ] Capability-aware ship assignment: station checks ship hull/fittings before assigning objectives
- [ ] Double-assignment prevention preserved (station-layer shared-iterator pattern)
- [ ] PropellantPipeline / StationModuleManager coordination preserved via role-based ownership
- [ ] LabAssignment cross-tick cache works correctly per station agent instance
- [ ] Trade-unlock gating applied once at station agent level
- [ ] New ships/stations get agents automatically; removed entities lose their agents

### Non-Functional Requirements

- [ ] Determinism preserved: same seed produces identical state (determinism canary passes)
- [ ] No performance regression: sim_bench throughput within 5% of current
- [ ] `CommandSource` trait API unchanged — sim_cli, sim_daemon, sim_bench require zero changes

### Quality Gates

- [ ] Behavioral equivalence test passes (old system output == new system output for same inputs)
- [ ] `progression.rs` integration test passes at each migration step
- [ ] sim_bench baseline regression passes
- [ ] No new clippy warnings
- [ ] Test coverage maintained at 83%+

## Dependencies & Prerequisites

- Hull+slot system defines `hull_id` and `fitted_modules` on `ShipState` — ship capability evaluation depends on this. If hull+slots isn't merged yet, ship capability checks in Phase B can be stubbed (all ships can do all tasks, matching current behavior).
- No external dependencies. Pure internal refactor of sim_control.

## Risk Analysis & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Behavioral divergence during migration | Medium | High | Behavioral equivalence test + determinism canary at every step |
| Performance regression from agent overhead | Low | Medium | Agents are simple structs, no allocation overhead. Benchmark with sim_bench. |
| Ordering change breaks determinism | Medium | High | BTreeMap iteration order documented and tested. Command ID assignment verified. |
| Over-engineering the trait/objective system | Medium | Medium | Start with concrete types, no generics. Generalize in Phase C when we know more. |

## Future Considerations (Phases C+D — deferred, not planned here)

### Phase C: Strategic Layer

- Define `StrategicConfig` schema (priority weights per resource, expansion targets, fleet size goals)
- Build rule interpreter: config + current state → `StationObjective` per station
- Station objectives influence (not replace) housekeeping: e.g., `PrioritizeResource(Iron)` increases iron mining assignment weight
- Config interface consumed by: hand-tuning, sim_bench parameter sweep, MCP advisor, eventually LLM
- sim_bench scenario files gain `strategic_config` override section

### Phase D: Multi-station + Intermediate Layers

- Station agents work independently at different orbital bodies with different resource profiles
- Ship-to-station assignment: ships initially owned by building station, strategic layer can reassign
- Inter-station objectives: `TransferResource { from, to, resource, amount }`, `SupplyChain { source_station, sink_station, resource }`
- **Sector layer:** Groups stations by orbital region. Coordinates resource flow between stations in a sector. Accepts strategic objectives, emits station objectives. Slot between strategic and station — no code changes to either.
- **Squadron layer:** Groups ships on shared missions (mining convoy, exploration fleet). Accepts station objectives for the group, coordinates member ship objectives. Slot between station and ship — no code changes to either.
- The agent trait from Phase A+B supports these naturally: each new layer implements the trait, accepts objectives from above, emits objectives downward.

### Trait Generalization (after Phase B, before Phase C)

After Phase B completes, evaluate whether the `Agent` trait needs:
- Generic objective type parameter (`Agent<O>`) for type-safe layer composition
- Associated type (`type Objective`) for per-layer objective enums
- Layer composition configuration (tree structure describing the hierarchy)

**Do not design this now.** Phase A+B experience will reveal what's actually needed.

## Documentation Plan

- Update `docs/reference.md` if `AutopilotController` public API changes (unlikely — internal refactor)
- Update CLAUDE.md "Architecture" section to describe the agent hierarchy
- Update `.claude/skills/rust-sim-core.md` if sim_control conventions change

## Sources & References

### Origin

- **Origin document:** [docs/brainstorms/hierarchical-agent-decomposition-requirements.md](docs/brainstorms/hierarchical-agent-decomposition-requirements.md) — Key decisions carried forward: objective passing over blackboard/events, one agent per station, ships have aptitudes with stations allocating, config+rules hybrid for strategy, incremental migration.

### Internal References

- Current autopilot: `crates/sim_control/src/behaviors.rs` (10 behaviors, shared helpers)
- AutopilotController: `crates/sim_control/src/lib.rs:19`
- Command types: `crates/sim_core/src/types/commands.rs:24`
- TaskKind enum: `crates/sim_core/src/types/state.rs:590`
- ShipState: `crates/sim_core/src/types/state.rs:260`
- StationState: `crates/sim_core/src/types/state.rs:435`
- AutopilotConfig: `crates/sim_core/src/types/content.rs:61`
- AI progression roadmap: `docs/plans/2026-03-23-code-quality-and-ai-progression-roadmap.md`

### Learnings Applied

- Determinism via sorted collections: `docs/solutions/logic-errors/deterministic-integer-arithmetic.md`
- AHashMap non-determinism in module installation: `docs/solutions/patterns/molten-materials-thermal-container-system.md`
- Autopilot proximity-blind selection bug: `docs/solutions/patterns/multi-epic-project-execution.md`
- Crew system autopilot patterns: `docs/solutions/patterns/crew-system-multi-ticket-implementation.md`
- Cross-layer migration: `docs/solutions/patterns/hierarchical-polar-coordinate-migration.md`
- Function decomposition patterns: `docs/solutions/patterns/batch-code-quality-refactoring.md`

## Ticket Breakdown

### Phase A: Ship Agent Extraction

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| A1 | Define ShipObjective enum and Agent trait | — | `objectives.rs`, `agents/mod.rs` |
| A2 | Implement ShipAgent | A1 | `agents/ship_agent.rs` — tactical autonomy |
| A3 | Ship assignment bridge | A1, A2 | `agents/ship_assignment.rs` — replaces ShipTaskScheduler |
| A4 | Wire up and validate | A2, A3 | Controller integration, equivalence tests, determinism canary |

### Phase B: Station Agent Consolidation

| Ticket | Title | Blocked By | Key Deliverable |
|--------|-------|------------|-----------------|
| B1 | Define StationAgent with sub-concern structure | A4 | `agents/station_agent.rs` |
| B2 | Migrate station behaviors (3 groups) | B1 | Module mgmt, research+crew, economy+fleet methods |
| B3 | Absorb ship assignment into StationAgent | B1, B2 | Capability-aware assignment, remove bridge |
| B4 | Update AutopilotController (remove flat behaviors) | B3 | Pure agent-based controller |
| B5 | Remove legacy code and final validation | B4 | Remove old behavior structs, full regression suite |
