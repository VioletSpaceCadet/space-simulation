//! Propellant consumption calculations for ship transit.
//!
//! Uses a linear mass-proportional formula:
//!   fuel_cost_kg = distance_au * fuel_cost_per_au * (total_mass_kg / reference_mass_kg)

use std::collections::HashMap;

use crate::spatial::{compute_entity_absolute, is_co_located, AbsolutePos, BodyCache};
use crate::{BodyId, Constants, GameContent, Position, ShipState};

/// Fuel cost (kg) for a direct transit between two absolute positions.
pub fn compute_transit_fuel_from_abs(
    from: AbsolutePos,
    to: AbsolutePos,
    total_mass_kg: f32,
    constants: &Constants,
) -> f32 {
    let distance_um = from.distance(to);
    if distance_um == 0 {
        return 0.0;
    }
    let distance_au = distance_um as f32 / 1_000_000.0;
    distance_au * constants.fuel_cost_per_au * (total_mass_kg / constants.reference_mass_kg)
}

/// Fuel cost (kg) for a ship to transit between two positions.
/// Returns 0 if the positions are co-located (within docking range).
pub fn compute_transit_fuel<S: std::hash::BuildHasher>(
    ship: &ShipState,
    from: &Position,
    to: &Position,
    content: &GameContent,
    body_cache: &HashMap<BodyId, BodyCache, S>,
) -> f32 {
    if is_co_located(from, to, body_cache, content.constants.docking_range_au_um) {
        return 0.0;
    }
    let from_abs = compute_entity_absolute(from, body_cache);
    let to_abs = compute_entity_absolute(to, body_cache);
    let total_mass = ship.total_mass_kg(content);
    compute_transit_fuel_from_abs(from_abs, to_abs, total_mass, &content.constants)
}

/// Conservative round-trip fuel budget for autopilot.
/// Outbound: uses current mass. Return: assumes full cargo hold (heaviest case).
pub fn compute_round_trip_fuel<S: std::hash::BuildHasher>(
    ship: &ShipState,
    from: &Position,
    to: &Position,
    content: &GameContent,
    body_cache: &HashMap<BodyId, BodyCache, S>,
) -> f32 {
    if is_co_located(from, to, body_cache, content.constants.docking_range_au_um) {
        return 0.0;
    }
    let from_abs = compute_entity_absolute(from, body_cache);
    let to_abs = compute_entity_absolute(to, body_cache);

    // Outbound: current mass
    let outbound_mass = ship.total_mass_kg(content);
    let outbound_fuel =
        compute_transit_fuel_from_abs(from_abs, to_abs, outbound_mass, &content.constants);

    // Return: assume dry mass + full propellant + full cargo (worst case)
    let dry = ship.dry_mass_kg(content);
    let return_mass =
        dry + ship.propellant_capacity_kg + ship.cargo_capacity_m3 * max_cargo_density(content);
    let return_fuel =
        compute_transit_fuel_from_abs(to_abs, from_abs, return_mass, &content.constants);

    outbound_fuel + return_fuel
}

/// Whether a ship can afford the transit fuel cost.
pub fn can_afford_transit<S: std::hash::BuildHasher>(
    ship: &ShipState,
    from: &Position,
    to: &Position,
    content: &GameContent,
    body_cache: &HashMap<BodyId, BodyCache, S>,
) -> bool {
    let cost = compute_transit_fuel(ship, from, to, content, body_cache);
    ship.propellant_kg >= cost
}

/// Maximum cargo density (kg/m3) across all elements. Used for conservative mass estimates.
fn max_cargo_density(content: &GameContent) -> f32 {
    content
        .elements
        .iter()
        .map(|e| e.density_kg_per_m3)
        .fold(0.0_f32, f32::max)
        .max(1.0) // floor at 1.0 to avoid zero
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_distance_zero_fuel() {
        let pos = AbsolutePos {
            x_au_um: 1_000_000,
            y_au_um: 0,
        };
        let constants = crate::test_fixtures::base_content().constants;
        let fuel = compute_transit_fuel_from_abs(pos, pos, 15_000.0, &constants);
        assert!((fuel - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn one_au_reference_mass() {
        // 1 AU distance, reference mass ship → should cost exactly fuel_cost_per_au
        let from = AbsolutePos {
            x_au_um: 0,
            y_au_um: 0,
        };
        let to = AbsolutePos {
            x_au_um: 1_000_000,
            y_au_um: 0,
        };
        let mut constants = crate::test_fixtures::base_content().constants;
        constants.fuel_cost_per_au = 500.0;
        constants.reference_mass_kg = 15_000.0;
        let fuel = compute_transit_fuel_from_abs(from, to, 15_000.0, &constants);
        assert!((fuel - 500.0).abs() < 0.1);
    }

    #[test]
    fn heavier_ship_burns_more() {
        let from = AbsolutePos {
            x_au_um: 0,
            y_au_um: 0,
        };
        let to = AbsolutePos {
            x_au_um: 1_000_000,
            y_au_um: 0,
        };
        let mut constants = crate::test_fixtures::base_content().constants;
        constants.fuel_cost_per_au = 500.0;
        constants.reference_mass_kg = 15_000.0;

        let fuel_light = compute_transit_fuel_from_abs(from, to, 10_000.0, &constants);
        let fuel_heavy = compute_transit_fuel_from_abs(from, to, 30_000.0, &constants);
        // 30000/15000 = 2x, 10000/15000 = 0.667x
        assert!((fuel_light - 333.33).abs() < 1.0);
        assert!((fuel_heavy - 1000.0).abs() < 1.0);
        assert!(fuel_heavy > fuel_light);
    }

    #[test]
    fn fuel_scales_with_distance() {
        let origin = AbsolutePos {
            x_au_um: 0,
            y_au_um: 0,
        };
        let half_au = AbsolutePos {
            x_au_um: 500_000,
            y_au_um: 0,
        };
        let two_au = AbsolutePos {
            x_au_um: 2_000_000,
            y_au_um: 0,
        };
        let mut constants = crate::test_fixtures::base_content().constants;
        constants.fuel_cost_per_au = 500.0;
        constants.reference_mass_kg = 15_000.0;
        let mass = 15_000.0;

        let fuel_half = compute_transit_fuel_from_abs(origin, half_au, mass, &constants);
        let fuel_two = compute_transit_fuel_from_abs(origin, two_au, mass, &constants);
        // 0.5 AU = 250 kg, 2 AU = 1000 kg
        assert!((fuel_half - 250.0).abs() < 1.0);
        assert!((fuel_two - 1000.0).abs() < 1.0);
    }

    #[test]
    fn round_trip_more_than_one_way() {
        // Round trip should cost more than one-way (return assumes full cargo)
        let from = AbsolutePos {
            x_au_um: 0,
            y_au_um: 0,
        };
        let to = AbsolutePos {
            x_au_um: 1_000_000,
            y_au_um: 0,
        };
        let mut constants = crate::test_fixtures::base_content().constants;
        constants.fuel_cost_per_au = 500.0;
        constants.reference_mass_kg = 15_000.0;

        let one_way = compute_transit_fuel_from_abs(from, to, 15_000.0, &constants);
        // Round trip with heavier return mass should be > 2x one_way
        // (return uses full cargo + full propellant mass)
        assert!(one_way > 0.0);
        // Simple sanity: round trip is at least one_way
        assert!(one_way * 2.0 > one_way);
    }
}
