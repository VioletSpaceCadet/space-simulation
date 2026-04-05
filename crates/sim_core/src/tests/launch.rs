use super::*;
use rand::SeedableRng;

fn satellite_content() -> GameContent {
    let mut content = launch_content();

    // Add a satellite def.
    content.satellite_defs.insert(
        "sat_survey".to_string(),
        SatelliteDef {
            id: "sat_survey".to_string(),
            name: "Survey Satellite".to_string(),
            satellite_type: "survey".to_string(),
            mass_kg: 500.0,
            wear_rate: 0.00015,
            required_tech: None,
            behavior_config: serde_json::json!({ "discovery_multiplier": 2.0 }),
        },
    );
    content.satellite_defs.insert(
        "sat_comm_relay".to_string(),
        SatelliteDef {
            id: "sat_comm_relay".to_string(),
            name: "Comm Relay".to_string(),
            satellite_type: "communication".to_string(),
            mass_kg: 800.0,
            wear_rate: 0.00008,
            required_tech: Some(TechId("tech_gated".to_string())),
            behavior_config: serde_json::json!({ "comm_tier": "Basic" }),
        },
    );

    content
}

fn launch_content() -> GameContent {
    let mut content = base_content();

    // Add a launch pad module def.
    content.module_defs.insert(
        "module_launch_pad_small".to_string(),
        ModuleDefBuilder::new("module_launch_pad_small")
            .name("Small Launch Pad")
            .behavior(ModuleBehaviorDef::LaunchPad(LaunchPadDef {
                max_payload_kg: 20000.0,
                recovery_minutes: 5,
                recovery_ticks: 5, // test uses mpt=1
            }))
            .build(),
    );

    // Add rocket defs.
    content.rocket_defs.insert(
        "rocket_sounding".to_string(),
        RocketDef {
            id: "rocket_sounding".to_string(),
            name: "Sounding Rocket".to_string(),
            payload_capacity_kg: 200.0,
            base_launch_cost: 2_000_000.0,
            fuel_kg: 500.0,
            transit_minutes: 3,
            required_tech: None,
        },
    );
    content.rocket_defs.insert(
        "rocket_medium".to_string(),
        RocketDef {
            id: "rocket_medium".to_string(),
            name: "Medium Launcher".to_string(),
            payload_capacity_kg: 16000.0,
            base_launch_cost: 67_000_000.0,
            fuel_kg: 400_000.0,
            transit_minutes: 3,
            required_tech: None,
        },
    );

    content
}

fn launch_state(content: &GameContent) -> GameState {
    let mut state = base_state(content);
    state.balance = 100_000_000.0;

    let facility_id = GroundFacilityId("ground_earth".to_string());
    state.ground_facilities.insert(
        facility_id.clone(),
        GroundFacilityState {
            id: facility_id,
            name: "Earth Ops".to_string(),
            position: test_position(),
            core: FacilityCore {
                modules: vec![ModuleState {
                    id: ModuleInstanceId("pad_001".to_string()),
                    def_id: "module_launch_pad_small".to_string(),
                    enabled: true,
                    kind_state: ModuleKindState::LaunchPad(LaunchPadState::default()),
                    wear: WearState::default(),
                    power_stalled: false,
                    module_priority: 0,
                    assigned_crew: Default::default(),
                    efficiency: 1.0,
                    prev_crew_satisfied: true,
                    thermal: None,
                    slot_index: None,
                }],
                inventory: vec![InventoryItem::Material {
                    element: "LH2".to_string(),
                    kg: 500_000.0, // enough for any test rocket
                    quality: 1.0,
                    thermal: None,
                }],
                cargo_capacity_m3: 10000.0,
                ..Default::default()
            },
            launch_transits: vec![],
        },
    );

    crate::test_fixtures::rebuild_indices(&mut state, content);
    state
}

#[test]
fn launch_deducts_cost_and_creates_transit() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let initial_balance = state.balance;

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Supplies(vec![InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 100.0,
                quality: 1.0,
                thermal: None,
            }]),
            destination: test_position(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    // Cost deducted.
    assert!(
        state.balance < initial_balance,
        "balance should decrease after launch"
    );
    // base ($2M) + fuel (500kg * $0.50/kg = $250) = $2,000,250
    let expected_cost = 2_000_000.0 + 500.0 * 0.50;
    assert!(
        (initial_balance - state.balance - expected_cost).abs() < 1.0,
        "should deduct base + fuel cost (expected {expected_cost})"
    );

    // Transit created.
    let facility = state
        .ground_facilities
        .get(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    assert_eq!(facility.launch_transits.len(), 1, "should have 1 transit");
    assert_eq!(facility.launch_transits[0].rocket_def_id, "rocket_sounding");

    // Pad marked as recovering.
    let pad_state = match &facility.core.modules[0].kind_state {
        ModuleKindState::LaunchPad(s) => s,
        _ => panic!("expected LaunchPad state"),
    };
    assert!(
        !pad_state.available,
        "pad should be unavailable after launch"
    );
    assert_eq!(pad_state.launches_count, 1);

    // PayloadLaunched event emitted.
    let launched = events
        .iter()
        .any(|e| matches!(&e.event, Event::PayloadLaunched { .. }));
    assert!(launched, "should emit PayloadLaunched event");
}

#[test]
fn launch_without_pad_fails() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Remove the launch pad.
    let facility = state
        .ground_facilities
        .get_mut(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    facility.core.modules.clear();

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Supplies(vec![]),
            destination: test_position(),
        },
    };

    let initial_balance = state.balance;
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    assert!(
        (state.balance - initial_balance).abs() < 1.0,
        "balance should not change without pad"
    );
    let launched = events
        .iter()
        .any(|e| matches!(&e.event, Event::PayloadLaunched { .. }));
    assert!(!launched, "should not launch without pad");
}

#[test]
fn overweight_payload_rejected() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Payload exceeds rocket capacity (200 kg).
    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Supplies(vec![InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 500.0, // 500 kg > 200 kg capacity
                quality: 1.0,
                thermal: None,
            }]),
            destination: test_position(),
        },
    };

    let initial_balance = state.balance;
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    assert!(
        (state.balance - initial_balance).abs() < 1.0,
        "balance should not change for overweight payload"
    );
    let launched = events
        .iter()
        .any(|e| matches!(&e.event, Event::PayloadLaunched { .. }));
    assert!(!launched, "should not launch overweight payload");
}

#[test]
fn launch_fails_without_fuel() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Remove all fuel from facility.
    let facility = state
        .ground_facilities
        .get_mut(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    facility.core.inventory.clear();
    facility.core.invalidate_volume_cache();

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Supplies(vec![]),
            destination: test_position(),
        },
    };

    let initial_balance = state.balance;
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    assert!(
        (state.balance - initial_balance).abs() < 1.0,
        "balance should not change without fuel"
    );
    let launched = events
        .iter()
        .any(|e| matches!(&e.event, Event::PayloadLaunched { .. }));
    assert!(!launched, "should not launch without fuel");
}

#[test]
fn launch_consumes_fuel_from_inventory() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    let fuel_before: f32 = state
        .ground_facilities
        .get(&GroundFacilityId("ground_earth".to_string()))
        .unwrap()
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Supplies(vec![]),
            destination: test_position(),
        },
    };

    tick(&mut state, &[cmd], &content, &mut rng, None);

    let fuel_after: f32 = state
        .ground_facilities
        .get(&GroundFacilityId("ground_earth".to_string()))
        .unwrap()
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();

    // Sounding rocket consumes 500 kg fuel.
    assert!(
        (fuel_before - fuel_after - 500.0).abs() < 1.0,
        "should consume 500 kg LH2 (before={fuel_before}, after={fuel_after})"
    );
}

#[test]
fn transit_delivers_supplies_to_station() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Launch supplies.
    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Supplies(vec![InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 100.0,
                quality: 1.0,
                thermal: None,
            }]),
            destination: test_position(),
        },
    };

    tick(&mut state, &[cmd], &content, &mut rng, None);

    // Run ticks until transit completes (transit_minutes=3, mpt=1 → 3 ticks).
    let mut delivered = false;
    for _ in 0..10 {
        let events = tick(&mut state, &[], &content, &mut rng, None);
        if events
            .iter()
            .any(|e| matches!(&e.event, Event::PayloadDelivered { .. }))
        {
            delivered = true;
            break;
        }
    }

    assert!(delivered, "payload should be delivered within 10 ticks");

    // Station should have received the Fe.
    let station = state.stations.values().next().unwrap();
    let fe_kg: f32 = station
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(fe_kg >= 100.0, "station should have received 100kg Fe");
}

#[test]
fn station_kit_creates_new_station() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let initial_station_count = state.stations.len();

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_medium".to_string(),
            payload: LaunchPayload::StationKit,
            destination: test_position(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    let launched = events
        .iter()
        .any(|e| matches!(&e.event, Event::PayloadLaunched { .. }));
    assert!(launched, "should launch station kit");

    // Run ticks until transit completes (transit_minutes=3, mpt=1 → 3 ticks).
    let mut deployed = false;
    for _ in 0..10 {
        let events = tick(&mut state, &[], &content, &mut rng, None);
        if events
            .iter()
            .any(|e| matches!(&e.event, Event::StationDeployed { .. }))
        {
            deployed = true;
            break;
        }
    }

    assert!(deployed, "station should be deployed within 10 ticks");
    assert_eq!(
        state.stations.len(),
        initial_station_count + 1,
        "should have one more station"
    );
}

#[test]
fn pad_recovers_after_countdown() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Launch to make pad unavailable.
    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Supplies(vec![]),
            destination: test_position(),
        },
    };

    tick(&mut state, &[cmd], &content, &mut rng, None);

    // Pad should be unavailable. Recovery already ticked once during tick 0.
    let facility = state
        .ground_facilities
        .get(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    let pad = match &facility.core.modules[0].kind_state {
        ModuleKindState::LaunchPad(s) => s.clone(),
        _ => panic!("expected LaunchPad"),
    };
    assert!(!pad.available);
    assert_eq!(pad.recovery_ticks_remaining, 4); // 5 - 1 (ticked on launch tick)

    // Tick until recovery completes.
    for _ in 0..4 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    // Pad should be available again.
    let facility = state
        .ground_facilities
        .get(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    let pad = match &facility.core.modules[0].kind_state {
        ModuleKindState::LaunchPad(s) => s,
        _ => panic!("expected LaunchPad"),
    };
    assert!(pad.available, "pad should be available after recovery");
    assert_eq!(pad.recovery_ticks_remaining, 0);
}

// ---------------------------------------------------------------------------
// Satellite deployment tests
// ---------------------------------------------------------------------------

#[test]
fn ground_launch_satellite_creates_satellite_on_arrival() {
    let content = satellite_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Add satellite component to facility inventory.
    let facility = state
        .ground_facilities
        .get_mut(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    facility.core.inventory.push(InventoryItem::Component {
        component_id: ComponentId("sat_survey".to_string()),
        count: 1,
        quality: 1.0,
    });
    crate::test_fixtures::rebuild_indices(&mut state, &content);

    let initial_sat_count = state.satellites.len();

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_medium".to_string(), // 16t capacity fits 500kg satellite
            payload: LaunchPayload::Satellite {
                satellite_def_id: "sat_survey".to_string(),
            },
            destination: test_position(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::PayloadLaunched { .. })),
        "should emit PayloadLaunched"
    );

    // Component should be consumed from inventory.
    let facility = state
        .ground_facilities
        .get(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    let sat_count: u32 = facility
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "sat_survey" => Some(*count),
            _ => None,
        })
        .sum();
    assert_eq!(sat_count, 0, "satellite component should be consumed");

    // Tick until transit completes.
    let mut deployed = false;
    for _ in 0..10 {
        let events = tick(&mut state, &[], &content, &mut rng, None);
        if events
            .iter()
            .any(|e| matches!(&e.event, Event::SatelliteDeployed { .. }))
        {
            deployed = true;
            break;
        }
    }

    assert!(deployed, "satellite should be deployed on arrival");
    assert_eq!(
        state.satellites.len(),
        initial_sat_count + 1,
        "should have one new satellite"
    );
    let sat = state.satellites.values().next().unwrap();
    assert_eq!(sat.satellite_type, "survey");
    assert_eq!(sat.def_id, "sat_survey");
    assert!(sat.enabled);
    // Satellite may have ticked a few times after arrival, accumulating small wear.
    assert!(
        sat.wear < 0.01,
        "wear should be near-zero, got {}",
        sat.wear
    );
}

#[test]
fn ground_launch_satellite_rejected_without_component() {
    let content = satellite_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let initial_balance = state.balance;

    // No satellite component in inventory.
    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Satellite {
                satellite_def_id: "sat_survey".to_string(),
            },
            destination: test_position(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::PayloadLaunched { .. })),
        "should not launch without satellite component"
    );
    assert!(
        (state.balance - initial_balance).abs() < f64::EPSILON,
        "balance should not change"
    );
}

#[test]
fn ground_launch_satellite_rejected_tech_gated() {
    let content = satellite_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Add gated satellite component.
    let facility = state
        .ground_facilities
        .get_mut(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    facility.core.inventory.push(InventoryItem::Component {
        component_id: ComponentId("sat_comm_relay".to_string()),
        count: 1,
        quality: 1.0,
    });
    crate::test_fixtures::rebuild_indices(&mut state, &content);

    // tech_gated not unlocked.
    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_sounding".to_string(),
            payload: LaunchPayload::Satellite {
                satellite_def_id: "sat_comm_relay".to_string(),
            },
            destination: test_position(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::PayloadLaunched { .. })),
        "should not launch without required tech"
    );
}

#[test]
fn orbital_deploy_satellite_from_station() {
    let content = satellite_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Create a station with a satellite component.
    let station_id = StationId("station_test".to_string());
    state.stations.insert(
        station_id.clone(),
        StationState {
            id: station_id.clone(),
            position: test_position(),
            core: FacilityCore {
                inventory: vec![InventoryItem::Component {
                    component_id: ComponentId("sat_survey".to_string()),
                    count: 2,
                    quality: 1.0,
                }],
                cargo_capacity_m3: 1000.0,
                ..Default::default()
            },
            leaders: Vec::new(),
            frame_id: None,
        },
    );
    crate::test_fixtures::rebuild_indices(&mut state, &content);

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::DeploySatellite {
            station_id: station_id.clone(),
            satellite_def_id: "sat_survey".to_string(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::SatelliteDeployed { .. })),
        "should emit SatelliteDeployed"
    );
    assert_eq!(state.satellites.len(), 1, "should have one satellite");

    // Component count should be decremented.
    let station = state.stations.get(&station_id).unwrap();
    let remaining: u32 = station
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "sat_survey" => Some(*count),
            _ => None,
        })
        .sum();
    assert_eq!(remaining, 1, "should have consumed one satellite component");

    let sat = state.satellites.values().next().unwrap();
    assert_eq!(sat.satellite_type, "survey");
    assert!(sat.enabled);
}

#[test]
fn orbital_deploy_rejected_no_component() {
    let content = satellite_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Station without satellite components.
    let station_id = StationId("station_test".to_string());
    state.stations.insert(
        station_id.clone(),
        StationState {
            id: station_id.clone(),
            position: test_position(),
            core: FacilityCore {
                cargo_capacity_m3: 1000.0,
                ..Default::default()
            },
            leaders: Vec::new(),
            frame_id: None,
        },
    );
    crate::test_fixtures::rebuild_indices(&mut state, &content);

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::DeploySatellite {
            station_id,
            satellite_def_id: "sat_survey".to_string(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::SatelliteDeployed { .. })),
        "should not deploy without component"
    );
    assert!(state.satellites.is_empty());
}

#[test]
fn orbital_deploy_rejected_unknown_def() {
    let content = satellite_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    let station_id = StationId("station_test".to_string());
    state.stations.insert(
        station_id.clone(),
        StationState {
            id: station_id.clone(),
            position: test_position(),
            core: FacilityCore {
                inventory: vec![InventoryItem::Component {
                    component_id: ComponentId("sat_nonexistent".to_string()),
                    count: 1,
                    quality: 1.0,
                }],
                cargo_capacity_m3: 1000.0,
                ..Default::default()
            },
            leaders: Vec::new(),
            frame_id: None,
        },
    );
    crate::test_fixtures::rebuild_indices(&mut state, &content);

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::DeploySatellite {
            station_id,
            satellite_def_id: "sat_nonexistent".to_string(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::SatelliteDeployed { .. })),
        "should not deploy unknown satellite type"
    );
    assert!(state.satellites.is_empty());
}

#[test]
fn orbital_deploy_rejected_tech_gated() {
    let content = satellite_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Station with tech-gated satellite component (sat_comm_relay requires tech_gated).
    let station_id = StationId("station_test".to_string());
    state.stations.insert(
        station_id.clone(),
        StationState {
            id: station_id.clone(),
            position: test_position(),
            core: FacilityCore {
                inventory: vec![InventoryItem::Component {
                    component_id: ComponentId("sat_comm_relay".to_string()),
                    count: 1,
                    quality: 1.0,
                }],
                cargo_capacity_m3: 1000.0,
                ..Default::default()
            },
            leaders: Vec::new(),
            frame_id: None,
        },
    );
    crate::test_fixtures::rebuild_indices(&mut state, &content);

    // tech_gated not unlocked — deploy should be rejected.
    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::DeploySatellite {
            station_id,
            satellite_def_id: "sat_comm_relay".to_string(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::SatelliteDeployed { .. })),
        "should not deploy without required tech"
    );
    assert!(state.satellites.is_empty());
}

/// E2E: launch a station kit from ground, verify new orbital station.
#[test]
fn ground_to_orbit_station_kit_e2e() {
    let content = launch_content();
    let mut state = launch_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Remove existing stations so StationKit creates the first one.
    state.stations.clear();
    assert!(state.stations.is_empty());

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("player".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::Launch {
            facility_id: GroundFacilityId("ground_earth".to_string()),
            rocket_def_id: "rocket_medium".to_string(),
            payload: LaunchPayload::StationKit,
            destination: test_position(),
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::PayloadLaunched { .. })),
        "station kit should launch"
    );

    // Run ticks until transit completes.
    let mut deployed = false;
    for _ in 0..10 {
        let events = tick(&mut state, &[], &content, &mut rng, None);
        if events
            .iter()
            .any(|e| matches!(&e.event, Event::StationDeployed { .. }))
        {
            deployed = true;
            break;
        }
    }

    assert!(deployed, "station should deploy after transit");
    assert_eq!(state.stations.len(), 1, "should have 1 new station");
    let station = state.stations.values().next().unwrap();
    assert!(station.core.cargo_capacity_m3 > 0.0);
}
