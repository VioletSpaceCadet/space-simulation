//! Thermal scenario integration tests (VIO-229).
//!
//! Validates that a 30-day thermal simulation runs to completion with real content,
//! no collapses, and reasonable thermal metrics.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::{AutopilotController, CommandSource};
use sim_core::StationId;

fn content_dir() -> String {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    format!("{manifest}/../../content")
}

fn test_station_id() -> StationId {
    StationId("station_earth_orbit".to_string())
}

#[test]
fn thermal_30d_scenario_completes_no_collapse() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let state_path = format!(
        "{}/../../content/dev_base_state.json",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    let state_json = std::fs::read_to_string(&state_path).unwrap();
    let mut state: sim_core::GameState = serde_json::from_str(&state_json).unwrap();
    state.body_cache = sim_core::build_body_cache(&content.solar_system.bodies);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut autopilot = AutopilotController::new();
    let mut next_cmd_id = 0_u64;

    // Run 720 ticks (30 sim-days at 60 min/tick)
    for _ in 0..720 {
        let envelopes = autopilot.generate_commands(&state, &content, &mut next_cmd_id);
        sim_core::tick(&mut state, &envelopes, &content, &mut rng, None);
    }

    let station = &state.stations[&test_station_id()];

    // Station should still be operational
    assert!(
        !station.modules.is_empty(),
        "station should still have modules after 30 days"
    );

    // Check thermal modules exist and have warmed up
    let thermal_modules: Vec<_> = station
        .modules
        .iter()
        .filter(|m| m.thermal.is_some())
        .collect();
    assert!(
        !thermal_modules.is_empty(),
        "station should have thermal modules"
    );

    // At least one thermal module should have warmed up from ambient
    let has_warm_module = thermal_modules
        .iter()
        .any(|m| m.thermal.as_ref().unwrap().temp_mk > 293_000);
    assert!(
        has_warm_module,
        "at least one thermal module should be above ambient temp"
    );

    // No modules should be in overheat critical auto-disable state
    // (2 radiators should prevent overheating)
    let overheat_disabled = thermal_modules
        .iter()
        .filter(|m| {
            m.thermal.as_ref().unwrap().overheat_zone == sim_core::OverheatZone::Critical
                && !m.enabled
        })
        .count();
    assert_eq!(
        overheat_disabled, 0,
        "no thermal modules should be critically overheated with 2 radiators"
    );
}
