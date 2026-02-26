# Station Tick Optimization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Convert `module_defs` from `Vec<ModuleDef>` to `HashMap<String, ModuleDef>` for O(1) lookups, then merge 5 station tick passes into a single loop.

**Architecture:** Two-phase change. Phase 1 converts the data structure and updates all 27 call sites (type change, `.iter().find()` → `.get()`, `for x in &vec` → `.values()`/`.values_mut()`, `vec[0]` → `.get_mut(key)`). Phase 2 merges 5 station tick functions into a single loop with match dispatch. Both phases are independently valuable.

**Tech Stack:** Rust, HashMap, cargo test

---

### Task 1: Change `module_defs` type in `GameContent`

**Files:**
- Modify: `crates/sim_core/src/types.rs:638`

**Step 1: Change the field type**

In `GameContent` struct, change:
```rust
pub module_defs: Vec<ModuleDef>,
```
to:
```rust
pub module_defs: HashMap<String, ModuleDef>,
```

The `HashMap` import already exists in the file (used by `density_map`, `PricingTable`, etc.).

**Step 2: Run `cargo test -p sim_core` to see compilation errors**

Run: `cargo test -p sim_core 2>&1 | head -50`
Expected: FAIL — many compilation errors across all crates that use `module_defs` as a Vec.

**Step 3: Commit (will not compile yet — that's fine)**

```bash
git add crates/sim_core/src/types.rs
git commit -m "refactor(types): change module_defs from Vec to HashMap [WIP]"
```

---

### Task 2: Update test fixtures

**Files:**
- Modify: `crates/sim_core/src/test_fixtures.rs:77`
- Modify: `crates/sim_core/src/tests/mod.rs:101-135, 169-194, 231-247`

**Step 1: Update `base_content()` and `minimal_content()`**

In `crates/sim_core/src/test_fixtures.rs`, change both:
```rust
module_defs: vec![],
```
to:
```rust
module_defs: HashMap::new(),
```

**Step 2: Update `refinery_content()` in `tests/mod.rs:100-136`**

Change from:
```rust
content.module_defs = vec![ModuleDef {
    id: "module_basic_iron_refinery".to_string(),
    ...
}];
```
to:
```rust
content.module_defs = HashMap::from([("module_basic_iron_refinery".to_string(), ModuleDef {
    id: "module_basic_iron_refinery".to_string(),
    ...
})]);
```

**Step 3: Update `assembler_content()` in `tests/mod.rs:169-194`**

Same pattern — wrap the single `ModuleDef` in `HashMap::from([(key, value)])`.

**Step 4: Update `maintenance_content()` in `tests/mod.rs:231-247`**

Change `.push(ModuleDef { id: "module_maintenance_bay"... })` to `.insert("module_maintenance_bay".to_string(), ModuleDef { ... })`.

**Step 5: Commit**

```bash
git add crates/sim_core/src/test_fixtures.rs crates/sim_core/src/tests/mod.rs
git commit -m "refactor(tests): update test fixtures for HashMap module_defs"
```

---

### Task 3: Update assembler test direct indexing

**Files:**
- Modify: `crates/sim_core/src/tests/assembler.rs:271, 327, 375`

**Step 1: Replace `content.module_defs[0]` with `.get_mut(key)`**

All three sites have the same pattern:
```rust
if let ModuleBehaviorDef::Assembler(ref mut asm_def) = content.module_defs[0].behavior {
```

Change each to:
```rust
if let Some(def) = content.module_defs.get_mut("module_basic_assembler") {
    if let ModuleBehaviorDef::Assembler(ref mut asm_def) = def.behavior {
```

Adjust the closing braces accordingly (add one more `}`).

**Step 2: Commit**

```bash
git add crates/sim_core/src/tests/assembler.rs
git commit -m "refactor(tests): update assembler tests for HashMap module_defs"
```

---

### Task 4: Update wear test mutable iteration

**Files:**
- Modify: `crates/sim_core/src/tests/wear.rs:258, 315, 368`

**Step 1: Replace `for def in &mut content.module_defs` with `.values_mut()`**

Line 258 pattern:
```rust
for def in &mut content.module_defs {
    if def.id == "module_basic_iron_refinery" {
        def.wear_per_run = 0.3;
    }
}
```
Change to:
```rust
if let Some(def) = content.module_defs.get_mut("module_basic_iron_refinery") {
    def.wear_per_run = 0.3;
}
```

Lines 315 and 368 both iterate looking for a Maintenance variant — use `.values_mut()`:
```rust
for def in content.module_defs.values_mut() {
    if let ModuleBehaviorDef::Maintenance(ref mut maint_def) = def.behavior {
```

**Step 2: Commit**

```bash
git add crates/sim_core/src/tests/wear.rs
git commit -m "refactor(tests): update wear tests for HashMap module_defs"
```

---

### Task 5: Update `.iter().find()` sites in `station.rs`

**Files:**
- Modify: `crates/sim_core/src/station.rs:124, 199, 321, 469-473, 548, 1094, 1187, 1404`

**Step 1: Replace all 8 `.iter().find()` calls with `.get()`**

The common pattern:
```rust
let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else {
    continue;
};
```
becomes:
```rust
let Some(def) = content.module_defs.get(&module.def_id) else {
    continue;
};
```

For lines using `def_id` (a local variable) instead of `module.def_id`:
```rust
let Some(def) = content.module_defs.get(&def_id) else { ... };
```

The chained pattern at line 469-473:
```rust
let wear_per_run = content
    .module_defs
    .iter()
    .find(|d| d.id == def_id)
    .map_or(0.0, |d| d.wear_per_run);
```
becomes:
```rust
let wear_per_run = content
    .module_defs
    .get(&def_id)
    .map_or(0.0, |d| d.wear_per_run);
```

**Step 2: Commit**

```bash
git add crates/sim_core/src/station.rs
git commit -m "refactor(station): use HashMap::get for module_defs lookups"
```

---

### Task 6: Update remaining `.iter().find()` sites

**Files:**
- Modify: `crates/sim_core/src/engine.rs:106`
- Modify: `crates/sim_core/src/metrics.rs:185`
- Modify: `crates/sim_core/src/tasks.rs:104-107`
- Modify: `crates/sim_core/src/trade.rs:32-35`
- Modify: `crates/sim_control/src/lib.rs:231, 314-318`

**Step 1: Update engine.rs:106**

```rust
let kind_state = match content.module_defs.iter().find(|d| d.id == module_def_id) {
```
becomes:
```rust
let kind_state = match content.module_defs.get(&module_def_id) {
```

Note: `module_def_id` is a `String` — `get()` takes `&String` which auto-derefs to `&str`.

**Step 2: Update metrics.rs:185**

```rust
let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else {
```
becomes:
```rust
let Some(def) = content.module_defs.get(&module.def_id) else {
```

**Step 3: Update tasks.rs:104-107**

```rust
.module_defs
.iter()
.find(|m| m.id == *module_def_id)
.map_or(0.0, |m| m.volume_m3),
```
becomes:
```rust
.module_defs
.get(module_def_id)
.map_or(0.0, |m| m.volume_m3),
```

Note: `module_def_id` is likely `&String` — no deref needed, `.get()` takes `&Q` where `String: Borrow<Q>`.

**Step 4: Update trade.rs:32-35**

```rust
.module_defs
.iter()
.find(|d| d.id == *module_def_id)?;
```
becomes:
```rust
.module_defs
.get(module_def_id)?;
```

**Step 5: Update sim_control/src/lib.rs:231**

```rust
let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else {
```
becomes:
```rust
let Some(def) = content.module_defs.get(&module.def_id) else {
```

**Step 6: Update sim_control/src/lib.rs:314-318**

```rust
.module_defs
.iter()
.find(|def| def.id == "module_shipyard")
.and_then(|def| match &def.behavior {
```
becomes:
```rust
.module_defs
.get("module_shipyard")
.and_then(|def| match &def.behavior {
```

**Step 7: Commit**

```bash
git add crates/sim_core/src/engine.rs crates/sim_core/src/metrics.rs \
  crates/sim_core/src/tasks.rs crates/sim_core/src/trade.rs \
  crates/sim_control/src/lib.rs
git commit -m "refactor(core): convert remaining iter().find() to HashMap::get"
```

---

### Task 7: Update sim_bench overrides

**Files:**
- Modify: `crates/sim_bench/src/overrides.rs:11, 19-20, 31, 93, 253, 284, 303, 319, 340, 385`

**Step 1: Update `apply_module_override` signature**

Change:
```rust
fn apply_module_override(
    module_defs: &mut [sim_core::ModuleDef],
```
to:
```rust
fn apply_module_override(
    module_defs: &mut HashMap<String, sim_core::ModuleDef>,
```

Add `use std::collections::HashMap;` if not already imported.

**Step 2: Update iteration in `apply_module_override`**

Change:
```rust
for module_def in module_defs.iter_mut() {
```
to:
```rust
for module_def in module_defs.values_mut() {
```

**Step 3: Update test assertions**

All test `for module_def in &content.module_defs` become `for module_def in content.module_defs.values()`.

**Step 4: Commit**

```bash
git add crates/sim_bench/src/overrides.rs
git commit -m "refactor(bench): update overrides for HashMap module_defs"
```

---

### Task 8: Update sim_world content loading

**Files:**
- Modify: `crates/sim_world/src/lib.rs:93, ~222-226`

**Step 1: Update validation loop**

Change:
```rust
for module_def in &content.module_defs {
```
to:
```rust
for module_def in content.module_defs.values() {
```

**Step 2: Update content loading**

The content loading currently deserializes `module_defs.json` as a `Vec<ModuleDef>` and assigns directly. After deserialization, convert to HashMap:

If the current pattern is something like `content.module_defs = serde_json::from_str(...)`, the JSON file is an array. You need to:
1. Deserialize as `Vec<ModuleDef>`
2. Convert: `.into_iter().map(|d| (d.id.clone(), d)).collect::<HashMap<_,_>>()`

Check the exact deserialization site and add the conversion step.

**Step 3: Run full test suite**

Run: `cargo test`
Expected: ALL PASS — the HashMap conversion is complete.

**Step 4: Commit**

```bash
git add crates/sim_world/src/lib.rs
git commit -m "refactor(world): convert module_defs Vec to HashMap on load"
```

---

### Task 9: Verify Phase 1 — all tests pass

**Step 1: Run full workspace tests**

Run: `cargo test`
Expected: ALL PASS

**Step 2: Run clippy**

Run: `cargo clippy`
Expected: No warnings

**Step 3: Commit (tag Phase 1 done)**

No commit needed unless clippy required fixes. Phase 1 (HashMap conversion) is complete.

---

### Task 10: Single-pass station tick — merge 5 functions

**Files:**
- Modify: `crates/sim_core/src/station.rs:76-98`

**Step 1: Replace `tick_stations` body**

The current `tick_stations` calls 5 functions in 5 separate loops:
```rust
pub(crate) fn tick_stations(...) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        tick_station_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_assembler_modules(state, station_id, content, rng, events);
    }
    for station_id in &station_ids {
        tick_sensor_array_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_lab_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_maintenance_modules(state, station_id, content, events);
    }
}
```

Replace with a single loop per station that dispatches each module to the correct handler:
```rust
pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        tick_station_all_modules(state, station_id, content, rng, events);
    }
}
```

The new `tick_station_all_modules` function iterates modules once, looks up each def via `content.module_defs.get()`, and dispatches based on behavior type. The existing per-type functions remain as helpers — this task only changes the orchestration.

**Important:** The tick order within a station must be preserved: processors first, then assemblers, then sensors, then labs, then maintenance. The simplest approach is to keep the existing functions but call them all per-station instead of per-type-across-stations. This avoids a risky refactor of the inner logic.

Actually, looking at the current code more carefully: each per-type function already iterates ALL modules for ONE station and skips non-matching types. So they already do one pass each. The 5-loop overhead is 5 × (station count) function calls + 5 × (module count) HashMap lookups per station.

The single-pass approach would iterate modules once and dispatch. But each tick function has complex internal state (cached_volume, module_idx tracking, etc.) that makes a naive merge non-trivial.

**Safer approach:** Keep the 5 per-type helper functions exactly as they are, but call them all in a single loop over stations instead of 5 separate loops:

```rust
pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        tick_station_modules(state, station_id, content, events);
        tick_assembler_modules(state, station_id, content, rng, events);
        tick_sensor_array_modules(state, station_id, content, events);
        tick_lab_modules(state, station_id, content, events);
        tick_maintenance_modules(state, station_id, content, events);
    }
}
```

This reduces from 5 loops to 1, each inner function still iterates modules but now does a HashMap::get instead of iter().find(). The lookup count stays the same but the per-station overhead is reduced.

**Step 2: Run tests**

Run: `cargo test -p sim_core`
Expected: ALL PASS

**Step 3: Commit**

```bash
git add crates/sim_core/src/station.rs
git commit -m "perf(station): merge 5 station loops into single per-station dispatch"
```

---

### Task 11: Run benchmark comparison

**Step 1: Run bench on main (baseline)**

```bash
cd /Users/joshuamcmorris/space-simulation
cargo run --release -p sim_bench -- run --scenario scenarios/baseline.json
```

Note the TPS from the output.

**Step 2: Run bench on this branch**

```bash
cd /Users/joshuamcmorris/space-simulation/.worktrees/vio-82-station-tick
cargo run --release -p sim_bench -- run --scenario scenarios/baseline.json
```

Compare TPS. Expect measurable improvement from removing 5 × N_modules × O(N_defs) linear scans per tick.

**Step 3: Commit benchmark note (optional)**

If results are interesting, note them in the PR description.

---

### Task 12: Final CI checks and PR

**Step 1: Run full CI**

```bash
./scripts/ci_rust.sh
```

**Step 2: Create PR**

```bash
git push -u origin fix/vio-82-station-tick-optimize
gh pr create --title "perf(station): HashMap module_defs + single-pass tick" \
  --body "$(cat <<'EOF'
## Summary

- Convert `GameContent.module_defs` from `Vec<ModuleDef>` to `HashMap<String, ModuleDef>`
- Replace 14 `.iter().find(|d| d.id == ...)` call sites with `HashMap::get()`
- Merge 5 station tick loops into 1 per-station dispatch loop
- Update all test fixtures, bench overrides, and content loading

Closes VIO-82

## Test plan

- [ ] `cargo test` — all tests pass
- [ ] `cargo clippy` — no warnings
- [ ] `sim_bench baseline.json` — TPS improvement vs main
- [ ] Determinism check: same seed produces same results before/after

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

**Step 3: Follow PR review workflow**

Per CLAUDE.md: watch CI, review diff, post Claude Code Review comment. Do NOT merge — owner approval required for main.
