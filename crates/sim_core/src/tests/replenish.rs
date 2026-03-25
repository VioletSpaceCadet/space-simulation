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
            bodies: vec![OrbitalBodyDef {
                id: BodyId("test_body".to_string()),
                name: "Test Belt".to_string(),
                parent: None,
                body_type: BodyType::Belt,
                radius_au_um: 0,
                angle_mdeg: 0,
                solar_intensity: 1.0,
                zone: Some(ZoneDef {
                    radius_min_au_um: 1_000,
                    radius_max_au_um: 2_000,
                    angle_start_mdeg: 0,
                    angle_span_mdeg: 360_000,
                    resource_class: ResourceClass::MetalRich,
                    scan_site_weight: 1,
                }),
            }],
            nodes: vec![NodeDef {
                id: NodeId("node_test".to_string()),
                name: "Test Node".to_string(),
                solar_intensity: 1.0,
            }],
            edges: vec![],
        },
        asteroid_templates: vec![AsteroidTemplateDef {
            id: "tmpl_iron_rich".to_string(),
            anomaly_tags: vec![AnomalyTag::new("IronRich")],
            composition_ranges: HashMap::from([
                ("Fe".to_string(), (0.7, 0.7)),
                ("Si".to_string(), (0.3, 0.3)),
            ]),
            preferred_class: Some(ResourceClass::MetalRich),
        }],
        elements: vec![ElementDef {
            id: "ore".to_string(),
            density_kg_per_m3: 3000.0,
            display_name: "Raw Ore".to_string(),
            refined_name: None,
            category: "raw_ore".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: None,
            boiling_point_mk: None,
            boiloff_curve: None,
        }],
        module_defs: AHashMap::default(),
        component_defs: vec![],
        recipes: std::collections::BTreeMap::new(),
        pricing: PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: AHashMap::default(),
        },
        constants: Constants {
            survey_scan_minutes: 1,
            deep_scan_minutes: 1,
            survey_tag_detection_probability: 1.0,
            asteroid_count_per_template: 1,
            asteroid_mass_min_kg: 500.0,
            asteroid_mass_max_kg: 500.0,
            ship_cargo_capacity_m3: 20.0,
            station_cargo_capacity_m3: 10_000.0,
            station_power_available_per_minute: 100.0,
            mining_rate_kg_per_minute: 50.0,
            deposit_minutes: 1,
            autopilot_volatile_threshold_kg: 500.0,
            autopilot_refinery_threshold_kg: 500.0,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            autopilot_slag_jettison_pct: 0.75,
            autopilot_export_batch_size_kg: 500.0,
            autopilot_export_min_revenue: 1_000.0,
            autopilot_lh2_threshold_kg: 5_000.0,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
            minutes_per_tick: 1,
            // Spatial system
            docking_range_au_um: 10_000,
            ticks_per_au: 2_133,
            min_transit_ticks: 1,
            replenish_check_interval_ticks: 1,
            replenish_target_count: 5,
            // Thermal system
            thermal_sink_temp_mk: 293_000,
            thermal_overheat_warning_offset_mk: 200_000,
            thermal_overheat_critical_offset_mk: 500_000,
            thermal_overheat_damage_offset_mk: 800_000,
            thermal_wear_multiplier_warning: 2.0,
            thermal_wear_multiplier_critical: 4.0,
            // Extracted constants (previously hardcoded)
            t_max_absolute_mk: 10_000_000,
            min_meaningful_kg: 1e-3,
            replenish_batch_size: 5,
            trade_unlock_delay_minutes: 525_600,
            autopilot_budget_cap_fraction: 0.05,
            autopilot_lh2_abundant_multiplier: 2.0,
            boiloff_hot_offset_mk: 100_000,
            // Derived fields — filled by derive_tick_values()
            survey_scan_ticks: 0,
            deep_scan_ticks: 0,
            mining_rate_kg_per_tick: 0.0,
            deposit_ticks: 0,
            station_power_available_per_tick: 0.0,
            events_enabled: false,
            event_global_cooldown_ticks: 200,
            event_history_capacity: 100,
            // Bottleneck detection
            bottleneck_storage_threshold_pct: 0.95,
            bottleneck_slag_ratio_threshold: 0.5,
            bottleneck_wear_threshold: 0.8,
        },
        alert_rules: Vec::new(),
        events: Vec::new(),
        hulls: std::collections::BTreeMap::new(),
        fitting_templates: std::collections::BTreeMap::new(),
        initial_station: crate::InitialStationDef::default(),
        autopilot: crate::AutopilotConfig::default(),
        density_map: AHashMap::default(),
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
        asteroids: std::collections::BTreeMap::new(),
        ships: std::collections::BTreeMap::new(),
        stations: [(
            StationId("station_test".to_string()),
            StationState {
                id: StationId("station_test".to_string()),
                position: crate::test_fixtures::test_position(),
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                modifiers: crate::modifiers::ModifierSet::default(),
                power: PowerState::default(),
                cached_inventory_volume_m3: None,
                module_type_index: crate::ModuleTypeIndex::default(),
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
            next_module_instance_id: 0,
        },
        modifiers: crate::modifiers::ModifierSet::default(),
        events: crate::sim_events::SimEventState::default(),
        body_cache: AHashMap::default(),
    }
}

#[test]
fn replenish_spawns_sites_when_below_threshold() {
    let content = replenish_test_content();
    let mut state = empty_sites_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = tick(&mut state, &[], &content, &mut rng, None);

    assert_eq!(state.scan_sites.len(), 5); // replenish_batch_size
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
            position: crate::test_fixtures::test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        });
    }
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = tick(&mut state, &[], &content, &mut rng, None);

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
    tick(&mut state, &[], &content, &mut rng, None);

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
            position: crate::test_fixtures::test_position(),
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
        thermal: None,
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
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

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
            position: crate::test_fixtures::test_position(),
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
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

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
    tick(&mut state1, &[], &content, &mut rng1, None);

    let mut state2 = empty_sites_state(&content);
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    tick(&mut state2, &[], &content, &mut rng2, None);

    let ids1: Vec<_> = state1.scan_sites.iter().map(|s| s.id.0.clone()).collect();
    let ids2: Vec<_> = state2.scan_sites.iter().map(|s| s.id.0.clone()).collect();
    assert_eq!(ids1, ids2);
}

#[test]
fn replenish_interval_gating_skips_off_ticks() {
    let mut content = replenish_test_content();
    content.constants.replenish_check_interval_ticks = 10;
    content.constants.derive_tick_values();

    let mut state = empty_sites_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Tick 0 is a multiple of 10 — should spawn
    tick(&mut state, &[], &content, &mut rng, None);
    assert_eq!(state.scan_sites.len(), 5);

    // Consume all sites to trigger replenish again
    state.scan_sites.clear();

    // Tick 1 is NOT a multiple of 10 — should NOT spawn
    tick(&mut state, &[], &content, &mut rng, None);
    assert_eq!(state.scan_sites.len(), 0, "tick 1 should skip replenish");

    // Advance to tick 10: replenish checks happen BEFORE tick increment,
    // so we need tick() called when state.meta.tick == 10.
    // After 2 calls, tick=2. Need 9 more calls to reach tick 10 check + increment to 11.
    for _ in 0..9 {
        tick(&mut state, &[], &content, &mut rng, None);
    }
    assert_eq!(state.meta.tick, 11);
    assert_eq!(
        state.scan_sites.len(),
        5,
        "replenish should fire at tick 10"
    );
}

#[test]
fn replenish_target_count_controls_threshold() {
    let mut content = replenish_test_content();
    content.constants.replenish_target_count = 3;
    content.constants.derive_tick_values();

    let mut state = empty_sites_state(&content);
    // Pre-fill with 3 sites (at target)
    for i in 0..3 {
        state.scan_sites.push(ScanSite {
            id: SiteId(format!("site_existing_{i}")),
            position: crate::test_fixtures::test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        });
    }

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    tick(&mut state, &[], &content, &mut rng, None);
    assert_eq!(state.scan_sites.len(), 3, "should not spawn when at target");
}

#[test]
fn replenish_spawns_deficit_up_to_batch() {
    let mut content = replenish_test_content();
    content.constants.replenish_target_count = 8;
    content.constants.derive_tick_values();

    let mut state = empty_sites_state(&content);
    // Pre-fill with 5 sites (deficit = 3, less than batch size 5)
    for i in 0..5 {
        state.scan_sites.push(ScanSite {
            id: SiteId(format!("site_existing_{i}")),
            position: crate::test_fixtures::test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        });
    }

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = tick(&mut state, &[], &content, &mut rng, None);
    let spawned: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.event, Event::ScanSiteSpawned { .. }))
        .collect();
    assert_eq!(
        spawned.len(),
        3,
        "should spawn only the deficit (3), not batch (5)"
    );
    assert_eq!(state.scan_sites.len(), 8);
}

#[test]
fn replenish_uses_zone_weighted_positions() {
    let content = replenish_test_content();
    let mut state = empty_sites_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    tick(&mut state, &[], &content, &mut rng, None);

    // All sites should be in the test_body zone (radius 1000-2000)
    for site in &state.scan_sites {
        assert_eq!(
            site.position.parent_body,
            BodyId("test_body".to_string()),
            "site should be placed in zone body"
        );
        assert!(
            site.position.radius_au_um.0 >= 1000 && site.position.radius_au_um.0 <= 2000,
            "radius {} should be within zone bounds",
            site.position.radius_au_um.0
        );
    }
}
