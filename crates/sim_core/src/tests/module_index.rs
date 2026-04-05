use super::*;
use crate::test_fixtures::{
    base_content, base_state, make_rng, rebuild_indices, test_module, test_station_id,
    ModuleDefBuilder,
};

// ---------------------------------------------------------------------------
// 1. module_id_index correct after install via command
// ---------------------------------------------------------------------------

#[test]
fn module_id_index_after_install() {
    let mut content = base_content();
    content.module_defs.insert(
        "module_test_storage".to_string(),
        ModuleDefBuilder::new("module_test_storage")
            .name("Test Storage")
            .mass(100.0)
            .volume(1.0)
            .behavior(ModuleBehaviorDef::Storage { capacity_m3: 100.0 })
            .build(),
    );
    let mut state = base_state(&content);
    let station_id = test_station_id();
    let mut rng = make_rng();

    // Put a module item in inventory to install
    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.inventory.push(InventoryItem::Module {
        item_id: ModuleItemId("mod_item_test".to_string()),
        module_def_id: "module_test_storage".to_string(),
    });
    station.invalidate_volume_cache();

    let install_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::InstallModule {
            facility_id: station_id.clone().into(),
            module_item_id: ModuleItemId("mod_item_test".to_string()),
        },
    };

    tick(&mut state, &[install_cmd], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    // The installed module gets an auto-generated ID
    let last_module = station.core.modules.last().unwrap();
    let expected_idx = station.core.modules.len() - 1;

    assert_eq!(
        station.module_index_by_id(&last_module.id),
        Some(expected_idx),
        "module_id_index should map installed module to last position"
    );
}

// ---------------------------------------------------------------------------
// 2. module_id_index correct after uninstall
// ---------------------------------------------------------------------------

#[test]
fn module_id_index_after_uninstall() {
    let mut content = base_content();
    // Need a module def so the id index can be built
    content.module_defs.insert(
        "module_test_storage".to_string(),
        ModuleDefBuilder::new("module_test_storage")
            .name("Test Storage")
            .mass(100.0)
            .volume(1.0)
            .power(1.0)
            .behavior(ModuleBehaviorDef::Storage { capacity_m3: 100.0 })
            .build(),
    );

    let mut state = base_state(&content);
    let station_id = test_station_id();

    // Install two modules directly
    let station = state.stations.get_mut(&station_id).unwrap();
    station
        .core
        .modules
        .push(test_module("module_test_storage", ModuleKindState::Storage));
    station.core.modules.last_mut().unwrap().id = ModuleInstanceId("mod_first".to_string());
    station
        .core
        .modules
        .push(test_module("module_test_storage", ModuleKindState::Storage));
    station.core.modules.last_mut().unwrap().id = ModuleInstanceId("mod_second".to_string());
    station.rebuild_module_index(&content);

    let first_idx = station.module_index_by_id(&ModuleInstanceId("mod_first".to_string()));
    let second_idx = station.module_index_by_id(&ModuleInstanceId("mod_second".to_string()));
    assert!(first_idx.is_some());
    assert!(second_idx.is_some());

    // Uninstall the first module via command
    let mut rng = make_rng();
    let uninstall_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::UninstallModule {
            facility_id: station_id.clone().into(),
            module_id: ModuleInstanceId("mod_first".to_string()),
        },
    };

    tick(&mut state, &[uninstall_cmd], &content, &mut rng, None);

    let station = &state.stations[&station_id];

    // mod_first should be gone from index
    assert_eq!(
        station.module_index_by_id(&ModuleInstanceId("mod_first".to_string())),
        None,
        "uninstalled module should not be in index"
    );

    // mod_second should still be findable and point to a valid module
    let second_new_idx = station.module_index_by_id(&ModuleInstanceId("mod_second".to_string()));
    assert!(
        second_new_idx.is_some(),
        "remaining module should still be in index"
    );
    let idx = second_new_idx.unwrap();
    assert_eq!(
        station.core.modules[idx].id,
        ModuleInstanceId("mod_second".to_string()),
        "index should point to the correct module"
    );
}

// ---------------------------------------------------------------------------
// 3. module_type_index tracks correct types
// ---------------------------------------------------------------------------

#[test]
fn module_type_index_tracks_correct_types() {
    let mut content = base_content();
    let recipe_id = insert_recipe(
        &mut content,
        RecipeDef {
            id: RecipeId("recipe_test".to_string()),
            inputs: vec![],
            outputs: vec![],
            efficiency: 1.0,
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        },
    );
    content.module_defs.insert(
        "module_test_proc".to_string(),
        ModuleDefBuilder::new("module_test_proc")
            .name("Test Proc")
            .mass(100.0)
            .volume(1.0)
            .power(1.0)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![recipe_id],
            }))
            .build(),
    );
    content.module_defs.insert(
        "module_test_lab".to_string(),
        ModuleDefBuilder::new("module_test_lab")
            .name("Test Lab")
            .mass(100.0)
            .volume(1.0)
            .power(1.0)
            .behavior(ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::new(ResearchDomain::SURVEY),
                data_consumption_per_run: 1.0,
                research_points_per_run: 1.0,
                accepted_data: vec![DataKind::new(DataKind::SURVEY)],
                research_interval_minutes: 1,
                research_interval_ticks: 1,
            }))
            .build(),
    );

    let mut state = base_state(&content);
    let station_id = test_station_id();
    let station = state.stations.get_mut(&station_id).unwrap();

    station.core.modules.push(test_module(
        "module_test_proc",
        ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
    ));
    station.core.modules.push(test_module(
        "module_test_lab",
        ModuleKindState::Lab(LabState {
            ticks_since_last_run: 0,
            assigned_tech: None,
            starved: false,
        }),
    ));
    station.rebuild_module_index(&content);

    let proc_idx = station.core.modules.len() - 2;
    let lab_idx = station.core.modules.len() - 1;

    assert!(
        station
            .core
            .module_type_index
            .processors
            .contains(&proc_idx),
        "processor should be in processors index"
    );
    assert!(
        station.core.module_type_index.labs.contains(&lab_idx),
        "lab should be in labs index"
    );
    assert!(
        !station.core.module_type_index.processors.contains(&lab_idx),
        "lab should NOT be in processors index"
    );
    assert!(
        !station.core.module_type_index.labs.contains(&proc_idx),
        "processor should NOT be in labs index"
    );
}

// ---------------------------------------------------------------------------
// 4. Empty station has valid indexes
// ---------------------------------------------------------------------------

#[test]
fn empty_station_indexes() {
    let content = base_content();
    let mut state = base_state(&content);
    let station_id = test_station_id();
    let station = state.stations.get_mut(&station_id).unwrap();

    // Clear all modules
    station.core.modules.clear();
    station.rebuild_module_index(&content);

    assert!(station.core.module_type_index.is_initialized());
    assert!(station.core.module_type_index.processors.is_empty());
    assert!(station.core.module_type_index.labs.is_empty());
    assert!(station.core.module_type_index.assemblers.is_empty());
    assert!(station.core.module_type_index.sensors.is_empty());
    assert!(station.core.module_type_index.maintenance.is_empty());
    assert!(station.core.module_type_index.thermal.is_empty());
    assert!(station.core.module_type_index.roles.is_empty());
    assert!(station.core.module_id_index.is_empty());

    // Lookup should return None
    assert_eq!(
        station.module_index_by_id(&ModuleInstanceId("anything".to_string())),
        None
    );

    // Ticking should not panic
    let mut rng = make_rng();
    rebuild_indices(&mut state, &content);
    tick(&mut state, &[], &content, &mut rng, None);
}
