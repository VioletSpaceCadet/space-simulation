use super::*;
use crate::test_fixtures::{base_content, base_state, make_rng};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;

// --- Test helpers -------------------------------------------------------

fn test_content() -> GameContent {
    base_content()
}

fn test_state(content: &GameContent) -> GameState {
    base_state(content)
}

fn survey_command(state: &GameState) -> CommandEnvelope {
    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();
    CommandEnvelope {
        id: CommandId("cmd_000001".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::Survey {
                site: SiteId("site_0001".to_string()),
            },
        },
    }
}

// --- Command application ------------------------------------------------

#[test]
fn test_assign_survey_sets_task() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

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
    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

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
        id: CommandId("cmd_000001".to_string()),
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

    tick(
        &mut state,
        &[bad_command],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

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
        id: CommandId("cmd_000001".to_string()),
        issued_by: state.ships[&ship_id].owner.clone(),
        issued_tick: 0,
        execute_at_tick: 5, // scheduled for tick 5, not now
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Survey {
                site: SiteId("site_0001".to_string()),
            },
        },
    };

    tick(
        &mut state,
        &[future_command],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    let ship = &state.ships[&ship_id];
    assert!(
        ship.task.is_none(),
        "command scheduled for a future tick should not apply yet"
    );
}

// --- Survey scan --------------------------------------------------------

#[test]
fn test_survey_creates_asteroid_on_completion() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    // Tick 0: assign task (eta_tick = 0 + 1 = 1).
    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    assert!(
        state.asteroids.is_empty(),
        "asteroid should not exist before task completes"
    );

    // Tick 1: task resolves.
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    assert_eq!(
        state.asteroids.len(),
        1,
        "asteroid should be created on survey completion"
    );
    assert!(
        !state.scan_sites.iter().any(|s| s.id.0 == "site_0001"),
        "original scan site should be consumed"
    );
}

#[test]
fn test_survey_emits_expected_events() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let completion_events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let event_kinds: Vec<&str> = completion_events
        .iter()
        .map(|e| match &e.event {
            Event::AsteroidDiscovered { .. } => "AsteroidDiscovered",
            Event::ScanResult { .. } => "ScanResult",
            Event::DataGenerated { .. } => "DataGenerated",
            Event::TaskCompleted { .. } => "TaskCompleted",
            _ => "other",
        })
        .collect();

    assert!(event_kinds.contains(&"AsteroidDiscovered"));
    assert!(event_kinds.contains(&"ScanResult"));
    assert!(event_kinds.contains(&"DataGenerated"));
    assert!(event_kinds.contains(&"TaskCompleted"));
}

#[test]
fn test_survey_detects_tags_with_prob_one() {
    // test_content sets detection probability to 1.0, so all tags must be detected.
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let tags = events
        .iter()
        .find_map(|e| match &e.event {
            Event::ScanResult { tags, .. } => Some(tags.clone()),
            _ => None,
        })
        .expect("ScanResult should be emitted");

    assert!(
        tags.iter().any(|(tag, _)| *tag == AnomalyTag::IronRich),
        "IronRich tag should be detected when probability is 1.0"
    );
}

#[test]
fn test_survey_accumulates_scan_data() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let scan_data = state
        .research
        .data_pool
        .get(&DataKind::ScanData)
        .copied()
        .unwrap_or(0.0);
    assert!(
        scan_data > 0.0,
        "ScanData should accumulate in the data pool after a survey"
    );
}

#[test]
fn test_ship_returns_to_idle_after_survey() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let ship = &state.ships[&ShipId("ship_0001".to_string())];
    assert!(
        matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Idle)),
        "ship should return to Idle after survey completes"
    );
}

// --- Deep scan ----------------------------------------------------------

#[test]
fn test_deep_scan_blocked_without_tech() {
    // Use very high difficulty so research cannot unlock the tech in the setup ticks.
    let mut content = test_content();
    content.techs[0].difficulty = 1_000_000.0;
    let mut state = test_state(&content);
    let mut rng = make_rng();

    // Create an asteroid first.
    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();

    let deep_cmd = CommandEnvelope {
        id: CommandId("cmd_000002".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::DeepScan {
                asteroid: asteroid_id,
            },
        },
    };

    tick(
        &mut state,
        &[deep_cmd],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    let ship = &state.ships[&ship_id];
    assert!(
        !matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::DeepScan { .. })),
        "DeepScan command should be dropped when tech is not unlocked"
    );
}

#[test]
fn test_deep_scan_maps_composition() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    // Unlock the tech directly.
    state
        .research
        .unlocked
        .insert(TechId("tech_deep_scan_v1".to_string()));

    // Survey to create an asteroid.
    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    assert!(
        state.asteroids[&asteroid_id]
            .knowledge
            .composition
            .is_none(),
        "composition should be unknown before deep scan"
    );

    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();
    let deep_cmd = CommandEnvelope {
        id: CommandId("cmd_000002".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::DeepScan {
                asteroid: asteroid_id.clone(),
            },
        },
    };

    tick(
        &mut state,
        &[deep_cmd],
        &content,
        &mut rng,
        EventLevel::Normal,
    );
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let composition = state.asteroids[&asteroid_id].knowledge.composition.as_ref();
    assert!(
        composition.is_some(),
        "composition should be mapped after deep scan"
    );
}

#[test]
fn test_deep_scan_composition_matches_truth_with_zero_sigma() {
    // test_content sets sigma=0.0, so mapped should exactly equal true.
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    state
        .research
        .unlocked
        .insert(TechId("tech_deep_scan_v1".to_string()));

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();
    let deep_cmd = CommandEnvelope {
        id: CommandId("cmd_000002".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::DeepScan {
                asteroid: asteroid_id.clone(),
            },
        },
    };
    tick(
        &mut state,
        &[deep_cmd],
        &content,
        &mut rng,
        EventLevel::Normal,
    );
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid = &state.asteroids[&asteroid_id];
    let mapped = asteroid.knowledge.composition.as_ref().unwrap();
    for (element, &true_val) in &asteroid.true_composition {
        let mapped_val = mapped.get(element).copied().unwrap_or(0.0);
        assert!(
            (mapped_val - true_val).abs() < 1e-5,
            "mapped {element} ({mapped_val}) should equal true value ({true_val}) with sigma=0"
        );
    }
}

// --- Research -----------------------------------------------------------

#[test]
fn test_research_evidence_accumulates_each_tick() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(999); // seed unlikely to unlock tech immediately

    let tech_id = TechId("tech_deep_scan_v1".to_string());

    // Run enough ticks that evidence grows, but use a known-safe seed.
    // We just want to observe accumulation, not guarantee no unlock.
    // Instead, set difficulty very high so unlock is practically impossible.
    let mut high_difficulty_content = content.clone();
    high_difficulty_content.techs[0].difficulty = 1_000_000.0;

    tick(
        &mut state,
        &[],
        &high_difficulty_content,
        &mut rng,
        EventLevel::Normal,
    );
    let evidence_t1 = *state.research.evidence.get(&tech_id).unwrap_or(&0.0);

    tick(
        &mut state,
        &[],
        &high_difficulty_content,
        &mut rng,
        EventLevel::Normal,
    );
    let evidence_t2 = *state.research.evidence.get(&tech_id).unwrap_or(&0.0);

    assert!(
        evidence_t1 > 0.0,
        "evidence should be positive after first tick"
    );
    assert!(
        evidence_t2 > evidence_t1,
        "evidence should increase each tick"
    );
}

#[test]
fn test_research_emits_power_consumed() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::PowerConsumed { .. })),
        "PowerConsumed should be emitted each tick that research runs"
    );
}

#[test]
fn test_research_power_amount_correct() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let power = events
        .iter()
        .find_map(|e| match &e.event {
            Event::PowerConsumed { amount, .. } => Some(*amount),
            _ => None,
        })
        .expect("PowerConsumed event should be present");

    // compute_units_total=10, power_per_unit=1.0 → 10.0
    assert!(
        (power - 10.0).abs() < 1e-5,
        "power consumed should equal compute_units_total * power_per_unit"
    );
}

#[test]
fn test_research_prereq_blocks_evidence() {
    let mut content = test_content();
    content.techs[0].prereqs = vec![TechId("tech_not_yet_unlocked".to_string())];

    let mut state = test_state(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let tech_id = TechId("tech_deep_scan_v1".to_string());
    let evidence = state
        .research
        .evidence
        .get(&tech_id)
        .copied()
        .unwrap_or(0.0);
    assert_eq!(
        evidence, 0.0,
        "evidence should not accumulate when prerequisites are unmet"
    );
}

#[test]
fn test_research_no_power_consumed_when_no_eligible_techs() {
    let content = test_content();
    let mut state = test_state(&content);
    // Pre-unlock the only tech so nothing is eligible.
    state
        .research
        .unlocked
        .insert(TechId("tech_deep_scan_v1".to_string()));
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, Event::PowerConsumed { .. })),
        "no PowerConsumed when all techs are already unlocked"
    );
}

#[test]
fn test_tech_unlocks_eventually() {
    let content = test_content(); // difficulty=10, compute=10
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let tech_id = TechId("tech_deep_scan_v1".to_string());
    let mut unlocked_at = None;

    for tick_num in 0..500 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        if state.research.unlocked.contains(&tech_id) {
            unlocked_at = Some(tick_num);
            break;
        }
    }

    assert!(unlocked_at.is_some(), "tech should unlock within 500 ticks");
}

#[test]
fn test_tech_unlock_tick_is_deterministic() {
    let content = test_content();

    let unlock_tick = |seed: u64| -> Option<u64> {
        let mut state = test_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let tech_id = TechId("tech_deep_scan_v1".to_string());
        for _ in 0..500 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
            if state.research.unlocked.contains(&tech_id) {
                return Some(state.meta.tick);
            }
        }
        None
    };

    assert_eq!(
        unlock_tick(42),
        unlock_tick(42),
        "same seed must produce the same unlock tick"
    );
}

#[test]
fn test_debug_level_emits_research_roll_events() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::ResearchRoll { .. })),
        "ResearchRoll events should be emitted at EventLevel::Debug"
    );
}

#[test]
fn test_normal_level_suppresses_research_roll_events() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, Event::ResearchRoll { .. })),
        "ResearchRoll events should not be emitted at EventLevel::Normal"
    );
}

// --- Element definitions ------------------------------------------------

#[test]
fn test_content_has_element_densities() {
    let content = test_content();
    let fe = content
        .elements
        .iter()
        .find(|e| e.id == "Fe")
        .expect("Fe element must be defined in content");
    assert!(
        (fe.density_kg_per_m3 - 7874.0).abs() < 1.0,
        "Fe density should be ~7874 kg/m³"
    );
}

// --- Asteroid mass ------------------------------------------------------

#[test]
fn test_asteroid_has_mass_after_survey() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid = state.asteroids.values().next().unwrap();
    assert!(
        asteroid.mass_kg > 0.0,
        "asteroid must have positive mass after survey"
    );
}

#[test]
fn test_asteroid_mass_within_range() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let asteroid = state.asteroids.values().next().unwrap();
    // test_content sets min=max=500.0 for determinism
    assert!(
        (asteroid.mass_kg - 500.0).abs() < 1e-3,
        "mass should be 500.0 in test content (fixed range)"
    );
}

// --- Determinism --------------------------------------------------------

#[test]
fn test_identical_seeds_produce_identical_event_logs() {
    let content = test_content();

    let run = |seed: u64| -> Vec<(String, u64)> {
        let mut state = test_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut log = Vec::new();

        let cmd = survey_command(&state);
        for i in 0..20u64 {
            let commands = if i == 0 {
                std::slice::from_ref(&cmd)
            } else {
                &[]
            };
            let events = tick(&mut state, commands, &content, &mut rng, EventLevel::Debug);
            for event in events {
                log.push((event.id.0.clone(), event.tick));
            }
        }
        log
    };

    assert_eq!(
        run(42),
        run(42),
        "identical seeds must produce identical event logs"
    );
}

#[test]
fn test_different_seeds_produce_different_results() {
    let content = test_content();

    let unlock_tick = |seed: u64| -> Option<u64> {
        let mut state = test_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let tech_id = TechId("tech_deep_scan_v1".to_string());
        for _ in 0..500 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
            if state.research.unlocked.contains(&tech_id) {
                return Some(state.meta.tick);
            }
        }
        None
    };

    // Seeds 42 and 1234 should unlock at different ticks (very likely with this model).
    // If they happen to collide the test is a false failure — acceptable in practice.
    let tick_42 = unlock_tick(42);
    let tick_1234 = unlock_tick(1234);
    assert_ne!(
        tick_42, tick_1234,
        "different seeds should generally produce different results"
    );
}

// --- Mine ---------------------------------------------------------------

// Helper: build a state with an already-surveyed asteroid (mass 500, 70% Fe / 30% Si).
fn state_with_asteroid(content: &GameContent) -> (GameState, AsteroidId) {
    let mut state = test_state(content);
    let mut rng = make_rng();
    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], content, &mut rng, EventLevel::Normal);
    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    (state, asteroid_id)
}

fn mine_command(
    state: &GameState,
    asteroid_id: &AsteroidId,
    _content: &GameContent,
) -> CommandEnvelope {
    let ship_id = ShipId("ship_0001".to_string());
    let ship = &state.ships[&ship_id];
    // Use a simple fixed duration for tests
    let duration_ticks = 10;
    CommandEnvelope {
        id: CommandId("cmd_mine_001".to_string()),
        issued_by: ship.owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::Mine {
                asteroid: asteroid_id.clone(),
                duration_ticks,
            },
        },
    }
}

#[test]
fn test_mine_emits_ore_mined_event() {
    let content = test_content();
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    // Fast forward to completion (duration_ticks=10, so task eta is 10 ticks from start)
    let completion_tick = state.meta.tick + 9;
    while state.meta.tick < completion_tick {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }
    // Tick once more to hit the eta_tick and resolve the task
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::OreMined { .. })),
        "OreMined event should be emitted when mining completes"
    );
}

#[test]
fn test_mine_adds_ore_to_ship_inventory() {
    let content = test_content();
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    assert!(state.ships[&ship_id].inventory.is_empty());

    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let completion_tick = state.meta.tick + 10;
    while state.meta.tick <= completion_tick {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let inv = &state.ships[&ship_id].inventory;
    assert!(
        !inv.is_empty(),
        "ship inventory should not be empty after mining"
    );
    assert!(
        inv.iter()
            .any(|i| matches!(i, InventoryItem::Ore { kg, .. } if *kg > 0.0)),
        "extracted mass must be positive"
    );
}

#[test]
fn test_mine_reduces_asteroid_mass() {
    let content = test_content();
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let original_mass = state.asteroids[&asteroid_id].mass_kg;
    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let completion_tick = state.meta.tick + 10;
    while state.meta.tick <= completion_tick {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let remaining = state
        .asteroids
        .get(&asteroid_id)
        .map(|a| a.mass_kg)
        .unwrap_or(0.0);
    assert!(
        remaining < original_mass,
        "asteroid mass must decrease after mining"
    );
}

#[test]
fn test_mine_removes_depleted_asteroid() {
    let mut content = test_content();
    content.constants.mining_rate_kg_per_tick = 1_000_000.0; // deplete in 1 tick
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    // Run to completion
    for _ in 0..11 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    assert!(
        !state.asteroids.contains_key(&asteroid_id),
        "fully mined asteroid should be removed from state"
    );
}

// --- Deposit ------------------------------------------------------------

fn deposit_command(state: &GameState) -> CommandEnvelope {
    let ship_id = ShipId("ship_0001".to_string());
    let station_id = StationId("station_earth_orbit".to_string());
    let ship = &state.ships[&ship_id];
    CommandEnvelope {
        id: CommandId("cmd_deposit_001".to_string()),
        issued_by: ship.owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::Deposit {
                station: station_id,
                blocked: false,
            },
        },
    }
}

#[test]
fn test_deposit_moves_inventory_to_station() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_test_0001".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 100.0,
            composition: std::collections::HashMap::from([
                ("Fe".to_string(), 0.7_f32),
                ("Si".to_string(), 0.3_f32),
            ]),
        });

    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station_has_ore = state.stations[&station_id]
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Ore { kg, .. } if *kg == 100.0));
    assert!(station_has_ore, "ore should transfer to station");
}

#[test]
fn test_deposit_clears_ship_inventory() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_test_0001".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 100.0,
            composition: std::collections::HashMap::from([
                ("Fe".to_string(), 0.7_f32),
                ("Si".to_string(), 0.3_f32),
            ]),
        });

    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        state.ships[&ship_id].inventory.is_empty(),
        "ship inventory should be empty after deposit"
    );
}

#[test]
fn test_deposit_emits_ore_deposited_event() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_test_0001".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 50.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::OreDeposited { .. })),
        "OreDeposited event should be emitted"
    );
}

// --- Cargo holds --------------------------------------------------------

#[test]
fn test_ship_starts_with_empty_inventory() {
    let content = test_content();
    let state = test_state(&content);
    let ship = state.ships.values().next().unwrap();
    assert!(
        ship.inventory.is_empty(),
        "ship inventory should be empty at start"
    );
    assert!(
        (ship.cargo_capacity_m3 - 20.0).abs() < 1e-5,
        "ship capacity should be 20 m³"
    );
}

#[test]
fn test_station_starts_with_empty_inventory() {
    let content = test_content();
    let state = test_state(&content);
    let station = state.stations.values().next().unwrap();
    assert!(
        station.inventory.is_empty(),
        "station inventory should be empty at start"
    );
    assert!(
        (station.cargo_capacity_m3 - 10_000.0).abs() < 1e-5,
        "station capacity should be 10,000 m³"
    );
}

// --- Station capacity enforcement ----------------------------------------

#[test]
fn test_deposit_respects_station_capacity() {
    // Set up a very small station capacity so the ore lot won't fit.
    let mut content = test_content();
    content.constants.station_cargo_capacity_m3 = 0.001; // tiny — no room for ore
    let mut state = test_state(&content);

    // Give the station the tiny capacity.
    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    // Load ship with ore.
    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_cap_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 500.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Station should still be empty (no room).
    assert!(
        state.stations[&station_id].inventory.is_empty(),
        "station should not accept ore beyond its capacity"
    );
    // Ship should retain the ore it couldn't deposit.
    assert!(
        !state.ships[&ship_id].inventory.is_empty(),
        "ship should retain ore that did not fit in the station"
    );
    // Ship should stay in Deposit task (blocked), not go idle.
    let ship = &state.ships[&ship_id];
    assert!(
        matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Deposit { blocked: true, .. })),
        "ship should stay in blocked Deposit task when station is full"
    );
}

#[test]
fn test_deposit_partial_when_station_partially_full() {
    // Station has room for exactly one ore lot (100 kg ore ≈ 0.033 m³ at 3000 kg/m³).
    // Set capacity to 0.04 m³ — fits 100 kg but not 200 kg.
    let mut content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.04;
    content.constants.station_cargo_capacity_m3 = 0.04;

    let ship_id = ShipId("ship_0001".to_string());
    let ship = state.ships.get_mut(&ship_id).unwrap();
    // Two 100 kg lots — only the first should fit.
    ship.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_a".to_string()),
        asteroid_id: AsteroidId("asteroid_test".to_string()),
        kg: 100.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
    });
    ship.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_b".to_string()),
        asteroid_id: AsteroidId("asteroid_test".to_string()),
        kg: 100.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
    });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_ore_kg: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        (station_ore_kg - 100.0).abs() < 1.0,
        "station should have accepted only the first lot (100 kg), got {station_ore_kg} kg"
    );

    let ship_ore_kg: f32 = state.ships[&ship_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        (ship_ore_kg - 100.0).abs() < 1.0,
        "ship should retain the second lot (100 kg) that didn't fit, got {ship_ore_kg} kg"
    );
}

// --- Transit ------------------------------------------------------------

#[test]
fn test_shortest_hop_count_same_node() {
    let content = test_content();
    let node = NodeId("node_test".to_string());
    assert_eq!(
        shortest_hop_count(&node, &node, &content.solar_system),
        Some(0)
    );
}

#[test]
fn test_shortest_hop_count_adjacent() {
    let solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: NodeId("a".to_string()),
                name: "A".to_string(),
            },
            NodeDef {
                id: NodeId("b".to_string()),
                name: "B".to_string(),
            },
        ],
        edges: vec![(NodeId("a".to_string()), NodeId("b".to_string()))],
    };
    assert_eq!(
        shortest_hop_count(
            &NodeId("a".to_string()),
            &NodeId("b".to_string()),
            &solar_system
        ),
        Some(1)
    );
    // Undirected: reverse also works.
    assert_eq!(
        shortest_hop_count(
            &NodeId("b".to_string()),
            &NodeId("a".to_string()),
            &solar_system
        ),
        Some(1)
    );
}

#[test]
fn test_shortest_hop_count_two_hops() {
    let solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: NodeId("a".to_string()),
                name: "A".to_string(),
            },
            NodeDef {
                id: NodeId("b".to_string()),
                name: "B".to_string(),
            },
            NodeDef {
                id: NodeId("c".to_string()),
                name: "C".to_string(),
            },
        ],
        edges: vec![
            (NodeId("a".to_string()), NodeId("b".to_string())),
            (NodeId("b".to_string()), NodeId("c".to_string())),
        ],
    };
    assert_eq!(
        shortest_hop_count(
            &NodeId("a".to_string()),
            &NodeId("c".to_string()),
            &solar_system
        ),
        Some(2)
    );
}

#[test]
fn test_shortest_hop_count_no_path() {
    let solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: NodeId("a".to_string()),
                name: "A".to_string(),
            },
            NodeDef {
                id: NodeId("b".to_string()),
                name: "B".to_string(),
            },
        ],
        edges: vec![],
    };
    assert_eq!(
        shortest_hop_count(
            &NodeId("a".to_string()),
            &NodeId("b".to_string()),
            &solar_system
        ),
        None
    );
}

// --- Refinery ---

fn refinery_content() -> GameContent {
    let mut content = test_content();
    content.module_defs = vec![ModuleDef {
        id: "module_basic_iron_refinery".to_string(),
        name: "Basic Iron Refinery".to_string(),
        mass_kg: 5000.0,
        volume_m3: 10.0,
        power_consumption_per_run: 10.0,
        behavior: ModuleBehaviorDef::Processor(ProcessorDef {
            processing_interval_ticks: 2, // short for tests
            recipes: vec![RecipeDef {
                id: "recipe_basic_iron".to_string(),
                inputs: vec![RecipeInput {
                    filter: InputFilter::ItemKind(ItemKind::Ore),
                    amount: InputAmount::Kg(500.0),
                }],
                outputs: vec![
                    OutputSpec::Material {
                        element: "Fe".to_string(),
                        yield_formula: YieldFormula::ElementFraction {
                            element: "Fe".to_string(),
                        },
                        quality_formula: QualityFormula::ElementFractionTimesMultiplier {
                            element: "Fe".to_string(),
                            multiplier: 1.0,
                        },
                    },
                    OutputSpec::Slag {
                        yield_formula: YieldFormula::FixedFraction(1.0),
                    },
                ],
                efficiency: 1.0,
            }],
        }),
    }];
    content
}

fn state_with_refinery(content: &GameContent) -> GameState {
    let mut state = test_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
    });

    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_0001".to_string()),
        asteroid_id: AsteroidId("asteroid_0001".to_string()),
        kg: 1000.0,
        composition: std::collections::HashMap::from([
            ("Fe".to_string(), 0.7f32),
            ("Si".to_string(), 0.3f32),
        ]),
    });

    state
}

#[test]
fn test_refinery_produces_material_and_slag() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let has_material = station.inventory.iter().any(|i| {
        matches!(i, InventoryItem::Material { element, kg, .. } if element == "Fe" && *kg > 0.0)
    });
    assert!(
        has_material,
        "station should have Fe Material after refinery runs"
    );

    let has_slag = station
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Slag { kg, .. } if *kg > 0.0));
    assert!(has_slag, "station should have Slag after refinery runs");
}

#[test]
fn test_refinery_quality_equals_fe_fraction() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let quality = station.inventory.iter().find_map(|i| {
        if let InventoryItem::Material {
            element, quality, ..
        } = i
        {
            if element == "Fe" {
                Some(*quality)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(quality.is_some(), "Fe Material should exist");
    assert!(
        (quality.unwrap() - 0.7).abs() < 1e-4,
        "quality should equal Fe fraction (0.7) with multiplier 1.0"
    );
}

#[test]
fn test_refinery_skips_when_below_threshold() {
    let content = refinery_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 9999.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
    });
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_0001".to_string()),
        asteroid_id: AsteroidId("asteroid_0001".to_string()),
        kg: 1000.0,
        composition: std::collections::HashMap::from([
            ("Fe".to_string(), 0.7f32),
            ("Si".to_string(), 0.3f32),
        ]),
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Material { .. })),
        "refinery should not run when ore is below threshold"
    );
}

#[test]
fn test_refinery_emits_refinery_ran_event() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "RefineryRan event should be emitted when refinery processes ore"
    );
}

#[test]
fn transit_moves_ship_and_starts_next_task() {
    // Two-node solar system; ship starts at node_a, site is at node_b.
    let mut content = test_content();
    let node_a = NodeId("node_a".to_string());
    let node_b = NodeId("node_b".to_string());
    content.solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: node_a.clone(),
                name: "A".to_string(),
            },
            NodeDef {
                id: node_b.clone(),
                name: "B".to_string(),
            },
        ],
        edges: vec![(node_a.clone(), node_b.clone())],
    };
    content.constants.travel_ticks_per_hop = 5;
    content.constants.survey_scan_ticks = 1;

    let ship_id = ShipId("ship_0001".to_string());
    let site_id = SiteId("site_0001".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = StationId("station_test".to_string());

    let mut state = GameState {
        meta: MetaState {
            tick: 0,
            seed: 0,
            schema_version: 1,
            content_version: "test".to_string(),
        },
        scan_sites: vec![ScanSite {
            id: site_id.clone(),
            node: node_b.clone(),
            template_id: "tmpl_iron_rich".to_string(),
        }],
        asteroids: HashMap::new(),
        ships: HashMap::from([(
            ship_id.clone(),
            ShipState {
                id: ship_id.clone(),
                location_node: node_a.clone(),
                owner: owner.clone(),
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
            },
        )]),
        stations: HashMap::from([(
            station_id.clone(),
            StationState {
                id: station_id,
                location_node: node_a.clone(),
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                facilities: FacilitiesState {
                    compute_units_total: 10,
                    power_per_compute_unit_per_tick: 1.0,
                    efficiency: 1.0,
                },
                modules: vec![],
            },
        )]),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: HashMap::new(),
            evidence: HashMap::new(),
        },
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
    };

    let mut rng = ChaCha8Rng::seed_from_u64(0);

    // Assign a Transit task: 5 ticks to node_b, then Survey.
    let transit_cmd = CommandEnvelope {
        id: CommandId("cmd_000000".to_string()),
        issued_by: owner,
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Transit {
                destination: node_b.clone(),
                total_ticks: 5,
                then: Box::new(TaskKind::Survey {
                    site: site_id.clone(),
                }),
            },
        },
    };

    // Tick 0: assign transit.
    tick(
        &mut state,
        &[transit_cmd],
        &content,
        &mut rng,
        EventLevel::Normal,
    );
    assert_eq!(
        state.ships[&ship_id].location_node, node_a,
        "ship still at origin during transit"
    );

    // Ticks 1–4: transit in progress, ship still at node_a.
    for _ in 1..5 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }
    assert_eq!(
        state.ships[&ship_id].location_node, node_a,
        "ship still in transit"
    );

    // Tick 5: transit resolves → ship moves to node_b, survey starts.
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    assert_eq!(
        state.ships[&ship_id].location_node, node_b,
        "ship arrived at destination"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ShipArrived { node, .. } if node == &node_b)),
        "ShipArrived event should be emitted"
    );
    let survey_started = events.iter().any(|e| {
        matches!(&e.event,
            Event::TaskStarted { task_kind, .. } if task_kind == "Survey"
        )
    });
    assert!(
        survey_started,
        "Survey task should start immediately after arrival"
    );

    // Tick 6: survey resolves → asteroid discovered.
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::AsteroidDiscovered { .. })),
        "AsteroidDiscovered after survey completes"
    );
}

// --- Module stall / resume -----------------------------------------------

#[test]
fn test_refinery_stalls_when_station_full() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Recipe consumes 500 kg ore (70% Fe, 30% Si):
    // Material: 500 * 0.7 = 350 kg Fe at density 7874 -> ~0.0444 m³
    // Slag: (500 - 350) * 1.0 = 150 kg at density 2500 -> 0.06 m³
    // Total output ~0.104 m³
    // Existing ore: 1000 kg at density 3000 -> ~0.333 m³
    // Set capacity so there is no room for output (but ore already fits)
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.34;

    let mut rng = make_rng();
    // Tick 1: timer increments to 1 (interval is 2)
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    // Tick 2: timer reaches 2, processor tries to run -> should stall
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled, "module should be stalled when output won't fit");
        assert_eq!(ps.ticks_since_last_run, 0, "timer should reset on stall");
    } else {
        panic!("expected processor state");
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::ModuleStalled { .. })),
        "ModuleStalled event should be emitted"
    );

    // No material or slag should exist (only the original ore)
    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Material { .. })),
        "no material should be produced when stalled"
    );
    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Slag { .. })),
        "no slag should be produced when stalled"
    );
}

#[test]
fn test_refinery_resumes_after_stall_cleared() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Cause stall first
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.34;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Confirm stalled
    if let ModuleKindState::Processor(ps) = &state.stations[&station_id].modules[0].kind_state {
        assert!(ps.stalled);
    }

    // Increase capacity so output fits
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 10_000.0;

    // Timer was reset to 0 on stall, need another full interval (2 ticks)
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Module should have resumed and produced output
    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(!ps.stalled, "module should no longer be stalled");
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::ModuleResumed { .. })),
        "ModuleResumed event should be emitted"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "RefineryRan should fire after resuming"
    );
}

#[test]
fn test_stall_event_only_emitted_once() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.34;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events1 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // First stall should emit event
    let stall_count_1 = events1
        .iter()
        .filter(|e| matches!(e.event, Event::ModuleStalled { .. }))
        .count();
    assert_eq!(stall_count_1, 1);

    // Tick through another interval while still stalled
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events2 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Second stall attempt should NOT emit event (already stalled)
    let stall_count_2 = events2
        .iter()
        .filter(|e| matches!(e.event, Event::ModuleStalled { .. }))
        .count();
    assert_eq!(
        stall_count_2, 0,
        "ModuleStalled should not be re-emitted while already stalled"
    );
}

#[test]
fn test_storage_pressure_cascade() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // The capacity pre-check computes current_used + output_volume (without
    // subtracting the about-to-be-consumed ore). With 1000 kg ore the first
    // check sees 0.333 m³ (ore) + 0.104 m³ (output) = 0.437 m³.
    // After the first run completes, inventory is ~0.271 m³. We then inject
    // extra material so that the second run's check (current + 0.104) exceeds
    // capacity, triggering a stall.
    //
    // Set capacity to 0.50 m³ — comfortably passes the first run (0.437 < 0.50).
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.50;

    let mut rng = make_rng();

    // Tick 1: timer increments (interval=2)
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    // Tick 2: first refinery run should succeed
    let events_run1 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events_run1
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "first refinery run should succeed"
    );

    let station = &state.stations[&station_id];
    assert!(
        station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Material { .. })),
        "material should exist after first run"
    );

    // After first run, station has ~0.271 m³ used. Add extra material to push
    // it close to capacity. We need current_used + 0.104 > 0.50, so we need
    // current_used > 0.396. Currently at ~0.271, add ~0.15 m³ of Fe
    // (0.15 m³ * 7874 kg/m³ ≈ 1181 kg Fe).
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 1200.0,
            quality: 0.5,
        });

    // Ticks 3-4: second interval
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events_run2 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Second run should STALL
    assert!(
        !events_run2
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "second refinery run should NOT happen (stalled)"
    );
    assert!(
        events_run2
            .iter()
            .any(|e| matches!(e.event, Event::ModuleStalled { .. })),
        "ModuleStalled should be emitted"
    );

    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled, "module should be stalled");
    }
}

// --- Full gameplay cycle integration test --------------------------------

#[test]
fn test_full_survey_deepscan_mine_deposit_cycle() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    let station_id = StationId("station_earth_orbit".to_string());
    let owner = state.ships[&ship_id].owner.clone();

    // --- Phase 1: Survey ---
    // Issue a survey command and tick until the asteroid is created.
    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Debug);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    assert_eq!(
        state.asteroids.len(),
        1,
        "one asteroid should exist after survey"
    );
    assert!(
        !state.scan_sites.iter().any(|s| s.id.0 == "site_0001"),
        "original scan site should be consumed"
    );
    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    assert!(
        (state.asteroids[&asteroid_id].mass_kg - 500.0).abs() < 1e-3,
        "asteroid mass should be 500 kg"
    );

    // Ship should be idle after survey completion.
    assert!(
        matches!(
            &state.ships[&ship_id].task,
            Some(task) if matches!(task.kind, TaskKind::Idle)
        ),
        "ship should be idle after survey"
    );

    // --- Phase 2: Research unlocks deep scan ---
    // difficulty=10, survey gave 5 ScanData. Research auto-advances each tick.
    // Tick until tech_deep_scan_v1 is unlocked (should happen quickly with difficulty=10).
    let tech_id = TechId("tech_deep_scan_v1".to_string());
    let mut unlocked = false;
    for _ in 0..100 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);
        if state.research.unlocked.contains(&tech_id) {
            unlocked = true;
            break;
        }
    }
    assert!(unlocked, "tech_deep_scan_v1 should unlock within 100 ticks");

    // --- Phase 3: Deep Scan ---
    assert!(
        state.asteroids[&asteroid_id]
            .knowledge
            .composition
            .is_none(),
        "composition should be unknown before deep scan"
    );

    let deep_cmd = CommandEnvelope {
        id: CommandId(format!("cmd_{:06}", state.counters.next_command_id)),
        issued_by: owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::DeepScan {
                asteroid: asteroid_id.clone(),
            },
        },
    };
    tick(
        &mut state,
        &[deep_cmd],
        &content,
        &mut rng,
        EventLevel::Debug,
    );
    // deep_scan_ticks=1, so one more tick resolves it.
    tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    let composition = state.asteroids[&asteroid_id].knowledge.composition.as_ref();
    assert!(
        composition.is_some(),
        "composition should be known after deep scan"
    );
    // With sigma=0, mapped composition should match true composition exactly.
    let mapped = composition.unwrap();
    for (element, &true_val) in &state.asteroids[&asteroid_id].true_composition {
        let mapped_val = mapped.get(element).copied().unwrap_or(0.0);
        assert!(
            (mapped_val - true_val).abs() < 1e-5,
            "mapped {element} should match true composition (sigma=0)"
        );
    }

    // --- Phase 4: Mine ---
    // 500 kg at 50 kg/tick = 10 ticks duration.
    let duration_ticks = (state.asteroids[&asteroid_id].mass_kg
        / content.constants.mining_rate_kg_per_tick)
        .ceil() as u64;
    assert_eq!(duration_ticks, 10, "mining should take 10 ticks");

    let mine_cmd = CommandEnvelope {
        id: CommandId(format!("cmd_{:06}", state.counters.next_command_id + 1)),
        issued_by: owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Mine {
                asteroid: asteroid_id.clone(),
                duration_ticks,
            },
        },
    };
    tick(
        &mut state,
        &[mine_cmd],
        &content,
        &mut rng,
        EventLevel::Debug,
    );

    // Tick through the mining duration.
    for _ in 0..duration_ticks {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);
    }

    // Ship should now have ore in its inventory.
    let ship_ore_kg: f32 = state.ships[&ship_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        ship_ore_kg > 0.0,
        "ship should have ore after mining, got {ship_ore_kg} kg"
    );

    // Asteroid should be depleted (500 kg mined at 50 kg/tick over 10 ticks = fully consumed).
    assert!(
        !state.asteroids.contains_key(&asteroid_id),
        "asteroid should be removed after full depletion"
    );

    // --- Phase 5: Deposit ---
    let deposit_cmd = CommandEnvelope {
        id: CommandId(format!("cmd_{:06}", state.counters.next_command_id + 2)),
        issued_by: owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Deposit {
                station: station_id.clone(),
                blocked: false,
            },
        },
    };
    tick(
        &mut state,
        &[deposit_cmd],
        &content,
        &mut rng,
        EventLevel::Debug,
    );
    // deposit_ticks=1, so one more tick resolves it.
    tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    // Ship inventory should be empty.
    assert!(
        state.ships[&ship_id].inventory.is_empty(),
        "ship inventory should be empty after deposit"
    );

    // Station should now have the ore.
    let station_ore_kg: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        station_ore_kg > 0.0,
        "station should have ore after deposit, got {station_ore_kg} kg"
    );
    assert!(
        (station_ore_kg - ship_ore_kg).abs() < 1e-3,
        "station ore ({station_ore_kg} kg) should match what the ship had ({ship_ore_kg} kg)"
    );
}

// --- Deposit blocking / unblocking ----------------------------------------

#[test]
fn test_deposit_ship_waits_when_station_full() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_block_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 500.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Ship should still be in Deposit task, NOT idle.
    let ship = &state.ships[&ship_id];
    assert!(
        matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Deposit { .. })),
        "ship should stay in Deposit task when station is full"
    );
    assert!(
        !ship.inventory.is_empty(),
        "ship should retain ore when station is full"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::DepositBlocked { .. })),
        "DepositBlocked event should be emitted"
    );
}

#[test]
fn test_deposit_unblocks_when_space_opens() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_unblock_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 100.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal); // blocked

    // Now open capacity.
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 10_000.0;
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Ship should have deposited and gone idle.
    assert!(
        state.ships[&ship_id].inventory.is_empty(),
        "ship should have deposited ore after space opened"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::DepositUnblocked { .. })),
        "DepositUnblocked event should be emitted"
    );
}

#[test]
fn test_deposit_blocked_event_only_emitted_once() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_dedup_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 500.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events1 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events2 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let count1 = events1
        .iter()
        .filter(|e| matches!(e.event, Event::DepositBlocked { .. }))
        .count();
    let count2 = events2
        .iter()
        .filter(|e| matches!(e.event, Event::DepositBlocked { .. }))
        .count();
    assert_eq!(count1, 1, "first tick should emit DepositBlocked");
    assert_eq!(count2, 0, "second tick should NOT re-emit DepositBlocked");
}
