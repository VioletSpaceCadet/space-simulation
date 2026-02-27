# Minutes-Per-Tick Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Change the sim time scale from 1 tick = 1 minute to 1 tick = 1 hour by adding a `minutes_per_tick` constant, then rescaling all tick-denominated content values accordingly.

**Architecture:** Add `minutes_per_tick: u32` to `Constants` (value 60 in production, 1 in tests). Expose through daemon `/api/v1/meta` so the FE can derive game time. Rescale all tick-duration constants (÷60), rate constants (×60), and lab output (×60). Derive `TRADE_UNLOCK_TICK` from the constant. Update scenarios and bump content version.

**Tech Stack:** Rust (sim_core, sim_bench, sim_daemon), TypeScript/React (ui_web), JSON content files.

**PR structure:** Three PRs into `feat/minutes-per-tick`, then final PR into `main`.

---

## PR 1: Add constant, helpers, expose to FE

### Task 1: Add `minutes_per_tick` to Constants struct

**Files:**
- Modify: `crates/sim_core/src/types.rs:922-957`

**Step 1: Add the field to Constants**

After line 956 (`wear_band_critical_efficiency`), add:

```rust
    // Time scale
    /// Game-time minutes per simulation tick. Production = 60 (1 tick = 1 hour).
    /// Test fixtures use 1 to preserve existing assertions.
    pub minutes_per_tick: u32,
```

**Step 2: Add to constants.json**

- Modify: `content/constants.json`

Add at the end (before closing `}`):

```json
  "minutes_per_tick": 60
```

**Step 3: Add to test fixtures**

- Modify: `crates/sim_core/src/test_fixtures.rs:85-110` and `154-178`

Add `minutes_per_tick: 1,` to both `base_content()` and `minimal_content()` Constants blocks (after `wear_band_critical_efficiency`).

**Step 4: Run tests to verify compilation**

Run: `cargo test -p sim_core`
Expected: All pass (value is 1 in tests, no behavior change).

**Step 5: Commit**

```
feat(sim_core): add minutes_per_tick constant to Constants struct
```

---

### Task 2: Add helper functions

**Files:**
- Modify: `crates/sim_core/src/types.rs` (add impl block on Constants)

**Step 1: Write failing tests for helpers**

Add at the bottom of `types.rs` (or a new `#[cfg(test)] mod tests` block):

```rust
#[cfg(test)]
mod time_scale_tests {
    use super::*;

    fn constants_mpt(mpt: u32) -> Constants {
        Constants {
            survey_scan_ticks: 0,
            deep_scan_ticks: 0,
            travel_ticks_per_hop: 0,
            survey_tag_detection_probability: 0.0,
            asteroid_count_per_template: 0,
            asteroid_mass_min_kg: 0.0,
            asteroid_mass_max_kg: 0.0,
            ship_cargo_capacity_m3: 0.0,
            station_cargo_capacity_m3: 0.0,
            mining_rate_kg_per_tick: 0.0,
            deposit_ticks: 0,
            station_power_available_per_tick: 0.0,
            autopilot_iron_rich_confidence_threshold: 0.0,
            autopilot_refinery_threshold_kg: 0.0,
            research_roll_interval_ticks: 0,
            data_generation_peak: 0.0,
            data_generation_floor: 0.0,
            data_generation_decay_rate: 0.0,
            autopilot_slag_jettison_pct: 0.75,
            wear_band_degraded_threshold: 0.0,
            wear_band_critical_threshold: 0.0,
            wear_band_degraded_efficiency: 0.0,
            wear_band_critical_efficiency: 0.0,
            minutes_per_tick: mpt,
        }
    }

    #[test]
    fn game_minutes_to_ticks_exact_division() {
        let c = constants_mpt(60);
        assert_eq!(c.game_minutes_to_ticks(120), 2); // 120 min = 2 hours = 2 ticks
    }

    #[test]
    fn game_minutes_to_ticks_rounds_up() {
        let c = constants_mpt(60);
        assert_eq!(c.game_minutes_to_ticks(30), 1); // 30 min < 1 hour, rounds up to 1 tick
    }

    #[test]
    fn game_minutes_to_ticks_mpt_1() {
        let c = constants_mpt(1);
        assert_eq!(c.game_minutes_to_ticks(120), 120); // identity when mpt=1
    }

    #[test]
    fn game_minutes_to_ticks_zero_minutes() {
        let c = constants_mpt(60);
        assert_eq!(c.game_minutes_to_ticks(0), 0);
    }

    #[test]
    fn rate_per_minute_to_per_tick() {
        let c = constants_mpt(60);
        let result = c.rate_per_minute_to_per_tick(15.0); // 15 kg/min
        assert!((result - 900.0).abs() < f32::EPSILON); // 15 * 60 = 900 kg/tick
    }

    #[test]
    fn rate_per_minute_to_per_tick_mpt_1() {
        let c = constants_mpt(1);
        let result = c.rate_per_minute_to_per_tick(15.0);
        assert!((result - 15.0).abs() < f32::EPSILON); // identity when mpt=1
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core time_scale_tests`
Expected: FAIL — methods not defined.

**Step 3: Implement helper methods**

Add `impl Constants` block in `types.rs` (after the struct definition, before WearState):

```rust
impl Constants {
    /// Convert a game-time duration in minutes to ticks, rounding up.
    /// ceil_div ensures short durations always take at least 1 tick.
    pub fn game_minutes_to_ticks(&self, minutes: u64) -> u64 {
        let mpt = u64::from(self.minutes_per_tick);
        if minutes == 0 {
            return 0;
        }
        (minutes + mpt - 1) / mpt
    }

    /// Convert a per-minute rate to a per-tick rate.
    /// E.g. 15 kg/min × 60 min/tick = 900 kg/tick.
    pub fn rate_per_minute_to_per_tick(&self, rate_per_minute: f32) -> f32 {
        rate_per_minute * self.minutes_per_tick as f32
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p sim_core time_scale_tests`
Expected: All 6 pass.

**Step 5: Commit**

```
feat(sim_core): add game_minutes_to_ticks and rate_per_minute_to_per_tick helpers
```

---

### Task 3: Expose `minutes_per_tick` in daemon `/api/v1/meta`

**Files:**
- Modify: `crates/sim_daemon/src/routes.rs:50-62`

**Step 1: Add `minutes_per_tick` to meta response**

In `meta_handler`, add to the `serde_json::json!` block:

```rust
    Json(serde_json::json!({
        "tick": sim.game_state.meta.tick,
        "seed": sim.game_state.meta.seed,
        "content_version": sim.game_state.meta.content_version,
        "ticks_per_sec": ticks_per_sec,
        "paused": paused,
        "trade_unlock_tick": sim_core::TRADE_UNLOCK_TICK,
        "minutes_per_tick": sim.content.constants.minutes_per_tick,
    }))
```

Note: `meta_handler` takes `State(app_state)` which has `app_state.sim` (the Mutex-locked sim). The sim has `content` on it. Check the exact field path — it may be `app_state.content` or `sim.content`. Read the `AppState` struct to confirm.

**Step 2: Run daemon tests**

Run: `cargo test -p sim_daemon`
Expected: Pass.

**Step 3: Commit**

```
feat(daemon): expose minutes_per_tick in /api/v1/meta response
```

---

### Task 4: Add `minutes_per_tick` to sim_bench overrides

**Files:**
- Modify: `crates/sim_bench/src/overrides.rs:113-171`

**Step 1: Add override arm**

In `apply_constant_override`, add before the catch-all `_ => bail!(...)`:

```rust
        "minutes_per_tick" => {
            constants.minutes_per_tick = as_u32(key, value)?;
        }
```

**Step 2: Write a test**

```rust
    #[test]
    fn test_minutes_per_tick_override() {
        let mut content = test_content();
        let overrides = HashMap::from([("minutes_per_tick".to_string(), serde_json::json!(1))]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert_eq!(content.constants.minutes_per_tick, 1);
    }
```

**Step 3: Run tests**

Run: `cargo test -p sim_bench`
Expected: Pass.

**Step 4: Commit**

```
feat(sim_bench): support minutes_per_tick override in scenarios
```

---

### Task 5: Update FE to use `minutes_per_tick` for time display

**Files:**
- Modify: `ui_web/src/types.ts:1-7` (MetaInfo interface)
- Modify: `ui_web/src/App.tsx` (pass minutesPerTick to StatusBar)
- Modify: `ui_web/src/components/StatusBar.tsx:34-42`
- Modify: `ui_web/src/components/StatusBar.test.tsx`

**Step 1: Add `minutes_per_tick` to MetaInfo type**

In `ui_web/src/types.ts`, update `MetaInfo`:

```typescript
export interface MetaInfo {
  tick: number
  seed: number
  content_version: string
  ticks_per_sec: number
  paused: boolean
  minutes_per_tick: number
}
```

**Step 2: Store `minutesPerTick` in App state and pass to StatusBar**

In `App.tsx`, add state:

```typescript
const [minutesPerTick, setMinutesPerTick] = useState(1); // safe default
```

In the `fetchMeta` effect, add:

```typescript
setMinutesPerTick(meta.minutes_per_tick ?? 1);
```

Pass to StatusBar:

```tsx
<StatusBar
  tick={displayTick}
  minutesPerTick={minutesPerTick}
  // ... other props
/>
```

**Step 3: Update StatusBar Props and time display**

Add `minutesPerTick: number` to Props interface.

Replace the time computation (lines 39-42):

```typescript
const roundedTick = Math.floor(tick);
const totalMinutes = roundedTick * minutesPerTick;
const day = Math.floor(totalMinutes / 1440);
const hour = Math.floor((totalMinutes % 1440) / 60);
const minute = Math.floor(totalMinutes % 60);
```

**Step 4: Update StatusBar tests**

Update `defaultProps` to include `minutesPerTick: 1`.

Update the "shows day and hour" test — at `minutesPerTick=1`, tick 1440 = day 1 (unchanged).

Add a test for `minutesPerTick=60`:

```typescript
it('shows correct day with minutesPerTick=60', () => {
  render(<StatusBar tick={24} connected measuredTickRate={10} minutesPerTick={60} {...defaultProps} />);
  expect(screen.getByText(/day 1/i)).toBeInTheDocument();
});
```

**Step 5: Run FE tests**

Run: `cd ui_web && npm test`
Expected: All pass.

**Step 6: Commit**

```
feat(ui_web): use minutes_per_tick from daemon for time display
```

---

## PR 2: Rescale content and derive TRADE_UNLOCK_TICK

### Task 6: Rescale `constants.json` tick values

**Files:**
- Modify: `content/constants.json`

**Step 1: Apply rescaling**

Durations ÷60:
- `survey_scan_ticks`: 120 → 2
- `deep_scan_ticks`: 480 → 8
- `travel_ticks_per_hop`: 2880 → 48
- `deposit_ticks`: 120 → 2
- `research_roll_interval_ticks`: 60 → 1

Rates ×60:
- `mining_rate_kg_per_tick`: 15.0 → 900.0
- `station_power_available_per_tick`: 100.0 → 6000.0

No change: all dimensionless values, mass/volume values, data generation values.

Final `constants.json`:

```json
{
  "survey_scan_ticks": 2,
  "deep_scan_ticks": 8,
  "travel_ticks_per_hop": 48,
  "survey_tag_detection_probability": 0.85,
  "asteroid_count_per_template": 10,
  "asteroid_mass_min_kg": 500000.0,
  "asteroid_mass_max_kg": 10000000.0,
  "ship_cargo_capacity_m3": 50.0,
  "station_cargo_capacity_m3": 2000.0,
  "station_power_available_per_tick": 6000.0,
  "mining_rate_kg_per_tick": 900.0,
  "deposit_ticks": 2,
  "autopilot_iron_rich_confidence_threshold": 0.7,
  "autopilot_refinery_threshold_kg": 2000.0,
  "research_roll_interval_ticks": 1,
  "data_generation_peak": 100.0,
  "data_generation_floor": 5.0,
  "data_generation_decay_rate": 0.7,
  "autopilot_slag_jettison_pct": 0.75,
  "wear_band_degraded_threshold": 0.5,
  "wear_band_critical_threshold": 0.8,
  "wear_band_degraded_efficiency": 0.75,
  "wear_band_critical_efficiency": 0.5,
  "minutes_per_tick": 60
}
```

**Step 2: Commit**

```
chore(content): rescale constants.json for 1 tick = 1 hour
```

---

### Task 7: Rescale `module_defs.json` intervals and lab output

**Files:**
- Modify: `content/module_defs.json`

**Step 1: Apply rescaling**

Intervals ÷60:
- Basic Iron Refinery: `processing_interval_ticks` 60 → 1
- Maintenance Bay: `repair_interval_ticks` 30 → 1 (ceil(30/60) = 1; gameplay change: repair hourly instead of every 30min)
- Basic Assembler: `assembly_interval_ticks` 360 → 6
- Sensor Array: `scan_interval_ticks` 120 → 2
- Shipyard: `assembly_interval_ticks` 20160 → 336

Labs: interval stays at 1. Compensate by ×60 on output:
- Materials Lab: `data_consumption_per_run` 10.0 → 600.0, `research_points_per_run` 5.0 → 300.0
- Exploration Lab: `data_consumption_per_run` 8.0 → 480.0, `research_points_per_run` 4.0 → 240.0
- Engineering Lab: `data_consumption_per_run` 10.0 → 600.0, `research_points_per_run` 5.0 → 300.0

No change: `wear_per_run` (per-execution, not per-time), solar array `base_output_kw`, battery rates, power_consumption_per_run.

**Step 2: Run full test suite**

Run: `cargo test`
Expected: sim_core tests pass (fixtures use mpt=1, module_defs are test-local). sim_bench tests that load `content/` will see new values.

**Step 3: Commit**

```
chore(content): rescale module_defs.json intervals and lab output for hourly ticks
```

---

### Task 8: Derive TRADE_UNLOCK_TICK from constants

**Files:**
- Modify: `crates/sim_core/src/engine.rs:13-14`
- Modify: `crates/sim_core/src/engine.rs` (tick function signature — pass constants)
- Modify: `crates/sim_daemon/src/routes.rs:60` (meta_handler)

**Step 1: Replace hardcoded constant with function**

In `engine.rs`, replace:

```rust
pub const TRADE_UNLOCK_TICK: u64 = 525_600;
```

With:

```rust
/// Trade (import/export) unlocks after 1 simulated year.
/// 365 days × 24 hours × 60 minutes = 525,600 game-minutes.
pub fn trade_unlock_tick(minutes_per_tick: u32) -> u64 {
    let total_minutes: u64 = 365 * 24 * 60;
    // Use ceil_div to avoid rounding down
    (total_minutes + u64::from(minutes_per_tick) - 1) / u64::from(minutes_per_tick)
}
```

At mpt=60: `525_600 / 60 = 8_760` ticks (exactly 1 year).
At mpt=1: `525_600 / 1 = 525_600` ticks (same as before for tests).

**Step 2: Update all call sites**

In `engine.rs`, the `tick()` function uses `TRADE_UNLOCK_TICK` in the import/export command handling. Replace:

```rust
if current_tick < TRADE_UNLOCK_TICK {
```

With:

```rust
if current_tick < trade_unlock_tick(content.constants.minutes_per_tick) {
```

(Two occurrences — one for Import, one for Export.)

In `routes.rs` (`meta_handler`), replace:

```rust
"trade_unlock_tick": sim_core::TRADE_UNLOCK_TICK,
```

With:

```rust
"trade_unlock_tick": sim_core::trade_unlock_tick(sim.content.constants.minutes_per_tick),
```

(Adjust the field path based on how `content` is accessed in the handler. May need `app_state.content` or similar.)

**Step 3: Run tests**

Run: `cargo test`
Expected: Pass. Test fixtures use mpt=1, so `trade_unlock_tick(1) = 525_600` — same as the old constant.

**Step 4: Commit**

```
feat(sim_core): derive TRADE_UNLOCK_TICK from minutes_per_tick
```

---

### Task 9: Update `dev_base_state.json` if needed

**Files:**
- Modify: `content/dev_base_state.json`

**Step 1: Check if any tick-denominated values exist in dev_base_state**

Read the file. It contains initial game state (stations, ships, etc). Key concern:
- `power_available_per_tick` on stations — this mirrors the constant and should match the new value (6000.0).
- Any hardcoded tick values in task states.

If `power_available_per_tick` appears in the state file, update it from 100.0 to 6000.0.

**Step 2: Run the full test suite**

Run: `cargo test`
Expected: Pass.

**Step 3: Commit (only if changes needed)**

```
chore(content): update dev_base_state.json for hourly tick scale
```

---

## PR 3: Scenarios, cleanup, content version bump

### Task 10: Rescale scenario files

**Files:**
- Modify: `scenarios/baseline.json`
- Modify: `scenarios/ci_smoke.json`
- Modify: `scenarios/balance_v1.json`
- Modify: `scenarios/month.json`
- Modify: `scenarios/quarter.json`
- Modify: `scenarios/economy_baseline.json`
- Modify: `scenarios/economy_long.json`
- Modify: `scenarios/cargo_sweep.json`

**Step 1: Rescale all scenario `ticks` and `metrics_every` values**

| Scenario | `ticks` old → new | `metrics_every` old → new | Notes |
|---|---|---|---|
| baseline | 20,160 → 336 | 60 → 1 | 14 days |
| ci_smoke | 2,000 → 34 | 100 → 2 | ~1.4 days (keep short for CI) |
| balance_v1 | 20,160 → 336 | 60 → 1 | 14 days |
| month | 43,200 → 720 | 60 → 1 | 30 days |
| quarter | 129,600 → 2,160 | 120 → 2 | 90 days |
| economy_baseline | 525,600 → 8,760 | 1,440 → 24 | 1 year |
| economy_long | 1,051,200 → 17,520 | 1,440 → 24 | 2 years |
| cargo_sweep | 10,000 → 167 | 60 → 1 | ~7 days |

**Step 2: Rescale `balance_v1.json` module overrides**

```json
"overrides": {
  "module.lab.research_interval_ticks": 1,
  "module.lab.wear_per_run": 0.002,
  "module.processor.processing_interval_ticks": 3,
  "module.assembler.assembly_interval_ticks": 4
}
```

Original: lab 10→1 (ceil(10/60)), processor 180→3, assembler 240→4.

Note: `cargo_sweep.json` overrides are `station_cargo_capacity_m3` and `wear_band_degraded_threshold` — not tick-denominated, no change needed.

**Step 3: Run sim_bench CI smoke**

Run: `./scripts/ci_bench_smoke.sh`
Expected: Pass.

**Step 4: Commit**

```
chore(scenarios): rescale all scenario files for hourly ticks
```

---

### Task 11: Bump content version

**Files:**
- Modify: `content/techs.json` (line 2: `"content_version": "0.0.1"` → `"0.1.0"`)
- Modify: `content/dev_base_state.json` (update `content_version` field to match)

**Step 1: Update content_version**

Change `"0.0.1"` to `"0.1.0"` in both files.

**Step 2: Run tests**

Run: `cargo test`
Expected: Pass. Test fixtures use `content_version: "test"`, so no mismatch.

**Step 3: Commit**

```
chore(content): bump content_version to 0.1.0 for hourly tick scale
```

---

### Task 12: Update documentation

**Files:**
- Modify: `docs/reference.md` (if tick values are documented)
- Modify: `CLAUDE.md` (if tick order or constants are documented)

**Step 1: Search for hardcoded tick values in docs**

Grep for `525,600`, `525_600`, `1440`, `2880`, `20160` in docs/ and CLAUDE.md. Update any references to reflect new values.

**Step 2: Add a note about `minutes_per_tick`**

In `CLAUDE.md` under "Key design rules", add:

```
- **Time scale:** `minutes_per_tick` in constants.json (default 60 = 1 tick per hour). Test fixtures use 1. All tick durations in content are pre-scaled. Helpers: `Constants::game_minutes_to_ticks()`, `Constants::rate_per_minute_to_per_tick()`.
```

**Step 3: Commit**

```
docs: update references for hourly tick scale
```

---

### Task 13: Run full CI suite

**Step 1: Run all CI scripts**

```bash
./scripts/ci_rust.sh
./scripts/ci_web.sh
./scripts/ci_bench_smoke.sh
```

**Step 2: Fix any failures**

If tests fail, investigate and fix. Common issues:
- Research tests that hardcode `tick = 60` for roll triggers — with `research_roll_interval_ticks` still 60 in test fixtures, these should still work.
- Economy tests that reference `TRADE_UNLOCK_TICK` — now a function call.
- Sim_bench tests that load content and assert specific values.

**Step 3: Commit any fixes**

---

## Checklist: Values NOT to change

These are dimensionless or mass/volume and must stay unchanged:

- `autopilot_iron_rich_confidence_threshold` (0.7)
- `autopilot_refinery_threshold_kg` (2000.0)
- `autopilot_slag_jettison_pct` (0.75)
- `asteroid_mass_*` (mass, not time)
- `ship_cargo_capacity_m3`, `station_cargo_capacity_m3` (volume)
- `survey_tag_detection_probability` (probability)
- `data_generation_peak/floor/decay_rate` (per-roll amounts, not per-time)
- `wear_band_*` thresholds and efficiencies (dimensionless)
- `wear_per_run` on all modules (per-execution, frequency already adjusted by interval change)
- `power_consumption_per_run` on modules (continuous draw semantics — unchanged)
- Battery `capacity_kwh`, `charge_rate_kw`, `discharge_rate_kw` (energy units, not tick-denominated)
- Solar array `base_output_kw` (power unit)

## Checklist: Potential gotchas

1. **Research tests**: `research.rs` tests set `state.meta.tick = 60` to trigger rolls. Test fixtures have `research_roll_interval_ticks: 60`. Since test fixtures use `minutes_per_tick: 1`, the roll interval stays 60 ticks — these tests should still pass without changes.

2. **Replenish loop**: `replenish_scan_sites()` runs every tick (no interval constant). At hourly ticks it fires once per game-hour instead of once per game-minute. This should be fine — sites don't deplete that fast. Monitor in sim_bench runs.

3. **Data generation**: `data_generation_peak/floor/decay_rate` are per-roll amounts. With `research_roll_interval_ticks: 1` in production, rolls happen every tick (every hour). At mpt=1, rolls happen every 60 ticks (every 60 minutes). Same game-time frequency, so no scaling needed on generation amounts.

4. **Mining rate**: `mining_rate_kg_per_tick` scales to 900 kg/tick. The `Mine` task's `duration_ticks` is set by `mine_duration()` which divides asteroid mass by this rate. At 900 kg/tick, a 500,000 kg asteroid takes ~556 ticks = 556 hours = 23 days. Previously: 500,000/15 = 33,333 ticks = 33,333 minutes = 23 days. Same game-time. Correct.

5. **Power budget**: `station_power_available_per_tick` goes from 100 to 6000. This is the legacy field on StationState — it may need to match. Check if any code path uses this field vs the solar array compute_power_budget path. The energy system (PR #27) may have made this field vestigial.
