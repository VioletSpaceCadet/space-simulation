---
title: Cross-station autopilot coordination — layered execution with hull-based filtering
category: pattern
date: 2026-04-05
tags:
  - autopilot
  - fleet-coordination
  - multi-station
  - ship-objectives
  - hull-classification
  - double-assignment
  - sim_control
problem_type: architectural-coordination
component: sim_control/agents
symptoms:
  - Ships double-assigned by both station agent and fleet coordinator
  - Mining ships pulled into logistics transfers instead of staying local
  - Vec::pop() consumes from the end so priority ordering must be reversed
  - Option::take() unconditionally clears the value regardless of pattern match outcome
---

## Problem

When multiple agent layers (FleetCoordinator, module delivery, station agents) all scan for idle ships and assign objectives, three coordination failures emerge:

1. **Double-assignment**: FleetCoordinator assigns a Transfer, then a station agent assigns Mine to the same ship in the same tick — last writer wins, silently overwriting the transfer.
2. **Role mismatch**: Mining barges get pulled into inter-station logistics when they should stay local. Dedicated haulers get assigned to mine asteroids.
3. **Two Rust footguns** make the coordination code subtly wrong even when the architecture is correct.

## Root Cause

The autopilot architecture (documented in `hierarchical-agent-decomposition.md`) runs agents in a fixed step order within `AutopilotController::generate_commands()`. Before VIO-596/598/599, all ship objective assignment happened in step 4 (per-station `assign_ship_objectives`). Adding cross-station coordinators required inserting new steps that compete for the same idle ship pool.

Without explicit ordering + filtering:
- Multiple layers see the same `objective.is_none()` ship and both assign
- Hull-agnostic filtering treats all ships equally regardless of specialization

## Solution: Layered Execution + Hull Tag Filtering

### Step ordering in `generate_commands`

```
1.   Lifecycle sync (create/remove agents for new/deleted entities)
2.   Station agents generate commands (modules, labs, crew, trade)
3.   Ground facility agents generate commands
3.5a FleetCoordinator: global supply/demand → Transfer objectives (VIO-598)
3.5b Module delivery: cross-station module transfers (VIO-596)
4.   Station agents assign ship objectives (mine, survey, deposit)
5.   Ship agents convert objectives to tactical commands
```

Steps 3.5a and 3.5b claim idle ships first. Step 4 only sees ships not yet claimed. The `ship_agents` map (`&mut BTreeMap<ShipId, ShipAgent>`) carries objective state between steps — step 4 checks `agent.objective.is_none()` and skips already-claimed ships.

### Hull tag filtering

Hull tags are content-driven strings in `hull_defs.json`:

| Hull | Tags | Role |
|------|------|------|
| `hull_mining_barge` | `["mining"]` | Local mining only |
| `hull_transport_hauler` | `["logistics"]` | Preferred for transfers |
| `hull_construction_vessel` | `["construction", "heavy"]` | Station deployment |
| `hull_general_purpose` | `[]` | Fallback for any role |

Each agent layer filters the idle ship pool:

- **FleetCoordinator + module_delivery**: Prefer `"logistics"` ships, accept untagged general-purpose, exclude `"mining"` ships
- **Station agent objectives**: Exclude `"logistics"` ships (reserved for transfers)

The filtering helper:

```rust
pub(crate) fn ship_has_hull_tag(
    ship: &ShipState,
    tag: &str,
    content: &GameContent,
) -> bool {
    content
        .hulls
        .get(&ship.hull_id)
        .is_some_and(|hull| hull.tags.iter().any(|t| t == tag))
}
```

### Priority ordering with `Vec::pop()`

Logistics ships must be consumed first. Since `pop()` takes from the **end**, logistics ships go **last** in the vec:

```rust
// General-purpose first, logistics last — pop() takes from end.
let mut available_ships: Vec<ShipId> = all_idle
    .into_iter()
    .filter(|id| /* exclude mining + logistics */)
    .chain(logistics_ships)  // logistics LAST = popped FIRST
    .filter(|id| ship_agents.get(id).is_some_and(|a| a.objective.is_none()))
    .collect();
```

### `Option::take()` trap in pattern matching

When the ship agent handles a Transfer objective, it must not destroy non-Transfer objectives:

```rust
// BUG: take() runs unconditionally — Mine objectives silently lost
if let Some(ShipObjective::Transfer { .. }) = self.objective.take() { ... }

// FIX: borrow via ref, clear explicitly after confirmed match
if let Some(ShipObjective::Transfer {
    ref from_station, ref to_station, ref items,
}) = self.objective
{
    let commands = vec![make_cmd(/* clone fields */)];
    self.objective = None; // Clear only after successful match
    return commands;
}
```

## When to Apply

Use this pattern whenever:

- **A new agent layer needs to assign ship objectives** alongside existing layers. Insert it at the correct step number and gate on `objective.is_none()`.
- **A new hull type is added** with a specialized role. Add the appropriate tag to `hull_defs.json` and update the filtering in each agent layer.
- **Building a priority-ordered consumer using `Vec::pop()`**. Remember: highest priority goes LAST.
- **Pattern-matching on `Option<Enum>` where only one variant should trigger action**. Use `ref` borrow, not `.take()`.

## Prevention Strategies

1. **PR review checklist item — Option::take()**: Grep diff for `.take()` inside `if let`, `match`, or `while let` conditions. The side effect executes regardless of which variant matches. Require borrow-then-clear separation.

2. **PR review checklist item — double-assignment**: Grep diff for `.objective = Some(`. Every assignment site must have an `is_none()` guard. Multiple agent layers assigning in the same tick without guards cause silent overwrites.

3. **PR review checklist item — Vec::pop() ordering**: When a Vec is consumed via `pop()`, verify sort order — `pop()` takes from the END. Prefer `sort + iterate` over `sort + pop-loop` for clarity.

4. **Content validation for hull tags**: Every hull_def should have at least one recognized role tag. Missing tags cause silent exclusion from coordination. Add a content validation test asserting tags are present.

## Test Patterns

### Double-assignment prevention

```rust
#[test]
fn pre_assigned_ship_not_reassigned_by_station_agent() {
    // Ship already has a fleet-level Transfer objective
    agent.objective = Some(ShipObjective::Transfer { ... });
    // Run station-level assignment
    assign_ship_objectives(&mut ship_agents, ...);
    // Ship must keep its Transfer objective
    assert!(matches!(agent.objective, Some(ShipObjective::Transfer { .. })));
}
```

### Hull tag filtering

```rust
#[test]
fn mining_ship_excluded_from_transfers() {
    // Ship with hull_mining_barge (tagged "mining")
    evaluate_and_assign(&mut ship_agents, ...);
    assert!(agent.objective.is_none(), "mining ships should not do logistics");
}

#[test]
fn logistics_ship_preferred_over_general_purpose() {
    // Both a hauler and a general-purpose ship are idle
    evaluate_and_assign(&mut ship_agents, ...);
    assert!(hauler_agent.objective.is_some(), "logistics ship should be preferred");
}
```

## Related Documentation

### Directly related

- **[`hierarchical-agent-decomposition.md`](./hierarchical-agent-decomposition.md)** — Foundational architecture this pattern extends. **STALE**: execution order diagram missing steps 3.5a/3.5b; "multi-station support" note is now addressed.
- **[`autopilot-strategic-layer-foundation-patterns.md`](./autopilot-strategic-layer-foundation-patterns.md)** — Strategic layer above fleet coordination. Needs cross-reference to this doc.
- **[`chained-task-fuel-accounting.md`](./chained-task-fuel-accounting.md)** — Fuel pre-deduction pattern used by `Command::TransferItems` that the fleet coordinator issues. Up-to-date — correctly forecasts VIO-596/598/599/600.

### Adjacent

- **[`p5-station-construction-patterns.md`](./p5-station-construction-patterns.md)** — Pattern C2 (Transit→X chained task). **STALE**: caveats section missing fuel accounting warning for nested Transit chains.
- **[`scoring-and-measurement-pipeline.md`](./scoring-and-measurement-pipeline.md)** — VIO-600 extends with `transfer_volume_kg` and `transfer_count` metrics.

### Related PRs

- **[#476](https://github.com/VioletSpaceCadet/space-simulation/pull/476)** — VIO-596: Module delivery coordinator
- **[#477](https://github.com/VioletSpaceCadet/space-simulation/pull/477)** — VIO-598: FleetCoordinator supply/demand
- **[#478](https://github.com/VioletSpaceCadet/space-simulation/pull/478)** — VIO-599: Hull tag-based ship role classification
- **[#479](https://github.com/VioletSpaceCadet/space-simulation/pull/479)** — VIO-600: Supply chain metrics
