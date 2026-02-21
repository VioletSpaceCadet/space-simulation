use super::*;

#[test]
fn test_deep_scan_blocked_without_tech() {
    let mut content = test_content();
    content.techs[0].difficulty = 1_000_000.0;
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();

    let deep_cmd = CommandEnvelope {
        id: CommandId("cmd_000002".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::DeepScan {
                asteroid: asteroid_id,
            },
        },
    };

    tick(
        &mut state,
        &[deep_cmd],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    let ship = &state.ships[&ship_id];
    assert!(
        !matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::DeepScan { .. })),
        "DeepScan command should be dropped when tech is not unlocked"
    );
}

#[test]
fn test_deep_scan_maps_composition() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    state
        .research
        .unlocked
        .insert(TechId("tech_deep_scan_v1".to_string()));

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    assert!(
        state.asteroids[&asteroid_id]
            .knowledge
            .composition
            .is_none(),
        "composition should be unknown before deep scan"
    );

    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();
    let deep_cmd = CommandEnvelope {
        id: CommandId("cmd_000002".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
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
        EventLevel::Normal,
    );
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let composition = state.asteroids[&asteroid_id].knowledge.composition.as_ref();
    assert!(
        composition.is_some(),
        "composition should be mapped after deep scan"
    );
}

#[test]
fn test_deep_scan_composition_matches_truth_with_zero_sigma() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    state
        .research
        .unlocked
        .insert(TechId("tech_deep_scan_v1".to_string()));

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();
    let deep_cmd = CommandEnvelope {
        id: CommandId("cmd_000002".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
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
        EventLevel::Normal,
    );
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid = &state.asteroids[&asteroid_id];
    let mapped = asteroid.knowledge.composition.as_ref().unwrap();
    for (element, &true_val) in &asteroid.true_composition {
        let mapped_val = mapped.get(element).copied().unwrap_or(0.0);
        assert!(
            (mapped_val - true_val).abs() < 1e-5,
            "mapped {element} ({mapped_val}) should equal true value ({true_val}) with sigma=0"
        );
    }
}
