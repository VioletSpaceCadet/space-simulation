//! Composition math helpers for ore processing.
//!
//! These functions are extracted from `station.rs` to make the composition
//! arithmetic testable in isolation and to keep station tick logic readable.

use std::collections::HashMap;

use crate::{InventoryItem, MaterialThermalProps};

/// Returns a mass-weighted average composition given a slice of
/// `(composition, kg)` pairs.
///
/// For each element, its fraction in the result equals:
/// `sum(fraction_i * kg_i) / sum(kg_i)`
///
/// Returns an empty map when the total kg is zero or near-zero.
pub(crate) fn weighted_composition(pairs: &[(&HashMap<String, f32>, f32)]) -> HashMap<String, f32> {
    let total_kg: f32 = pairs.iter().map(|(_, kg)| kg).sum();
    if total_kg < 1e-9 {
        return HashMap::new();
    }
    let mut result: HashMap<String, f32> = HashMap::new();
    for (composition, kg) in pairs {
        for (element, fraction) in *composition {
            *result.entry(element.clone()).or_insert(0.0) += fraction * kg;
        }
    }
    for v in result.values_mut() {
        *v /= total_kg;
    }
    result
}

/// Blends an incoming slag lot into existing slag and returns the blended
/// composition as a new `HashMap`.
///
/// The result is mass-weighted across both lots.
pub(crate) fn blend_slag_composition(
    existing_composition: &HashMap<String, f32>,
    existing_kg: f32,
    new_composition: &HashMap<String, f32>,
    new_kg: f32,
) -> HashMap<String, f32> {
    let total_kg = existing_kg + new_kg;
    if total_kg < 1e-9 {
        return HashMap::new();
    }
    let mut all_keys: std::collections::HashSet<String> =
        existing_composition.keys().cloned().collect();
    all_keys.extend(new_composition.keys().cloned());
    all_keys
        .into_iter()
        .map(|key| {
            let existing_contrib =
                existing_composition.get(&key).copied().unwrap_or(0.0) * existing_kg;
            let new_contrib = new_composition.get(&key).copied().unwrap_or(0.0) * new_kg;
            (key, (existing_contrib + new_contrib) / total_kg)
        })
        .collect()
}

/// Blend two optional thermal property sets, weighted by mass.
///
/// If both are `None`, returns `None`. If only one is set, returns that one.
/// If both are set, produces a mass-weighted average of temperature and latent
/// heat, keeping the phase of the larger batch.
pub(crate) fn blend_thermal(
    a: Option<&MaterialThermalProps>,
    a_kg: f32,
    b: Option<&MaterialThermalProps>,
    b_kg: f32,
) -> Option<MaterialThermalProps> {
    match (a, b) {
        (None, None) => None,
        (Some(t), None) | (None, Some(t)) => Some(t.clone()),
        (Some(ta), Some(tb)) => {
            let total = f64::from(a_kg) + f64::from(b_kg);
            if total <= 0.0 {
                return Some(ta.clone());
            }
            let wa = f64::from(a_kg) / total;
            let wb = f64::from(b_kg) / total;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let blended_temp =
                (f64::from(ta.temp_mk) * wa + f64::from(tb.temp_mk) * wb).round() as u32;
            #[allow(clippy::cast_possible_truncation)]
            let blended_latent = (ta.latent_heat_buffer_j as f64 * wa
                + tb.latent_heat_buffer_j as f64 * wb)
                .round() as i64;
            let phase = if a_kg >= b_kg { ta.phase } else { tb.phase };
            Some(MaterialThermalProps {
                temp_mk: blended_temp,
                phase,
                latent_heat_buffer_j: blended_latent,
            })
        }
    }
}

/// Merges a material lot into an inventory vec.
///
/// If an existing `Material` item with the same element and exact quality is
/// found, its kg is incremented and thermal properties are blended by mass.
/// Otherwise a new item is pushed.
pub(crate) fn merge_material_lot(
    inventory: &mut Vec<InventoryItem>,
    element: String,
    kg: f32,
    quality: f32,
    thermal: Option<MaterialThermalProps>,
) {
    #[allow(clippy::float_cmp)]
    let existing = inventory.iter_mut().find(|item| {
        matches!(
            item,
            InventoryItem::Material {
                element: existing_element,
                quality: existing_quality,
                ..
            } if existing_element == &element && *existing_quality == quality
        )
    });
    if let Some(InventoryItem::Material {
        kg: existing_kg,
        thermal: existing_thermal,
        ..
    }) = existing
    {
        let blended = blend_thermal(
            existing_thermal.as_ref(),
            *existing_kg,
            thermal.as_ref(),
            kg,
        );
        *existing_kg += kg;
        *existing_thermal = blended;
    } else {
        inventory.push(InventoryItem::Material {
            element,
            kg,
            quality,
            thermal,
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_composition_single_lot_returns_same_fractions() {
        let mut composition = HashMap::new();
        composition.insert("Fe".to_string(), 0.7);
        composition.insert("Si".to_string(), 0.3);

        let result = weighted_composition(&[(&composition, 100.0)]);

        assert!((result["Fe"] - 0.7).abs() < 1e-6);
        assert!((result["Si"] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn weighted_composition_two_lots_averages_by_mass() {
        let mut comp_a = HashMap::new();
        comp_a.insert("Fe".to_string(), 0.8);
        comp_a.insert("Si".to_string(), 0.2);

        let mut comp_b = HashMap::new();
        comp_b.insert("Fe".to_string(), 0.4);
        comp_b.insert("Si".to_string(), 0.6);

        // Expected weighted average over 400 kg total:
        //   Fe = (0.8*100 + 0.4*300) / 400 = 0.5
        //   Si = (0.2*100 + 0.6*300) / 400 = 0.5
        let result = weighted_composition(&[(&comp_a, 100.0), (&comp_b, 300.0)]);

        assert!((result["Fe"] - 0.5).abs() < 1e-6);
        assert!((result["Si"] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn weighted_composition_zero_total_kg_returns_empty() {
        let composition = HashMap::new();
        let result = weighted_composition(&[(&composition, 0.0)]);
        assert!(result.is_empty());
    }

    #[test]
    fn merge_material_lot_pushes_new_item_when_inventory_empty() {
        let mut inventory: Vec<InventoryItem> = Vec::new();
        merge_material_lot(&mut inventory, "Fe".to_string(), 50.0, 0.9, None);

        assert_eq!(inventory.len(), 1);
        match &inventory[0] {
            InventoryItem::Material {
                element,
                kg,
                quality,
                ..
            } => {
                assert_eq!(element, "Fe");
                assert!((kg - 50.0).abs() < 1e-6);
                assert!((quality - 0.9).abs() < 1e-6);
            }
            other => panic!("expected Material, got {other:?}"),
        }
    }

    #[test]
    fn merge_material_lot_merges_into_matching_lot() {
        let mut inventory = vec![InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 30.0,
            quality: 0.9,
            thermal: None,
        }];
        merge_material_lot(&mut inventory, "Fe".to_string(), 20.0, 0.9, None);

        assert_eq!(inventory.len(), 1);
        match &inventory[0] {
            InventoryItem::Material { kg, .. } => assert!((kg - 50.0).abs() < 1e-6),
            other => panic!("expected Material, got {other:?}"),
        }
    }

    #[test]
    fn merge_material_lot_different_quality_adds_new_lot() {
        let mut inventory = vec![InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 30.0,
            quality: 0.9,
            thermal: None,
        }];
        merge_material_lot(&mut inventory, "Fe".to_string(), 20.0, 0.5, None);

        assert_eq!(inventory.len(), 2);
    }

    #[test]
    fn blend_thermal_both_none_returns_none() {
        assert!(blend_thermal(None, 100.0, None, 50.0).is_none());
    }

    #[test]
    fn blend_thermal_one_some_returns_it() {
        let t = MaterialThermalProps {
            temp_mk: 500_000,
            phase: crate::Phase::Liquid,
            latent_heat_buffer_j: 1000,
        };
        let result = blend_thermal(Some(&t), 100.0, None, 50.0);
        assert_eq!(result.unwrap().temp_mk, 500_000);
    }

    #[test]
    fn blend_thermal_weighted_average() {
        let a = MaterialThermalProps {
            temp_mk: 300_000,
            phase: crate::Phase::Solid,
            latent_heat_buffer_j: 0,
        };
        let b = MaterialThermalProps {
            temp_mk: 500_000,
            phase: crate::Phase::Liquid,
            latent_heat_buffer_j: 1000,
        };
        // 100kg at 300K + 100kg at 500K => 400K, latent = 500
        let result = blend_thermal(Some(&a), 100.0, Some(&b), 100.0).unwrap();
        assert_eq!(result.temp_mk, 400_000);
        assert_eq!(result.latent_heat_buffer_j, 500);
        // Equal mass => phase from a (a_kg >= b_kg)
        assert_eq!(result.phase, crate::Phase::Solid);
    }

    #[test]
    fn blend_thermal_larger_batch_phase_wins() {
        let a = MaterialThermalProps {
            temp_mk: 300_000,
            phase: crate::Phase::Solid,
            latent_heat_buffer_j: 0,
        };
        let b = MaterialThermalProps {
            temp_mk: 500_000,
            phase: crate::Phase::Liquid,
            latent_heat_buffer_j: 0,
        };
        // 50kg solid + 200kg liquid => liquid phase wins
        let result = blend_thermal(Some(&a), 50.0, Some(&b), 200.0).unwrap();
        assert_eq!(result.phase, crate::Phase::Liquid);
        // temp = (300K*50 + 500K*200) / 250 = 460K
        assert_eq!(result.temp_mk, 460_000);
    }

    #[test]
    fn merge_material_lot_blends_thermal() {
        let mut inventory = vec![InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 100.0,
            quality: 0.9,
            thermal: Some(MaterialThermalProps {
                temp_mk: 300_000,
                phase: crate::Phase::Solid,
                latent_heat_buffer_j: 0,
            }),
        }];
        merge_material_lot(
            &mut inventory,
            "Fe".to_string(),
            100.0,
            0.9,
            Some(MaterialThermalProps {
                temp_mk: 500_000,
                phase: crate::Phase::Liquid,
                latent_heat_buffer_j: 1000,
            }),
        );

        assert_eq!(inventory.len(), 1);
        match &inventory[0] {
            InventoryItem::Material { kg, thermal, .. } => {
                assert!((kg - 200.0).abs() < 1e-6);
                let t = thermal.as_ref().unwrap();
                assert_eq!(t.temp_mk, 400_000);
                assert_eq!(t.latent_heat_buffer_j, 500);
            }
            other => panic!("expected Material, got {other:?}"),
        }
    }

    #[test]
    fn blend_slag_composition_weighted_by_mass() {
        let mut existing = HashMap::new();
        existing.insert("Si".to_string(), 1.0);

        let mut new_slag = HashMap::new();
        new_slag.insert("Si".to_string(), 0.5);
        new_slag.insert("Al".to_string(), 0.5);

        // 100 kg existing + 100 kg new -> blended:
        //   Si = (1.0*100 + 0.5*100) / 200 = 0.75
        //   Al = (0.0*100 + 0.5*100) / 200 = 0.25
        let blended = blend_slag_composition(&existing, 100.0, &new_slag, 100.0);

        assert!((blended["Si"] - 0.75).abs() < 1e-6);
        assert!((blended["Al"] - 0.25).abs() < 1e-6);
    }
}
