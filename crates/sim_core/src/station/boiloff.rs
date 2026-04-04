use crate::{
    boiloff_rate_per_tick, BoiloffCurveDef, Event, EventEnvelope, GameContent, GameState,
    InventoryItem, StationId,
};

/// Piecewise linear temperature multiplier for boiloff.
/// - At or below boiling point: `cold_multiplier` (default 0.1x)
/// - Boiling point → ambient: linear ramp `cold_multiplier` → `ambient_multiplier`
/// - Ambient → ambient + `hot_offset`: linear ramp `ambient_multiplier` → `hot_multiplier`
/// - Above ambient + `hot_offset`: clamped at `hot_multiplier`
fn boiloff_temp_multiplier(
    temp_mk: u32,
    boiling_point_mk: u32,
    ambient_mk: u32,
    hot_offset_mk: u32,
    curve: &BoiloffCurveDef,
) -> f64 {
    let t_amb = ambient_mk;
    let t_hot = t_amb + hot_offset_mk;
    if temp_mk <= boiling_point_mk {
        curve.cold_multiplier
    } else if temp_mk <= t_amb {
        let frac = f64::from(temp_mk - boiling_point_mk) / f64::from(t_amb - boiling_point_mk);
        curve.cold_multiplier + (curve.ambient_multiplier - curve.cold_multiplier) * frac
    } else if temp_mk <= t_hot {
        let frac = f64::from(temp_mk - t_amb) / f64::from(hot_offset_mk);
        curve.ambient_multiplier + (curve.hot_multiplier - curve.ambient_multiplier) * frac
    } else {
        curve.hot_multiplier
    }
}

/// Step 3.7: Apply boiloff to cryogenic materials in station inventories.
///
/// Runs AFTER the thermal tick (step 3.6). Cooling installed this tick
/// helps immediately — Contract A ("Boiloff uses end-of-tick temperatures").
pub(super) fn apply_boiloff(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let minutes_per_tick = content.constants.minutes_per_tick;
    let current_tick = state.meta.tick;

    // Resolve global boiloff rate modifier (e.g. tech_cryo_insulation).
    let boiloff_rate_mult = state
        .modifiers
        .resolve_f32(crate::modifiers::StatId::BoiloffRate, 1.0);

    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };

    let mut losses: Vec<(String, f32)> = Vec::new();
    let default_curve = BoiloffCurveDef::default();

    for item in &mut station.core.inventory {
        let InventoryItem::Material {
            element,
            kg,
            thermal,
            ..
        } = item
        else {
            continue;
        };

        let element_def = content.elements.iter().find(|e| &e.id == element);
        let Some(rate_per_day) = element_def.and_then(|e| e.boiloff_rate_per_day_at_293k) else {
            continue;
        };

        let base_rate = boiloff_rate_per_tick(rate_per_day, minutes_per_tick);

        // Temperature scaling: use material thermal state if available, else ambient (1.0x)
        let multiplier = match (
            thermal.as_ref(),
            element_def.and_then(|e| e.boiling_point_mk),
        ) {
            (Some(mat_thermal), Some(bp_mk)) => {
                let curve = element_def
                    .and_then(|e| e.boiloff_curve.as_ref())
                    .unwrap_or(&default_curve);
                boiloff_temp_multiplier(
                    mat_thermal.temp_mk,
                    bp_mk,
                    content.constants.thermal_sink_temp_mk,
                    content.constants.boiloff_hot_offset_mk,
                    curve,
                )
            }
            _ => 1.0,
        };

        #[allow(clippy::cast_possible_truncation)]
        let loss = ((f64::from(*kg) * base_rate * multiplier * f64::from(boiloff_rate_mult))
            as f32)
            .min(*kg);
        if loss > content.constants.min_meaningful_kg {
            *kg -= loss;
            losses.push((element.clone(), loss));
        }
    }

    // Remove material items below threshold
    let min_kg = content.constants.min_meaningful_kg;
    station.core.inventory.retain(|item| match item {
        InventoryItem::Material { kg, .. } => *kg >= min_kg,
        _ => true,
    });

    if !losses.is_empty() {
        station.invalidate_volume_cache();
    }

    // Emit events
    for (element, kg_lost) in losses {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::BoiloffLoss {
                station_id: station_id.clone(),
                element,
                kg_lost,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{base_content, base_state, make_rng};
    use crate::{tick, ElementDef};

    fn boiloff_content() -> GameContent {
        let mut content = base_content();
        content.elements.push(ElementDef {
            id: "LH2".to_string(),
            density_kg_per_m3: 71.0,
            display_name: "Liquid Hydrogen".to_string(),
            refined_name: Some("LH2".to_string()),
            category: "material".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: Some(0.014),
            boiling_point_mk: Some(20_300),
            boiloff_curve: None,
        });
        content.elements.push(ElementDef {
            id: "LOX".to_string(),
            density_kg_per_m3: 1141.0,
            display_name: "Liquid Oxygen".to_string(),
            refined_name: Some("LOX".to_string()),
            category: "material".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: Some(0.003),
            boiling_point_mk: Some(90_200),
            boiloff_curve: None,
        });
        content
    }

    fn state_with_lh2(content: &GameContent, kg: f32) -> GameState {
        let mut state = base_state(content);
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.core.inventory.push(InventoryItem::Material {
            element: "LH2".to_string(),
            kg,
            quality: 1.0,
            thermal: None,
        });
        state
    }

    #[test]
    fn test_lh2_boiloff_at_ambient() {
        let content = boiloff_content();
        let mut state = state_with_lh2(&content, 10_000.0);
        let mut rng = make_rng();

        let station_id = crate::StationId("station_earth_orbit".to_string());
        let initial_kg = 10_000.0_f32;

        tick(&mut state, &[], &content, &mut rng, None);

        let remaining: f32 = state.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
                _ => None,
            })
            .sum();

        assert!(
            remaining < initial_kg,
            "LH2 should decrease due to boiloff: {remaining} vs {initial_kg}"
        );
        // At ambient, 1.4%/day, 1 tick = 1 min (test fixture), loss should be small
        let lost = initial_kg - remaining;
        assert!(
            lost > 0.0 && lost < initial_kg * 0.01,
            "loss should be small per tick: {lost}"
        );
    }

    #[test]
    fn test_no_boiloff_on_non_cryo_element() {
        let content = boiloff_content();
        let mut state = base_state(&content);
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.core.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 1000.0,
            quality: 1.0,
            thermal: None,
        });

        let mut rng = make_rng();
        tick(&mut state, &[], &content, &mut rng, None);

        let remaining: f32 = state.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();

        assert!(
            (remaining - 1000.0).abs() < 0.01,
            "Fe should not boil off: {remaining}"
        );
    }

    #[test]
    fn test_boiloff_removes_tiny_amounts() {
        let content = boiloff_content();
        let mut state = state_with_lh2(&content, 0.0005);
        let mut rng = make_rng();

        // Run several ticks — tiny amount should be removed
        for _ in 0..100 {
            tick(&mut state, &[], &content, &mut rng, None);
        }

        let station_id = crate::StationId("station_earth_orbit".to_string());
        let remaining: f32 = state.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
                _ => None,
            })
            .sum();

        assert!(
            remaining < 0.001,
            "tiny LH2 amount should be removed after many ticks: {remaining}"
        );
    }

    #[test]
    fn test_boiloff_emits_event() {
        let content = boiloff_content();
        let mut state = state_with_lh2(&content, 10_000.0);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, None);

        let boiloff_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.event, Event::BoiloffLoss { .. }))
            .collect();

        assert!(
            !boiloff_events.is_empty(),
            "should emit BoiloffLoss event for LH2"
        );
    }

    #[test]
    fn test_lox_boils_slower_than_lh2() {
        let content = boiloff_content();
        let mut state = base_state(&content);
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.core.inventory.push(InventoryItem::Material {
            element: "LH2".to_string(),
            kg: 10_000.0,
            quality: 1.0,
            thermal: None,
        });
        station.core.inventory.push(InventoryItem::Material {
            element: "LOX".to_string(),
            kg: 10_000.0,
            quality: 1.0,
            thermal: None,
        });

        let mut rng = make_rng();
        tick(&mut state, &[], &content, &mut rng, None);

        let lh2_remaining: f32 = state.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
                _ => None,
            })
            .sum();

        let lox_remaining: f32 = state.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "LOX" => Some(*kg),
                _ => None,
            })
            .sum();

        let lh2_loss = 10_000.0 - lh2_remaining;
        let lox_loss = 10_000.0 - lox_remaining;

        assert!(
            lh2_loss > lox_loss,
            "LH2 ({lh2_loss} lost) should boil faster than LOX ({lox_loss} lost)"
        );
    }

    #[test]
    fn test_boiloff_reduced_by_tech_modifier() {
        let content = boiloff_content();

        // Run without modifier
        let mut state_baseline = state_with_lh2(&content, 10_000.0);
        let mut rng = make_rng();
        tick(&mut state_baseline, &[], &content, &mut rng, None);

        let station_id = crate::StationId("station_earth_orbit".to_string());
        let baseline_remaining: f32 = state_baseline.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
                _ => None,
            })
            .sum();
        let baseline_loss = 10_000.0 - baseline_remaining;

        // Run with -75% boiloff rate modifier
        let mut state_tech = state_with_lh2(&content, 10_000.0);
        state_tech.modifiers.add(crate::modifiers::Modifier {
            stat: crate::modifiers::StatId::BoiloffRate,
            op: crate::modifiers::ModifierOp::PctAdditive,
            value: -0.75,
            source: crate::modifiers::ModifierSource::Tech("tech_cryo_insulation".into()),
            condition: None,
        });
        let mut rng = make_rng();
        tick(&mut state_tech, &[], &content, &mut rng, None);

        let tech_remaining: f32 = state_tech.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
                _ => None,
            })
            .sum();
        let tech_loss = 10_000.0 - tech_remaining;

        // With -75%, loss should be ~25% of baseline
        let ratio = tech_loss / baseline_loss;
        assert!(
            (ratio - 0.25).abs() < 0.01,
            "tech loss should be ~25% of baseline: baseline_loss={baseline_loss}, tech_loss={tech_loss}, ratio={ratio}"
        );
    }

    #[test]
    fn test_boiloff_modifier_does_not_affect_non_cryo() {
        let content = boiloff_content();
        let mut state = base_state(&content);
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.core.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 1000.0,
            quality: 1.0,
            thermal: None,
        });

        // Add boiloff rate modifier — should not affect non-cryo Fe
        state.modifiers.add(crate::modifiers::Modifier {
            stat: crate::modifiers::StatId::BoiloffRate,
            op: crate::modifiers::ModifierOp::PctAdditive,
            value: -0.75,
            source: crate::modifiers::ModifierSource::Tech("tech_cryo_insulation".into()),
            condition: None,
        });

        let mut rng = make_rng();
        tick(&mut state, &[], &content, &mut rng, None);

        let remaining: f32 = state.stations[&station_id]
            .core
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();

        assert!(
            (remaining - 1000.0).abs() < 0.01,
            "Fe should not boil off even with boiloff modifier: {remaining}"
        );
    }

    #[test]
    fn test_temp_multiplier_piecewise() {
        let amb = 293_000;
        let hot_off = 100_000;
        let curve = BoiloffCurveDef::default();
        // At boiling point: 0.1x
        assert!(
            (boiloff_temp_multiplier(20_300, 20_300, amb, hot_off, &curve) - 0.1).abs() < 0.001
        );
        // Below boiling point: 0.1x
        assert!(
            (boiloff_temp_multiplier(10_000, 20_300, amb, hot_off, &curve) - 0.1).abs() < 0.001
        );
        // At ambient (293K): 1.0x
        assert!(
            (boiloff_temp_multiplier(293_000, 20_300, amb, hot_off, &curve) - 1.0).abs() < 0.01
        );
        // At ambient+100K (393K): 3.0x
        assert!(
            (boiloff_temp_multiplier(393_000, 20_300, amb, hot_off, &curve) - 3.0).abs() < 0.01
        );
        // Above ambient+100K: clamped at 3.0x
        assert!(
            (boiloff_temp_multiplier(500_000, 20_300, amb, hot_off, &curve) - 3.0).abs() < 0.001
        );
        // Midway between boiling and ambient: ~0.55x
        let midpoint = (20_300 + 293_000) / 2;
        let mid_val = boiloff_temp_multiplier(midpoint, 20_300, amb, hot_off, &curve);
        assert!(mid_val > 0.4 && mid_val < 0.7, "midpoint value: {mid_val}");
    }

    #[test]
    fn test_custom_boiloff_curve() {
        let amb = 293_000;
        let hot_off = 100_000;
        let curve = BoiloffCurveDef {
            cold_multiplier: 0.05,
            ambient_multiplier: 0.5,
            hot_multiplier: 2.0,
        };
        // At boiling point: cold_multiplier
        assert!(
            (boiloff_temp_multiplier(20_300, 20_300, amb, hot_off, &curve) - 0.05).abs() < 0.001
        );
        // At ambient: ambient_multiplier
        assert!(
            (boiloff_temp_multiplier(293_000, 20_300, amb, hot_off, &curve) - 0.5).abs() < 0.01
        );
        // Above hot: hot_multiplier
        assert!(
            (boiloff_temp_multiplier(500_000, 20_300, amb, hot_off, &curve) - 2.0).abs() < 0.001
        );
        // At hot threshold: hot_multiplier
        assert!(
            (boiloff_temp_multiplier(393_000, 20_300, amb, hot_off, &curve) - 2.0).abs() < 0.01
        );
        // Midpoint between boiling and ambient: (0.05 + 0.5) / 2 = 0.275
        let midpoint = (20_300 + 293_000) / 2;
        let mid_val = boiloff_temp_multiplier(midpoint, 20_300, amb, hot_off, &curve);
        assert!(
            (mid_val - 0.275).abs() < 0.01,
            "midpoint with custom curve: {mid_val}"
        );
    }
}
