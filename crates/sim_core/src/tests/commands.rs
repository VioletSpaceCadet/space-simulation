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
    station.core.inventory.push(InventoryItem::Module {
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
            facility_id: station_id.clone().into(),
            module_item_id,
            slot_index: None,
        },
    };

    tick(&mut state, &[install_cmd], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    let smelter = station
        .core
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
    station.core.inventory.push(InventoryItem::Module {
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
            facility_id: station_id.clone().into(),
            module_item_id,
            slot_index: None,
        },
    };

    tick(&mut state, &[install_cmd], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    let refinery = station
        .core
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
    station.core.modules.push(ModuleState {
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
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        slot_index: None,
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
    let module = station
        .core
        .modules
        .iter()
        .find(|m| m.id == module_id)
        .unwrap();
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
    station.core.modules.push(ModuleState {
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
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        slot_index: None,
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
    let module = station
        .core
        .modules
        .iter()
        .find(|m| m.id == module_id)
        .unwrap();
    if let ModuleKindState::Processor(ps) = &module.kind_state {
        assert_eq!(
            ps.selected_recipe, None,
            "invalid recipe_id should be rejected"
        );
    } else {
        panic!("expected Processor state");
    }
}

// --------------------------------------------------------------------
// SF-05: InstallModule slot validation (framed station)
// --------------------------------------------------------------------

/// Build a framed test environment: station carries a 2-slot test frame
/// (1 industrial + 1 research) and has two minimal module defs registered:
///   - `module_sf05_industrial` fits industrial slots
///   - `module_sf05_research` fits research slots
/// Both are seeded into station inventory as module items ready to install.
fn framed_install_setup() -> (GameContent, GameState) {
    let mut content = test_content();

    // Register two minimal module defs with the right compatible_slots.
    let industrial_def = ModuleDefBuilder::new("module_sf05_industrial")
        .behavior(crate::ModuleBehaviorDef::Equipment)
        .compatible_slots(vec![crate::SlotType("industrial".to_string())])
        .build();
    content
        .module_defs
        .insert("module_sf05_industrial".to_string(), industrial_def);
    let research_def = ModuleDefBuilder::new("module_sf05_research")
        .behavior(crate::ModuleBehaviorDef::Equipment)
        .compatible_slots(vec![crate::SlotType("research".to_string())])
        .build();
    content
        .module_defs
        .insert("module_sf05_research".to_string(), research_def);

    // Two slots: idx 0 industrial, idx 1 research.
    let frame_id = crate::FrameId("frame_test_install".to_string());
    content.frames.insert(
        frame_id.clone(),
        crate::FrameDef {
            id: frame_id.clone(),
            name: "Test Install".to_string(),
            base_cargo_capacity_m3: 500.0,
            base_power_capacity_kw: 30.0,
            slots: vec![
                crate::SlotDef {
                    slot_type: crate::SlotType("industrial".to_string()),
                    label: "I1".to_string(),
                },
                crate::SlotDef {
                    slot_type: crate::SlotType("research".to_string()),
                    label: "R1".to_string(),
                },
            ],
            bonuses: vec![],
            required_tech: None,
            tags: vec![],
        },
    );

    let mut state = test_state(&content);
    let station = state.stations.get_mut(&test_station_id()).unwrap();
    station.frame_id = Some(frame_id);
    station.core.inventory.push(InventoryItem::Module {
        item_id: crate::ModuleItemId("inv_industrial".to_string()),
        module_def_id: "module_sf05_industrial".to_string(),
    });
    station.core.inventory.push(InventoryItem::Module {
        item_id: crate::ModuleItemId("inv_research".to_string()),
        module_def_id: "module_sf05_research".to_string(),
    });

    (content, state)
}

fn install_command(
    state: &GameState,
    station_id: &StationId,
    item_id: &str,
    slot_index: Option<usize>,
) -> CommandEnvelope {
    CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::InstallModule {
            facility_id: station_id.clone().into(),
            module_item_id: crate::ModuleItemId(item_id.to_string()),
            slot_index,
        },
    }
}

#[test]
fn install_module_auto_finds_compatible_slot_on_framed_station() {
    let (content, mut state) = framed_install_setup();
    let station_id = test_station_id();
    let mut rng = make_rng();

    let cmd = install_command(&state, &station_id, "inv_industrial", None);
    tick(&mut state, &[cmd], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    let refinery = station
        .core
        .modules
        .iter()
        .find(|m| m.def_id == "module_sf05_industrial")
        .expect("refinery should be installed");
    assert_eq!(
        refinery.slot_index,
        Some(0),
        "refinery should take the first compatible slot (industrial, idx 0)"
    );
}

#[test]
fn install_module_honors_explicit_slot_index() {
    let (content, mut state) = framed_install_setup();
    let station_id = test_station_id();
    let mut rng = make_rng();

    // Target slot 1 directly (research slot, propulsion_lab is compatible).
    let cmd = install_command(&state, &station_id, "inv_research", Some(1));
    tick(&mut state, &[cmd], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    let lab = station
        .core
        .modules
        .iter()
        .find(|m| m.def_id == "module_sf05_research")
        .expect("lab should be installed");
    assert_eq!(lab.slot_index, Some(1));
}

#[test]
fn install_module_wrong_slot_type_rejected_and_event_emitted() {
    let (content, mut state) = framed_install_setup();
    let station_id = test_station_id();
    let mut rng = make_rng();

    // refinery (industrial) targeted at slot 1 (research) → should fail.
    let cmd = install_command(&state, &station_id, "inv_industrial", Some(1));
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    assert!(
        station
            .core
            .modules
            .iter()
            .all(|m| m.def_id != "module_sf05_industrial"),
        "refinery should not be installed"
    );
    // Module should be back in inventory.
    assert!(
        station.core.inventory.iter().any(|i| matches!(
            i,
            InventoryItem::Module { module_def_id, .. } if module_def_id == "module_sf05_industrial"
        )),
        "refinery should be returned to inventory on failed install"
    );
    // ModuleNoCompatibleSlot event should have fired.
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleNoCompatibleSlot { .. })),
        "expected ModuleNoCompatibleSlot event"
    );
}

#[test]
fn install_module_occupied_slot_rejected() {
    let (content, mut state) = framed_install_setup();
    let station_id = test_station_id();
    let mut rng = make_rng();

    // Pre-occupy slot 0 by installing the refinery first.
    let first = install_command(&state, &station_id, "inv_industrial", Some(0));
    tick(&mut state, &[first], &content, &mut rng, None);

    // Add a second refinery item to inventory and try to install it at the
    // same slot — should fail.
    {
        let station = state.stations.get_mut(&station_id).unwrap();
        station.core.inventory.push(InventoryItem::Module {
            item_id: crate::ModuleItemId("inv_industrial_2".to_string()),
            module_def_id: "module_sf05_industrial".to_string(),
        });
    }

    let second = install_command(&state, &station_id, "inv_industrial_2", Some(0));
    let events = tick(&mut state, &[second], &content, &mut rng, None);

    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleNoCompatibleSlot { .. })),
        "occupied slot should emit ModuleNoCompatibleSlot"
    );
}

#[test]
fn install_module_frameless_station_keeps_legacy_behavior() {
    // Reuse framed_install_setup so the test content includes the
    // module_sf05_industrial def, then strip the frame to exercise the
    // legacy path.
    let (content, mut state) = framed_install_setup();
    let station_id = test_station_id();
    {
        let station = state.stations.get_mut(&station_id).unwrap();
        station.frame_id = None;
    }
    let mut rng = make_rng();

    let cmd = install_command(&state, &station_id, "inv_industrial", None);
    tick(&mut state, &[cmd], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    let module = station
        .core
        .modules
        .iter()
        .find(|m| m.def_id == "module_sf05_industrial")
        .expect("module should install on frameless station");
    assert_eq!(
        module.slot_index, None,
        "frameless install should leave slot_index None"
    );
}
