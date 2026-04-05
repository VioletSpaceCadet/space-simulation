use super::*;

// Tests for deterministic threshold-based research unlock via the full tick() path.

#[test]
fn test_research_does_not_panic() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    // Should not panic
    tick(&mut state, &[], &content, &mut rng, None);
}

#[test]
fn test_research_unlocks_when_requirements_met_via_tick() {
    let mut content = test_content();
    // No domain requirements = unlocks immediately
    content.techs[0].domain_requirements = std::collections::HashMap::new();

    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, None);

    assert!(
        state
            .research
            .unlocked
            .contains(&TechId("tech_deep_scan_v1".to_string())),
        "tech with no domain requirements should unlock on first tick"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::TechUnlocked { .. })),
        "should emit TechUnlocked event"
    );
}

#[test]
fn test_research_no_unlock_when_requirements_unmet() {
    let mut content = test_content();
    content.techs[0].domain_requirements = std::collections::HashMap::from([(
        ResearchDomain::new(ResearchDomain::SURVEY),
        1_000_000.0,
    )]);

    let mut state = test_state(&content);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, None);

    assert!(
        !state
            .research
            .unlocked
            .contains(&TechId("tech_deep_scan_v1".to_string())),
        "tech should NOT unlock when requirements are not met"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::TechUnlocked { .. })),
        "should not emit TechUnlocked event"
    );
}
