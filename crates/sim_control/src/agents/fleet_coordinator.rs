//! VIO-598: `FleetCoordinator` — global supply/demand evaluation across stations.
//!
//! Evaluates per-station inventory levels, identifies surpluses and deficits,
//! and generates transfer plans to redistribute resources. Runs before station
//! agents in `generate_commands()` so that transfer objectives are assigned
//! before per-station ship objectives.
//!
//! Priority: propellant > `repair_kits` > materials (Fe, Si, He).
//! Transfer plans are assigned to idle ships as `ShipObjective::Transfer`.

use std::collections::BTreeMap;

use sim_core::{
    ComponentId, GameContent, GameState, InventoryItem, PrincipalId, ShipId, StationId,
    TradeItemSpec,
};

use super::ship_agent::ShipAgent;
use super::DecisionRecord;
use crate::behaviors::{collect_idle_ships, collect_idle_ships_with_tag};
use crate::objectives::ShipObjective;

/// A planned inter-station resource transfer.
#[derive(Debug)]
struct TransferPlan {
    source: StationId,
    target: StationId,
    item: TradeItemSpec,
    priority: u8,
}

/// Per-station inventory snapshot for a specific resource.
struct StationLevel {
    station_id: StationId,
    amount: f32,
}

/// Compute material levels (kg) for a given element across all stations.
fn material_levels(state: &GameState, element: &str) -> Vec<StationLevel> {
    state
        .stations
        .iter()
        .map(|(station_id, station)| {
            let amount: f32 = station
                .core
                .inventory
                .iter()
                .filter_map(|item| match item {
                    InventoryItem::Material {
                        element: el, kg, ..
                    } if el == element => Some(*kg),
                    _ => None,
                })
                .sum();
            StationLevel {
                station_id: station_id.clone(),
                amount,
            }
        })
        .collect()
}

/// Compute component count for a given `component_id` across all stations.
fn component_levels(state: &GameState, component_id: &str) -> Vec<StationLevel> {
    state
        .stations
        .iter()
        .map(|(station_id, station)| {
            let amount: f32 = station
                .core
                .inventory
                .iter()
                .filter_map(|item| match item {
                    InventoryItem::Component {
                        component_id: cid,
                        count,
                        ..
                    } if cid.0 == component_id => Some(*count as f32),
                    _ => None,
                })
                .sum();
            StationLevel {
                station_id: station_id.clone(),
                amount,
            }
        })
        .collect()
}

/// Match surpluses to deficits for a given resource, producing transfer plans.
/// Stations with inventory above `surplus_threshold` are sources; stations
/// below `deficit_threshold` are targets.
fn match_surplus_to_deficit(
    levels: &[StationLevel],
    surplus_threshold: f32,
    deficit_threshold: f32,
    make_item: &dyn Fn(f32) -> TradeItemSpec,
    priority: u8,
    batch_limit: f32,
    plans: &mut Vec<TransferPlan>,
) {
    // Build mutable surplus/deficit pools.
    let mut surpluses: Vec<(&StationId, f32)> = levels
        .iter()
        .filter(|l| l.amount > surplus_threshold)
        .map(|l| (&l.station_id, l.amount - surplus_threshold))
        .collect();
    let mut deficits: Vec<(&StationId, f32)> = levels
        .iter()
        .filter(|l| l.amount < deficit_threshold)
        .map(|l| (&l.station_id, deficit_threshold - l.amount))
        .collect();

    // Sort for determinism: largest surplus first, largest deficit first.
    surpluses.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));
    deficits.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));

    for (target_id, deficit) in &mut deficits {
        if *deficit <= 0.0 {
            continue;
        }
        for (source_id, surplus) in &mut surpluses {
            if *surplus <= 0.0 || *source_id == *target_id {
                continue;
            }
            let transfer_amount = deficit.min(*surplus).min(batch_limit);
            if transfer_amount <= 0.0 {
                continue;
            }
            plans.push(TransferPlan {
                source: (*source_id).clone(),
                target: (*target_id).clone(),
                item: make_item(transfer_amount),
                priority,
            });
            *surplus -= transfer_amount;
            *deficit -= transfer_amount;
            if *deficit <= 0.0 {
                break;
            }
        }
    }
}

/// Build transfer plans for all resource types, sorted by priority.
fn build_transfer_plans(state: &GameState, content: &GameContent) -> Vec<TransferPlan> {
    let mut plans: Vec<TransferPlan> = Vec::new();

    // --- Propellant element (LH2): highest priority ---
    let propellant_element = &content.autopilot.propellant_element;
    let lh2_threshold = content.autopilot.lh2_threshold_kg;
    let propellant_levels = material_levels(state, propellant_element);
    match_surplus_to_deficit(
        &propellant_levels,
        lh2_threshold * 2.0,
        lh2_threshold,
        &|kg| TradeItemSpec::Material {
            element: propellant_element.clone(),
            kg,
        },
        0,
        content.autopilot.export_batch_size_kg,
        &mut plans,
    );

    // --- Repair kits: priority 1 ---
    let repair_kit_id = &content.autopilot.export_component.component_id;
    let repair_reserve = content.autopilot.export_component.reserve as f32;
    let repair_levels = component_levels(state, repair_kit_id);
    match_surplus_to_deficit(
        &repair_levels,
        repair_reserve * 2.0,
        repair_reserve * 0.5,
        &|count| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let count = count.clamp(0.0, f32::from(u16::MAX)) as u32;
            TradeItemSpec::Component {
                component_id: ComponentId(repair_kit_id.clone()),
                count,
            }
        },
        1,
        repair_reserve,
        &mut plans,
    );

    // --- Export elements (Fe, Si, He): priority 2+ ---
    for (idx, export_cfg) in content.autopilot.export_elements.iter().enumerate() {
        let levels = material_levels(state, &export_cfg.element);
        let reserve = export_cfg.reserve_kg;
        match_surplus_to_deficit(
            &levels,
            reserve * 2.0,
            reserve * 0.25,
            &|kg| TradeItemSpec::Material {
                element: export_cfg.element.clone(),
                kg,
            },
            2_u8.saturating_add(u8::try_from(idx).unwrap_or(u8::MAX - 2)),
            content.autopilot.export_batch_size_kg,
            &mut plans,
        );
    }

    // Sort by priority then deterministic tiebreak.
    plans.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.target.0.cmp(&b.target.0))
            .then_with(|| a.source.0.cmp(&b.source.0))
    });

    plans
}

/// Evaluate supply/demand across all stations and assign Transfer objectives
/// to idle ships for material/component redistribution.
///
/// Runs as step 3.5a in `generate_commands`, before module delivery (step 3.5b)
/// and ship objective assignment (step 4).
pub(crate) fn evaluate_and_assign(
    ship_agents: &mut BTreeMap<ShipId, ShipAgent>,
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    mut decisions: Option<&mut Vec<DecisionRecord>>,
) {
    if state.stations.len() < 2 {
        return;
    }

    // VIO-599: Prefer logistics-tagged ships for transfers; fall back to
    // general-purpose ships (but never use mining-tagged ships).
    // Order: general-purpose first, logistics last — `pop()` takes from end.
    let logistics_ships = collect_idle_ships_with_tag(state, owner, "logistics", content);
    let all_idle = collect_idle_ships(state, owner);
    let mut available_ships: Vec<ShipId> = all_idle
        .into_iter()
        .filter(|id| {
            state.ships.get(id).is_some_and(|s| {
                !crate::behaviors::ship_has_hull_tag(s, "logistics", content)
                    && !crate::behaviors::ship_has_hull_tag(s, "mining", content)
            })
        })
        .chain(logistics_ships)
        .filter(|id| ship_agents.get(id).is_some_and(|a| a.objective.is_none()))
        .collect();
    if available_ships.is_empty() {
        return;
    }

    let plans = build_transfer_plans(state, content);
    if plans.is_empty() {
        return;
    }

    // Assign one transfer per idle ship.
    for plan in plans {
        let Some(ship_id) = available_ships.pop() else {
            break;
        };

        if let Some(ref mut log) = decisions {
            log.push(DecisionRecord {
                tick: state.meta.tick,
                agent: "fleet_coordinator".to_string(),
                concern: "supply_demand".to_string(),
                decision_type: "assign_transfer".to_string(),
                chosen_id: format!("{}->{}:{:?}", plan.source.0, plan.target.0, plan.item),
                chosen_score: f64::from(plan.priority),
                alt_1_id: ship_id.0.clone(),
                alt_1_score: 0.0,
                alt_2_id: String::new(),
                alt_2_score: 0.0,
                alt_3_id: String::new(),
                alt_3_score: 0.0,
                context_json: String::new(),
            });
        }

        if let Some(agent) = ship_agents.get_mut(&ship_id) {
            agent.objective = Some(ShipObjective::Transfer {
                from_station: plan.source,
                to_station: plan.target,
                items: vec![plan.item],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::test_fixtures::{base_content, base_state, test_position};
    use sim_core::{BodyId, HullId, ShipState};

    fn owner() -> PrincipalId {
        PrincipalId("principal_autopilot".to_string())
    }

    fn ship_id() -> ShipId {
        ShipId("ship_hauler".to_string())
    }

    fn station_a() -> StationId {
        StationId("station_earth_orbit".to_string())
    }

    fn station_b() -> StationId {
        StationId("station_belt_outpost".to_string())
    }

    /// Content with export config for Fe (12000 kg reserve) and repair kits (10 reserve).
    /// Includes hull defs with tags for role-based ship filtering (VIO-599).
    fn fleet_content() -> GameContent {
        let mut content = base_content();
        // Add tagged hull defs for role filtering tests.
        content.hulls.insert(
            HullId("hull_mining_barge".to_string()),
            sim_core::HullDef {
                id: HullId("hull_mining_barge".to_string()),
                name: "Mining Barge".to_string(),
                mass_kg: 8000.0,
                cargo_capacity_m3: 80.0,
                base_speed_ticks_per_au: 3200,
                base_propellant_capacity_kg: 8000.0,
                slots: vec![],
                bonuses: vec![],
                required_tech: None,
                tags: vec!["mining".to_string()],
            },
        );
        content.hulls.insert(
            HullId("hull_transport_hauler".to_string()),
            sim_core::HullDef {
                id: HullId("hull_transport_hauler".to_string()),
                name: "Transport Hauler".to_string(),
                mass_kg: 12000.0,
                cargo_capacity_m3: 200.0,
                base_speed_ticks_per_au: 2666,
                base_propellant_capacity_kg: 15000.0,
                slots: vec![],
                bonuses: vec![],
                required_tech: None,
                tags: vec!["logistics".to_string()],
            },
        );
        content
    }

    /// State with two stations and one idle ship. Station A has surplus Fe,
    /// station B is empty (deficit).
    fn fleet_state(content: &GameContent) -> GameState {
        let mut state = base_state(content);

        // Station A (earth_orbit) already exists; give it surplus Fe.
        if let Some(station) = state.stations.get_mut(&station_a()) {
            station.core.inventory.push(InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 30_000.0, // well above 2x reserve of 12,000
                quality: 0.8,
                thermal: None,
            });
        }

        // Station B: empty station.
        let target = sim_core::StationState {
            id: station_b(),
            position: sim_core::Position {
                parent_body: BodyId("sol".to_string()),
                radius_au_um: sim_core::RadiusAuMicro(2_000_000),
                angle_mdeg: sim_core::AngleMilliDeg(180_000),
            },
            core: sim_core::FacilityCore {
                inventory: vec![],
                cargo_capacity_m3: 500.0,
                power_available_per_tick: 0.0,
                modules: vec![],
                modifiers: Default::default(),
                crew: BTreeMap::new(),
                thermal_links: vec![],
                power: Default::default(),
                cached_inventory_volume_m3: None,
                module_type_index: Default::default(),
                module_id_index: Default::default(),
                power_budget_cache: Default::default(),
            },
            frame_id: None,
            leaders: vec![],
        };
        state.stations.insert(station_b(), target);

        // Idle ship.
        state.ships.insert(
            ship_id(),
            ShipState {
                id: ship_id(),
                owner: owner(),
                position: test_position(),
                inventory: vec![],
                task: None,
                hull_id: HullId("hull_general_purpose".to_string()),
                fitted_modules: vec![],
                modifiers: Default::default(),
                propellant_kg: 0.0,
                propellant_capacity_kg: 0.0,
                cargo_capacity_m3: 100.0,
                speed_ticks_per_au: None,
                crew: BTreeMap::new(),
                leaders: vec![],
                home_station: None,
            },
        );

        state
    }

    #[test]
    fn surplus_at_a_deficit_at_b_generates_transfer() {
        let content = fleet_content();
        let state = fleet_state(&content);

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        let agent = &ship_agents[&ship_id()];
        match &agent.objective {
            Some(ShipObjective::Transfer {
                from_station,
                to_station,
                items,
            }) => {
                assert_eq!(*from_station, station_a());
                assert_eq!(*to_station, station_b());
                assert!(!items.is_empty());
                // Should transfer Fe (element with 30k vs 12k*2=24k surplus threshold,
                // and B has 0 which is below 12k*0.25=3k deficit threshold).
                assert!(matches!(
                    &items[0],
                    TradeItemSpec::Material { element, .. } if element == "Fe"
                ));
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn no_transfer_when_single_station() {
        let content = fleet_content();
        let mut state = fleet_state(&content);
        state.stations.remove(&station_b());

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        assert!(ship_agents[&ship_id()].objective.is_none());
    }

    #[test]
    fn no_transfer_when_no_surplus() {
        let content = fleet_content();
        let mut state = fleet_state(&content);
        // Remove the surplus Fe from station A.
        if let Some(station) = state.stations.get_mut(&station_a()) {
            station.core.inventory.retain(
                |item| !matches!(item, InventoryItem::Material { element, .. } if element == "Fe"),
            );
        }

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        assert!(ship_agents[&ship_id()].objective.is_none());
    }

    #[test]
    fn propellant_prioritized_over_materials() {
        let content = fleet_content();
        let mut state = fleet_state(&content);
        let propellant_element = content.autopilot.propellant_element.clone();
        let lh2_threshold = content.autopilot.lh2_threshold_kg;

        // Station A: surplus of both LH2 and Fe.
        if let Some(station) = state.stations.get_mut(&station_a()) {
            station.core.inventory.push(InventoryItem::Material {
                element: propellant_element.clone(),
                kg: lh2_threshold * 3.0, // surplus: 3x threshold > 2x threshold
                quality: 1.0,
                thermal: None,
            });
        }

        // Only one ship — should pick propellant (priority 0) over Fe (priority 2+).
        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        match &ship_agents[&ship_id()].objective {
            Some(ShipObjective::Transfer { items, .. }) => {
                assert!(
                    matches!(
                        &items[0],
                        TradeItemSpec::Material { element, .. } if *element == propellant_element
                    ),
                    "propellant should be transferred before Fe under scarcity"
                );
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn does_not_assign_already_claimed_ship() {
        let content = fleet_content();
        let state = fleet_state(&content);

        let mut ship_agents = BTreeMap::new();
        let mut agent = ShipAgent::new(ship_id());
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: sim_core::AsteroidId("asteroid_1".to_string()),
        });
        ship_agents.insert(ship_id(), agent);

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        assert!(matches!(
            &ship_agents[&ship_id()].objective,
            Some(ShipObjective::Mine { .. })
        ));
    }

    #[test]
    fn deterministic_same_state_same_plans() {
        let content = fleet_content();
        let state = fleet_state(&content);

        // Run twice with fresh ship agents.
        let run = || {
            let mut ship_agents = BTreeMap::new();
            ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));
            evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);
            format!("{:?}", ship_agents[&ship_id()].objective)
        };

        let result1 = run();
        let result2 = run();
        assert_eq!(result1, result2, "same state should produce same plans");
    }

    #[test]
    fn repair_kit_surplus_generates_component_transfer() {
        let content = fleet_content();
        let mut state = fleet_state(&content);
        let repair_kit_id = &content.autopilot.export_component.component_id;
        let reserve = content.autopilot.export_component.reserve;

        // Station A: surplus repair kits (above 2x reserve).
        if let Some(station) = state.stations.get_mut(&station_a()) {
            station.core.inventory.push(InventoryItem::Component {
                component_id: ComponentId(repair_kit_id.clone()),
                count: reserve * 3, // 3x reserve → surplus
                quality: 1.0,
            });
        }
        // Station B has 0 repair kits (below 50% reserve → deficit).

        // Remove Fe surplus so repair_kit is the only matching transfer.
        if let Some(station) = state.stations.get_mut(&station_a()) {
            station.core.inventory.retain(
                |item| !matches!(item, InventoryItem::Material { element, .. } if element == "Fe"),
            );
        }

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        match &ship_agents[&ship_id()].objective {
            Some(ShipObjective::Transfer {
                from_station,
                to_station,
                items,
            }) => {
                assert_eq!(*from_station, station_a());
                assert_eq!(*to_station, station_b());
                assert!(matches!(
                    &items[0],
                    TradeItemSpec::Component { component_id, count }
                        if component_id.0 == *repair_kit_id && *count > 0
                ));
            }
            other => panic!("expected Transfer with repair kits, got {other:?}"),
        }
    }

    #[test]
    fn mining_ship_excluded_from_transfers() {
        let content = fleet_content();
        let mut state = fleet_state(&content);
        // Replace the ship with a mining barge.
        state.ships.clear();
        let mining_ship = ShipState {
            id: ship_id(),
            owner: owner(),
            position: test_position(),
            inventory: vec![],
            task: None,
            hull_id: HullId("hull_mining_barge".to_string()),
            fitted_modules: vec![],
            modifiers: Default::default(),
            propellant_kg: 0.0,
            propellant_capacity_kg: 0.0,
            cargo_capacity_m3: 80.0,
            speed_ticks_per_au: None,
            crew: BTreeMap::new(),
            leaders: vec![],
            home_station: None,
        };
        state.ships.insert(ship_id(), mining_ship);

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        assert!(
            ship_agents[&ship_id()].objective.is_none(),
            "mining ships should not be assigned logistics transfers"
        );
    }

    #[test]
    fn logistics_ship_preferred_over_general_purpose() {
        let content = fleet_content();
        let mut state = fleet_state(&content);

        // Add a logistics ship alongside the existing general-purpose ship.
        let hauler_id = ShipId("ship_hauler_logistics".to_string());
        state.ships.insert(
            hauler_id.clone(),
            ShipState {
                id: hauler_id.clone(),
                owner: owner(),
                position: test_position(),
                inventory: vec![],
                task: None,
                hull_id: HullId("hull_transport_hauler".to_string()),
                fitted_modules: vec![],
                modifiers: Default::default(),
                propellant_kg: 0.0,
                propellant_capacity_kg: 0.0,
                cargo_capacity_m3: 200.0,
                speed_ticks_per_au: None,
                crew: BTreeMap::new(),
                leaders: vec![],
                home_station: None,
            },
        );

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));
        ship_agents.insert(hauler_id.clone(), ShipAgent::new(hauler_id.clone()));

        evaluate_and_assign(&mut ship_agents, &state, &content, &owner(), None);

        // The logistics ship should be assigned (it's preferred).
        assert!(
            ship_agents[&hauler_id].objective.is_some(),
            "logistics ship should be assigned a transfer"
        );
    }
}
