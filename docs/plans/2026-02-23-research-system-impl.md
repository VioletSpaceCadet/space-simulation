# Research System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the minimal auto-split evidence research system with a lab-based system: labs consume raw data, produce domain-specific points, and domain sufficiency drives probabilistic tech unlock.

**Architecture:** Labs are a new `ModuleBehaviorDef` variant. Raw data is a sim-wide resource on `ResearchState` with diminishing returns. Research domains (Materials, Exploration, Engineering) accumulate per-tech via `DomainProgress`. Tech unlock uses exponential CDF with geometric-mean domain sufficiency. Labs tick after assemblers, before maintenance.

**Tech Stack:** Rust (sim_core, sim_control), JSON content files, React/TypeScript (ui_web)

**Design doc:** `docs/plans/2026-02-23-research-system-design.md`

---

### Task 1: Extend types — ResearchDomain, DataKind, DomainProgress, action_counts

**Files:**
- Modify: `crates/sim_core/src/types.rs`
- Test: `crates/sim_core/src/types.rs` (compile check) and `crates/sim_core/src/test_fixtures.rs`

**Step 1: Add ResearchDomain enum and DomainProgress struct, extend DataKind, update ResearchState**

Add to `types.rs` after the existing `DataKind` enum:

```rust
// Extend DataKind with new variants:
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataKind {
    ScanData,
    MiningData,
    EngineeringData,
}

// New enum after DataKind:
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResearchDomain {
    Materials,
    Exploration,
    Engineering,
}

// New struct:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainProgress {
    pub points: HashMap<ResearchDomain, f32>,
}
```

Update `ResearchState` (currently at line ~237):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    pub unlocked: HashSet<TechId>,
    pub data_pool: HashMap<DataKind, f32>,
    pub evidence: HashMap<TechId, DomainProgress>,
    pub action_counts: HashMap<String, u64>,
}
```

**Step 2: Add `domain_requirements` to TechDef**

Update `TechDef` (currently at line ~502):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechDef {
    pub id: TechId,
    pub name: String,
    pub prereqs: Vec<TechId>,
    pub domain_requirements: HashMap<ResearchDomain, f32>,
    pub accepted_data: Vec<DataKind>,
    pub difficulty: f32,
    pub effects: Vec<TechEffect>,
}
```

**Step 3: Add LabDef and LabState**

Add `LabDef` after `MaintenanceDef` (line ~577):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabDef {
    pub domain: ResearchDomain,
    pub data_consumption_per_run: f32,
    pub research_points_per_run: f32,
    pub accepted_data: Vec<DataKind>,
    pub research_interval_ticks: u64,
}
```

Add `Lab` variant to `ModuleBehaviorDef` (line ~560):

```rust
pub enum ModuleBehaviorDef {
    Processor(ProcessorDef),
    Storage { capacity_m3: f32 },
    Maintenance(MaintenanceDef),
    Assembler(AssemblerDef),
    Lab(LabDef),
}
```

Add `LabState` after `AssemblerState` (line ~186):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabState {
    pub ticks_since_last_run: u64,
    pub assigned_tech: Option<TechId>,
    #[serde(default)]
    pub starved: bool,
}
```

Add `Lab(LabState)` variant to `ModuleKindState` (line ~162):

```rust
pub enum ModuleKindState {
    Processor(ProcessorState),
    Storage,
    Maintenance(MaintenanceState),
    Assembler(AssemblerState),
    Lab(LabState),
}
```

**Step 4: Add new constants**

Add to `Constants` struct (line ~652):

```rust
// After wear fields:
pub research_roll_interval_ticks: u64,
pub data_generation_peak: f32,
pub data_generation_floor: f32,
pub data_generation_decay_rate: f32,
```

**Step 5: Add AssignLabTech command**

Add variant to `Command` enum (line ~293):

```rust
AssignLabTech {
    station_id: StationId,
    module_id: ModuleInstanceId,
    tech_id: Option<TechId>,
},
```

**Step 6: Add new events**

Add variants to `Event` enum (after `AssemblerRan`, line ~450):

```rust
LabRan {
    station_id: StationId,
    module_id: ModuleInstanceId,
    tech_id: TechId,
    data_consumed: f32,
    points_produced: f32,
    domain: ResearchDomain,
},
LabStarved {
    station_id: StationId,
    module_id: ModuleInstanceId,
},
LabResumed {
    station_id: StationId,
    module_id: ModuleInstanceId,
},
```

**Step 7: Fix all compilation errors**

The `evidence` field type changed from `HashMap<TechId, f32>` to `HashMap<TechId, DomainProgress>`. Fix every place that reads/writes `evidence`:
- `research.rs`: `advance_research` — will be rewritten in Task 4, stub for now
- `test_fixtures.rs`: `base_state()` and `base_content()` — update `evidence` init and add new constants
- All test files that build `ResearchState` or `Constants` manually

For now, stub `advance_research` to do nothing (or minimal). It will be fully rewritten in Task 4.

The `Constants` struct has 4 new fields — every place that constructs `Constants` in tests must add them. Use these defaults:
```rust
research_roll_interval_ticks: 60,
data_generation_peak: 100.0,
data_generation_floor: 5.0,
data_generation_decay_rate: 0.7,
```

**Step 8: Run tests to verify compilation**

Run: `cargo test -p sim_core`
Expected: All existing tests pass (research tests may need adjustment since evidence type changed).

**Step 9: Commit**

```bash
git add -A && git commit -m "feat(research): add types — ResearchDomain, DomainProgress, LabState, LabDef, extend DataKind"
```

---

### Task 2: Data generation with diminishing returns

**Files:**
- Modify: `crates/sim_core/src/tasks.rs` (data generation on survey/deep-scan/mine)
- Modify: `crates/sim_core/src/station.rs` (data generation on assembler run)
- Modify: `crates/sim_core/src/engine.rs` (if needed for wiring)
- Test: `crates/sim_core/src/research.rs` (new test module for data generation)

**Step 1: Write failing tests for diminishing returns formula**

Create a test in `crates/sim_core/src/research.rs`:

```rust
#[cfg(test)]
mod data_generation_tests {
    use super::*;

    #[test]
    fn diminishing_returns_first_action_yields_peak() {
        let amount = data_yield(0, 100.0, 5.0, 0.7);
        assert!((amount - 100.0).abs() < 1e-3);
    }

    #[test]
    fn diminishing_returns_decays_over_actions() {
        let first = data_yield(0, 100.0, 5.0, 0.7);
        let second = data_yield(1, 100.0, 5.0, 0.7);
        let tenth = data_yield(9, 100.0, 5.0, 0.7);
        assert!(second < first);
        assert!(tenth < second);
        assert!(tenth >= 5.0); // floor
    }

    #[test]
    fn diminishing_returns_converges_to_floor() {
        let amount = data_yield(100, 100.0, 5.0, 0.7);
        assert!((amount - 5.0).abs() < 0.1);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core data_generation`
Expected: FAIL — `data_yield` not found.

**Step 3: Implement `data_yield` function**

Add to `research.rs`:

```rust
/// Diminishing-returns yield: `floor + (peak - floor) * decay_rate^count`
pub(crate) fn data_yield(count: u64, peak: f32, floor: f32, decay_rate: f32) -> f32 {
    floor + (peak - floor) * decay_rate.powi(count as i32)
}
```

**Step 4: Implement `generate_data` helper**

Add to `research.rs`:

```rust
/// Generate raw data with diminishing returns, updating pool and action counter.
pub(crate) fn generate_data(
    research: &mut ResearchState,
    kind: DataKind,
    action_key: &str,
    constants: &Constants,
) -> f32 {
    let count = research.action_counts.get(action_key).copied().unwrap_or(0);
    let amount = data_yield(
        count,
        constants.data_generation_peak,
        constants.data_generation_floor,
        constants.data_generation_decay_rate,
    );
    *research.data_pool.entry(kind).or_insert(0.0) += amount;
    *research.action_counts.entry(action_key.to_string()).or_insert(0) += 1;
    amount
}
```

**Step 5: Write tests for `generate_data`**

```rust
#[test]
fn generate_data_adds_to_pool_and_increments_counter() {
    let mut research = ResearchState {
        unlocked: HashSet::new(),
        data_pool: HashMap::new(),
        evidence: HashMap::new(),
        action_counts: HashMap::new(),
    };
    let constants = crate::test_fixtures::base_content().constants;

    let amount = generate_data(&mut research, DataKind::ScanData, "survey", &constants);
    assert!(amount > 0.0);
    assert!(research.data_pool[&DataKind::ScanData] > 0.0);
    assert_eq!(research.action_counts["survey"], 1);

    // Second call should yield less
    let amount2 = generate_data(&mut research, DataKind::ScanData, "survey", &constants);
    assert!(amount2 < amount);
    assert_eq!(research.action_counts["survey"], 2);
}
```

**Step 6: Run tests**

Run: `cargo test -p sim_core data_generation`
Expected: PASS

**Step 7: Wire data generation into existing task resolvers**

In `tasks.rs`, update the four data-producing functions to call `generate_data`:

- `resolve_survey` — after generating `DataGenerated` event, also call `generate_data(&mut state.research, DataKind::ScanData, "survey", &content.constants)`
- `resolve_deep_scan` — `generate_data(&mut state.research, DataKind::ScanData, "deep_scan", &content.constants)`
- `resolve_mine` — `generate_data(&mut state.research, DataKind::MiningData, &format!("mine_{}", template_id_if_available), &content.constants)`. For mine, the action key should be based on the asteroid's template — but we don't store template_id on `AsteroidState`. Use `"mine"` as the action key for now.
- In `station.rs` for `resolve_assembler_run` — `generate_data(&mut state.research, DataKind::EngineeringData, &format!("assemble_{}", recipe.id), &content.constants)`

Note: The existing `DataGenerated` event and `data_pool` updates in `resolve_survey`/`resolve_deep_scan` already exist. Remove the old manual `data_pool` update since `generate_data` now handles it. Keep the `DataGenerated` event emission but use the amount returned by `generate_data`.

**Step 8: Run all tests**

Run: `cargo test -p sim_core`
Expected: PASS

**Step 9: Commit**

```bash
git add -A && git commit -m "feat(research): data generation with diminishing returns"
```

---

### Task 3: Lab tick logic

**Files:**
- Modify: `crates/sim_core/src/station.rs` (add `tick_lab_modules`)
- Modify: `crates/sim_core/src/engine.rs` (if Lab install needs wiring)
- Test: `crates/sim_core/src/station.rs` (new test module)

**Step 1: Write failing tests for lab tick**

Add a test module in `station.rs`:

```rust
#[cfg(test)]
mod lab_tests {
    use super::*;
    use crate::test_fixtures::{base_content, base_state};
    use crate::{
        DataKind, LabDef, LabState, ModuleBehaviorDef, ModuleDef, ModuleInstanceId,
        ModuleKindState, ModuleState, ResearchDomain, StationId, TechId, WearState,
    };

    fn lab_content() -> crate::GameContent {
        let mut content = base_content();
        content.module_defs.push(ModuleDef {
            id: "module_exploration_lab".to_string(),
            name: "Exploration Lab".to_string(),
            mass_kg: 3500.0,
            volume_m3: 7.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.005,
            behavior: ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::Exploration,
                data_consumption_per_run: 8.0,
                research_points_per_run: 4.0,
                accepted_data: vec![DataKind::ScanData],
                research_interval_ticks: 1,
            }),
        });
        content
    }

    #[test]
    fn lab_consumes_data_and_produces_points() {
        let content = lab_content();
        let mut state = base_state(&content);
        let station_id = StationId("station_earth_orbit".to_string());
        let tech_id = TechId("tech_deep_scan_v1".to_string());

        // Add data to pool
        *state.research.data_pool.entry(DataKind::ScanData).or_insert(0.0) = 100.0;

        // Install lab with assigned tech
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(ModuleState {
            id: ModuleInstanceId("module_inst_lab_001".to_string()),
            def_id: "module_exploration_lab".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Lab(LabState {
                ticks_since_last_run: 0,
                assigned_tech: Some(tech_id.clone()),
                starved: false,
            }),
            wear: WearState::default(),
        });

        let mut events = Vec::new();
        tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Data consumed
        assert!(state.research.data_pool[&DataKind::ScanData] < 100.0);
        // Points produced
        let progress = &state.research.evidence[&tech_id];
        assert!(progress.points[&ResearchDomain::Exploration] > 0.0);
        // LabRan event emitted
        assert!(events.iter().any(|e| matches!(&e.event, Event::LabRan { .. })));
    }

    #[test]
    fn lab_starves_when_no_data() {
        let content = lab_content();
        let mut state = base_state(&content);
        let station_id = StationId("station_earth_orbit".to_string());
        let tech_id = TechId("tech_deep_scan_v1".to_string());

        // No data in pool
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(ModuleState {
            id: ModuleInstanceId("module_inst_lab_001".to_string()),
            def_id: "module_exploration_lab".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Lab(LabState {
                ticks_since_last_run: 0,
                assigned_tech: Some(tech_id),
                starved: false,
            }),
            wear: WearState::default(),
        });

        let mut events = Vec::new();
        tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Should be starved
        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Lab(lab) = &station.modules[0].kind_state {
            assert!(lab.starved);
        }
        assert!(events.iter().any(|e| matches!(&e.event, Event::LabStarved { .. })));
    }

    #[test]
    fn lab_partial_data_produces_proportional_points() {
        let content = lab_content();
        let mut state = base_state(&content);
        let station_id = StationId("station_earth_orbit".to_string());
        let tech_id = TechId("tech_deep_scan_v1".to_string());

        // Only 4.0 data available (lab wants 8.0)
        *state.research.data_pool.entry(DataKind::ScanData).or_insert(0.0) = 4.0;

        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(ModuleState {
            id: ModuleInstanceId("module_inst_lab_001".to_string()),
            def_id: "module_exploration_lab".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Lab(LabState {
                ticks_since_last_run: 0,
                assigned_tech: Some(tech_id.clone()),
                starved: false,
            }),
            wear: WearState::default(),
        });

        let mut events = Vec::new();
        tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // All data consumed
        assert!(state.research.data_pool.get(&DataKind::ScanData).copied().unwrap_or(0.0) < 0.01);
        // Points produced at half rate (4/8 * 4.0 = 2.0)
        let progress = &state.research.evidence[&tech_id];
        let points = progress.points[&ResearchDomain::Exploration];
        assert!((points - 2.0).abs() < 0.1);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core lab_`
Expected: FAIL — `tick_lab_modules` not found.

**Step 3: Implement `tick_lab_modules`**

Add to `station.rs`:

```rust
pub(crate) fn tick_lab_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let module_count = state
        .stations
        .get(station_id)
        .map_or(0, |s| s.modules.len());

    for module_idx in 0..module_count {
        let (lab_def, power_needed, wear_per_run) = {
            let Some(station) = state.stations.get(station_id) else { return };
            let module = &station.modules[module_idx];
            if !module.enabled { continue; }
            let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else { continue };
            let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else { continue };
            (lab_def.clone(), def.power_consumption_per_run, def.wear_per_run)
        };

        // Tick timer
        {
            let Some(station) = state.stations.get_mut(station_id) else { return };
            if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                ls.ticks_since_last_run += 1;
                if ls.ticks_since_last_run < lab_def.research_interval_ticks { continue; }
            } else { continue; }
        }

        // Check power
        {
            let Some(station) = state.stations.get(station_id) else { return };
            if station.power_available_per_tick < power_needed { continue; }
        }

        // Check assigned tech
        let assigned_tech = {
            let Some(station) = state.stations.get(station_id) else { return };
            match &station.modules[module_idx].kind_state {
                ModuleKindState::Lab(ls) => ls.assigned_tech.clone(),
                _ => continue,
            }
        };

        let Some(tech_id) = assigned_tech else {
            // No tech assigned — reset timer
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    ls.ticks_since_last_run = 0;
                }
            }
            continue;
        };

        // Skip if tech already unlocked
        if state.research.unlocked.contains(&tech_id) {
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    ls.ticks_since_last_run = 0;
                }
            }
            continue;
        }

        // Sum available accepted data
        let available_data: f32 = lab_def.accepted_data.iter()
            .map(|kind| state.research.data_pool.get(kind).copied().unwrap_or(0.0))
            .sum();

        let module_id = state.stations.get(station_id).unwrap().modules[module_idx].id.clone();
        let was_starved = match &state.stations.get(station_id).unwrap().modules[module_idx].kind_state {
            ModuleKindState::Lab(ls) => ls.starved,
            _ => false,
        };

        if available_data < 1e-6 {
            // Starved
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    ls.ticks_since_last_run = 0;
                    if !ls.starved {
                        ls.starved = true;
                        events.push(crate::emit(
                            &mut state.counters, state.meta.tick,
                            Event::LabStarved { station_id: station_id.clone(), module_id },
                        ));
                    }
                }
            }
            continue;
        }

        // Resume if was starved
        if was_starved {
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    ls.starved = false;
                }
            }
            events.push(crate::emit(
                &mut state.counters, state.meta.tick,
                Event::LabResumed { station_id: station_id.clone(), module_id: module_id.clone() },
            ));
        }

        // Consume data (proportionally from accepted kinds)
        let consumption_target = lab_def.data_consumption_per_run;
        let ratio = if available_data >= consumption_target { 1.0 } else { available_data / consumption_target };
        let actual_consumed = consumption_target.min(available_data);

        // Consume proportionally from each accepted data kind
        if available_data > 0.0 {
            let mut remaining_to_consume = actual_consumed;
            for kind in &lab_def.accepted_data {
                let pool = state.research.data_pool.entry(kind.clone()).or_insert(0.0);
                let take = (*pool / available_data * actual_consumed).min(*pool);
                *pool -= take;
                remaining_to_consume -= take;
            }
            // Absorb any floating-point remainder
            if remaining_to_consume > 1e-6 {
                for kind in &lab_def.accepted_data {
                    let pool = state.research.data_pool.entry(kind.clone()).or_insert(0.0);
                    let take = remaining_to_consume.min(*pool);
                    *pool -= take;
                    remaining_to_consume -= take;
                    if remaining_to_consume < 1e-6 { break; }
                }
            }
        }

        // Produce research points (wear-adjusted, proportional to data consumed)
        let wear_value = state.stations.get(station_id)
            .map_or(0.0, |s| s.modules[module_idx].wear.wear);
        let efficiency = crate::wear::wear_efficiency(wear_value, &content.constants);
        let points_produced = lab_def.research_points_per_run * ratio * efficiency;

        // Add points to tech's domain progress
        let progress = state.research.evidence
            .entry(tech_id.clone())
            .or_insert_with(|| crate::DomainProgress { points: std::collections::HashMap::new() });
        *progress.points.entry(lab_def.domain.clone()).or_insert(0.0) += points_produced;

        events.push(crate::emit(
            &mut state.counters, state.meta.tick,
            Event::LabRan {
                station_id: station_id.clone(),
                module_id: module_id.clone(),
                tech_id: tech_id.clone(),
                data_consumed: actual_consumed,
                points_produced,
                domain: lab_def.domain.clone(),
            },
        ));

        // Reset timer
        if let Some(station) = state.stations.get_mut(station_id) {
            if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                ls.ticks_since_last_run = 0;
            }
        }

        // Accumulate wear
        apply_wear(state, station_id, module_idx, wear_per_run, events);
    }
}
```

**Step 4: Wire `tick_lab_modules` into `tick_stations`**

In `station.rs`, update `tick_stations` to call `tick_lab_modules` after assemblers, before maintenance:

```rust
pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        tick_station_modules(state, station_id, content, events);  // processors
    }
    for station_id in &station_ids {
        tick_assembler_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_lab_modules(state, station_id, content, events);  // NEW
    }
    for station_id in &station_ids {
        tick_maintenance_modules(state, station_id, content, events);
    }
}
```

**Step 5: Wire Lab variant into `apply_commands` (InstallModule)**

In `engine.rs`, update the `InstallModule` match in `apply_commands` to handle the new `Lab` variant:

```rust
crate::ModuleBehaviorDef::Lab(_) => {
    crate::ModuleKindState::Lab(crate::LabState {
        ticks_since_last_run: 0,
        assigned_tech: None,
        starved: false,
    })
}
```

Also add `AssignLabTech` command handling in `apply_commands`:

```rust
Command::AssignLabTech { station_id, module_id, tech_id } => {
    let Some(station) = state.stations.get_mut(station_id) else { continue };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else { continue };
    if let ModuleKindState::Lab(ls) = &mut module.kind_state {
        ls.assigned_tech = tech_id.clone();
    }
}
```

**Step 6: Run tests**

Run: `cargo test -p sim_core`
Expected: PASS

**Step 7: Commit**

```bash
git add -A && git commit -m "feat(research): lab tick logic — consume data, produce domain points"
```

---

### Task 4: Rewrite advance_research — batched domain-sufficiency model

**Files:**
- Modify: `crates/sim_core/src/research.rs`
- Test: `crates/sim_core/src/research.rs` (new test module)

**Step 1: Write failing tests for new research roll**

```rust
#[cfg(test)]
mod research_roll_tests {
    use super::*;
    use crate::test_fixtures::{base_content, base_state, make_rng};
    use crate::{DomainProgress, ResearchDomain, TechId};
    use std::collections::HashMap;

    #[test]
    fn research_roll_skips_when_not_interval_tick() {
        let content = base_content();
        let mut state = base_state(&content);
        state.meta.tick = 1; // not a multiple of research_roll_interval_ticks (60)
        let mut rng = make_rng();
        let mut events = Vec::new();

        advance_research(&mut state, &content, &mut rng, crate::EventLevel::Normal, &mut events);

        // No TechUnlocked events
        assert!(!events.iter().any(|e| matches!(&e.event, crate::Event::TechUnlocked { .. })));
    }

    #[test]
    fn research_roll_runs_at_interval() {
        let mut content = base_content();
        // Make tech very easy to unlock
        content.techs[0].difficulty = 0.001;
        content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Exploration, 1.0)]);

        let mut state = base_state(&content);
        state.meta.tick = 60; // exactly at interval

        // Give enough domain points
        state.research.evidence.insert(
            TechId("tech_deep_scan_v1".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Exploration, 1000.0)]),
            },
        );

        let mut rng = make_rng();
        let mut events = Vec::new();

        advance_research(&mut state, &content, &mut rng, crate::EventLevel::Normal, &mut events);

        assert!(state.research.unlocked.contains(&TechId("tech_deep_scan_v1".to_string())));
    }

    #[test]
    fn zero_domain_progress_means_zero_probability() {
        let mut content = base_content();
        content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Exploration, 100.0)]);

        let mut state = base_state(&content);
        state.meta.tick = 60;
        // No domain progress at all

        let mut rng = make_rng();
        let mut events = Vec::new();

        advance_research(&mut state, &content, &mut rng, crate::EventLevel::Normal, &mut events);

        assert!(!state.research.unlocked.contains(&TechId("tech_deep_scan_v1".to_string())));
    }

    #[test]
    fn domain_sufficiency_geometric_mean() {
        // Verify the sufficiency calculation via the public helper
        let ratios = vec![1.0, 0.5]; // one domain fully met, one at 50%
        let sufficiency = geometric_mean(&ratios);
        assert!((sufficiency - 0.7071).abs() < 0.01); // sqrt(0.5)
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_core research_roll`
Expected: FAIL

**Step 3: Implement new `advance_research`**

Rewrite `research.rs`:

```rust
use crate::{Constants, DomainProgress, Event, EventLevel, GameContent, GameState, ResearchDomain, TechId};
use rand::Rng;

/// Geometric mean of a slice of f32 values.
pub(crate) fn geometric_mean(values: &[f32]) -> f32 {
    if values.is_empty() { return 0.0; }
    let product: f32 = values.iter().product();
    product.powf(1.0 / values.len() as f32)
}

/// Diminishing-returns yield: `floor + (peak - floor) * decay_rate^count`
pub(crate) fn data_yield(count: u64, peak: f32, floor: f32, decay_rate: f32) -> f32 {
    floor + (peak - floor) * decay_rate.powi(count as i32)
}

/// Generate raw data with diminishing returns, updating pool and action counter.
pub(crate) fn generate_data(
    research: &mut crate::ResearchState,
    kind: crate::DataKind,
    action_key: &str,
    constants: &Constants,
) -> f32 {
    let count = research.action_counts.get(action_key).copied().unwrap_or(0);
    let amount = data_yield(
        count,
        constants.data_generation_peak,
        constants.data_generation_floor,
        constants.data_generation_decay_rate,
    );
    *research.data_pool.entry(kind).or_insert(0.0) += amount;
    *research.action_counts.entry(action_key.to_string()).or_insert(0) += 1;
    amount
}

pub(crate) fn advance_research(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Only roll every N ticks
    if current_tick == 0 || current_tick % content.constants.research_roll_interval_ticks != 0 {
        return;
    }

    // Collect eligible techs: prereqs met, not yet unlocked. Sort for determinism.
    let mut eligible: Vec<TechId> = content
        .techs
        .iter()
        .filter(|tech| {
            !state.research.unlocked.contains(&tech.id)
                && tech.prereqs.iter().all(|prereq| state.research.unlocked.contains(prereq))
        })
        .map(|tech| tech.id.clone())
        .collect();
    eligible.sort_by(|a, b| a.0.cmp(&b.0));

    for tech_id in eligible {
        let tech_def = content.techs.iter().find(|t| t.id == tech_id).unwrap();

        let progress = state.research.evidence.get(&tech_id);

        // Compute domain sufficiency
        let sufficiency = if tech_def.domain_requirements.is_empty() {
            // No domain requirements — sufficiency is 1.0 if any points exist
            1.0
        } else {
            let ratios: Vec<f32> = tech_def.domain_requirements.iter()
                .map(|(domain, required)| {
                    let accumulated = progress
                        .map(|p| p.points.get(domain).copied().unwrap_or(0.0))
                        .unwrap_or(0.0);
                    (accumulated / required).min(1.0)
                })
                .collect();
            geometric_mean(&ratios)
        };

        // Total accumulated points across all domains
        let total_points: f32 = progress
            .map(|p| p.points.values().sum())
            .unwrap_or(0.0);

        let effective = sufficiency * total_points;
        let p = 1.0 - (-effective / tech_def.difficulty).exp();
        let rolled: f32 = rng.gen();

        if event_level == EventLevel::Debug {
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ResearchRoll {
                    tech_id: tech_id.clone(),
                    evidence: effective,
                    p,
                    rolled,
                },
            ));
        }

        if rolled < p {
            state.research.unlocked.insert(tech_id.clone());
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::TechUnlocked { tech_id },
            ));
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p sim_core`
Expected: PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(research): domain-sufficiency unlock model with batched rolls"
```

---

### Task 5: Update content files

**Files:**
- Modify: `content/constants.json`
- Modify: `content/techs.json`
- Modify: `content/module_defs.json`

**Step 1: Add new constants**

Add to `content/constants.json`:

```json
"research_roll_interval_ticks": 60,
"data_generation_peak": 100.0,
"data_generation_floor": 5.0,
"data_generation_decay_rate": 0.7
```

**Step 2: Update techs.json**

Add `domain_requirements` to existing tech and add 1-2 new techs:

```json
{
  "content_version": "0.0.1",
  "techs": [
    {
      "id": "tech_deep_scan_v1",
      "name": "Deep Scan v1",
      "prereqs": [],
      "domain_requirements": { "Exploration": 100.0 },
      "accepted_data": ["ScanData"],
      "difficulty": 200.0,
      "effects": [
        { "type": "EnableDeepScan" },
        { "type": "DeepScanCompositionNoise", "sigma": 0.02 }
      ]
    },
    {
      "id": "tech_advanced_refining",
      "name": "Advanced Refining",
      "prereqs": [],
      "domain_requirements": { "Materials": 150.0, "Engineering": 50.0 },
      "accepted_data": ["MiningData", "EngineeringData"],
      "difficulty": 400.0,
      "effects": []
    }
  ]
}
```

**Step 3: Add lab module definitions**

Add to `content/module_defs.json`:

```json
{
  "id": "module_materials_lab",
  "name": "Materials Lab",
  "mass_kg": 4000.0,
  "volume_m3": 8.0,
  "power_consumption_per_run": 12.0,
  "wear_per_run": 0.005,
  "behavior": {
    "Lab": {
      "domain": "Materials",
      "data_consumption_per_run": 10.0,
      "research_points_per_run": 5.0,
      "accepted_data": ["MiningData", "EngineeringData"],
      "research_interval_ticks": 1
    }
  }
},
{
  "id": "module_exploration_lab",
  "name": "Exploration Lab",
  "mass_kg": 3500.0,
  "volume_m3": 7.0,
  "power_consumption_per_run": 10.0,
  "wear_per_run": 0.005,
  "behavior": {
    "Lab": {
      "domain": "Exploration",
      "data_consumption_per_run": 8.0,
      "research_points_per_run": 4.0,
      "accepted_data": ["ScanData"],
      "research_interval_ticks": 1
    }
  }
}
```

**Step 4: Verify content loads correctly**

Run: `cargo test -p sim_world`
Expected: PASS (sim_world's `load_content` should deserialize new types)

**Step 5: Run full test suite**

Run: `cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A && git commit -m "feat(research): update content — lab modules, domain requirements, research constants"
```

---

### Task 6: Remove old FacilitiesState research system

**Files:**
- Modify: `crates/sim_core/src/types.rs` (remove FacilitiesState or repurpose)
- Modify: `crates/sim_core/src/research.rs` (remove station-based compute loop)
- Modify: related tests

The old research system loops over stations and uses `FacilitiesState.compute_units_total` to split evidence. This is replaced by labs. Remove:
- The per-station loop in `advance_research` — research roll is now sim-wide
- `FacilitiesState.compute_units_total`, `power_per_compute_unit_per_tick`, `efficiency` — no longer needed for research
- `PowerConsumed` event from research (labs handle their own power)

Keep `FacilitiesState` struct but remove research-related fields. Or if nothing else uses it, remove it entirely from `StationState`.

**Decision:** Keep `FacilitiesState` with compute fields but mark them as unused/deprecated. The power budget for individual modules is already checked per-module. The old station-level power budget for research is obsolete.

Actually, looking at the code: `FacilitiesState` is only used by the old `advance_research`. Remove `FacilitiesState` entirely and drop the `facilities` field from `StationState`.

**Step 1: Remove `FacilitiesState` from `StationState`**
**Step 2: Remove `station_compute_units_total`, `station_power_per_compute_unit_per_tick`, `station_efficiency` from `Constants`**
**Step 3: Fix all compilation errors (test fixtures, sim_control tests, sim_world)**
**Step 4: Run tests**

Run: `cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "refactor(research): remove FacilitiesState — research now driven by labs"
```

---

### Task 7: Autopilot lab management

**Files:**
- Modify: `crates/sim_control/src/lib.rs`
- Test: `crates/sim_control/src/lib.rs`

**Step 1: Write failing tests**

```rust
#[test]
fn test_autopilot_installs_lab_module() {
    let mut content = autopilot_content();
    content.module_defs.push(sim_core::ModuleDef {
        id: "module_exploration_lab".to_string(),
        name: "Exploration Lab".to_string(),
        mass_kg: 3500.0,
        volume_m3: 7.0,
        power_consumption_per_run: 10.0,
        wear_per_run: 0.005,
        behavior: sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
            domain: sim_core::ResearchDomain::Exploration,
            data_consumption_per_run: 8.0,
            research_points_per_run: 4.0,
            accepted_data: vec![sim_core::DataKind::ScanData],
            research_interval_ticks: 1,
        }),
    });
    let mut state = autopilot_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().inventory.push(
        sim_core::InventoryItem::Module {
            item_id: sim_core::ModuleItemId("module_item_lab".to_string()),
            module_def_id: "module_exploration_lab".to_string(),
        },
    );

    let mut autopilot = AutopilotController;
    let mut next_id = 0u64;
    let commands = autopilot.generate_commands(&state, &content, &mut next_id);

    assert!(commands.iter().any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })));
}

#[test]
fn test_autopilot_assigns_lab_to_eligible_tech() {
    let mut content = autopilot_content();
    content.techs.push(sim_core::TechDef {
        id: TechId("tech_test".to_string()),
        name: "Test Tech".to_string(),
        prereqs: vec![],
        domain_requirements: std::collections::HashMap::from([
            (sim_core::ResearchDomain::Exploration, 100.0),
        ]),
        accepted_data: vec![sim_core::DataKind::ScanData],
        difficulty: 100.0,
        effects: vec![],
    });
    content.module_defs.push(sim_core::ModuleDef {
        id: "module_exploration_lab".to_string(),
        name: "Exploration Lab".to_string(),
        mass_kg: 3500.0,
        volume_m3: 7.0,
        power_consumption_per_run: 10.0,
        wear_per_run: 0.005,
        behavior: sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
            domain: sim_core::ResearchDomain::Exploration,
            data_consumption_per_run: 8.0,
            research_points_per_run: 4.0,
            accepted_data: vec![sim_core::DataKind::ScanData],
            research_interval_ticks: 1,
        }),
    });
    let mut state = autopilot_state(&content);

    // Install lab module (already installed, unassigned)
    let station_id = StationId("station_earth_orbit".to_string());
    state.stations.get_mut(&station_id).unwrap().modules.push(
        sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("module_inst_lab_001".to_string()),
            def_id: "module_exploration_lab".to_string(),
            enabled: true,
            kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                ticks_since_last_run: 0,
                assigned_tech: None,
                starved: false,
            }),
            wear: sim_core::WearState::default(),
        },
    );

    let mut autopilot = AutopilotController;
    let mut next_id = 0u64;
    let commands = autopilot.generate_commands(&state, &content, &mut next_id);

    assert!(commands.iter().any(|cmd| matches!(
        &cmd.command,
        sim_core::Command::AssignLabTech { tech_id: Some(_), .. }
    )));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p sim_control autopilot_assigns_lab`
Expected: FAIL

**Step 3: Implement autopilot lab assignment**

Add a `lab_assignment_commands` function in `sim_control/src/lib.rs`:

```rust
fn lab_assignment_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();

    for station in state.stations.values() {
        for module in &station.modules {
            let ModuleKindState::Lab(lab_state) = &module.kind_state else { continue };
            if lab_state.assigned_tech.is_some() { continue; }

            // Find lab's domain from def
            let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else { continue };
            let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else { continue };

            // Find eligible techs that need this domain, sorted by sufficiency (highest first)
            let mut candidates: Vec<(TechId, f32)> = content.techs.iter()
                .filter(|tech| {
                    !state.research.unlocked.contains(&tech.id)
                        && tech.prereqs.iter().all(|p| state.research.unlocked.contains(p))
                        && tech.domain_requirements.contains_key(&lab_def.domain)
                })
                .map(|tech| {
                    let progress = state.research.evidence.get(&tech.id);
                    let sufficiency = compute_sufficiency(tech, progress);
                    (tech.id.clone(), sufficiency)
                })
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.0.cmp(&b.0.0)));

            if let Some((tech_id, _)) = candidates.first() {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::AssignLabTech {
                        station_id: station.id.clone(),
                        module_id: module.id.clone(),
                        tech_id: Some(tech_id.clone()),
                    },
                ));
            }
        }
    }
    commands
}

fn compute_sufficiency(tech: &sim_core::TechDef, progress: Option<&sim_core::DomainProgress>) -> f32 {
    if tech.domain_requirements.is_empty() { return 1.0; }
    let ratios: Vec<f32> = tech.domain_requirements.iter()
        .map(|(domain, required)| {
            let accumulated = progress
                .map(|p| p.points.get(domain).copied().unwrap_or(0.0))
                .unwrap_or(0.0);
            (accumulated / required).min(1.0)
        })
        .collect();
    let product: f32 = ratios.iter().product();
    product.powf(1.0 / ratios.len() as f32)
}
```

Call `lab_assignment_commands` from `generate_commands`:

```rust
commands.extend(lab_assignment_commands(state, content, &owner, next_command_id));
```

**Step 4: Run tests**

Run: `cargo test -p sim_control`
Expected: PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(research): autopilot lab installation and tech assignment"
```

---

### Task 8: Update sim_daemon snapshot/SSE for new types

**Files:**
- Modify: `crates/sim_daemon/src/state.rs` (if needed for serialization)
- Test: Integration test — run daemon briefly, check snapshot includes new fields

The new types (`ResearchDomain`, `DomainProgress`, `LabState`, etc.) already derive `Serialize`/`Deserialize`. The daemon's `/api/v1/snapshot` endpoint serializes `GameState` directly, so the new fields will appear automatically.

Verify:
- `data_pool` shows all `DataKind` variants
- `evidence` now contains `DomainProgress` objects with domain points
- Lab module state serializes `assigned_tech` and `starved`

**Step 1: Run daemon tests**

Run: `cargo test -p sim_daemon`
Expected: PASS

**Step 2: Verify CLI still works**

Run: `cargo run -p sim_cli -- run --ticks 100 --seed 42`
Expected: Runs without error. Research events may change behavior.

**Step 3: Commit (if any daemon changes were needed)**

```bash
git add -A && git commit -m "chore: verify daemon and CLI with new research types"
```

---

### Task 9: Update CLAUDE.md and reference docs

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/reference.md`

**Step 1: Update CLAUDE.md**

- Update tick order to show `3a. Processors → 3b. Assemblers → 3c. Labs → 3d. Maintenance`
- Add Lab to module behavior types
- Update research description: "Labs consume raw data, produce domain-specific points. Probabilistic unlock with domain sufficiency."
- Add `AssignLabTech` to command list
- Add new events: `LabRan`, `LabStarved`, `LabResumed`
- Add new content files info: lab module defs
- Update sim_core public API if any new functions exposed

**Step 2: Update docs/reference.md**

- Add `ResearchDomain`, `DomainProgress`, `LabState`, `LabDef` type docs
- Add `DataKind` variants
- Document new constants
- Document research unlock formula

**Step 3: Commit**

```bash
git add -A && git commit -m "docs: update CLAUDE.md and reference.md for research system"
```

---

### Task 10: Integration test — full research lifecycle

**Files:**
- Create: `crates/sim_core/tests/research_lifecycle.rs`

**Step 1: Write integration test**

```rust
//! Integration test: survey generates ScanData → lab consumes data → domain points accumulate → tech unlocks.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_core::*;
use sim_core::test_fixtures::{base_content, base_state};
use std::collections::HashMap;

#[test]
fn full_research_lifecycle() {
    let mut content = base_content();

    // Add exploration lab module def
    content.module_defs.push(ModuleDef {
        id: "module_exploration_lab".to_string(),
        name: "Exploration Lab".to_string(),
        mass_kg: 3500.0,
        volume_m3: 7.0,
        power_consumption_per_run: 10.0,
        wear_per_run: 0.0, // no wear for test simplicity
        behavior: ModuleBehaviorDef::Lab(LabDef {
            domain: ResearchDomain::Exploration,
            data_consumption_per_run: 8.0,
            research_points_per_run: 4.0,
            accepted_data: vec![DataKind::ScanData],
            research_interval_ticks: 1,
        }),
    });

    // Make tech require Exploration domain, low difficulty
    content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Exploration, 10.0)]);
    content.techs[0].difficulty = 5.0;

    let mut state = base_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Seed data pool
    *state.research.data_pool.entry(DataKind::ScanData).or_insert(0.0) = 1000.0;

    // Install lab with assigned tech
    state.stations.get_mut(&station_id).unwrap().modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_lab_001".to_string()),
        def_id: "module_exploration_lab".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Lab(LabState {
            ticks_since_last_run: 0,
            assigned_tech: Some(TechId("tech_deep_scan_v1".to_string())),
            starved: false,
        }),
        wear: WearState::default(),
    });

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let tech_id = TechId("tech_deep_scan_v1".to_string());

    // Run enough ticks for labs to accumulate points and research to roll
    for _ in 0..120 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    // Tech should be unlocked (low difficulty, lots of data)
    assert!(
        state.research.unlocked.contains(&tech_id),
        "tech should unlock after sufficient lab work. Evidence: {:?}",
        state.research.evidence.get(&tech_id),
    );
}
```

**Step 2: Run test**

Run: `cargo test -p sim_core --test research_lifecycle`
Expected: PASS

**Step 3: Commit**

```bash
git add -A && git commit -m "test: research lifecycle integration test"
```

---

### Task 11 (optional): UI Research Panel updates

**Files:**
- Modify: `ui_web/src/components/ResearchPanel.tsx`
- Modify: `ui_web/src/types.ts` (add TypeScript types for new fields)

This task updates the React frontend to display:
- Raw data pool levels
- Per-tech domain progress bars
- Lab module state (assigned tech, starved)

This is a larger UI task and can be deferred to a separate branch if desired. The backend changes are complete and the snapshot already exposes all needed data.

**Step 1: Add TypeScript types**

```typescript
export type ResearchDomain = 'Materials' | 'Exploration' | 'Engineering';
export type DataKind = 'ScanData' | 'MiningData' | 'EngineeringData';

export interface DomainProgress {
  points: Record<ResearchDomain, number>;
}

export interface ResearchState {
  unlocked: string[];
  data_pool: Record<DataKind, number>;
  evidence: Record<string, DomainProgress>;
  action_counts: Record<string, number>;
}

export interface LabState {
  ticks_since_last_run: number;
  assigned_tech: string | null;
  starved: boolean;
}
```

**Step 2: Update ResearchPanel to show domain progress**

Show per-tech progress bars colored by domain, with requirement thresholds marked. Show raw data pool as bar chart or numeric display.

**Step 3: Run frontend tests**

Run: `cd ui_web && npm test`
Expected: PASS

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(ui): research panel domain progress and data pool display"
```
