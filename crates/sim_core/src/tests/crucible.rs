//! Crucible (ThermalContainer) module tests (VIO-224).

use super::*;
use crate::test_fixtures::{make_rng, test_station_id, thermal_content};

fn crucible_content() -> GameContent {
    let mut content = thermal_content();
    content.module_defs.insert(
        "module_crucible".to_string(),
        ModuleDef {
            id: "module_crucible".to_string(),
            name: "Crucible".to_string(),
            mass_kg: 3000.0,
            volume_m3: 6.0,
            power_consumption_per_run: 0.0,
            wear_per_run: 0.001,
            behavior: ModuleBehaviorDef::ThermalContainer(ThermalContainerDef {
                capacity_kg: 1000.0,
            }),
            thermal: Some(ThermalDef {
                heat_capacity_j_per_k: 80_000.0,
                passive_cooling_coefficient: 0.5,
                max_temp_mk: 3_000_000,
                operating_min_mk: None,
                operating_max_mk: None,
                thermal_group: Some("default".to_string()),
                idle_heat_generation_w: None,
            }),
            compatible_slots: Vec::new(),
            ship_modifiers: Vec::new(),
            power_stall_priority: None,
            roles: vec![],
            crew_requirement: Default::default(),
            required_tech: None,
            ports: vec![
                ModulePort {
                    id: "molten_in".to_string(),
                    direction: PortDirection::Input,
                    accepts: PortFilter::AnyMolten,
                },
                ModulePort {
                    id: "molten_out".to_string(),
                    direction: PortDirection::Output,
                    accepts: PortFilter::AnyMolten,
                },
            ],
        },
    );
    content
}

fn crucible_state(content: &GameContent) -> GameState {
    let mut state = crate::test_fixtures::base_state(content);
    let station = state.stations.get_mut(&test_station_id()).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_crucible".to_string()),
        def_id: "module_crucible".to_string(),
        enabled: true,
        kind_state: ModuleKindState::ThermalContainer(ThermalContainerState {
            held_items: vec![InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 500.0,
                quality: 1.0,
                thermal: Some(MaterialThermalProps {
                    temp_mk: 1_900_000, // hot liquid Fe
                    phase: Phase::Liquid,
                    latent_heat_buffer_j: 247_000 * 500, // fully charged
                }),
            }],
        }),
        wear: WearState::default(),
        thermal: Some(ThermalState {
            temp_mk: 1_900_000,
            thermal_group: Some("default".to_string()),
            ..Default::default()
        }),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
    });
    state
}

#[test]
fn crucible_holds_molten_fe() {
    let content = crucible_content();
    let state = crucible_state(&content);
    let station = &state.stations[&test_station_id()];
    let crucible = &station
        .modules
        .iter()
        .find(|m| m.def_id == "module_crucible")
        .unwrap();

    if let ModuleKindState::ThermalContainer(ref container) = crucible.kind_state {
        assert_eq!(container.held_items.len(), 1);
        if let InventoryItem::Material { ref thermal, .. } = container.held_items[0] {
            let props = thermal.as_ref().unwrap();
            assert_eq!(props.phase, Phase::Liquid);
            assert_eq!(props.temp_mk, 1_900_000);
        } else {
            panic!("expected Material item");
        }
    } else {
        panic!("expected ThermalContainer state");
    }
}

#[test]
fn crucible_material_cools_slowly() {
    let content = crucible_content();
    let mut state = crucible_state(&content);
    let mut rng = make_rng();

    // Run 10 ticks
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&test_station_id()];
    let crucible = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_crucible")
        .unwrap();
    if let ModuleKindState::ThermalContainer(ref container) = crucible.kind_state {
        let props = match &container.held_items[0] {
            InventoryItem::Material {
                thermal: Some(props),
                ..
            } => props,
            _ => panic!("expected thermal material"),
        };
        // Should still be liquid (insulated — low cooling coefficient of 0.5)
        assert_eq!(
            props.phase,
            Phase::Liquid,
            "insulated crucible should cool slowly"
        );
        // Temperature should have decreased from 1_900_000 but still be well above solidification
        assert!(
            props.temp_mk < 1_900_000,
            "temp should decrease: {}",
            props.temp_mk
        );
        assert!(
            props.temp_mk > 1_761_000,
            "should not have solidified yet: {}",
            props.temp_mk
        );
    } else {
        panic!("expected ThermalContainer state");
    }
}

#[test]
fn crucible_material_eventually_solidifies() {
    let content = crucible_content();
    let mut state = crucible_state(&content);
    let mut rng = make_rng();

    // Run many ticks — with no heat source, material must eventually solidify
    for _ in 0..10_000 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&test_station_id()];
    let crucible = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_crucible")
        .unwrap();
    if let ModuleKindState::ThermalContainer(ref container) = crucible.kind_state {
        let props = match &container.held_items[0] {
            InventoryItem::Material {
                thermal: Some(props),
                ..
            } => props,
            _ => panic!("expected thermal material"),
        };
        assert_eq!(
            props.phase,
            Phase::Solid,
            "material should solidify without heat source"
        );
    } else {
        panic!("expected ThermalContainer state");
    }
}

#[test]
fn crucible_serde_round_trip() {
    let content = crucible_content();
    let state = crucible_state(&content);
    let json = serde_json::to_string(&state).unwrap();
    let loaded: GameState = serde_json::from_str(&json).unwrap();
    let station = &loaded.stations[&test_station_id()];
    let crucible = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_crucible")
        .unwrap();
    if let ModuleKindState::ThermalContainer(ref container) = crucible.kind_state {
        assert_eq!(container.held_items.len(), 1);
    } else {
        panic!("expected ThermalContainer after round-trip");
    }
}
