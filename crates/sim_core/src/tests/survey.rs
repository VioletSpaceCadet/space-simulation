use super::*;

#[test]
fn test_survey_creates_asteroid_on_completion() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    assert!(
        state.asteroids.is_empty(),
        "asteroid should not exist before task completes"
    );

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
    assert!(
        (asteroid.mass_kg - 500.0).abs() < 1e-3,
        "mass should be 500.0 in test content (fixed range)"
    );
}

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
