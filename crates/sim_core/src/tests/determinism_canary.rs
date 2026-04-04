//! VIO-413: Float determinism canary tests.
//!
//! These tests verify that the full simulation tick loop is deterministic:
//! two runs with the same seed must produce byte-for-byte identical final
//! state (serialized via serde_json). Any float arithmetic non-determinism
//! — whether from iteration order, HashMap traversal, or cross-platform
//! float semantics — will cause these tests to fail.
//!
//! The tests exercise the core production loop:
//! - Survey → asteroid discovery (RNG-dependent composition generation)
//! - Refinery → processing (composition-weighted yield, float quality)
//! - Wear → accumulation per processing run (float addition)
//! - Research → data pool generation from survey (float accumulation)
//!
//! Not yet covered (would need extended fixtures): thermal, boiloff, labs.

use super::*;

/// Number of ticks for full-system determinism canary.
/// 200 ticks is enough to complete survey→mine→deposit→refine cycles
/// and accumulate meaningful wear/research.
const CANARY_TICK_COUNT: usize = 200;

/// Run a full simulation through survey, mine, deposit, and refine cycles.
/// Returns `(state_value, event_count)`.
fn run_full_sim() -> (serde_json::Value, usize) {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    // Issue a survey command on tick 0 to kick off the discovery→mining pipeline.
    let cmd = survey_command(&state);
    let mut total_events = 0;

    for i in 0..CANARY_TICK_COUNT {
        let commands: &[CommandEnvelope] = if i == 0 {
            std::slice::from_ref(&cmd)
        } else {
            &[]
        };
        let events = tick(&mut state, commands, &content, &mut rng, None);
        total_events += events.len();
    }

    // Use serde_json::Value for comparison — HashMap key ordering is
    // non-deterministic across runs, but Value equality is order-independent.
    let state_value: serde_json::Value =
        serde_json::to_value(&state).expect("GameState should serialize");
    (state_value, total_events)
}

#[test]
fn full_sim_deterministic_across_runs() {
    let (state_a, events_a) = run_full_sim();
    let (state_b, events_b) = run_full_sim();

    assert_eq!(
        events_a, events_b,
        "two identical-seed runs must produce the same number of events"
    );
    assert_eq!(
        state_a, state_b,
        "two identical-seed full simulation runs must produce identical final state \
         (serialized JSON). If this fails, a float operation is non-deterministic."
    );
}

#[test]
fn full_sim_state_actually_changes() {
    // Guard against vacuously true determinism: verify that meaningful
    // state mutation occurred during the simulation.
    let content = refinery_content();
    let initial_state = state_with_refinery(&content);
    let mut state = initial_state.clone();
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    for i in 0..CANARY_TICK_COUNT {
        let commands: &[CommandEnvelope] = if i == 0 {
            std::slice::from_ref(&cmd)
        } else {
            &[]
        };
        tick(&mut state, commands, &content, &mut rng, None);
    }

    // Tick counter must advance
    assert_eq!(
        state.meta.tick,
        initial_state.meta.tick + CANARY_TICK_COUNT as u64,
        "tick counter should advance"
    );

    // Station inventory should have changed (refinery processed ore)
    let station_id = test_station_id();
    let initial_ore_kg: f32 = initial_state.stations[&station_id]
        .core
        .inventory
        .iter()
        .filter(|i| i.is_ore())
        .map(InventoryItem::mass_kg)
        .sum();
    let final_ore_kg: f32 = state.stations[&station_id]
        .core
        .inventory
        .iter()
        .filter(|i| i.is_ore())
        .map(InventoryItem::mass_kg)
        .sum();

    assert!(
        (final_ore_kg - initial_ore_kg).abs() > 1.0,
        "ore inventory should change after {CANARY_TICK_COUNT} ticks \
         (initial: {initial_ore_kg}, final: {final_ore_kg})"
    );

    // Wear should have accumulated on the refinery module
    let station = &state.stations[&station_id];
    let refinery_wear = station.core.modules[0].wear.wear;
    assert!(
        refinery_wear > 0.0,
        "refinery module should have accumulated wear (got {refinery_wear})"
    );

    // Research data pool should have data from the survey
    assert!(
        !state.research.data_pool.is_empty(),
        "research data pool should have entries from survey"
    );
}

/// Spot-check that specific high-risk float fields are deterministic.
/// This provides more targeted diagnostics if the full-state test fails.
#[test]
fn float_field_spot_check_deterministic() {
    let content = refinery_content();
    let run = || {
        let mut state = state_with_refinery(&content);
        let mut rng = make_rng();
        let cmd = survey_command(&state);
        for i in 0..CANARY_TICK_COUNT {
            let commands: &[CommandEnvelope] = if i == 0 {
                std::slice::from_ref(&cmd)
            } else {
                &[]
            };
            tick(&mut state, commands, &content, &mut rng, None);
        }
        state
    };

    let state_a = run();
    let state_b = run();

    let station_id = test_station_id();
    let station_a = &state_a.stations[&station_id];
    let station_b = &state_b.stations[&station_id];

    // Wear state (accumulated every tick via float addition)
    for (module_a, module_b) in station_a
        .core
        .modules
        .iter()
        .zip(station_b.core.modules.iter())
    {
        assert_eq!(
            module_a.wear.wear.to_bits(),
            module_b.wear.wear.to_bits(),
            "wear field must be bit-identical for module {}",
            module_a.id.0
        );
    }

    // Inventory masses (float arithmetic during processing)
    let masses_a: Vec<u32> = station_a
        .core
        .inventory
        .iter()
        .map(|i| i.mass_kg().to_bits())
        .collect();
    let masses_b: Vec<u32> = station_b
        .core
        .inventory
        .iter()
        .map(|i| i.mass_kg().to_bits())
        .collect();
    assert_eq!(
        masses_a, masses_b,
        "inventory mass fields must be bit-identical"
    );

    // Research state (float comparisons at RNG boundaries)
    let research_a = serde_json::to_value(&state_a.research).unwrap();
    let research_b = serde_json::to_value(&state_b.research).unwrap();
    assert_eq!(
        research_a, research_b,
        "research state must be identical (serde_json::Value comparison)"
    );
}
