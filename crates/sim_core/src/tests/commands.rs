use super::*;

#[test]
fn test_assign_survey_sets_task() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, None);

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
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

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
        id: CommandId(0),
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

    tick(&mut state, &[bad_command], &content, &mut rng, None);

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
        id: CommandId(0),
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

    tick(&mut state, &[future_command], &content, &mut rng, None);

    let ship = &state.ships[&ship_id];
    assert!(
        ship.task.is_none(),
        "command scheduled for a future tick should not apply yet"
    );
}

#[test]
fn test_install_module_initializes_thermal_state_for_thermal_modules() {
    use crate::test_fixtures::thermal_content;

    let content = thermal_content();
    let mut state = base_state(&content);
    let mut rng = make_rng();

    let station_id = StationId("station_earth_orbit".to_string());
    let module_item_id = ModuleItemId("smelter_item_001".to_string());

    // Add smelter module item to station inventory
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Module {
        item_id: module_item_id.clone(),
        module_def_id: "module_basic_smelter".to_string(),
    });
    station.invalidate_volume_cache();

    let install_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::InstallModule {
            station_id: station_id.clone(),
            module_item_id,
        },
    };

    tick(&mut state, &[install_cmd], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    let smelter = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_basic_smelter")
        .expect("smelter should be installed");

    assert!(
        smelter.thermal.is_some(),
        "installed thermal module must have ThermalState initialized"
    );
    let thermal = smelter.thermal.as_ref().unwrap();
    // Modules now initialize at ambient temp (293K). Idle heat generation
    // warms them up over time; no more operating_min_mk workaround.
    assert_eq!(
        thermal.temp_mk, content.constants.thermal_sink_temp_mk,
        "newly installed module should start at ambient temp"
    );
    assert_eq!(
        thermal.overheat_zone,
        OverheatZone::Nominal,
        "initial overheat zone should be Nominal"
    );
    assert!(
        !thermal.overheat_disabled,
        "module should not be overheat-disabled on install"
    );
    assert_eq!(
        thermal.thermal_group,
        Some("default".to_string()),
        "thermal_group should be propagated from ThermalDef"
    );
}

#[test]
fn test_install_module_no_thermal_state_for_non_thermal_modules() {
    let content = refinery_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let station_id = StationId("station_earth_orbit".to_string());
    let module_item_id = ModuleItemId("refinery_item_001".to_string());

    // Add a non-thermal module (refinery) to station inventory
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Module {
        item_id: module_item_id.clone(),
        module_def_id: "module_basic_iron_refinery".to_string(),
    });
    station.invalidate_volume_cache();

    let install_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::InstallModule {
            station_id: station_id.clone(),
            module_item_id,
        },
    };

    tick(&mut state, &[install_cmd], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    let refinery = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_basic_iron_refinery")
        .expect("refinery should be installed");

    assert!(
        refinery.thermal.is_none(),
        "non-thermal module should have thermal: None"
    );
}

#[test]
fn test_select_recipe_updates_processor_state() {
    let content = refinery_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();
    let station_id = StationId("station_earth_orbit".to_string());
    let module_id = ModuleInstanceId("module_inst_test".to_string());

    // Add a processor module directly to the station.
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: module_id.clone(),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        thermal: None,
        power_stalled: false,
        manufacturing_priority: 0,
    });

    // SelectRecipe with a valid recipe ID.
    let select_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::SelectRecipe {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            recipe_id: RecipeId("recipe_basic_iron".to_string()),
        },
    };
    tick(&mut state, &[select_cmd], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    let module = station.modules.iter().find(|m| m.id == module_id).unwrap();
    if let ModuleKindState::Processor(ps) = &module.kind_state {
        assert_eq!(
            ps.selected_recipe,
            Some(RecipeId("recipe_basic_iron".to_string()))
        );
    } else {
        panic!("expected Processor state");
    }
}

#[test]
fn test_select_recipe_out_of_bounds_rejected() {
    let content = refinery_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();
    let station_id = StationId("station_earth_orbit".to_string());
    let module_id = ModuleInstanceId("module_inst_test".to_string());

    // Add a processor module directly.
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: module_id.clone(),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        thermal: None,
        power_stalled: false,
        manufacturing_priority: 0,
    });

    // SelectRecipe with a recipe ID not in this module's list (should be rejected).
    let select_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::SelectRecipe {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            recipe_id: RecipeId("recipe_nonexistent".to_string()),
        },
    };
    tick(&mut state, &[select_cmd], &content, &mut rng, None);

    // selected_recipe should still be None (unchanged — command was rejected).
    let station = state.stations.get(&station_id).unwrap();
    let module = station.modules.iter().find(|m| m.id == module_id).unwrap();
    if let ModuleKindState::Processor(ps) = &module.kind_state {
        assert_eq!(
            ps.selected_recipe, None,
            "invalid recipe_id should be rejected"
        );
    } else {
        panic!("expected Processor state");
    }
}
