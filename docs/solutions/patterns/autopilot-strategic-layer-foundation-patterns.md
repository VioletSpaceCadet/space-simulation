---
title: Autopilot Strategic Layer Foundation — P6 Phase C Multi-Ticket Patterns
category: patterns
date: 2026-04-05
tags: [strategy-config, multi-ticket, serde-backcompat, event-sync, nested-overrides, autopilot, mcp-tools, safe-slice, cross-layer-command, rule-interpreter]
problem_type: "multi-ticket-feature-implementation"
component: "sim_core, sim_control, sim_bench, sim_daemon, mcp_advisor, ui_web"
severity: medium
related_tickets: [VIO-479, VIO-605, VIO-480, VIO-482, VIO-483, VIO-484]
---

# Autopilot Strategic Layer Foundation — P6 Phase C Multi-Ticket Patterns

## Problem

Implemented the P6 "Phase C" safe slice — six tickets that established the `StrategyConfig` / `PriorityWeights` / `StrategyMode` types, the rule-interpreter `evaluate_strategy` with cache gating, hysteresis and temporal bias, sim_bench override plumbing, daemon command + event + HTTP surface, and MCP advisor tools. This is the foundational layer of the P6 autopilot strategic system — it builds everything from types up through the MCP interface while **deliberately stopping before any consumer actually reads the output**. The station-agent wiring (VIO-481 and beyond) lives in files being actively reshaped by the parallel P5 station-frame project, so it has to wait.

The non-obvious part was scoping. Six tickets ran end-to-end in a single session, but the seventh (VIO-481) and all downstream work could not. Several latent issues surfaced only because the review checklist was enforced rigorously at every step — runtime state masquerading as catalog state, a unit-variant serde trap that broke FE dispatch, a nested-path error with no key context, and a `HashMap` ordering non-determinism in the sim_bench override routing.

## Root Cause / Design Rationale

This is not a single bug. It is a 6-ticket feature build with a careful scoping decision and a dozen cross-cutting implementation choices, each of which deserves future reference.

**Why the "safe slice" approach worked.** The P6 initiative has two halves: (a) the strategic layer — types, interpreter, command path, overrides, MCP tools — and (b) the station-agent consumption of those priorities (VIO-481+). Half (b) touches the same files as in-progress P5 station-frame work, so interleaving them would have produced merge conflicts on `sim_control/src/station_agent.rs`. The six tickets were chosen so every landed change lives in files P5 does not touch: new `types/strategy.rs`, new `strategy_interpreter.rs`, a new `Command`/`Event` variant, a new daemon route, and new MCP tools. The interpreter is wired into `AutopilotController::generate_commands` but its output is bound to `_priorities` — a deliberate unused-binding that preserves determinism and blocks downstream collisions. VIO-481 picks up the `evaluate_strategy` return value later.

**Why the cross-cutting decisions were made the way they were:**

- **`StrategyConfig` lives on `GameState`** (not in `Constants`) so it is mutable at runtime via the command queue, identical in lifecycle to every other sim state mutation. The seed comes from `GameContent.default_strategy` loaded from `content/strategy.json`.
- **Rule interpreter runtime state lives on the controller, not `GameState`.** Cache, dirty flag, `last_serviced` ticks are per-agent ephemera — they must not be persisted in saves or replayed.
- **`Command::SetStrategyConfig` uses full-replacement semantics** (not merge) because `#[serde(default)]` on every field already gives partial-update ergonomics at the deserialization layer — clients send a full `StrategyConfig` built from whatever subset of fields they included plus defaults for the rest.
- **`strategy-v2` is a superset**, absorbing four more `AutopilotConfig` behavioral parameters (`refuel_max_pct`, `shipyard_component_count`, `power_deficit_threshold_kw`, `crew_hire_projection_minutes`) with defaults that byte-match the existing `Constants.autopilot_*` values — switching consumers is a no-op behavior change.

## Patterns (11)

### Pattern 1: Type alias for input/output symmetry

`crates/sim_core/src/types/strategy.rs:354`

```rust
pub type ConcernPriorities = PriorityWeights;
```

`PriorityWeights` is used both as user-configured input (`StrategyConfig.priorities`) and as the interpreter's output. A type alias is strictly better than parallel types: `to_vec` / `from_vec` / `LEN` / `fields_mut` / `clamp_unit` all work on either side, the optimizer interface only needs one implementation, and the field order can never drift between the two shapes. Trade-off: the two semantic roles look the same in type signatures — mitigated by naming the alias and calling it out in doc comments.

### Pattern 2: `fields_mut()` to avoid 8-line iteration duplication

`crates/sim_core/src/types/strategy.rs:181-192`

```rust
pub fn fields_mut(&mut self) -> [&mut f32; Self::LEN] {
    [
        &mut self.mining,
        &mut self.survey,
        &mut self.deep_scan,
        &mut self.research,
        &mut self.maintenance,
        &mut self.export,
        &mut self.propellant,
        &mut self.fleet_expansion,
    ]
}
```

Hysteresis (`strategy_interpreter.rs:178`) and temporal bias (`strategy_interpreter.rs:358`) both need element-wise in-place updates zipped against a second sequence. Without this helper each call site repeats the 8-line field list, and drift between call sites (one site forgets a new field) produces silent weight-update bugs. The helper is pinned against drift by a dedicated test: `fields_mut_matches_to_vec_order` (`types/strategy.rs:530`).

**Rule:** Whenever a struct is traversed element-wise more than once, extract `fields_mut()` / `fields()` helpers on the struct itself and pin field order with a regression test that ties the helper to `to_vec`.

### Pattern 3: Cache gating with dirty flag + `game_minutes_to_ticks`

`crates/sim_control/src/strategy_interpreter.rs:32` and `80-87`

```rust
const STRATEGY_EVAL_INTERVAL_MINUTES: u64 = 600;

fn needs_recompute(&self, current_tick: u64, eval_interval_ticks: u64) -> bool {
    if self.cached_priorities.is_none() || self.strategy_dirty {
        return true;
    }
    current_tick.saturating_sub(self.last_strategy_tick) >= eval_interval_ticks
}
```

Three recompute triggers (empty cache, dirty flag, interval elapsed) merged into one predicate. The interval is defined in **game minutes** and converted through `content.constants.game_minutes_to_ticks(...)` at the call site (`strategy_interpreter.rs:105-108`) — never hardcoded. A scenario that rescales `minutes_per_tick` from 60 to 1 automatically preserves the 10-game-hour cadence. The `.max(1)` guards against minutes-per-tick configurations that would round to 0.

### Pattern 4: Catalog counts come from `content`, never from runtime state

`crates/sim_control/src/strategy_interpreter.rs:272-276`

```rust
total_techs: u32::try_from(content.techs.len()).unwrap_or(u32::MAX),
```

**Bug caught during review.** An earlier draft read `state.research.unlocked.len() + state.research.evidence.len()` as the total tech count — plausible-looking code that silently returns 0 on a fresh run (no unlocks, no evidence). That inverts the research urgency signal: instead of "no techs unlocked yet → max urgency," you get "no catalog → zero urgency." The fix threads `&GameContent` into `compute_aggregates` and reads `content.techs.len()` as the authoritative catalog count.

**Rule:** Runtime state is not the authoritative source for catalog cardinality — `GameContent` is. For any "how many X exist" question, route through `state.content.*.len()` or a dedicated catalog accessor.

### Pattern 5: Unit-variant event trap — always use empty struct variants

`crates/sim_core/src/types/events.rs:454`

```rust
/// Declared as an empty-struct variant (not a unit variant) so serde
/// serializes it as `{"StrategyConfigChanged": {}}` — matching the
/// object-shaped wire format every other Event variant uses. The FE
/// dispatch in `ui_web/src/utils.ts::getEventKey` assumes an object,
/// and `ci_event_sync.sh`'s struct-variant grep also depends on this.
StrategyConfigChanged {},
```

**Failure chain (caught during review):**

1. Author declares `Event::StrategyConfigChanged,` (unit variant).
2. `sim_core` emits it. Serde serializes unit variants as bare JSON strings: `"StrategyConfigChanged"`.
3. SSE delivers the envelope to the FE. `applyEvents.ts` reads `evt.event`, which is now the string `"StrategyConfigChanged"` rather than an object.
4. `getEventKey` in `ui_web/src/utils.ts:7-9` runs `Object.keys(event)[0] ?? 'Unknown'`. On a string primitive, `Object.keys("StrategyConfigChanged")` returns `["0","1","2",...]` (character indices). `[0]` returns `"0"`.
5. `EVENT_HANDLERS["0"]` is undefined → falls through the "unknown event" path. The `StrategyConfigChanged: noOp` handler is dead.

**Fix:** Declare as empty-struct variant `StrategyConfigChanged {}`. Serde emits `{"StrategyConfigChanged": {}}`, `Object.keys` returns `["StrategyConfigChanged"]`, dispatch routes correctly.

**Latent bonus bug:** `ci_event_sync.sh`'s grep only matched struct variants with `{...}`, so unit variants silently bypassed handler enforcement. Switching to the struct-variant form made the script actually count the variant — total went 76 → 77 on the fix PR.

### Pattern 6: `set_nested_path` for sim_bench overrides

`crates/sim_bench/src/overrides.rs:245-282`

```rust
fn set_nested_path(
    root: &mut serde_json::Map<String, serde_json::Value>,
    dotted_path: &str,
    value: serde_json::Value,
) -> Result<()> {
    let segments: Vec<&str> = dotted_path.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        bail!("empty path segment in '{dotted_path}'");
    }

    let mut current = root;
    for (i, segment) in segments.iter().enumerate() {
        let is_leaf = i == segments.len() - 1;
        if is_leaf {
            if !current.contains_key(*segment) {
                bail!(
                    "unknown field '{segment}'. Valid keys at this level: {}",
                    current.keys().map(String::as_str).collect::<Vec<_>>().join(", ")
                );
            }
            current.insert((*segment).to_string(), value);
            return Ok(());
        }
        let child = current
            .get_mut(*segment)
            .ok_or_else(|| anyhow::anyhow!("unknown field '{segment}' in path"))?;
        let serde_json::Value::Object(child_map) = child else {
            bail!("field '{segment}' is not an object; cannot descend for nested override");
        };
        current = child_map;
    }
    unreachable!("loop returns on leaf");
}
```

Walks dotted paths into a serialized `StrategyConfig`. Three explicit error cases: empty/empty-segment path, unknown leaf key (enumerates valid siblings in the error message for discoverability), and intermediate segment that is not an object. Scenario authors can now write `"strategy.priorities.mining": 0.9` and get a real error if they fat-finger `priorities.minng`.

### Pattern 7: Sort overrides by dot-depth to beat `HashMap` non-determinism

`crates/sim_bench/src/overrides.rs:216-220`

```rust
// Apply longer (more specific) paths last so a scenario mixing
// `strategy.priorities` with a sibling top-level key ends with the
// nested override winning regardless of HashMap iteration order.
let mut sorted_overrides: Vec<&(&str, &serde_json::Value, &str)> = overrides.iter().collect();
sorted_overrides.sort_by_key(|o| o.0.matches('.').count());
```

Scenarios pass overrides as `HashMap<String, Value>`. Iteration order is randomized per process. A scenario combining `"strategy.priorities": {...}` with `"strategy.priorities.mining": 0.9` could apply the specific nested key first and then get clobbered by the general sibling, or vice versa. Classic flake that only shows up in CI sharding. Sorting by `matches('.').count()` (dot depth) guarantees general overrides apply first, nested ones last — specific always wins. The regression test `test_strategy_override_specific_wins_over_general_regardless_of_map_order` loops 16 iterations to stress the discipline.

**Rule:** Any iteration over `HashMap` / `HashSet` that writes to externally-visible state must either use `BTreeMap` or explicit `sort_by` before the loop.

### Pattern 8: Context-preserving deserialize with applied-key list

`crates/sim_bench/src/overrides.rs:222-237`

```rust
for &&(stripped_key, value, original_key) in &sorted_overrides {
    set_nested_path(&mut map, stripped_key, value.clone())
        .with_context(|| format!("invalid strategy override key '{original_key}'"))?;
}

let applied_keys: Vec<&str> = sorted_overrides.iter().map(|o| o.2).collect();
*strategy = serde_json::from_value(serde_json::Value::Object(map)).with_context(|| {
    format!(
        "failed to deserialize StrategyConfig after applying overrides [{}]",
        applied_keys.join(", ")
    )
})?;
```

Two layers of `with_context`. Layer 1 catches bad paths during `set_nested_path`. Layer 2 catches shape violations during the final `from_value`: a fat-fingered `"strategy.priorities": 0.5` passes `set_nested_path` (top-level `priorities` key exists) but fails deserialize back into `PriorityWeights` (can't turn a bare `0.5` into an 8-field struct). Without the second context, the scenario author sees only "invalid type: floating point '0.5', expected struct PriorityWeights" with no hint which override broke the shape. The applied-key list in the error names exactly which overrides were in flight.

### Pattern 9: Command → Event pair for cross-layer mutation

`crates/sim_core/src/types/commands.rs:148-150` and `crates/sim_core/src/engine.rs:501-513`

```rust
SetStrategyConfig {
    config: crate::StrategyConfig,
},
```

```rust
Command::SetStrategyConfig { config } => {
    state.strategy_config = config.clone();
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::StrategyConfigChanged {},
    ));
}
```

The command carries the full config (full-replacement semantics); the event carries nothing. Clients that need the new config refetch from `GET /api/v1/strategy`. This keeps the SSE stream small — pushing a ~16-field config to every SSE consumer on every change would bloat the stream for data most clients don't render. Trade-off: any client that wants to be notified of changes needs a second HTTP round-trip. Acceptable because strategy changes are rare (minutes apart, not ticks apart).

### Pattern 10: Daemon POST wraps in `CommandEnvelope` on the existing queue

`crates/sim_daemon/src/routes.rs:359-379`

```rust
pub async fn strategy_post_handler(
    State(app_state): State<AppState>,
    Json(config): Json<sim_core::StrategyConfig>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (command_id, tick) = { /* ... */ };
    let envelope = CommandEnvelope {
        id: command_id,
        issued_by: PrincipalId("principal_player".to_string()),
        issued_tick: tick,
        execute_at_tick: tick,
        command: sim_core::Command::SetStrategyConfig { config },
    };
    app_state.command_queue.lock().push(envelope);
    // ...
}
```

No new queue, no new lock. Identical shape to the existing `command_handler` — POST to `/api/v1/strategy` is just a typed alias for POST to `/api/v1/command` with a `SetStrategyConfig` payload. The mutation applies at the next tick boundary, which means **determinism is preserved** (ticks are the only place state changes) and **repeated POSTs within one tick coalesce to the last one**.

Route registration at `routes.rs:59-62`:

```rust
.route(
    "/api/v1/strategy",
    get(strategy_get_handler).post(strategy_post_handler),
)
```

### Pattern 11: Cross-layer command checklist

Adding `SetStrategyConfig` / `StrategyConfigChanged` touched **5 layers**. Every future cross-layer command mutation should walk this list:

1. **sim_core types** — `Command::SetStrategyConfig` in `commands.rs`, `Event::StrategyConfigChanged {}` in `events.rs`
2. **sim_core engine** — match arm in `engine.rs::apply_commands` that mutates state and emits the event
3. **Daemon route** — GET + POST handlers in `sim_daemon/src/routes.rs`, registered in the router
4. **FE event handler** — entry in `ui_web/src/hooks/applyEvents.ts` (`noOp` is a legitimate choice)
5. **CI event sync** — `scripts/ci_event_sync.sh` passes because the variant is declared as struct-form

Skipping any one of these produces a silent break: skip (1) and commands don't deserialize; skip (2) and state doesn't mutate; skip (3) and clients can't trigger it; skip (4) and the FE throws "unknown event key"; skip (5) and CI blocks the PR.

## Prevention Strategies (15)

1. **Always declare `Event` variants as struct-shaped, even when empty.** Unit variants serialize as bare JSON strings and break FE dispatchers that key by object discriminant. Grep new diffs for `Event::[A-Z][A-Za-z]+,` and flag them.

2. **The event-sync lint must match both struct and unit variant syntax.** `ci_event_sync.sh` currently only matches struct variants — file a follow-up to extend the regex. Until then, pattern 1 is mandatory.

3. **Catalog size comes from `content.*`, never from runtime-accumulated state.** Runtime state is not catalog state. For any "how many X exist" question, route through `state.content.*.len()`.

4. **Every new evaluator/heuristic needs at least one integration test against `load_content("../../content")`.** Zero-populated fixtures mask whole code paths. Checklist item 18.

5. **Decompose functions at natural seams before review, not after.** Checklist item 17. Never reach for `#[allow(clippy::too_many_lines)]`.

6. **Provide a single `fields_mut()` accessor for any struct iterated as an array of fields.** Pin order with a regression test tying the helper to `to_vec`.

7. **Wrap content/config deserialization in `with_context` that names the applied keys.** `set_nested_path` happily inserts a scalar into an object slot; the downstream deserialize failure needs the key path to be debuggable.

8. **Sort overrides by dot-depth (shallowest first) before applying.** Defeats `HashMap` iteration order non-determinism. Add a test that sets parent + child and asserts the child wins.

9. **Harmonize argument naming across sibling MCP tools.** Before publishing a new tool, grep `mcp_advisor/src/` for existing argument names on analogous tools. VIO-484 renamed `field_path` → `parameter_path` mid-review.

10. **Any cross-layer mutation ships with a complete stack update in one PR.** Use the migration checklist below as a PR template for every `Command` + `Event` pair.

11. **Prefer integration tests with real content over synthetic fixtures for "does it respond to real inputs?" questions.** Two different bugs (missing modules, empty tech catalog) were both fixture masking.

12. **Assert deterministic ordering wherever a map or set feeds into output.** Grep new code for `HashMap::iter`, `.keys()`, `.values()` followed by a mutation or write. Require `BTreeMap` or explicit `sort_by`.

13. **Reject hardcoded content IDs in rule/heuristic logic.** Checklist item 12. Grep diffs for `"Fe"`, `"ore"`, `"tech_*"`, `"repair_kit"` in files outside `content/`, `tests/`, or constant modules.

14. **New MCP tools must be callable against a real running daemon before merge.** Argument-naming mismatches and schema drift only surface under live invocation.

15. **When extending a shared config struct, merge-don't-duplicate.** VIO-605 existed because strategy-v2 would have otherwise introduced parallel config paths. Before adding a new config type, check whether an existing struct owns the conceptual space.

## Testing Recommendations

- **FE event roundtrip test:** serialize a sample Rust `Event` variant, run `getEventKey`, assert the result equals the variant name. Would have caught the unit-variant trap immediately.
- **Event sync lint fixture:** unit + struct variant test fixtures under `scripts/ci_event_sync_test/`; assert the lint fails on unit. Prevents regex regression.
- **Catalog-vs-state assertion:** for any heuristic with a denominator, add a `tick_zero` test asserting the denominator is non-zero and matches `content.*.len()`. Would have caught the research urgency inversion.
- **Integration test with real content per new system:** one test per new module calling `load_content("../../content")`. Not optional.
- **Override apply determinism test:** set two overrides at different depths targeting the same field; assert the deeper one wins regardless of insertion order. Run twice with shuffled input.
- **Nested-path deserialize error test:** feed an override that replaces an object slot with a scalar; assert the error message contains the offending key path.
- **Field-iteration invariant test:** for any struct with a `fields_mut` accessor, assert length and order match `to_vec`.
- **Strategy evaluator behavior matrix:** parameterized test across (high wear, low wear) × (high research backlog, low) × (low fuel, high fuel) — verify each scoring axis actually moves the output.
- **MCP tool schema conformance test:** snapshot-test every MCP tool's JSON schema; diff on every PR.
- **Cross-layer smoke test:** for any new `Command`, add an integration test that sends it via the daemon HTTP endpoint, ticks, and asserts the corresponding `Event` lands in the SSE stream.

## Migration Checklist: Adding a New `Command` + `Event` Pair

1. **Define the `Command` variant** in `sim_core/src/types/commands.rs`. Prefer struct-shaped payload for forward compatibility.
2. **Define the `Event` variant** in `sim_core/src/types/events.rs`. **Always use struct syntax** (`VariantName {}` even if empty).
3. **Implement the match arm** in `sim_core/src/engine.rs::apply_commands`. Emit the event on successful mutation.
4. **Add sim_core unit tests** for both happy-path and failure-path command application.
5. **Add the daemon HTTP route** in `sim_daemon/src/routes.rs`. Enqueue via `CommandEnvelope` on the existing command queue — no new lock.
6. **Test the daemon route** — POST the payload, assert 2xx, assert the command lands in the queue.
7. **Add the FE event handler** in `ui_web/src/hooks/applyEvents.ts`. Even if it's `noOp`, register under the exact variant name.
8. **Add an FE roundtrip test** asserting `getEventKey` on a sample event returns the expected variant string.
9. **Run `./scripts/ci_event_sync.sh`** locally; confirm it passes without an allow-list entry.
10. **Add an integration test** covering command issued → state mutation → event observed at the FE layer.
11. **Update `docs/reference.md`** with the new endpoint and event semantics.
12. **If the command mutates config:** add sim_bench override keys in the same PR and write an override test.
13. **If the command is exposed via MCP:** reuse argument names from sibling tools; add a schema snapshot test.
14. **Grep for hardcoded content IDs** in the new code; route through `content.*` lookups.
15. **Review function sizes** — decompose anything approaching 60 lines before opening the PR.

## Multi-Ticket Project Execution: The "Safe Slice" Pattern

When a project is planned while another related project is in flight, pick tickets that:

- **Do not touch files actively being edited** by the in-progress project. Collision on shared files produces merge hell and silent feature interaction.
- **Are upstream in the dependency graph** — types and plumbing before consumer wiring. VIO-479 (types) → VIO-605 (merge) → VIO-480 (interpreter) → VIO-482 (bench overrides) → VIO-483 (daemon endpoints) → VIO-484 (MCP tools) formed a clean DAG.
- **Do not require runtime coupling** to the blocking project's work. If ticket X can only be validated after feature Y in the blocked project ships, X is not safe.
- **Have independent test surfaces.** If two projects must share integration fixtures, one will break the other's tests on merge.
- **Can be merged in arbitrary intra-slice order** without breaking main. Each ticket should stand alone at PR time.

**Conditions that made VIO-479 → VIO-484 safe:**

- Clean additive surface — new `StrategyConfig` types, new `Event` variant, new MCP tools — no mutation of shared hot paths.
- The blocking project (VIO-481+) lives in a different module area (consumers, not producers).
- Each ticket had a distinct test seam: types → unit tests, interpreter → behavior tests, bench → override tests, daemon → HTTP tests, MCP → schema tests.
- The DAG was linear: no two tickets in the slice could be worked in parallel without ordering hazards, so sequencing was natural.

**Conditions that made VIO-481+ unsafe to include:**

- They depend on consumer wiring that would land in the blocking project's feature branch, creating a merge-ordering trap.
- Their test coverage overlaps with the blocked project's fixtures.

**Rule of thumb:** "Safe slice" = additive types, new endpoints, new lint/analysis tools, and any work whose blast radius is confined to new files or append-only extensions of existing ones. "Unsafe" = anything that mutates a hot-path function or shares a test fixture with the blocked project.

## Specific Gotchas (Before → After)

### The unit-variant event trap

**Before:** `StrategyConfigChanged,` (unit variant) → serde emits `"StrategyConfigChanged"` → FE `getEventKey` returns `"0"` → handler dead.

**After:** `StrategyConfigChanged {}` (empty struct variant) → serde emits `{"StrategyConfigChanged": {}}` → FE dispatch routes correctly. Bonus: `ci_event_sync.sh` now enforces the handler (76 → 77 detected variants).

### The `content_techs_len_from_research` bug

**Before:** `total_techs = state.research.unlocked.len() + state.research.evidence.len()` → 0 on fresh runs → research urgency inverted.

**After:** `total_techs = u32::try_from(content.techs.len()).unwrap_or(u32::MAX)` at `strategy_interpreter.rs:272-276`. Pinned by `evaluates_against_real_content` test asserting `scores.research > 0.0` on a fresh state.

### The `strategy.priorities = 0.5` nested-path trap

**Before:** `set_nested_path` accepts the insertion (valid top-level key), final deserialize fails with context-free "invalid type: floating point" and no key.

**After:** Deserialize wrapped in `with_context` that lists applied keys. Error reads: `failed to deserialize StrategyConfig after applying overrides [strategy.priorities]: invalid type: floating point '0.5', expected struct PriorityWeights`.

### The `HashMap` ordering issue

**Before:** Scenarios mixing `strategy.priorities` and `strategy.priorities.mining` behaved non-deterministically depending on hash seed — specific wins half the time, general wins the other half.

**After:** `sort_by_key(|o| o.0.matches('.').count())` before the apply loop guarantees specific nested keys apply last. Regression test loops 16 iterations to stress different hash seeds.

## Related Documentation

- [Hierarchical Agent Decomposition](hierarchical-agent-decomposition.md) — the two-layer foundation the strategic layer sits on top of (refresh candidate: add strategic layer to architecture diagram)
- [Cross-Layer Feature Development](cross-layer-feature-development.md) — architectural template for cross-layer features (refresh candidate: add command+event checklist)
- [Module Behavior Extensibility](../integration-issues/module-behavior-extensibility.md) — cross-layer checklist cited by the plan
- [Event Sync Enforcement](../integration-issues/event-sync-enforcement.md) — refresh candidate: add unit-variant gotcha
- [Backward-Compatible Type Evolution](../integration-issues/backward-compatible-type-evolution.md) — `serde(default)` on `strategy_config: StrategyConfig`
- [Deterministic Integer Arithmetic](../logic-errors/deterministic-integer-arithmetic.md) — rule interpreter determinism rules
- [Scoring and Measurement Pipeline](scoring-and-measurement-pipeline.md) — prior art for `autopilot.*` sim_bench override routing
- [Batch Code Quality Refactoring](batch-code-quality-refactoring.md) — canonical serde serialize-patch-deserialize recipe
- [Progression System Implementation](progression-system-implementation.md) — Pattern 2 (GameState field addition) + Pattern 4 (event variant addition)
- [Multi-Ticket Satellite System Implementation](multi-ticket-satellite-system-implementation.md) — sibling "typed cross-layer feature" example (P4)
- [Multi-Project Planning and Consolidation](multi-project-planning-and-consolidation.md) — documents the absorption of the Strategic Layer into P6 M1
- `docs/plans/2026-03-28-003-feat-strategic-layer-and-multi-station-plan.md` — the source plan (also a refresh candidate; Phase C is now complete)
- `docs/reference.md` — downstream consumer that needs updating for new endpoints and override keys (high-confidence refresh candidate)
