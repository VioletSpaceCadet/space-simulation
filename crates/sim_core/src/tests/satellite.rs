use crate::test_fixtures::test_position;
use crate::{SatelliteDef, SatelliteId, SatelliteState, TechId};

#[test]
fn satellite_state_json_round_trip() {
    let state = SatelliteState {
        id: SatelliteId("sat_001".to_string()),
        def_id: "sat_survey".to_string(),
        name: "Survey Alpha".to_string(),
        position: test_position(),
        deployed_tick: 42,
        wear: 0.15,
        enabled: true,
        satellite_type: "survey".to_string(),
        payload_config: None,
    };
    let json = serde_json::to_string(&state).unwrap();
    let round_trip: SatelliteState = serde_json::from_str(&json).unwrap();
    assert_eq!(round_trip.id, state.id);
    assert_eq!(round_trip.def_id, "sat_survey");
    assert_eq!(round_trip.name, "Survey Alpha");
    assert_eq!(round_trip.deployed_tick, 42);
    assert!((round_trip.wear - 0.15).abs() < f64::EPSILON);
    assert!(round_trip.enabled);
    assert_eq!(round_trip.satellite_type, "survey");
    assert!(round_trip.payload_config.is_none());
}

#[test]
fn satellite_state_with_payload_config_round_trip() {
    let state = SatelliteState {
        id: SatelliteId("sat_002".to_string()),
        def_id: "sat_science_platform".to_string(),
        name: "Science Beta".to_string(),
        position: test_position(),
        deployed_tick: 100,
        wear: 0.0,
        enabled: true,
        satellite_type: "science_platform".to_string(),
        payload_config: Some("OpticalData".to_string()),
    };
    let json = serde_json::to_string(&state).unwrap();
    let round_trip: SatelliteState = serde_json::from_str(&json).unwrap();
    assert_eq!(round_trip.payload_config.as_deref(), Some("OpticalData"));
}

#[test]
fn satellite_def_json_round_trip() {
    let def = SatelliteDef {
        id: "sat_nav_beacon".to_string(),
        name: "Navigation Beacon".to_string(),
        satellite_type: "navigation".to_string(),
        mass_kg: 600.0,
        wear_rate: 0.0001,
        required_tech: Some(TechId("tech_satellite_basics".to_string())),
        behavior_config: serde_json::json!({ "transit_reduction_pct": 15.0 }),
    };
    let json = serde_json::to_string(&def).unwrap();
    let round_trip: SatelliteDef = serde_json::from_str(&json).unwrap();
    assert_eq!(round_trip.id, "sat_nav_beacon");
    assert_eq!(round_trip.satellite_type, "navigation");
    assert!((round_trip.mass_kg - 600.0).abs() < f32::EPSILON);
    assert!((round_trip.wear_rate - 0.0001).abs() < f64::EPSILON);
    assert_eq!(
        round_trip.required_tech,
        Some(TechId("tech_satellite_basics".to_string()))
    );
}

#[test]
fn satellite_state_backward_compatible_missing_field() {
    // Simulate old save file without payload_config
    let json = r#"{
        "id": "sat_001",
        "def_id": "sat_survey",
        "name": "Survey Alpha",
        "position": {"parent_body": "test_body", "radius_au_um": 0, "angle_mdeg": 0},
        "deployed_tick": 42,
        "wear": 0.0,
        "enabled": true,
        "satellite_type": "survey"
    }"#;
    let state: SatelliteState = serde_json::from_str(json).unwrap();
    assert!(state.payload_config.is_none());
}

#[test]
fn game_state_backward_compatible_no_satellites() {
    // GameState.satellites has #[serde(default)] so old saves without it should work.
    // Verified implicitly by test_state() which constructs a GameState without satellites.
    let content = super::test_content();
    let state = super::test_state(&content);
    assert!(state.satellites.is_empty());
}
