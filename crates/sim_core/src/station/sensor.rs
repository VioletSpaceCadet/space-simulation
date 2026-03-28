use crate::{
    research::generate_data, Event, EventEnvelope, GameContent, GameState, ModuleBehaviorDef,
    StationId,
};

pub(super) fn tick_sensor_array_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    super::ensure_station_index(state, station_id, content);
    let indices: Vec<usize> = state
        .stations
        .get(station_id)
        .map(|s| s.module_type_index.sensors.clone())
        .unwrap_or_default();

    for module_idx in indices {
        let Some(ctx) = super::extract_context(state, station_id, module_idx, content) else {
            continue;
        };

        let ModuleBehaviorDef::SensorArray(sensor_def) = &ctx.def.behavior else {
            continue;
        };
        let sensor_def = sensor_def.clone();

        if !super::should_run(state, &ctx) {
            continue;
        }

        let outcome = execute(&ctx, &sensor_def, state, content, events);
        super::apply_run_result(state, &ctx, outcome, content, events);
    }
}

fn execute(
    _ctx: &super::ModuleTickContext,
    sensor_def: &crate::SensorArrayDef,
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) -> super::RunOutcome {
    let current_tick = state.meta.tick;

    let amount = generate_data(
        &mut state.research,
        sensor_def.data_kind.clone(),
        &sensor_def.action_key,
        &content.constants,
    );

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: sensor_def.data_kind.clone(),
            amount,
        },
    ));

    super::RunOutcome::Completed
}

#[cfg(test)]
mod tests {
    use crate::AHashMap;
    use crate::*;
    use std::collections::{HashMap, HashSet};

    fn sensor_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_sensor_array".to_string(),
            ModuleDef {
                id: "module_sensor_array".to_string(),
                name: "Sensor Array".to_string(),
                mass_kg: 2500.0,
                volume_m3: 6.0,
                power_consumption_per_run: 8.0,
                wear_per_run: 0.003,
                behavior: ModuleBehaviorDef::SensorArray(SensorArrayDef {
                    data_kind: DataKind::SurveyData,
                    action_key: "sensor_scan".to_string(),
                    scan_interval_minutes: 5,
                    scan_interval_ticks: 5,
                }),
                thermal: None,
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
                power_stall_priority: None,
                roles: vec![],
                crew_requirement: Default::default(),
                required_tech: None,
                ports: Vec::new(),
            },
        );
        content
    }

    fn sensor_state(content: &GameContent) -> GameState {
        let station_id = StationId("station_test".to_string());
        let mut state = GameState {
            meta: MetaState {
                tick: 0,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: std::collections::BTreeMap::new(),
            ships: std::collections::BTreeMap::new(),
            stations: [(
                station_id.clone(),
                StationState {
                    id: station_id,
                    position: crate::test_fixtures::test_position(),
                    inventory: vec![],
                    cargo_capacity_m3: 2000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("sensor_inst_0001".to_string()),
                        def_id: "module_sensor_array".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::SensorArray(SensorArrayState {
                            ticks_since_last_run: 0,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        module_priority: 0,
                        assigned_crew: Default::default(),
                        crew_satisfied: true,
                        thermal: None,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    crew: Default::default(),
                    leaders: Vec::new(),
                    thermal_links: Vec::new(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                    power_budget_cache: crate::PowerBudgetCache::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: AHashMap::default(),
                evidence: AHashMap::default(),
                action_counts: AHashMap::default(),
            },
            balance: 0.0,
            export_revenue_total: 0.0,
            export_count: 0,
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
                next_lot_id: 0,
                next_module_instance_id: 2,
            },
            modifiers: crate::modifiers::ModifierSet::default(),
            events: crate::sim_events::SimEventState::default(),
            propellant_consumed_total: 0.0,
            body_cache: AHashMap::default(),
        };
        crate::test_fixtures::rebuild_indices(&mut state, content);
        state
    }

    #[test]
    fn sensor_array_generates_scan_data_after_interval() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let station_id = StationId("station_test".to_string());

        // Tick 4 times — interval is 5, should not fire yet
        for _ in 0..4 {
            let mut events = Vec::new();
            super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
            let generated = events
                .iter()
                .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
            assert!(!generated, "should not generate data before interval");
        }

        // Tick once more — should fire
        let mut events = Vec::new();
        super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
        let generated = events
            .iter()
            .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
        assert!(generated, "should generate data at interval");

        // Check data pool has ScanData
        let scan_data = state
            .research
            .data_pool
            .get(&DataKind::SurveyData)
            .copied()
            .unwrap_or(0.0);
        assert!(scan_data > 0.0, "ScanData should be > 0 after sensor run");
    }

    #[test]
    fn sensor_array_uses_diminishing_returns() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let station_id = StationId("station_test".to_string());

        // Run through two complete intervals and capture amounts
        let mut amounts = Vec::new();
        for run in 0..2 {
            // Tick through interval
            for tick in 0..5 {
                let mut events = Vec::new();
                super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
                if tick == 4 {
                    // Last tick of interval — should fire
                    for event in &events {
                        if let Event::DataGenerated { amount, .. } = &event.event {
                            amounts.push(*amount);
                        }
                    }
                }
            }
            let _ = run;
        }

        assert_eq!(amounts.len(), 2, "should have fired twice");
        assert!(
            amounts[1] < amounts[0],
            "second run should yield less due to diminishing returns (got {} then {})",
            amounts[0],
            amounts[1]
        );
    }

    #[test]
    fn sensor_array_disabled_does_not_generate() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let station_id = StationId("station_test".to_string());

        // Disable the module
        state.stations.get_mut(&station_id).unwrap().modules[0].enabled = false;

        // Tick through full interval
        for _ in 0..10 {
            let mut events = Vec::new();
            super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
            let generated = events
                .iter()
                .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
            assert!(!generated, "disabled sensor should not generate data");
        }

        let scan_data = state
            .research
            .data_pool
            .get(&DataKind::SurveyData)
            .copied()
            .unwrap_or(0.0);
        assert!(
            scan_data == 0.0,
            "no ScanData should exist when sensor is disabled"
        );
    }
}
