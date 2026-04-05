//! Satellite tick behavior — per-type evaluation and zone effects.
//!
//! Called as a top-level tick step after ground facilities and before research.
//! Iterates satellites sorted by `SatelliteId` (`BTreeMap` order) for determinism.

use crate::{
    research::generate_data, DataKind, Event, EventEnvelope, GameContent, GameState, SatelliteId,
    SiteId,
};
use rand::Rng;

/// Tick all deployed satellites. Skips disabled or worn-out satellites.
/// Each satellite type dispatches to its own behavior via string match.
pub(crate) fn tick_satellites(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let satellite_ids: Vec<SatelliteId> = state.satellites.keys().cloned().collect();

    for satellite_id in satellite_ids {
        let Some(satellite) = state.satellites.get(&satellite_id) else {
            continue;
        };

        if !satellite.enabled || satellite.wear >= 1.0 {
            continue;
        }

        let def_id = satellite.def_id.clone();
        let satellite_type = satellite.satellite_type.clone();
        let Some(def) = content.satellite_defs.get(&def_id) else {
            continue;
        };
        let wear_rate = def.wear_rate;

        match satellite_type.as_str() {
            "survey" => tick_survey_satellite(state, content, rng, events, &def_id),
            "science_platform" => tick_science_satellite(state, content, events, &def_id),
            // Communication and navigation satellites produce zone-level effects
            // computed lazily by downstream systems (VIO-569, VIO-570).
            _ => {}
        }

        // Accumulate wear.
        if let Some(sat) = state.satellites.get_mut(&satellite_id) {
            sat.wear = (sat.wear + wear_rate).min(1.0);
        }
    }
}

/// Survey satellite: discover scan sites at 2x the rate of ground sensors.
fn tick_survey_satellite(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
    def_id: &str,
) {
    let Some(def) = content.satellite_defs.get(def_id) else {
        return;
    };
    let multiplier = def
        .behavior_config
        .get("discovery_multiplier")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(2.0);

    let at_cap = state.scan_sites.len() >= content.constants.replenish_target_count as usize;
    if at_cap {
        return;
    }

    // Use a base probability derived from content constants or a reasonable default.
    // Survey satellites roll once per tick with probability scaled by multiplier.
    let base_prob = 0.05; // 5% base per tick
    let prob = (base_prob * multiplier).min(1.0);
    let roll: f64 = rng.gen();
    if roll >= prob {
        return;
    }

    // Pick a random zone and create a scan site.
    let zone_bodies: Vec<&crate::OrbitalBodyDef> = content
        .solar_system
        .bodies
        .iter()
        .filter(|b| b.zone.is_some())
        .collect();

    if zone_bodies.is_empty() || content.asteroid_templates.is_empty() {
        return;
    }

    let body = crate::pick_zone_weighted(&zone_bodies, rng);
    let zone_class = body.zone.as_ref().expect("zone body").resource_class;
    let template = crate::pick_template_biased(&content.asteroid_templates, zone_class, rng);
    let position = crate::random_position_in_zone(body, rng);
    let uuid = crate::generate_uuid(rng);
    let site_id = SiteId(format!("site_{uuid}"));
    let current_tick = state.meta.tick;

    state.scan_sites.push(crate::ScanSite {
        id: site_id.clone(),
        position: position.clone(),
        template_id: template.id.clone(),
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ScanSiteSpawned {
            site_id,
            position,
            template_id: template.id.clone(),
        },
    ));
}

/// Science platform: generate data with an orbital multiplier (3-5x ground rate).
fn tick_science_satellite(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
    def_id: &str,
) {
    let Some(def) = content.satellite_defs.get(def_id) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)] // config value; truncation harmless
    let multiplier = def
        .behavior_config
        .get("data_multiplier")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(4.0) as f32;

    // Science platforms generate orbital observation data.
    let data_kind = DataKind::new(DataKind::OPTICAL);
    let action_key = format!("satellite_{def_id}");

    let base_amount = generate_data(
        &mut state.research,
        data_kind.clone(),
        &action_key,
        &content.constants,
    );

    // Apply the orbital multiplier as bonus data on top of base generation.
    let bonus = base_amount * (multiplier - 1.0);
    if bonus > 0.0 {
        state
            .research
            .data_pool
            .entry(data_kind.clone())
            .and_modify(|v| *v += bonus)
            .or_insert(bonus);
    }

    let total = base_amount + bonus;
    let current_tick = state.meta.tick;
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: data_kind,
            amount: total,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{base_content, base_state, test_position};
    use crate::{SatelliteDef, SatelliteState, TechId};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn make_rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(42)
    }

    fn add_satellite(state: &mut GameState, def_id: &str, satellite_type: &str) -> SatelliteId {
        let id = SatelliteId(format!("sat_{def_id}"));
        state.satellites.insert(
            id.clone(),
            SatelliteState {
                id: id.clone(),
                def_id: def_id.to_string(),
                name: format!("Test {satellite_type}"),
                position: test_position(),
                deployed_tick: 0,
                wear: 0.0,
                enabled: true,
                satellite_type: satellite_type.to_string(),
                payload_config: None,
            },
        );
        id
    }

    fn add_satellite_def(
        content: &mut GameContent,
        id: &str,
        satellite_type: &str,
        wear_rate: f64,
    ) {
        content.satellite_defs.insert(
            id.to_string(),
            SatelliteDef {
                id: id.to_string(),
                name: format!("Test {satellite_type}"),
                satellite_type: satellite_type.to_string(),
                mass_kg: 500.0,
                wear_rate,
                required_tech: None,
                behavior_config: match satellite_type {
                    "survey" => serde_json::json!({ "discovery_multiplier": 2.0 }),
                    "science_platform" => serde_json::json!({ "data_multiplier": 4.0 }),
                    "communication" => serde_json::json!({ "comm_tier": "Basic" }),
                    "navigation" => serde_json::json!({ "transit_reduction_pct": 15.0 }),
                    _ => serde_json::json!({}),
                },
            },
        );
    }

    #[test]
    fn disabled_satellite_skipped() {
        let mut content = base_content();
        add_satellite_def(&mut content, "sat_comm", "communication", 0.001);
        let mut state = base_state(&content);
        let id = add_satellite(&mut state, "sat_comm", "communication");
        state.satellites.get_mut(&id).unwrap().enabled = false;

        let initial_wear = state.satellites[&id].wear;
        let mut rng = make_rng();
        let mut events = Vec::new();
        tick_satellites(&mut state, &content, &mut rng, &mut events);

        // Disabled satellites should not accumulate wear.
        assert!(
            (state.satellites[&id].wear - initial_wear).abs() < f64::EPSILON,
            "disabled satellite should not accumulate wear"
        );
    }

    #[test]
    fn worn_out_satellite_skipped() {
        let mut content = base_content();
        add_satellite_def(&mut content, "sat_comm", "communication", 0.001);
        let mut state = base_state(&content);
        let id = add_satellite(&mut state, "sat_comm", "communication");
        state.satellites.get_mut(&id).unwrap().wear = 1.0;

        let mut rng = make_rng();
        let mut events = Vec::new();
        tick_satellites(&mut state, &content, &mut rng, &mut events);

        // Worn-out satellites stay at 1.0.
        assert!(
            (state.satellites[&id].wear - 1.0).abs() < f64::EPSILON,
            "worn-out satellite wear should stay at 1.0"
        );
    }

    #[test]
    fn wear_accumulates_per_tick() {
        let mut content = base_content();
        let wear_rate = 0.01;
        add_satellite_def(&mut content, "sat_nav", "navigation", wear_rate);
        let mut state = base_state(&content);
        let id = add_satellite(&mut state, "sat_nav", "navigation");

        let mut rng = make_rng();
        let mut events = Vec::new();

        // Run 10 ticks.
        for _ in 0..10 {
            tick_satellites(&mut state, &content, &mut rng, &mut events);
        }

        let expected = wear_rate * 10.0;
        assert!(
            (state.satellites[&id].wear - expected).abs() < f64::EPSILON * 100.0,
            "wear should be ~{expected}, got {}",
            state.satellites[&id].wear
        );
    }

    #[test]
    fn science_platform_generates_data() {
        let mut content = base_content();
        add_satellite_def(&mut content, "sat_sci", "science_platform", 0.0001);
        let mut state = base_state(&content);
        add_satellite(&mut state, "sat_sci", "science_platform");

        let mut rng = make_rng();
        let mut events = Vec::new();
        tick_satellites(&mut state, &content, &mut rng, &mut events);

        // Should have generated DataGenerated event.
        let data_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event, Event::DataGenerated { .. }))
            .collect();
        assert!(
            !data_events.is_empty(),
            "science platform should generate DataGenerated event"
        );

        // Should have data in the pool.
        let optical_key = DataKind::new(DataKind::OPTICAL);
        let pool_amount = state
            .research
            .data_pool
            .get(&optical_key)
            .copied()
            .unwrap_or(0.0);
        assert!(pool_amount > 0.0, "optical data should be in pool");
    }

    #[test]
    fn survey_satellite_respects_cap() {
        let mut content = base_content();
        add_satellite_def(&mut content, "sat_survey", "survey", 0.0001);
        let mut state = base_state(&content);
        add_satellite(&mut state, "sat_survey", "survey");

        // Fill to cap.
        let cap = content.constants.replenish_target_count as usize;
        while state.scan_sites.len() < cap {
            state.scan_sites.push(crate::ScanSite {
                id: SiteId(format!("site_fill_{}", state.scan_sites.len())),
                position: test_position(),
                template_id: "tmpl_iron_rich".to_string(),
            });
        }

        let sites_before = state.scan_sites.len();
        let mut rng = make_rng();
        let mut events = Vec::new();

        // Run many ticks — should not spawn any new sites.
        for _ in 0..100 {
            tick_satellites(&mut state, &content, &mut rng, &mut events);
        }

        assert_eq!(
            state.scan_sites.len(),
            sites_before,
            "should not exceed scan site cap"
        );
    }

    /// Ensure base_content has zone bodies for survey satellite discovery.
    fn add_zone_to_content(content: &mut GameContent) {
        use crate::{AnomalyTag, AsteroidTemplateDef, BodyId, BodyType, OrbitalBodyDef, ZoneDef};
        // Add a body with a zone.
        content.solar_system.bodies.push(OrbitalBodyDef {
            id: BodyId("zone_body".to_string()),
            name: "Zone Body".to_string(),
            parent: None,
            body_type: BodyType::Belt,
            radius_au_um: 1_000_000,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: Some(ZoneDef {
                radius_min_au_um: 900_000,
                radius_max_au_um: 1_100_000,
                angle_start_mdeg: 0,
                angle_span_mdeg: 360_000,
                resource_class: crate::spatial::ResourceClass::MetalRich,
                scan_site_weight: 10,
            }),
        });
        // Ensure we have at least one template.
        if content.asteroid_templates.is_empty() {
            content.asteroid_templates.push(AsteroidTemplateDef {
                id: "tmpl_test".to_string(),
                anomaly_tags: vec![AnomalyTag::new("IronRich")],
                composition_ranges: std::collections::HashMap::from([(
                    "Fe".to_string(),
                    (0.5, 0.9),
                )]),
                preferred_class: Some(crate::spatial::ResourceClass::MetalRich),
            });
        }
    }

    #[test]
    fn integration_all_types_over_500_ticks() {
        let mut content = base_content();
        add_zone_to_content(&mut content);
        add_satellite_def(&mut content, "sat_survey", "survey", 0.00015);
        add_satellite_def(&mut content, "sat_comm", "communication", 0.00008);
        add_satellite_def(&mut content, "sat_nav", "navigation", 0.0001);
        add_satellite_def(&mut content, "sat_sci", "science_platform", 0.00012);

        let mut state = base_state(&content);
        add_satellite(&mut state, "sat_survey", "survey");
        add_satellite(&mut state, "sat_comm", "communication");
        add_satellite(&mut state, "sat_nav", "navigation");
        add_satellite(&mut state, "sat_sci", "science_platform");

        let mut rng = make_rng();
        let mut all_events = Vec::new();

        for _ in 0..500 {
            let mut events = Vec::new();
            tick_satellites(&mut state, &content, &mut rng, &mut events);
            all_events.extend(events);
        }

        // All satellites should have accumulated wear.
        for sat in state.satellites.values() {
            assert!(sat.wear > 0.0, "{} should have wear > 0", sat.id);
            assert!(sat.wear < 1.0, "{} should not have failed yet", sat.id);
        }

        // Science platform should have generated data.
        let data_events = all_events
            .iter()
            .filter(|e| matches!(e.event, Event::DataGenerated { .. }))
            .count();
        assert!(
            data_events > 0,
            "science platform should produce data events over 500 ticks"
        );

        // Survey satellite should have discovered some scan sites (probabilistic, but
        // over 500 ticks with 10% effective probability it's near-certain).
        let spawn_events = all_events
            .iter()
            .filter(|e| matches!(e.event, Event::ScanSiteSpawned { .. }))
            .count();
        assert!(
            spawn_events > 0,
            "survey satellite should discover sites over 500 ticks"
        );
    }
}
