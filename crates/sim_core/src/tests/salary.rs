use super::*;
use crate::test_fixtures::{base_content, base_state, make_rng, test_station_id};
use std::collections::BTreeMap;

/// Content with two crew roles at known salaries.
fn salary_content() -> GameContent {
    let mut content = base_content();
    content.crew_roles.insert(
        CrewRole("operator".to_string()),
        CrewRoleDef {
            id: CrewRole("operator".to_string()),
            name: "Operator".to_string(),
            recruitment_cost: 50_000.0,
            salary_per_hour: 60.0, // $1/minute → easy math with minutes_per_tick=1
        },
    );
    content.crew_roles.insert(
        CrewRole("scientist".to_string()),
        CrewRoleDef {
            id: CrewRole("scientist".to_string()),
            name: "Scientist".to_string(),
            recruitment_cost: 80_000.0,
            salary_per_hour: 120.0, // $2/minute
        },
    );
    content
}

fn salary_state(content: &GameContent, crew: BTreeMap<CrewRole, u32>, balance: f64) -> GameState {
    let mut state = base_state(content);
    state.balance = balance;
    let station = state.stations.get_mut(&test_station_id()).unwrap();
    station.crew = crew;
    state
}

// ---------------------------------------------------------------------------
// 1. Basic salary deduction
// ---------------------------------------------------------------------------

#[test]
fn crew_salary_deduction() {
    // 2 operators at $60/hr, minutes_per_tick=1 → hours_per_tick = 1/60
    // Per tick: 60 * 2 * (1/60) = $2.00
    let content = salary_content();
    let crew = BTreeMap::from([(CrewRole("operator".to_string()), 2)]);
    let mut state = salary_state(&content, crew, 1000.0);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

    let expected = 1000.0 - 2.0;
    assert!(
        (state.balance - expected).abs() < 0.01,
        "balance should be ~{expected}, got {}",
        state.balance
    );
}

// ---------------------------------------------------------------------------
// 2. Bankruptcy event on zero-crossing
// ---------------------------------------------------------------------------

#[test]
fn station_bankrupt_event() {
    // 2 operators: $2/tick. Start with $1.50 → goes negative after 1 tick.
    let content = salary_content();
    let crew = BTreeMap::from([(CrewRole("operator".to_string()), 2)]);
    let mut state = salary_state(&content, crew, 1.50);
    let mut rng = make_rng();

    let events = tick(&mut state, &[], &content, &mut rng, None);

    assert!(
        state.balance < 0.0,
        "balance should be negative, got {}",
        state.balance
    );
    let bankrupt_count = events
        .iter()
        .filter(|e| matches!(&e.event, Event::StationBankrupt { .. }))
        .count();
    assert_eq!(
        bankrupt_count, 1,
        "should emit exactly one StationBankrupt event"
    );

    // Second tick while already bankrupt: no additional bankrupt event
    let events2 = tick(&mut state, &[], &content, &mut rng, None);
    let bankrupt_count2 = events2
        .iter()
        .filter(|e| matches!(&e.event, Event::StationBankrupt { .. }))
        .count();
    assert_eq!(
        bankrupt_count2, 0,
        "should NOT re-emit StationBankrupt when already negative"
    );
}

// ---------------------------------------------------------------------------
// 3. Multi-role salary accumulation
// ---------------------------------------------------------------------------

#[test]
fn multi_role_salary_accumulation() {
    // 2 operators ($60/hr) + 1 scientist ($120/hr)
    // Per tick: (60*2 + 120*1) * (1/60) = 240/60 = $4.00
    let content = salary_content();
    let crew = BTreeMap::from([
        (CrewRole("operator".to_string()), 2),
        (CrewRole("scientist".to_string()), 1),
    ]);
    let mut state = salary_state(&content, crew, 1000.0);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

    let expected = 1000.0 - 4.0;
    assert!(
        (state.balance - expected).abs() < 0.01,
        "balance should be ~{expected}, got {}",
        state.balance
    );
}

// ---------------------------------------------------------------------------
// 4. No salary when no crew
// ---------------------------------------------------------------------------

#[test]
fn no_salary_when_no_crew() {
    let content = salary_content();
    let mut state = salary_state(&content, BTreeMap::new(), 1000.0);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

    assert!(
        (state.balance - 1000.0).abs() < 0.01,
        "balance should be unchanged with no crew, got {}",
        state.balance
    );
}

// ---------------------------------------------------------------------------
// 5. Hours-per-tick conversion
// ---------------------------------------------------------------------------

#[test]
fn hours_per_tick_conversion() {
    // Override minutes_per_tick to 120 → hours_per_tick = 2.0
    // 1 operator at $60/hr → $120/tick
    let mut content = salary_content();
    content.constants.minutes_per_tick = 120;

    let crew = BTreeMap::from([(CrewRole("operator".to_string()), 1)]);
    let mut state = salary_state(&content, crew, 10_000.0);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

    let expected = 10_000.0 - 120.0;
    assert!(
        (state.balance - expected).abs() < 0.01,
        "balance should be ~{expected} with 120 min/tick, got {}",
        state.balance
    );
}
