use super::*;
use rand::SeedableRng;

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
    assert!(
        (initial_balance - state.balance - 2_000_000.0).abs() < 1.0,
        "should deduct rocket cost"
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
