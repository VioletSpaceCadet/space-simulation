use crate::{
    research::generate_data, Event, EventEnvelope, GameContent, GameState, ModuleBehaviorDef,
    SiteId, StationId,
};
use rand::Rng;

pub(super) fn tick_sensor_array_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    super::ensure_station_index(state, station_id, content);
    let indices: Vec<usize> = state
        .stations
        .get(station_id)
        .map(|s| s.core.module_type_index.sensors.clone())
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

        let outcome = execute(&ctx, &sensor_def, state, content, rng, events);
        super::apply_run_result(state, &ctx, outcome, content, events);
    }
}

fn execute(
    _ctx: &super::ModuleTickContext,
    sensor_def: &crate::SensorArrayDef,
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
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

    // Discovery: sensors with discovery_zones can spawn scan sites.
    if !sensor_def.discovery_zones.is_empty() && sensor_def.discovery_probability > 0.0 {
        let roll: f64 = rng.gen();
        if roll < sensor_def.discovery_probability {
            try_discover_scan_site(sensor_def, state, content, rng, events);
        }
    }

    super::RunOutcome::Completed
}

/// Attempt to discover a new scan site in one of the sensor's target zones.
fn try_discover_scan_site(
    sensor_def: &crate::SensorArrayDef,
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Find zone bodies matching the sensor's discovery zones.
    let zone_bodies: Vec<&crate::OrbitalBodyDef> = content
        .solar_system
        .bodies
        .iter()
        .filter(|b| b.zone.is_some() && sensor_def.discovery_zones.contains(&b.id.0))
        .collect();

    if zone_bodies.is_empty() {
        return;
    }

    let body = crate::pick_zone_weighted(&zone_bodies, rng);
    let zone_class = body.zone.as_ref().expect("zone body").resource_class;
    let template = crate::pick_template_biased(&content.asteroid_templates, zone_class, rng);
    let position = crate::random_position_in_zone(body, rng);
    let uuid = crate::generate_uuid(rng);
    let site_id = SiteId(format!("site_{uuid}"));

    state.scan_sites.push(crate::ScanSite {
        id: site_id.clone(),
        position: position.clone(),
        template_id: template.id.clone(),
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ScanSiteSpawned {
            site_id,
            position,
            template_id: template.id.clone(),
        },
    ));
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::ModuleDefBuilder;
    use crate::AHashMap;
    use crate::*;
    use rand::SeedableRng;
    use std::collections::{HashMap, HashSet};

    fn sensor_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_sensor_array".to_string(),
            ModuleDefBuilder::new("module_sensor_array")
                .name("Sensor Array")
                .mass(2500.0)
                .volume(6.0)
                .power(8.0)
                .wear(0.003)
                .behavior(ModuleBehaviorDef::SensorArray(SensorArrayDef {
                    data_kind: DataKind::new(DataKind::SURVEY),
                    action_key: "sensor_scan".to_string(),
                    scan_interval_minutes: 5,
                    scan_interval_ticks: 5,
                    sensor_type: "orbital".to_string(),
                    discovery_zones: vec![],
                    discovery_probability: 0.0,
                }))
                .build(),
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
                    core: FacilityCore {
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
                            efficiency: 1.0,
                            prev_crew_satisfied: true,
                            thermal: None,
                        }],
                        modifiers: crate::modifiers::ModifierSet::default(),
                        crew: Default::default(),
                        thermal_links: Vec::new(),
                        power: PowerState::default(),
                        cached_inventory_volume_m3: None,
                        module_type_index: crate::ModuleTypeIndex::default(),
                        module_id_index: HashMap::new(),
                        power_budget_cache: crate::PowerBudgetCache::default(),
                    },
                    leaders: Vec::new(),
                },
            )]
            .into_iter()
            .collect(),
            ground_facilities: std::collections::BTreeMap::new(),
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
            progression: Default::default(),
            body_cache: AHashMap::default(),
        };
        crate::test_fixtures::rebuild_indices(&mut state, content);
        state
    }

    #[test]
    fn sensor_array_generates_scan_data_after_interval() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_test".to_string());

        // Tick 4 times — interval is 5, should not fire yet
        for _ in 0..4 {
            let mut events = Vec::new();
            super::tick_sensor_array_modules(
                &mut state,
                &station_id,
                &content,
                &mut rng,
                &mut events,
            );
            let generated = events
                .iter()
                .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
            assert!(!generated, "should not generate data before interval");
        }

        // Tick once more — should fire
        let mut events = Vec::new();
        super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut rng, &mut events);
        let generated = events
            .iter()
            .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
        assert!(generated, "should generate data at interval");

        // Check data pool has ScanData
        let scan_data = state
            .research
            .data_pool
            .get(&DataKind::new(DataKind::SURVEY))
            .copied()
            .unwrap_or(0.0);
        assert!(scan_data > 0.0, "ScanData should be > 0 after sensor run");
    }

    #[test]
    fn sensor_array_uses_diminishing_returns() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_test".to_string());

        // Run through two complete intervals and capture amounts
        let mut amounts = Vec::new();
        for run in 0..2 {
            // Tick through interval
            for tick in 0..5 {
                let mut events = Vec::new();
                super::tick_sensor_array_modules(
                    &mut state,
                    &station_id,
                    &content,
                    &mut rng,
                    &mut events,
                );
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
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_test".to_string());

        // Disable the module
        state.stations.get_mut(&station_id).unwrap().core.modules[0].enabled = false;

        // Tick through full interval
        for _ in 0..10 {
            let mut events = Vec::new();
            super::tick_sensor_array_modules(
                &mut state,
                &station_id,
                &content,
                &mut rng,
                &mut events,
            );
            let generated = events
                .iter()
                .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
            assert!(!generated, "disabled sensor should not generate data");
        }

        let scan_data = state
            .research
            .data_pool
            .get(&DataKind::new(DataKind::SURVEY))
            .copied()
            .unwrap_or(0.0);
        assert!(
            scan_data == 0.0,
            "no ScanData should exist when sensor is disabled"
        );
    }
}
