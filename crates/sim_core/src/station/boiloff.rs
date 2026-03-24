use crate::{
    boiloff_rate_per_tick, Event, EventEnvelope, GameContent, GameState, InventoryItem, StationId,
};

/// Piecewise linear temperature multiplier for boiloff.
/// - At or below boiling point: 0.1x (minimal loss)
/// - Boiling point → ambient: linear 0.1x → 1.0x
/// - Ambient → ambient + `hot_offset`: linear 1.0x → 3.0x
/// - Above ambient + `hot_offset`: clamped at 3.0x
fn boiloff_temp_multiplier(
    temp_mk: u32,
    boiling_point_mk: u32,
    ambient_mk: u32,
    hot_offset_mk: u32,
) -> f64 {
    let t_amb = ambient_mk;
    let t_hot = t_amb + hot_offset_mk;
    if temp_mk <= boiling_point_mk {
        0.1
    } else if temp_mk <= t_amb {
        let frac = f64::from(temp_mk - boiling_point_mk) / f64::from(t_amb - boiling_point_mk);
        0.1 + 0.9 * frac
    } else if temp_mk <= t_hot {
        let frac = f64::from(temp_mk - t_amb) / 100_000.0;
        1.0 + 2.0 * frac
    } else {
        3.0
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

    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };

    let mut losses: Vec<(String, f32)> = Vec::new();

    for item in &mut station.inventory {
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
            (Some(mat_thermal), Some(bp_mk)) => boiloff_temp_multiplier(
                mat_thermal.temp_mk,
                bp_mk,
                content.constants.thermal_sink_temp_mk,
                content.constants.boiloff_hot_offset_mk,
            ),
            _ => 1.0,
        };

        #[allow(clippy::cast_possible_truncation)]
        let loss = ((f64::from(*kg) * base_rate * multiplier) as f32).min(*kg);
        if loss > content.constants.min_meaningful_kg {
            *kg -= loss;
            losses.push((element.clone(), loss));
        }
    }

    // Remove material items below threshold
    let min_kg = content.constants.min_meaningful_kg;
    station.inventory.retain(|item| match item {
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
    use crate::{tick, ElementDef, EventLevel};

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
        });
        content
    }

    fn state_with_lh2(content: &GameContent, kg: f32) -> GameState {
        let mut state = base_state(content);
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Material {
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

        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let remaining: f32 = state.stations[&station_id]
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
        station.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 1000.0,
            quality: 1.0,
            thermal: None,
        });

        let mut rng = make_rng();
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let remaining: f32 = state.stations[&station_id]
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
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        }

        let station_id = crate::StationId("station_earth_orbit".to_string());
        let remaining: f32 = state.stations[&station_id]
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

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

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
        station.inventory.push(InventoryItem::Material {
            element: "LH2".to_string(),
            kg: 10_000.0,
            quality: 1.0,
            thermal: None,
        });
        station.inventory.push(InventoryItem::Material {
            element: "LOX".to_string(),
            kg: 10_000.0,
            quality: 1.0,
            thermal: None,
        });

        let mut rng = make_rng();
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let lh2_remaining: f32 = state.stations[&station_id]
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
                _ => None,
            })
            .sum();

        let lox_remaining: f32 = state.stations[&station_id]
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
    fn test_temp_multiplier_piecewise() {
        let amb = 293_000;
        let hot_off = 100_000;
        // At boiling point: 0.1x
        assert!((boiloff_temp_multiplier(20_300, 20_300, amb, hot_off) - 0.1).abs() < 0.001);
        // Below boiling point: 0.1x
        assert!((boiloff_temp_multiplier(10_000, 20_300, amb, hot_off) - 0.1).abs() < 0.001);
        // At ambient (293K): 1.0x
        assert!((boiloff_temp_multiplier(293_000, 20_300, amb, hot_off) - 1.0).abs() < 0.01);
        // At ambient+100K (393K): 3.0x
        assert!((boiloff_temp_multiplier(393_000, 20_300, amb, hot_off) - 3.0).abs() < 0.01);
        // Above ambient+100K: clamped at 3.0x
        assert!((boiloff_temp_multiplier(500_000, 20_300, amb, hot_off) - 3.0).abs() < 0.001);
        // Midway between boiling and ambient: ~0.55x
        let midpoint = (20_300 + 293_000) / 2;
        let mid_val = boiloff_temp_multiplier(midpoint, 20_300, amb, hot_off);
        assert!(mid_val > 0.4 && mid_val < 0.7, "midpoint value: {mid_val}");
    }
}
