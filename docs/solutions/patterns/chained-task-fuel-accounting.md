---
title: Chained Task Fuel Accounting
category: pattern
date: 2026-04-05
tags:
  - autopilot
  - ship-tasks
  - fuel-accounting
  - balance-integrity
  - pr-review
problem_type: silent-balance-distortion
component: sim_core/ship_tasks
symptoms:
  - Ships complete multi-leg journeys burning zero propellant on one or more legs
  - Fuel metrics in sim_bench under-report consumption for commands using chained tasks
  - Balance tuning drifts because fuel pressure is absent from scripted objectives
  - Tests pass because unit fixtures use empty hulls where mass-based fuel math collapses to zero
  - Downstream tickets inherit the accounting hole and compound the distortion
---

## Problem

When a command chains multiple `TaskKind::Transit` variants into a single task graph via the `then` field (e.g., `Transit(src) → Pickup → Transit(dst) → Deposit` in `Command::TransferItems`), the sim has exactly one place that deducts propellant for a transit leg: `apply_ship_assignments` calls `deduct_transit_fuel` at the moment the ship is assigned a fresh objective. `resolve_transit` itself does **not** re-charge fuel when the next chained task is handed off — it only advances position and fires completion events.

This means any command handler that builds a multi-leg chain and bypasses `apply_ship_assignments` (the common case, since handlers like `handle_transfer_items` construct the task graph directly) will burn fuel for zero, one, or some legs but never for all of them. The ship arrives at its destination with the same propellant it started with, and every downstream balance measurement is silently wrong.

This is a trap because nothing panics, nothing logs a warning, and tests using zero-mass hull fixtures return `fuel = 0` regardless of distance — so the hole is invisible until a realistic integration test or sim_bench scenario exposes it. The trap compounds across features: VIO-595's Transfer was about to be followed by four downstream tickets (VIO-596, VIO-598, VIO-599, VIO-600) that all assumed fuel pressure on inter-station movement.

The pattern applies anywhere a command handler builds a chained task graph containing more than one `Transit` node.

## Root Cause

`resolve_transit` in `crates/sim_core/src/tasks.rs:305` is the arrival handler for a `TaskKind::Transit { destination, total_ticks, then }`. When the transit completes it teleports the ship to `destination`, emits `ShipArrived` + `DataGenerated`, then installs `then` as the ship's next task. **It never touches `ship.propellant_kg`.** Fuel is deducted exactly once per command, at assignment time by `apply_ship_assignments` in `crates/sim_core/src/commands.rs:2275`, which only inspects the outermost `TaskKind` — the one the command produces.

This is the right design for the common case. A single `Transit(A) → Mine` burns fuel exactly once when the command is issued, not again when `resolve_transit` hands off to `Mine` on arrival. Re-charging inside `resolve_transit` would double-bill re-entrant transit resolution and couple arrival logic to propulsion state for every task kind. Caching the cost at assignment time is cheap and avoids recomputing leg geometry on every tick.

The trap: any command that produces a **chain of two or more `Transit` variants** (e.g. `Transit(src) → Pickup → Transit(dst) → Deposit`) gets only the first leg charged. Every subsequent leg is free fuel — silent, determinism-preserving, and invisible without a targeted test.

## Solution: Pre-deduction Pattern

A command handler that builds a multi-leg transit chain must compute fuel for **every** leg upfront, check affordability **before** mutating anything, and deduct the sum atomically. The canonical implementation is `handle_transfer_items` in `crates/sim_core/src/commands.rs`:

```rust
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_transfer_items(
    state: &mut GameState,
    content: &GameContent,
    ship_id: &ShipId,
    from_station: &crate::StationId,
    to_station: &crate::StationId,
    items: &[crate::TradeItemSpec],
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    // ... filter + look up src_position, dst_position, ship ...

    // Pre-compute both travel legs and their fuel costs.
    let src_travel = travel_ticks_to_build_site(state, content, ship, &src_position);
    let dst_travel = travel_ticks_between(state, content, ship, &src_position, &dst_position);
    let total_fuel = compute_transfer_fuel(
        state, content, ship,
        &src_position, &dst_position,
        src_travel, dst_travel,
    );

    // Affordability check — reject atomically, no partial mutation.
    if content.constants.fuel_cost_per_au > 0.0
        && total_fuel > 0.0
        && ship.propellant_kg < total_fuel
    {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::InsufficientPropellant {
                ship_id: ship_id.clone(),
                destination: dst_position,
            },
        ));
        return false;
    }

    // ... build_transfer_task_chain(...) elided ...

    // Atomic deduction once affordability is confirmed.
    let Some(ship_mut) = state.ships.get_mut(ship_id) else {
        return false;
    };
    if content.constants.fuel_cost_per_au > 0.0 && total_fuel > 0.0 {
        ship_mut.propellant_kg -= total_fuel;
        state.propellant_consumed_total += f64::from(total_fuel);
    }

    // ... assign task + emit PropellantConsumed / TaskStarted ...
    true
}
```

Key properties:
1. **Geometry and fuel computed with the ship in its pre-task position** — no transient state to unwind on rejection.
2. **The `propellant_kg < total_fuel` branch returns `false` before any mutation** so the command is a no-op on rejection.
3. **A single `PropellantConsumed` event is emitted with the summed cost**, not one per leg — matches the semantic "one command = one charge event."

## Helper: `compute_transfer_fuel`

This is the reusable helper — it delegates to `propulsion::compute_transit_fuel` per leg and applies the same `FuelEfficiency` modifier that `deduct_transit_fuel` uses, guaranteeing parity with single-leg commands:

```rust
/// Compute total fuel required for a two-leg transfer (ship→src +
/// src→dst). Returns 0 for zero-length legs. Applies the global
/// `FuelEfficiency` modifier to match `deduct_transit_fuel`.
fn compute_transfer_fuel(
    state: &GameState,
    content: &GameContent,
    ship: &crate::ShipState,
    src_position: &crate::Position,
    dst_position: &crate::Position,
    src_travel: u64,
    dst_travel: u64,
) -> f32 {
    let fuel_efficiency = state
        .modifiers
        .resolve_f32(crate::modifiers::StatId::FuelEfficiency, 1.0);
    let leg1 = if src_travel == 0 {
        0.0
    } else {
        crate::propulsion::compute_transit_fuel(
            ship, &ship.position, src_position,
            content, &state.body_cache,
        ) * fuel_efficiency
    };
    let leg2 = if dst_travel == 0 {
        0.0
    } else {
        crate::propulsion::compute_transit_fuel(
            ship, src_position, dst_position,
            content, &state.body_cache,
        ) * fuel_efficiency
    };
    leg1 + leg2
}
```

Zero-length legs (ship already co-located with the endpoint) short-circuit to `0.0` so a co-located pickup doesn't spuriously charge.

## When to Apply

Use the pre-deduction pattern whenever **any** of these conditions hold:

- **The command produces a `TaskKind` tree containing two or more `Transit` variants chained via `then`.** `apply_ship_assignments` only sees the outermost kind; every nested `Transit` is free fuel unless pre-charged.
- **The command handler runs its own task assignment path** (e.g. `handle_transfer_items` and `handle_deploy_station` write `ship.task` directly) instead of delegating to `apply_ship_assignments`. Even single-leg commands on this path must pre-compute + deduct manually.
- **Future multi-leg logistics commands**: FleetCoordinator routing (VIO-598), autopilot module delivery (VIO-596), logistics ship assignment (VIO-599), supply chain metrics (VIO-600), round-trip refueling, multi-stop resupply chains, station-to-station crew rotation. Any handler that composes more than one `Transit` node in a single command must enumerate every leg's cost, sum them, reject atomically on insufficiency, and emit a single `PropellantConsumed` event with the total.

**The discipline is: one command = one fuel check = one deduction = one event, regardless of leg count.**

## Prevention Strategies

1. **Code review checklist item.** Add to the PR review checklist (and the `pr-reviewer` agent prompt): *"Does this command construct a `TaskKind::Transit` whose `then` field nests another `TaskKind::Transit`? If yes, confirm the command handler pre-deducts propellant for every leg before assignment, and that rejection occurs atomically when the total is unaffordable."*

2. **Grep-based detection.** During review, scan the diff for nested transit patterns:
   ```bash
   rg -U 'TaskKind::Transit\s*\{[^}]*then:\s*Box::new\(\s*TaskKind::Transit'
   ```
   Any hit must be accompanied by an explicit multi-leg fuel calculation in the handler.

3. **Mandatory test pattern.** Any command handler that emits a multi-leg task must ship with (a) a test asserting `propellant_kg` strictly decreases by the expected multi-leg total, and (b) a rejection test asserting the command is refused when propellant is below the total. Single-leg-style assertions are insufficient.

4. **Architecture consideration.** Extending `resolve_transit` to re-charge on chained hand-off would solve the trap globally but breaks two invariants: (i) fuel is charged atomically at command time so rejection is all-or-nothing, and (ii) determinism for replay is simpler when charges happen at known sites. If migration ever happens, it must be a single PR touching every multi-leg command in lockstep with sim_bench regression runs.

5. **Doc pointer.** Consider adding a `// NOTE:` comment above the `then` field in `TaskKind::Transit` pointing at this doc, and a one-line reminder in `apply_ship_assignments` where the single-leg deduction happens.

## Test Pattern: Spatial Two-Station Fixture

The fixture must produce non-zero fuel costs AND non-zero distance, otherwise the test silently passes even when the bug is present. Four things matter (all four were discovered the hard way during VIO-595 review):

```rust
fn spatial_two_station_state() -> (GameContent, GameState) {
    let mut content = transfer_content();

    // 1. Non-zero hull mass — empty hulls make ship.dry_mass_kg = 0,
    //    which collapses rocket-equation fuel cost to zero even at
    //    1 AU distance. The rejection test WILL silently pass.
    content.hulls.insert(
        crate::HullId("hull_general_purpose".to_string()),
        crate::HullDef {
            id: crate::HullId("hull_general_purpose".to_string()),
            name: "General Purpose".to_string(),
            mass_kg: 5000.0,
            cargo_capacity_m3: 50.0,
            base_speed_ticks_per_au: 2133,
            base_propellant_capacity_kg: 10_000.0,
            slots: vec![],
            bonuses: vec![],
            required_tech: None,
            tags: vec![],
        },
    );

    // 2. Place bodies at different radius_au_um — bodies with
    //    radius 0 project to origin regardless of angle.
    content.solar_system.bodies = vec![
        /* zone_a at radius 0 */,
        /* zone_b at radius 1_000_000 (1 AU away) */,
    ];
    content.constants.derive_tick_values();

    let mut state = test_fixtures::base_state(&content);
    state.body_cache = crate::build_body_cache(&content.solar_system.bodies);

    // ... place earth station at zone_a, mars station at zone_b ...

    let ship = state.ships.values_mut().next().unwrap();
    ship.position = zone_a;
    // 3. MUST recompute after adding hull — cached cargo_capacity /
    //    speed / propellant_cap diverge from content otherwise, and
    //    the debug_assert in engine::tick panics.
    crate::commands::recompute_ship_stats(ship, &content);
    // 4. Seed full tank LAST (after recompute_ship_stats sets the cap).
    ship.propellant_kg = ship.propellant_capacity_kg;

    (content, state)
}
```

For rejection tests, set `ship.propellant_kg = 0.0` after the recompute. The full tank stays as the default for success tests.

## Test Cases to Write

Every command handler that emits a chained `TaskKind::Transit` must include these four tests:

1. **Fuel deducted for all legs.** Snapshot `propellant_kg` before the command, dispatch, assert `after < before`. Cross-check against the handler's computed multi-leg total.
2. **Rejected on insufficient propellant.** Set `ship.propellant_kg = 0.0`. Dispatch the command, assert it is rejected, assert ship task/position/propellant/inventory and source/destination station inventories are all unchanged, and assert an `InsufficientPropellant` event was emitted.
3. **Collapsed case — ship already at source.** Place the ship at the first waypoint before dispatching. Verify the handler skips the zero-distance leg cleanly (both in task construction and fuel calculation) — no free-fuel exploit where nesting a zero-length leg bypasses the charge.
4. **Lifecycle completion.** Run the full task through `tick()` until every leg resolves, then assert the payload (items, module, crew) arrived at the final destination and intermediate state is consistent.

See `crates/sim_core/src/tests/transfer.rs` for the VIO-595 reference implementation (16 tests covering all four categories).

## Related Documentation

### Directly related pattern docs

- **[`p5-station-construction-patterns.md`](./p5-station-construction-patterns.md)** — Contains **Pattern C2: Transit→X chained task for atomic "go do thing"**. This is the single most relevant existing doc: it explains the `TaskKind::Transit { then: Box<TaskKind> }` recursive enum, the co-located fast path (skip Transit when `travel_ticks == 0`), and that `resolve_transit` picks up `then` in one step. **STALE ON FUEL ACCOUNTING** — its caveats section does not mention that `resolve_transit` does not re-charge fuel on chained hand-off. The new learning is a missing 4th caveat/gotcha that belongs directly in that section. Recommend a follow-up pass to add a cross-reference to this doc.

- **[`stat-modifier-tech-expansion.md`](./stat-modifier-tech-expansion.md)** — References `deduct_transit_fuel()` as the single-leg deduction helper. Not contradictory, but the only existing mention of the fuel-deduction call site, so this pattern doc cross-links there for readers tracing `FuelEfficiency` modifiers.

### Adjacent pattern docs

- **[`multi-ticket-satellite-system-implementation.md`](./multi-ticket-satellite-system-implementation.md)** — Describes `resolve_transit_payload` and a shared constructor pattern between the launch-transit handler and a deploy command. Closest parallel precedent: "command builds transit chain from a command path." Does not discuss fuel.
- **[`autopilot-strategic-layer-foundation-patterns.md`](./autopilot-strategic-layer-foundation-patterns.md)** — References `self.propellant` and parameterized low-fuel/high-fuel tests on the strategy evaluator. Strategic-layer context for readers looking at fuel checks higher in the call stack.
- **[`hierarchical-agent-decomposition.md`](./hierarchical-agent-decomposition.md)** — ShipAgent tactical command list `(transit, mine, deposit, refuel)`. Useful context for readers learning how ship tasks compose.

### Related GitHub PRs

- **[owner/repo#475](https://github.com/VioletSpaceCadet/space-simulation/pull/475) — VIO-595: ShipObjective::Transfer** — The direct source of this learning. Introduces `TaskKind::Pickup`, `Command::TransferItems`, and `handle_transfer_items` with the full pre-deduction pattern in its second commit (review fix).
- **[owner/repo#474](https://github.com/VioletSpaceCadet/space-simulation/pull/474) — VIO-603: Score dimension extension — multi-station bonus** — Merged the same day; adjacent P5 context.
- **Propellant system lineage** — VIO-117 (propellant mass helpers), VIO-118 (`compute_transit_fuel`), VIO-119 (deduct propellant on transit start — the foundational single-leg contract), VIO-120 (Refuel task), VIO-121 (autopilot propellant check + refuel fallback), VIO-122 (opportunistic refuel on deposit), VIO-136 (`FuelEfficiency` modifier), VIO-464 (content-driven propellant element). The new learning is essentially "VIO-119's contract didn't anticipate chained transits; VIO-595 is the first consumer that has to compensate."
