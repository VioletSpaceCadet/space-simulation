use super::*;

// NOTE: advance_research is currently stubbed (returns early, does nothing).
// These tests will be rewritten when the domain-sufficiency model is implemented in Task 4.
// For now, we just verify the stub doesn't panic and doesn't produce events.

#[test]
fn test_stubbed_research_does_not_panic() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    // Should not panic
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
}

#[test]
fn test_stubbed_research_no_power_consumed() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    // Stubbed research should not emit PowerConsumed events
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, Event::PowerConsumed { .. })),
        "stubbed advance_research should not emit PowerConsumed"
    );
}

#[test]
fn test_stubbed_research_no_research_roll() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, Event::ResearchRoll { .. })),
        "stubbed advance_research should not emit ResearchRoll"
    );
}
