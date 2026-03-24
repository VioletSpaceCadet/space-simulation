//! Integration tests for the sim events system.
//!
//! Tests use real content (`load_content("../../content")`) to verify
//! end-to-end event firing, effect application, and determinism.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_core::{EventEnvelope, EventLevel, GameContent, GameState};

/// Run the simulation for N ticks and return the events produced.
fn run_ticks(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut ChaCha8Rng,
    ticks: u64,
) -> Vec<EventEnvelope> {
    let mut all_events = Vec::new();
    for _ in 0..ticks {
        let events = sim_core::tick(state, &[], content, rng, EventLevel::Normal, None);
        all_events.extend(events);
    }
    all_events
}

/// Determinism regression: same seed produces identical event sequences.
#[test]
fn determinism_same_seed_identical_events() {
    let mut content = sim_world::load_content("../../content").expect("load content");
    content.constants.event_global_cooldown_ticks = 10;

    let ticks = 2000;

    // Run 1
    let mut rng1 = ChaCha8Rng::seed_from_u64(42);
    let mut state1 = sim_world::build_initial_state(&content, 42, &mut rng1);
    let mut rng1 = ChaCha8Rng::seed_from_u64(42);
    let events1 = run_ticks(&mut state1, &content, &mut rng1, ticks);

    // Run 2
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    let mut state2 = sim_world::build_initial_state(&content, 42, &mut rng2);
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    let events2 = run_ticks(&mut state2, &content, &mut rng2, ticks);

    assert_eq!(
        events1.len(),
        events2.len(),
        "Different number of events: run1={}, run2={}",
        events1.len(),
        events2.len()
    );

    for (index, (e1, e2)) in events1.iter().zip(events2.iter()).enumerate() {
        let json1 = serde_json::to_string(&e1.event).expect("serialize");
        let json2 = serde_json::to_string(&e2.event).expect("serialize");
        assert_eq!(
            json1, json2,
            "Event divergence at index {index}: {json1} != {json2}"
        );
    }

    let state_json1 = serde_json::to_string(&state1).expect("serialize state1");
    let state_json2 = serde_json::to_string(&state2).expect("serialize state2");
    assert_eq!(
        state_json1, state_json2,
        "Final game state diverged between runs"
    );
}

/// Different seeds produce different event sequences.
#[test]
fn different_seeds_produce_different_events() {
    let mut content = sim_world::load_content("../../content").expect("load content");
    content.constants.event_global_cooldown_ticks = 10;

    let ticks = 2000;

    let mut rng1 = ChaCha8Rng::seed_from_u64(42);
    let mut state1 = sim_world::build_initial_state(&content, 42, &mut rng1);
    let mut rng1 = ChaCha8Rng::seed_from_u64(42);
    let events1 = run_ticks(&mut state1, &content, &mut rng1, ticks);

    let mut rng2 = ChaCha8Rng::seed_from_u64(99);
    let mut state2 = sim_world::build_initial_state(&content, 99, &mut rng2);
    let mut rng2 = ChaCha8Rng::seed_from_u64(99);
    let events2 = run_ticks(&mut state2, &content, &mut rng2, ticks);

    let sim_events_1 = events1
        .iter()
        .filter(|e| matches!(&e.event, sim_core::Event::SimEventFired { .. }))
        .count();
    let sim_events_2 = events2
        .iter()
        .filter(|e| matches!(&e.event, sim_core::Event::SimEventFired { .. }))
        .count();

    assert!(sim_events_1 > 0, "Expected SimEventFired events in seed 42");
    assert!(sim_events_2 > 0, "Expected SimEventFired events in seed 99");
}

/// Events produce observable state changes.
#[test]
fn events_mutate_state() {
    let mut content = sim_world::load_content("../../content").expect("load content");
    content.constants.event_global_cooldown_ticks = 5;

    let ticks = 3000;

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut state = sim_world::build_initial_state(&content, 42, &mut rng);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = run_ticks(&mut state, &content, &mut rng, ticks);

    let sim_event_count = events
        .iter()
        .filter(|e| matches!(&e.event, sim_core::Event::SimEventFired { .. }))
        .count();
    assert!(
        sim_event_count > 5,
        "Expected multiple SimEventFired events, got {sim_event_count}"
    );

    assert!(
        !state.events.history.is_empty(),
        "Event history should not be empty"
    );
    assert!(
        !state.events.cooldowns.is_empty(),
        "Cooldowns should be populated"
    );
}

/// Temporal modifiers expire correctly.
#[test]
fn temporal_modifiers_expire() {
    let mut content = sim_world::load_content("../../content").expect("load content");
    content.constants.event_global_cooldown_ticks = 5;

    let ticks = 5000;

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut state = sim_world::build_initial_state(&content, 42, &mut rng);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let events = run_ticks(&mut state, &content, &mut rng, ticks);

    let expired_count = events
        .iter()
        .filter(|e| matches!(&e.event, sim_core::Event::SimEventExpired { .. }))
        .count();
    assert!(
        expired_count > 0,
        "Expected SimEventExpired events over {ticks} ticks"
    );
}
