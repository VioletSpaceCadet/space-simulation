//! VIO-228: Thermal determinism integration tests.
//!
//! Verifies that the thermal system (smelter + radiators) is fully
//! deterministic: two runs with the same seed produce byte-for-byte
//! identical final state and identical event streams.
//!
//! MVP scope: smelter + radiator only (no crucible — blocked by VIO-223).

use crate::test_fixtures::{make_rng, thermal_content};
use crate::{tick, ModuleInstanceId, StationId, ThermalState};

const TICK_COUNT: usize = 1_000;

/// The smelter recipe needs >= 1_800_000 mK to run. Starting at 1_900_000 mK
/// ensures that processing, heat injection, passive cooling, and radiator
/// cooling are all exercised from tick 1.
const HOT_SMELTER_TEMP_MK: u32 = 1_900_000;

/// Build a state with a hot smelter + 2 radiators + ore inventory.
///
/// Uses `state_with_smelter_at_temp` for the smelter, then manually adds
/// the two radiators so the full thermal loop (heat generation + cooling) is
/// active from the start.
fn hot_smelter_with_radiators_state(content: &crate::GameContent) -> crate::GameState {
    let mut state = crate::test_fixtures::state_with_smelter_at_temp(content, HOT_SMELTER_TEMP_MK);
    let station = state
        .stations
        .get_mut(&StationId("station_earth_orbit".to_string()))
        .expect("station should exist");

    // Add two radiators in the same thermal group as the smelter ("default").
    station.modules.push(crate::ModuleState {
        id: ModuleInstanceId("mod_radiator_001".to_string()),
        def_id: "module_basic_radiator".to_string(),
        enabled: true,
        kind_state: crate::ModuleKindState::Radiator(crate::RadiatorState::default()),
        wear: crate::WearState::default(),
        thermal: Some(ThermalState {
            temp_mk: 293_000,
            thermal_group: Some("default".to_string()),
            ..Default::default()
        }),
        power_stalled: false,
        manufacturing_priority: 0,
    });
    station.modules.push(crate::ModuleState {
        id: ModuleInstanceId("mod_radiator_002".to_string()),
        def_id: "module_basic_radiator".to_string(),
        enabled: true,
        kind_state: crate::ModuleKindState::Radiator(crate::RadiatorState::default()),
        wear: crate::WearState::default(),
        thermal: Some(ThermalState {
            temp_mk: 293_000,
            thermal_group: Some("default".to_string()),
            ..Default::default()
        }),
        power_stalled: false,
        manufacturing_priority: 0,
    });

    state
}

/// Run the sim for `TICK_COUNT` ticks with a hot smelter + radiators setup.
/// Returns `(serialized_state, serialized_events)` for comparison.
fn run_thermal_simulation() -> (String, String) {
    let content = thermal_content();
    let mut state = hot_smelter_with_radiators_state(&content);
    let mut rng = make_rng();

    let mut all_events = Vec::new();
    for _ in 0..TICK_COUNT {
        let events = tick(&mut state, &[], &content, &mut rng, None);
        all_events.extend(events);
    }

    let state_json = serde_json::to_string(&state).expect("GameState should serialize to JSON");
    let events_json = serde_json::to_string(&all_events).expect("events should serialize to JSON");

    (state_json, events_json)
}

#[test]
fn thermal_simulation_is_deterministic_across_runs() {
    let (state_a, events_a) = run_thermal_simulation();
    let (state_b, events_b) = run_thermal_simulation();

    assert_eq!(
        state_a, state_b,
        "two identical-seed thermal simulation runs must produce identical final state"
    );
    assert_eq!(
        events_a, events_b,
        "two identical-seed thermal simulation runs must produce identical event streams"
    );
}

#[test]
fn thermal_state_actually_changes() {
    // Guard against a vacuously true determinism test: verify that the
    // thermal system actually did work (smelter temperature changed).
    let content = thermal_content();
    let mut state = hot_smelter_with_radiators_state(&content);
    let mut rng = make_rng();

    let station_id = StationId("station_earth_orbit".to_string());

    let initial_smelter_temp = state.stations[&station_id].modules[0]
        .thermal
        .as_ref()
        .expect("smelter should have thermal state")
        .temp_mk;
    assert_eq!(
        initial_smelter_temp, HOT_SMELTER_TEMP_MK,
        "smelter should start at configured hot temperature"
    );

    for _ in 0..TICK_COUNT {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let final_smelter_temp = state.stations[&station_id].modules[0]
        .thermal
        .as_ref()
        .expect("smelter should still have thermal state after ticks")
        .temp_mk;

    assert_ne!(
        initial_smelter_temp, final_smelter_temp,
        "smelter temperature should change from {initial_smelter_temp} mK over \
         {TICK_COUNT} ticks — test would be vacuously true otherwise"
    );
}

#[test]
fn thermal_events_emitted_during_simulation() {
    // Verify that thermal-related events are actually produced, so the
    // determinism assertion covers non-trivial event streams.
    let content = thermal_content();
    let mut state = hot_smelter_with_radiators_state(&content);
    let mut rng = make_rng();

    let mut all_events = Vec::new();
    for _ in 0..TICK_COUNT {
        let events = tick(&mut state, &[], &content, &mut rng, None);
        all_events.extend(events);
    }

    // The smelter starts hot enough to process, so we should see
    // RefineryRan events from successful smelting runs.
    let has_refinery_ran = all_events
        .iter()
        .any(|envelope| matches!(&envelope.event, crate::Event::RefineryRan { .. }));

    assert!(
        has_refinery_ran,
        "expected RefineryRan events from hot smelter during simulation"
    );

    assert!(
        !all_events.is_empty(),
        "simulation should emit at least some events over {TICK_COUNT} ticks"
    );
}
