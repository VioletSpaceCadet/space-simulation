use super::*;

#[test]
fn test_identical_seeds_produce_identical_event_logs() {
    let content = test_content();

    let run = |seed: u64| -> Vec<(String, u64)> {
        let mut state = test_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut log = Vec::new();

        let cmd = survey_command(&state);
        for i in 0..20u64 {
            let commands = if i == 0 {
                std::slice::from_ref(&cmd)
            } else {
                &[]
            };
            let events = tick(&mut state, commands, &content, &mut rng, EventLevel::Debug);
            for event in events {
                log.push((event.id.0.clone(), event.tick));
            }
        }
        log
    };

    assert_eq!(
        run(42),
        run(42),
        "identical seeds must produce identical event logs"
    );
}

#[test]
fn test_different_seeds_produce_different_results() {
    let content = test_content();

    let unlock_tick = |seed: u64| -> Option<u64> {
        let mut state = test_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let tech_id = TechId("tech_deep_scan_v1".to_string());
        for _ in 0..500 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
            if state.research.unlocked.contains(&tech_id) {
                return Some(state.meta.tick);
            }
        }
        None
    };

    let tick_42 = unlock_tick(42);
    let tick_1234 = unlock_tick(1234);
    assert_ne!(
        tick_42, tick_1234,
        "different seeds should generally produce different results"
    );
}

#[test]
fn test_full_survey_deepscan_mine_deposit_cycle() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    let station_id = StationId("station_earth_orbit".to_string());
    let owner = state.ships[&ship_id].owner.clone();

    // --- Phase 1: Survey ---
    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Debug);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    assert_eq!(
        state.asteroids.len(),
        1,
        "one asteroid should exist after survey"
    );
    assert!(
        !state.scan_sites.iter().any(|s| s.id.0 == "site_0001"),
        "original scan site should be consumed"
    );
    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    assert!(
        (state.asteroids[&asteroid_id].mass_kg - 500.0).abs() < 1e-3,
        "asteroid mass should be 500 kg"
    );

    assert!(
        matches!(
            &state.ships[&ship_id].task,
            Some(task) if matches!(task.kind, TaskKind::Idle)
        ),
        "ship should be idle after survey"
    );

    // --- Phase 2: Research unlocks deep scan ---
    let tech_id = TechId("tech_deep_scan_v1".to_string());
    let mut unlocked = false;
    for _ in 0..100 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);
        if state.research.unlocked.contains(&tech_id) {
            unlocked = true;
            break;
        }
    }
    assert!(unlocked, "tech_deep_scan_v1 should unlock within 100 ticks");

    // --- Phase 3: Deep Scan ---
    assert!(
        state.asteroids[&asteroid_id]
            .knowledge
            .composition
            .is_none(),
        "composition should be unknown before deep scan"
    );

    let deep_cmd = CommandEnvelope {
        id: CommandId(format!("cmd_{:06}", state.counters.next_command_id)),
        issued_by: owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::DeepScan {
                asteroid: asteroid_id.clone(),
            },
        },
    };
    tick(
        &mut state,
        &[deep_cmd],
        &content,
        &mut rng,
        EventLevel::Debug,
    );
    tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    let composition = state.asteroids[&asteroid_id].knowledge.composition.as_ref();
    assert!(
        composition.is_some(),
        "composition should be known after deep scan"
    );
    let mapped = composition.unwrap();
    for (element, &true_val) in &state.asteroids[&asteroid_id].true_composition {
        let mapped_val = mapped.get(element).copied().unwrap_or(0.0);
        assert!(
            (mapped_val - true_val).abs() < 1e-5,
            "mapped {element} should match true composition (sigma=0)"
        );
    }

    // --- Phase 4: Mine ---
    let duration_ticks = (state.asteroids[&asteroid_id].mass_kg
        / content.constants.mining_rate_kg_per_tick)
        .ceil() as u64;
    assert_eq!(duration_ticks, 10, "mining should take 10 ticks");

    let mine_cmd = CommandEnvelope {
        id: CommandId(format!("cmd_{:06}", state.counters.next_command_id + 1)),
        issued_by: owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Mine {
                asteroid: asteroid_id.clone(),
                duration_ticks,
            },
        },
    };
    tick(
        &mut state,
        &[mine_cmd],
        &content,
        &mut rng,
        EventLevel::Debug,
    );

    for _ in 0..duration_ticks {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);
    }

    let ship_ore_kg: f32 = state.ships[&ship_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        ship_ore_kg > 0.0,
        "ship should have ore after mining, got {ship_ore_kg} kg"
    );

    assert!(
        !state.asteroids.contains_key(&asteroid_id),
        "asteroid should be removed after full depletion"
    );

    // --- Phase 5: Deposit ---
    let deposit_cmd = CommandEnvelope {
        id: CommandId(format!("cmd_{:06}", state.counters.next_command_id + 2)),
        issued_by: owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Deposit {
                station: station_id.clone(),
                blocked: false,
            },
        },
    };
    tick(
        &mut state,
        &[deposit_cmd],
        &content,
        &mut rng,
        EventLevel::Debug,
    );
    tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    assert!(
        state.ships[&ship_id].inventory.is_empty(),
        "ship inventory should be empty after deposit"
    );

    let station_ore_kg: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        station_ore_kg > 0.0,
        "station should have ore after deposit, got {station_ore_kg} kg"
    );
    assert!(
        (station_ore_kg - ship_ore_kg).abs() < 1e-3,
        "station ore ({station_ore_kg} kg) should match what the ship had ({ship_ore_kg} kg)"
    );
}
