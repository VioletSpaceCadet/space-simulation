use super::*;
use crate::test_fixtures::{base_content, base_state, make_rng, ModuleDefBuilder};
use std::collections::HashMap;

/// Create a ground facility with specified modules and insert into state.
fn add_ground_facility(
    state: &mut GameState,
    content: &GameContent,
    modules: Vec<ModuleState>,
) -> GroundFacilityId {
    let gf_id = GroundFacilityId("gf_test".to_string());
    let gf = GroundFacilityState {
        id: gf_id.clone(),
        name: "Test Ground Facility".to_string(),
        position: crate::test_fixtures::test_position(),
        core: FacilityCore {
            modules,
            inventory: vec![],
            cargo_capacity_m3: 1000.0,
            power_available_per_tick: 100.0,
            modifiers: crate::modifiers::ModifierSet::default(),
            crew: std::collections::BTreeMap::new(),
            thermal_links: Vec::new(),
            power: PowerState::default(),
            cached_inventory_volume_m3: None,
            module_type_index: ModuleTypeIndex::default(),
            module_id_index: HashMap::new(),
            power_budget_cache: PowerBudgetCache::default(),
        },
        launch_transits: Vec::new(),
    };
    state.ground_facilities.insert(gf_id.clone(), gf);
    gf_id
}

fn make_module(def_id: &str, enabled: bool) -> ModuleState {
    ModuleState {
        id: ModuleInstanceId(format!("mod_{def_id}")),
        def_id: def_id.to_string(),
        enabled,
        kind_state: ModuleKindState::Storage,
        wear: WearState { wear: 0.0 },
        efficiency: 1.0,
        power_stalled: false,
        assigned_crew: std::collections::BTreeMap::new(),
        prev_crew_satisfied: true,
        thermal: None,
        module_priority: 0,
        slot_index: None,
    }
}

fn content_with_costly_module() -> GameContent {
    let mut content = base_content();
    let mut def = ModuleDefBuilder::new("module_costly")
        .name("Costly Module")
        .behavior(ModuleBehaviorDef::Storage { capacity_m3: 100.0 })
        .build();
    def.operating_cost_per_tick = 50.0;
    content.module_defs.insert("module_costly".to_string(), def);
    content
}

#[test]
fn enabled_module_deducts_from_balance() {
    let content = content_with_costly_module();
    let mut state = base_state(&content);
    state.balance = 1000.0;
    add_ground_facility(
        &mut state,
        &content,
        vec![make_module("module_costly", true)],
    );
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

    assert!(
        (state.balance - 950.0).abs() < 0.01,
        "balance should be 950 after $50 deduction, got {}",
        state.balance
    );
}

#[test]
fn disabled_module_does_not_deduct() {
    let content = content_with_costly_module();
    let mut state = base_state(&content);
    state.balance = 1000.0;
    add_ground_facility(
        &mut state,
        &content,
        vec![make_module("module_costly", false)],
    );
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

    assert!(
        (state.balance - 1000.0).abs() < 1.0,
        "balance should be ~unchanged with disabled module, got {}",
        state.balance
    );
}

#[test]
fn zero_cost_modules_emit_no_event() {
    let content = base_content();
    let mut state = base_state(&content);
    state.balance = 1000.0;
    // Storage module has default operating_cost_per_tick = 0.0
    add_ground_facility(
        &mut state,
        &content,
        vec![make_module("module_basic_solar_array", true)],
    );
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, None);

    let cost_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.event, Event::OperatingCostDeducted { .. }))
        .collect();
    assert!(
        cost_events.is_empty(),
        "zero-cost modules should not emit OperatingCostDeducted"
    );
}

#[test]
fn operating_cost_emits_event() {
    let content = content_with_costly_module();
    let mut state = base_state(&content);
    state.balance = 1000.0;
    add_ground_facility(
        &mut state,
        &content,
        vec![make_module("module_costly", true)],
    );
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, None);

    let cost_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.event, Event::OperatingCostDeducted { .. }))
        .collect();
    assert_eq!(cost_events.len(), 1, "should emit exactly one cost event");
}

#[test]
fn empty_ground_facilities_no_deduction() {
    let content = base_content();
    let mut state = base_state(&content);
    state.balance = 1000.0;
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

    // Balance may change from crew salaries, but no operating cost events
    let events = tick(&mut state, &[], &content, &mut rng, None);
    let cost_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.event, Event::OperatingCostDeducted { .. }))
        .collect();
    assert!(
        cost_events.is_empty(),
        "no ground facilities = no operating cost events"
    );
}
