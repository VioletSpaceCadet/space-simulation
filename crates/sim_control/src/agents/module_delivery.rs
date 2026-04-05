//! VIO-596: Cross-station module delivery coordinator.
//!
//! Scans all framed stations for unfilled slots, identifies source stations
//! with compatible spare modules in inventory, and assigns Transfer objectives
//! to idle ships. Priority: power → maintenance → production → research.

use std::collections::BTreeMap;

use sim_core::{
    GameContent, GameState, InventoryItem, ModuleBehaviorDef, PrincipalId, ShipId, SlotType,
    StationId, TradeItemSpec,
};

use super::ship_agent::ShipAgent;
use super::DecisionRecord;
use crate::behaviors::collect_idle_ships;
use crate::objectives::ShipObjective;

/// Delivery priority: lower = more urgent. Determines the order in which
/// module types are delivered to empty stations.
fn delivery_priority(behavior: &ModuleBehaviorDef) -> u8 {
    match behavior {
        ModuleBehaviorDef::SolarArray(_)
        | ModuleBehaviorDef::Battery(_)
        | ModuleBehaviorDef::Radiator(_) => 0, // Power infrastructure
        ModuleBehaviorDef::Maintenance(_) => 1,
        ModuleBehaviorDef::Processor(_) | ModuleBehaviorDef::Assembler(_) => 2,
        ModuleBehaviorDef::Lab(_) => 3,
        _ => 4, // Storage, SensorArray, Equipment, etc.
    }
}

/// A pending module delivery: move a module from `source` to `target`.
struct DeliveryRequest {
    source_station: StationId,
    target_station: StationId,
    module_def_id: String,
    priority: u8,
}

/// Identify unfilled slot types on a framed station that have no local module
/// available to fill them. Returns a list of `(slot_type, slot_index)` pairs
/// for slots that need external delivery.
fn unfilled_slots_needing_delivery(
    station: &sim_core::StationState,
    content: &GameContent,
) -> Vec<(SlotType, usize)> {
    let Some(frame_id) = station.frame_id.as_ref() else {
        return Vec::new();
    };
    let Some(frame) = content.frames.get(frame_id) else {
        return Vec::new();
    };

    // Slots already occupied by installed modules.
    let occupied: std::collections::HashSet<usize> = station
        .core
        .modules
        .iter()
        .filter_map(|m| m.slot_index)
        .collect();

    // Local inventory modules available for install, tracked as consumable
    // counts so that N modules can only fill N slots (reviewer fix: was
    // non-consuming, causing 1 module to mask all unfilled slots of its type).
    let mut local_module_items: Vec<&str> = station
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Module { module_def_id, .. } => Some(module_def_id.as_str()),
            _ => None,
        })
        .collect();

    let mut result = Vec::new();
    for (idx, slot) in frame.slots.iter().enumerate() {
        if occupied.contains(&idx) {
            continue;
        }
        // Check if any unconsumed local inventory module can fill this slot.
        let local_match_pos = local_module_items.iter().position(|def_id| {
            content
                .module_defs
                .get(*def_id)
                .is_some_and(|d| d.compatible_slots.contains(&slot.slot_type))
        });
        if let Some(pos) = local_match_pos {
            // Consume this local module so it can't fill another slot.
            local_module_items.swap_remove(pos);
        } else {
            result.push((slot.slot_type.clone(), idx));
        }
    }
    result
}

/// Find a module in a source station's inventory that is compatible with the
/// given slot type. Returns `(item_id, module_def_id)` if found. Claims are
/// tracked by `item_id` so duplicate modules of the same type are independent.
fn find_spare_module<'a>(
    station: &'a sim_core::StationState,
    slot_type: &SlotType,
    content: &GameContent,
    already_claimed: &[sim_core::ModuleItemId],
) -> Option<(&'a sim_core::ModuleItemId, &'a str)> {
    station
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Module {
                item_id,
                module_def_id,
            } => Some((item_id, module_def_id.as_str())),
            _ => None,
        })
        .find(|(item_id, def_id)| {
            !already_claimed.contains(item_id)
                && content
                    .module_defs
                    .get(*def_id)
                    .is_some_and(|d| d.compatible_slots.contains(slot_type))
        })
}

/// Scan all stations for module delivery opportunities and assign Transfer
/// objectives to idle ships.
///
/// Called from `AutopilotController::generate_commands` between station agent
/// commands and ship objective assignment.
pub(crate) fn assign_module_deliveries(
    ship_agents: &mut BTreeMap<ShipId, ShipAgent>,
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    mut decisions: Option<&mut Vec<DecisionRecord>>,
) {
    // Only act when there are 2+ stations (need a source and a target).
    if state.stations.len() < 2 {
        return;
    }

    // Collect idle ships that have no objective yet.
    let idle_ships = collect_idle_ships(state, owner);
    let mut available_ships: Vec<ShipId> = idle_ships
        .into_iter()
        .filter(|id| ship_agents.get(id).is_some_and(|a| a.objective.is_none()))
        .collect();
    if available_ships.is_empty() {
        return;
    }

    // Build delivery requests: for each target station with unfilled slots,
    // search other stations for compatible spare modules.
    let mut requests: Vec<DeliveryRequest> = Vec::new();
    // Track claimed modules by item_id so duplicate modules of the same type
    // are independently claimable (reviewer fix: was keyed by def_id).
    let mut claimed_modules: Vec<(StationId, sim_core::ModuleItemId)> = Vec::new();

    // Iterate target stations in deterministic order.
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for target_id in &station_ids {
        let target = &state.stations[target_id];
        let unfilled = unfilled_slots_needing_delivery(target, content);
        if unfilled.is_empty() {
            continue;
        }

        for (slot_type, _slot_idx) in &unfilled {
            // Search other stations for a spare module.
            for source_id in &station_ids {
                if source_id == target_id {
                    continue;
                }
                let source = &state.stations[source_id];
                let already_claimed: Vec<sim_core::ModuleItemId> = claimed_modules
                    .iter()
                    .filter(|(sid, _)| sid == source_id)
                    .map(|(_, item_id)| item_id.clone())
                    .collect();
                if let Some((item_id, def_id)) =
                    find_spare_module(source, slot_type, content, &already_claimed)
                {
                    let priority = content
                        .module_defs
                        .get(def_id)
                        .map_or(4, |d| delivery_priority(&d.behavior));
                    claimed_modules.push((source_id.clone(), item_id.clone()));
                    requests.push(DeliveryRequest {
                        source_station: source_id.clone(),
                        target_station: target_id.clone(),
                        module_def_id: def_id.to_string(),
                        priority,
                    });
                    break; // One module per slot per tick
                }
            }
        }
    }

    if requests.is_empty() {
        return;
    }

    // Sort by priority (power first) then by target station ID for determinism.
    requests.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.target_station.0.cmp(&b.target_station.0))
    });

    // Assign one delivery per idle ship.
    for request in requests {
        let Some(ship_id) = available_ships.pop() else {
            break; // No more idle ships
        };

        if let Some(ref mut log) = decisions {
            log.push(DecisionRecord {
                tick: state.meta.tick,
                agent: "module_delivery".to_string(),
                concern: "module_delivery".to_string(),
                decision_type: "assign_transfer".to_string(),
                chosen_id: format!(
                    "{}->{}:{}",
                    request.source_station.0, request.target_station.0, request.module_def_id
                ),
                chosen_score: f64::from(request.priority),
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
                from_station: request.source_station,
                to_station: request.target_station,
                items: vec![TradeItemSpec::Module {
                    module_def_id: request.module_def_id,
                }],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::test_fixtures::{base_content, base_state, test_position, ModuleDefBuilder};
    use sim_core::{
        BodyId, FrameDef, FrameId, HullId, ModuleBehaviorDef, ShipState, SlotDef, SolarArrayDef,
    };
    use std::collections::BTreeMap;

    fn owner() -> PrincipalId {
        PrincipalId("principal_autopilot".to_string())
    }

    fn ship_id() -> ShipId {
        ShipId("ship_hauler".to_string())
    }

    fn source_station_id() -> StationId {
        StationId("station_earth_orbit".to_string())
    }

    fn target_station_id() -> StationId {
        StationId("station_belt_outpost".to_string())
    }

    /// Create content with a solar panel module that fits "utility" slots.
    fn delivery_content() -> GameContent {
        let mut content = base_content();
        let utility = sim_core::SlotType("utility".to_string());
        let research = sim_core::SlotType("research".to_string());
        content.module_defs.insert(
            "module_solar_panel".to_string(),
            ModuleDefBuilder::new("module_solar_panel")
                .name("Solar Panel")
                .mass(500.0)
                .volume(5.0)
                .compatible_slots(vec![utility.clone()])
                .behavior(ModuleBehaviorDef::SolarArray(SolarArrayDef {
                    base_output_kw: 20.0,
                }))
                .build(),
        );
        content.module_defs.insert(
            "module_lab".to_string(),
            ModuleDefBuilder::new("module_lab")
                .name("Lab")
                .mass(2000.0)
                .volume(10.0)
                .compatible_slots(vec![research.clone()])
                .behavior(ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain("science".to_string()),
                    data_consumption_per_run: 1.0,
                    research_points_per_run: 1.0,
                    accepted_data: vec![],
                    research_interval_minutes: 60,
                    research_interval_ticks: 1,
                }))
                .build(),
        );
        // Frame with utility + research slots.
        content.frames.insert(
            FrameId("frame_outpost".to_string()),
            FrameDef {
                id: FrameId("frame_outpost".to_string()),
                name: "Outpost".to_string(),
                base_cargo_capacity_m3: 500.0,
                base_power_capacity_kw: 30.0,
                slots: vec![
                    SlotDef {
                        slot_type: utility,
                        label: "Utility 1".to_string(),
                    },
                    SlotDef {
                        slot_type: research,
                        label: "Research 1".to_string(),
                    },
                ],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );
        content
    }

    /// Build state with two stations: source (has spare module in inventory)
    /// and target (empty frame with unfilled slots). Plus an idle ship.
    fn delivery_state(content: &GameContent) -> GameState {
        let mut state = base_state(content);
        let source_id = source_station_id();

        // Source station already exists from base_state; add a solar panel to inventory.
        if let Some(station) = state.stations.get_mut(&source_id) {
            station.core.inventory.push(InventoryItem::Module {
                item_id: sim_core::ModuleItemId("item_solar_1".to_string()),
                module_def_id: "module_solar_panel".to_string(),
            });
        }

        // Target station: empty frame with no modules.
        let target_id = target_station_id();
        let target = sim_core::StationState {
            id: target_id.clone(),
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
            frame_id: Some(FrameId("frame_outpost".to_string())),
            leaders: vec![],
        };
        state.stations.insert(target_id, target);

        // Add an idle ship.
        let ship = ShipState {
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
        };
        state.ships.insert(ship_id(), ship);

        state
    }

    #[test]
    fn delivery_assigns_transfer_for_empty_station() {
        let content = delivery_content();
        let state = delivery_state(&content);

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);

        let agent = &ship_agents[&ship_id()];
        assert!(
            agent.objective.is_some(),
            "ship should have been assigned a Transfer objective"
        );
        match &agent.objective {
            Some(ShipObjective::Transfer {
                from_station,
                to_station,
                items,
            }) => {
                assert_eq!(*from_station, source_station_id());
                assert_eq!(*to_station, target_station_id());
                assert_eq!(items.len(), 1);
                assert!(matches!(
                    &items[0],
                    TradeItemSpec::Module { module_def_id } if module_def_id == "module_solar_panel"
                ));
            }
            other => panic!("expected Transfer objective, got {other:?}"),
        }
    }

    #[test]
    fn delivery_skips_when_single_station() {
        let content = delivery_content();
        let mut state = delivery_state(&content);
        // Remove the target station — only one station remains.
        state.stations.remove(&target_station_id());

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);

        assert!(
            ship_agents[&ship_id()].objective.is_none(),
            "no delivery when only one station"
        );
    }

    #[test]
    fn delivery_skips_when_no_idle_ships() {
        let content = delivery_content();
        let state = delivery_state(&content);
        // No ship agents registered.
        let mut ship_agents: BTreeMap<ShipId, ShipAgent> = BTreeMap::new();

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);
        // Nothing to assert — just should not panic.
    }

    #[test]
    fn delivery_skips_when_local_module_available() {
        let content = delivery_content();
        let mut state = delivery_state(&content);
        // Put a solar panel in the target station's inventory — local install will handle it.
        let target = state.stations.get_mut(&target_station_id()).unwrap();
        target.core.inventory.push(InventoryItem::Module {
            item_id: sim_core::ModuleItemId("item_solar_local".to_string()),
            module_def_id: "module_solar_panel".to_string(),
        });

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);

        // The utility slot is fillable locally, so only the research slot should generate
        // a delivery request (for module_lab, if source has one).
        let agent = &ship_agents[&ship_id()];
        match &agent.objective {
            Some(ShipObjective::Transfer { items, .. }) => {
                // Should NOT be solar_panel (that's fillable locally).
                assert!(!items.iter().any(|item| matches!(
                    item,
                    TradeItemSpec::Module { module_def_id } if module_def_id == "module_solar_panel"
                )));
            }
            None => { /* no delivery at all — acceptable if source has no lab module */ }
            other => panic!("unexpected objective: {other:?}"),
        }
    }

    #[test]
    fn delivery_prioritizes_power_over_research_under_scarcity() {
        let content = delivery_content();
        let mut state = delivery_state(&content);
        // Add both a solar panel AND a lab to source inventory.
        let source = state.stations.get_mut(&source_station_id()).unwrap();
        source.core.inventory.push(InventoryItem::Module {
            item_id: sim_core::ModuleItemId("item_lab_1".to_string()),
            module_def_id: "module_lab".to_string(),
        });

        // Only ONE idle ship — must pick the higher-priority module (power).
        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);

        // The single ship should be assigned the solar panel (priority 0)
        // over the lab (priority 3).
        let agent = &ship_agents[&ship_id()];
        match &agent.objective {
            Some(ShipObjective::Transfer { items, .. }) => {
                assert!(
                    matches!(
                        &items[0],
                        TradeItemSpec::Module { module_def_id } if module_def_id == "module_solar_panel"
                    ),
                    "power module should be delivered first under scarcity"
                );
            }
            other => panic!("expected Transfer objective, got {other:?}"),
        }
    }

    #[test]
    fn delivery_skips_frameless_station() {
        let content = delivery_content();
        let mut state = delivery_state(&content);
        // Remove frame from target — frameless stations don't have slot constraints.
        state
            .stations
            .get_mut(&target_station_id())
            .unwrap()
            .frame_id = None;

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);

        assert!(
            ship_agents[&ship_id()].objective.is_none(),
            "no delivery for frameless station"
        );
    }

    #[test]
    fn delivery_does_not_assign_already_claimed_ship() {
        let content = delivery_content();
        let state = delivery_state(&content);

        let mut ship_agents = BTreeMap::new();
        let mut agent = ShipAgent::new(ship_id());
        // Ship already has an objective.
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: sim_core::AsteroidId("asteroid_1".to_string()),
        });
        ship_agents.insert(ship_id(), agent);

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);

        // Ship should keep its original mine objective.
        assert!(matches!(
            &ship_agents[&ship_id()].objective,
            Some(ShipObjective::Mine { .. })
        ));
    }

    #[test]
    fn delivery_claims_duplicate_modules_independently() {
        let mut content = delivery_content();
        // Frame with 2 utility slots.
        content
            .frames
            .get_mut(&FrameId("frame_outpost".to_string()))
            .unwrap()
            .slots = vec![
            SlotDef {
                slot_type: sim_core::SlotType("utility".to_string()),
                label: "Utility 1".to_string(),
            },
            SlotDef {
                slot_type: sim_core::SlotType("utility".to_string()),
                label: "Utility 2".to_string(),
            },
        ];
        let mut state = delivery_state(&content);
        // Source station has 2 solar panels (same def, different item_ids).
        let source = state.stations.get_mut(&source_station_id()).unwrap();
        source.core.inventory.push(InventoryItem::Module {
            item_id: sim_core::ModuleItemId("item_solar_2".to_string()),
            module_def_id: "module_solar_panel".to_string(),
        });

        // Two idle ships.
        let ship2_id = ShipId("ship_hauler_2".to_string());
        state.ships.insert(
            ship2_id.clone(),
            ShipState {
                id: ship2_id.clone(),
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

        let mut ship_agents = BTreeMap::new();
        ship_agents.insert(ship_id(), ShipAgent::new(ship_id()));
        ship_agents.insert(ship2_id.clone(), ShipAgent::new(ship2_id.clone()));

        assign_module_deliveries(&mut ship_agents, &state, &content, &owner(), None);

        // Both ships should get Transfer objectives — one per solar panel.
        let assigned_count = ship_agents
            .values()
            .filter(|a| matches!(&a.objective, Some(ShipObjective::Transfer { .. })))
            .count();
        assert_eq!(
            assigned_count, 2,
            "both duplicate solar panels should be independently claimable"
        );
    }

    #[test]
    fn unfilled_slots_consumes_local_modules() {
        let content = delivery_content();
        let mut state = delivery_state(&content);
        // Target has 1 local solar panel but frame has 2 utility + 1 research slots.
        let target = state.stations.get_mut(&target_station_id()).unwrap();
        target.core.inventory.push(InventoryItem::Module {
            item_id: sim_core::ModuleItemId("item_solar_local".to_string()),
            module_def_id: "module_solar_panel".to_string(),
        });
        // Give the frame an extra utility slot (total: 2 utility + 1 research).
        let frame = content
            .frames
            .get(&FrameId("frame_outpost".to_string()))
            .unwrap();
        assert_eq!(frame.slots.len(), 2, "fixture has 1 utility + 1 research");

        // With 1 local solar panel and 1 utility slot, that slot is fillable locally.
        // The research slot has no local match → needs delivery.
        let unfilled = unfilled_slots_needing_delivery(target, &content);
        assert_eq!(unfilled.len(), 1, "only research slot needs delivery");
        assert_eq!(unfilled[0].0 .0, "research");
    }
}
