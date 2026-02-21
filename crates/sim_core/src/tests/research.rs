use super::*;

#[test]
fn test_research_evidence_accumulates_each_tick() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(999);

    let tech_id = TechId("tech_deep_scan_v1".to_string());

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
    let content = test_content();
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
