use super::*;

#[test]
fn test_assign_survey_sets_task() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    let ship = &state.ships[&ShipId("ship_0001".to_string())];
    assert!(
        matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Survey { .. })),
        "ship should have a Survey task after command"
    );
}

#[test]
fn test_assign_command_emits_task_started() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::TaskStarted { .. })),
        "TaskStarted event should be emitted"
    );
}

#[test]
fn test_wrong_owner_command_is_dropped() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    let bad_command = CommandEnvelope {
        id: CommandId("cmd_000001".to_string()),
        issued_by: PrincipalId("principal_intruder".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Survey {
                site: SiteId("site_0001".to_string()),
            },
        },
    };

    tick(
        &mut state,
        &[bad_command],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    let ship = &state.ships[&ship_id];
    assert!(
        ship.task.is_none(),
        "command from wrong owner should be silently dropped"
    );
}

#[test]
fn test_future_command_not_applied_early() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    let future_command = CommandEnvelope {
        id: CommandId("cmd_000001".to_string()),
        issued_by: state.ships[&ship_id].owner.clone(),
        issued_tick: 0,
        execute_at_tick: 5,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Survey {
                site: SiteId("site_0001".to_string()),
            },
        },
    };

    tick(
        &mut state,
        &[future_command],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    let ship = &state.ships[&ship_id];
    assert!(
        ship.task.is_none(),
        "command scheduled for a future tick should not apply yet"
    );
}
