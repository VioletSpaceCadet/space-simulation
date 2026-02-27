use super::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

fn replenish_test_content() -> GameContent {
    let mut content = GameContent {
        content_version: "test".to_string(),
        techs: vec![],
        solar_system: SolarSystemDef {
            nodes: vec![NodeDef {
                id: NodeId("node_test".to_string()),
                name: "Test Node".to_string(),
                solar_intensity: 1.0,
            }],
            edges: vec![],
        },
        asteroid_templates: vec![AsteroidTemplateDef {
            id: "tmpl_iron_rich".to_string(),
            anomaly_tags: vec![AnomalyTag::IronRich],
            composition_ranges: HashMap::from([
                ("Fe".to_string(), (0.7, 0.7)),
                ("Si".to_string(), (0.3, 0.3)),
            ]),
        }],
        elements: vec![ElementDef {
            id: "ore".to_string(),
            density_kg_per_m3: 3000.0,
            display_name: "Raw Ore".to_string(),
            refined_name: None,
        }],
        module_defs: HashMap::new(),
        component_defs: vec![],
        pricing: PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: HashMap::new(),
        },
        constants: Constants {
            survey_scan_minutes: 1,
            deep_scan_minutes: 1,
            travel_minutes_per_hop: 1,
            survey_tag_detection_probability: 1.0,
            asteroid_count_per_template: 1,
            asteroid_mass_min_kg: 500.0,
            asteroid_mass_max_kg: 500.0,
            ship_cargo_capacity_m3: 20.0,
            station_cargo_capacity_m3: 10_000.0,
            station_power_available_per_minute: 100.0,
            mining_rate_kg_per_minute: 50.0,
            deposit_minutes: 1,
            autopilot_iron_rich_confidence_threshold: 0.7,
            autopilot_refinery_threshold_kg: 500.0,
            research_roll_interval_minutes: 60,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            autopilot_slag_jettison_pct: 0.75,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
            minutes_per_tick: 1,
            // Derived fields — filled by derive_tick_values()
            survey_scan_ticks: 0,
            deep_scan_ticks: 0,
            travel_ticks_per_hop: 0,
            mining_rate_kg_per_tick: 0.0,
            deposit_ticks: 0,
            station_power_available_per_tick: 0.0,
            research_roll_interval_ticks: 0,
        },
        density_map: HashMap::new(),
    };
    content.constants.derive_tick_values();
    content.init_caches();
    content
}

fn empty_sites_state(content: &GameContent) -> GameState {
    GameState {
        meta: MetaState {
            tick: 0,
            seed: 42,
            schema_version: 1,
            content_version: content.content_version.clone(),
        },
        scan_sites: vec![],
        asteroids: HashMap::new(),
        ships: HashMap::new(),
        stations: HashMap::from([(
            StationId("station_test".to_string()),
            StationState {
                id: StationId("station_test".to_string()),
                location_node: NodeId("node_test".to_string()),
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                power: PowerState::default(),
                cached_inventory_volume_m3: None,
            },
        )]),
        research: ResearchState {
            unlocked: HashSet::new(),
            data_pool: HashMap::new(),
            evidence: HashMap::new(),
            action_counts: HashMap::new(),
        },
        balance: 0.0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
    }
}

#[test]
fn replenish_spawns_sites_when_below_threshold() {
    let content = replenish_test_content();
    let mut state = empty_sites_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert_eq!(state.scan_sites.len(), 5); // REPLENISH_BATCH_SIZE
    let spawned_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.event, Event::ScanSiteSpawned { .. }))
        .collect();
    assert_eq!(spawned_events.len(), 5);
}

#[test]
fn replenish_does_not_spawn_when_at_threshold() {
    let content = replenish_test_content();
    let mut state = empty_sites_state(&content);
    // Pre-fill with MIN_UNSCANNED_SITES sites
    for i in 0..5 {
        state.scan_sites.push(ScanSite {
            id: SiteId(format!("site_existing_{i}")),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        });
    }
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let spawned_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.event, Event::ScanSiteSpawned { .. }))
        .collect();
    assert_eq!(spawned_events.len(), 0);
    assert_eq!(state.scan_sites.len(), 5);
}

#[test]
fn replenish_site_ids_are_unique_uuids() {
    let content = replenish_test_content();
    let mut state = empty_sites_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let ids: Vec<_> = state.scan_sites.iter().map(|s| s.id.0.clone()).collect();
    // All start with "site_"
    for id in &ids {
        assert!(id.starts_with("site_"), "ID should start with site_: {id}");
    }
    // All unique
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "Site IDs should be unique");
}

#[test]
fn jettison_slag_removes_all_slag_and_emits_event() {
    let content = replenish_test_content();
    let mut state = empty_sites_state(&content);
    // Pre-fill scan sites so replenish doesn't fire
    for i in 0..5 {
        state.scan_sites.push(ScanSite {
            id: SiteId(format!("site_existing_{i}")),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        });
    }

    let station_id = StationId("station_test".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Slag {
        kg: 100.0,
        composition: HashMap::from([("slag".to_string(), 1.0)]),
    });
    station.inventory.push(InventoryItem::Slag {
        kg: 50.0,
        composition: HashMap::from([("slag".to_string(), 1.0)]),
    });
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 200.0,
        quality: 0.8,
    });

    let cmd = CommandEnvelope {
        id: crate::CommandId("cmd_000001".to_string()),
        issued_by: crate::PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::JettisonSlag {
            station_id: station_id.clone(),
        },
    };

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    // Slag should be gone, material should remain
    let station = &state.stations[&station_id];
    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Slag { .. })),
        "all slag should be removed"
    );
    assert_eq!(station.inventory.len(), 1, "material should remain");

    // Should have emitted SlagJettisoned event with total kg
    let jettison_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.event, Event::SlagJettisoned { .. }))
        .collect();
    assert_eq!(jettison_events.len(), 1);
    if let Event::SlagJettisoned { kg, .. } = &jettison_events[0].event {
        assert!(
            (kg - 150.0).abs() < f32::EPSILON,
            "should jettison 150 kg total"
        );
    }
}

#[test]
fn jettison_slag_no_event_when_no_slag() {
    let content = replenish_test_content();
    let mut state = empty_sites_state(&content);
    for i in 0..5 {
        state.scan_sites.push(ScanSite {
            id: SiteId(format!("site_existing_{i}")),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        });
    }

    let station_id = StationId("station_test".to_string());

    let cmd = CommandEnvelope {
        id: crate::CommandId("cmd_000001".to_string()),
        issued_by: crate::PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::JettisonSlag {
            station_id: station_id.clone(),
        },
    };

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, Event::SlagJettisoned { .. })),
        "no event should be emitted when there is no slag"
    );
}

#[test]
fn replenish_is_deterministic() {
    let content = replenish_test_content();

    let mut state1 = empty_sites_state(&content);
    let mut rng1 = ChaCha8Rng::seed_from_u64(42);
    tick(&mut state1, &[], &content, &mut rng1, EventLevel::Normal);

    let mut state2 = empty_sites_state(&content);
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    tick(&mut state2, &[], &content, &mut rng2, EventLevel::Normal);

    let ids1: Vec<_> = state1.scan_sites.iter().map(|s| s.id.0.clone()).collect();
    let ids2: Vec<_> = state2.scan_sites.iter().map(|s| s.id.0.clone()).collect();
    assert_eq!(ids1, ids2);
}
