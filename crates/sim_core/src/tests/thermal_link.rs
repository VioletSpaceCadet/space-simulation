//! Thermal link command tests (VIO-217).

use super::*;
use crate::test_fixtures::{
    base_state, make_rng, test_station_id, thermal_content, ModuleDefBuilder,
};

/// Set up content with a smelter (has molten_out port) and a module with an input port.
fn link_content() -> GameContent {
    let mut content = thermal_content();
    // Add molten_out port to smelter
    content
        .module_defs
        .get_mut("module_basic_smelter")
        .expect("smelter must exist in thermal_content")
        .ports = vec![ModulePort {
        id: "molten_out".to_string(),
        direction: PortDirection::Output,
        accepts: PortFilter::AnyMolten,
    }];
    // Add a dummy module with an input port for testing
    content.module_defs.insert(
        "module_test_receiver".to_string(),
        ModuleDefBuilder::new("module_test_receiver")
            .name("Test Receiver")
            .mass(100.0)
            .volume(1.0)
            .ports(vec![ModulePort {
                id: "molten_in".to_string(),
                direction: PortDirection::Input,
                accepts: PortFilter::AnyMolten,
            }])
            .build(),
    );
    content
}

fn link_state(content: &GameContent) -> GameState {
    let mut state = base_state(content);
    let station = state.stations.get_mut(&test_station_id()).unwrap();

    // Add smelter module (has molten_out port from content)
    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_smelter".to_string()),
        def_id: "module_basic_smelter".to_string(),
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
    });

    // Add receiver module (has molten_in port)
    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_receiver".to_string()),
        def_id: "module_test_receiver".to_string(),
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
    });

    station.rebuild_module_index(&content);

    state
}

#[test]
fn create_valid_link() {
    let content = link_content();
    let mut state = link_state(&content);
    let mut rng = make_rng();

    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::CreateThermalLink {
            station_id: test_station_id(),
            from_module_id: ModuleInstanceId("mod_smelter".to_string()),
            from_port_id: "molten_out".to_string(),
            to_module_id: ModuleInstanceId("mod_receiver".to_string()),
            to_port_id: "molten_in".to_string(),
        },
    };
    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    let station = &state.stations[&test_station_id()];
    assert_eq!(station.thermal_links.len(), 1);
    assert_eq!(station.thermal_links[0].from_port_id, "molten_out");
    assert_eq!(station.thermal_links[0].to_port_id, "molten_in");
    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::ThermalLinkCreated { .. })));
}

#[test]
fn create_link_wrong_direction_rejected() {
    let content = link_content();
    let mut state = link_state(&content);
    let mut rng = make_rng();

    // Try to create link in wrong direction (input -> output)
    let cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::CreateThermalLink {
            station_id: test_station_id(),
            from_module_id: ModuleInstanceId("mod_receiver".to_string()),
            from_port_id: "molten_in".to_string(),
            to_module_id: ModuleInstanceId("mod_smelter".to_string()),
            to_port_id: "molten_out".to_string(),
        },
    };
    tick(&mut state, &[cmd], &content, &mut rng, None);

    let station = &state.stations[&test_station_id()];
    assert!(
        station.thermal_links.is_empty(),
        "wrong direction should be rejected"
    );
}

#[test]
fn duplicate_link_rejected() {
    let content = link_content();
    let mut state = link_state(&content);
    let mut rng = make_rng();

    let make_cmd = |id: u64| CommandEnvelope {
        id: CommandId(id),
        issued_by: PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::CreateThermalLink {
            station_id: test_station_id(),
            from_module_id: ModuleInstanceId("mod_smelter".to_string()),
            from_port_id: "molten_out".to_string(),
            to_module_id: ModuleInstanceId("mod_receiver".to_string()),
            to_port_id: "molten_in".to_string(),
        },
    };

    tick(
        &mut state,
        &[make_cmd(0), make_cmd(1)],
        &content,
        &mut rng,
        None,
    );

    let station = &state.stations[&test_station_id()];
    assert_eq!(
        station.thermal_links.len(),
        1,
        "duplicate should be rejected"
    );
}

#[test]
fn remove_link() {
    let content = link_content();
    let mut state = link_state(&content);
    let mut rng = make_rng();

    // Create link
    let create_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::CreateThermalLink {
            station_id: test_station_id(),
            from_module_id: ModuleInstanceId("mod_smelter".to_string()),
            from_port_id: "molten_out".to_string(),
            to_module_id: ModuleInstanceId("mod_receiver".to_string()),
            to_port_id: "molten_in".to_string(),
        },
    };
    tick(&mut state, &[create_cmd], &content, &mut rng, None);
    assert_eq!(state.stations[&test_station_id()].thermal_links.len(), 1);

    // Remove link
    let remove_cmd = CommandEnvelope {
        id: CommandId(1),
        issued_by: PrincipalId("test".to_string()),
        issued_tick: 1,
        execute_at_tick: 1,
        command: Command::RemoveThermalLink {
            station_id: test_station_id(),
            from_module_id: ModuleInstanceId("mod_smelter".to_string()),
            from_port_id: "molten_out".to_string(),
            to_module_id: ModuleInstanceId("mod_receiver".to_string()),
            to_port_id: "molten_in".to_string(),
        },
    };
    let events = tick(&mut state, &[remove_cmd], &content, &mut rng, None);

    assert!(state.stations[&test_station_id()].thermal_links.is_empty());
    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::ThermalLinkRemoved { .. })));
}

#[test]
fn link_serialize_round_trip() {
    let link = crate::ThermalLink {
        from_module_id: ModuleInstanceId("mod_a".to_string()),
        from_port_id: "out".to_string(),
        to_module_id: ModuleInstanceId("mod_b".to_string()),
        to_port_id: "in".to_string(),
    };

    let json = serde_json::to_string(&link).unwrap();
    let deserialized: crate::ThermalLink = serde_json::from_str(&json).unwrap();
    assert_eq!(link, deserialized);
}
