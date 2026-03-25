//! Command handler functions for `apply_commands`.
//!
//! Each public function handles one `Command` variant. Handlers return `false`
//! when the command should be skipped (invalid target, insufficient resources,
//! etc.) — the caller uses this to `continue` the command loop.

use crate::tasks::{deep_scan_enabled, inventory_volume_m3};
use crate::{
    trade, EventEnvelope, FittedModule, GameContent, GameState, InventoryItem, ModuleDefId, ShipId,
    StationId, TaskKind, TaskState,
};
use rand::Rng;

use crate::engine::trade_unlock_tick;

/// Validate an `AssignShipTask` command and collect it into the assignments vec
/// for deferred processing. Returns `false` if the command should be skipped.
pub(crate) fn handle_assign_ship_task(
    state: &GameState,
    content: &GameContent,
    ship_id: &ShipId,
    task_kind: &TaskKind,
    issued_by: &crate::PrincipalId,
    assignments: &mut Vec<(ShipId, TaskKind)>,
) -> bool {
    let Some(ship) = state.ships.get(ship_id) else {
        return false;
    };
    if ship.owner != *issued_by {
        return false;
    }
    if matches!(task_kind, TaskKind::DeepScan { .. })
        && !deep_scan_enabled(&state.research, content)
    {
        return false;
    }
    assignments.push((ship_id.clone(), task_kind.clone()));
    true
}

/// Build the default `ModuleKindState`, `BehaviorType`, and optional `ThermalState` for a module.
fn default_module_state(
    def: &crate::ModuleDef,
    content: &GameContent,
) -> (
    crate::ModuleKindState,
    crate::BehaviorType,
    Option<crate::ThermalState>,
) {
    let thermal_state = def.thermal.as_ref().map(|td| crate::ThermalState {
        temp_mk: content.constants.thermal_sink_temp_mk,
        thermal_group: td.thermal_group.clone(),
        ..Default::default()
    });
    let (kind_state, behavior_type) = def.behavior.default_state();
    (kind_state, behavior_type, thermal_state)
}

/// Install a module from station inventory into the station's active modules.
pub(crate) fn handle_install_module(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    module_item_id: &crate::ModuleItemId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let item_pos = station.inventory.iter().position(
        |i| matches!(i, InventoryItem::Module { item_id, .. } if item_id == module_item_id),
    );
    let Some(pos) = item_pos else { return false };
    let InventoryItem::Module {
        item_id,
        module_def_id,
    } = station.inventory.remove(pos)
    else {
        return false;
    };
    station.invalidate_volume_cache();

    let module_id_str = format!("module_inst_{:04}", state.counters.next_module_instance_id);
    state.counters.next_module_instance_id += 1;
    let module_id = crate::ModuleInstanceId(module_id_str);

    let Some(def) = content.module_defs.get(&module_def_id) else {
        return false;
    };
    let (kind_state, behavior_type, thermal) = default_module_state(def, content);

    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.modules.push(crate::ModuleState {
        id: module_id.clone(),
        def_id: module_def_id.clone(),
        enabled: false,
        kind_state,
        wear: crate::WearState::default(),
        thermal,
        power_stalled: false,
        manufacturing_priority: 0,
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleInstalled {
            station_id: station_id.clone(),
            module_id,
            module_item_id: item_id,
            module_def_id,
            behavior_type,
        },
    ));
    true
}

/// Uninstall a module from the station and return it to inventory.
pub(crate) fn handle_uninstall_module(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let pos = station.modules.iter().position(|m| &m.id == module_id);
    let Some(pos) = pos else { return false };
    let module = station.modules.remove(pos);

    let item_id = crate::ModuleItemId(format!(
        "module_item_{:04}",
        state.counters.next_module_instance_id
    ));
    state.counters.next_module_instance_id += 1;

    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.inventory.push(InventoryItem::Module {
        item_id: item_id.clone(),
        module_def_id: module.def_id.clone(),
    });
    station.invalidate_volume_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleUninstalled {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            module_item_id: item_id,
        },
    ));
    true
}

/// Toggle the enabled flag on a module.
pub(crate) fn handle_set_module_enabled(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    enabled: bool,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    module.enabled = enabled;
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleToggled {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            enabled,
        },
    ));
    true
}

/// Set the `threshold_kg` on a processor module.
pub(crate) fn handle_set_module_threshold(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    threshold_kg: f32,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    if let crate::ModuleKindState::Processor(ps) = &mut module.kind_state {
        ps.threshold_kg = threshold_kg;
    }
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleThresholdSet {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            threshold_kg,
        },
    ));
    true
}

/// Assign a tech to a lab module.
pub(crate) fn handle_assign_lab_tech(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    tech_id: Option<&crate::TechId>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    if let crate::ModuleKindState::Lab(ls) = &mut module.kind_state {
        ls.assigned_tech = tech_id.cloned();
    }
    true
}

/// Select a recipe on a processor or assembler module.
pub(crate) fn handle_select_recipe(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    recipe_id: &crate::RecipeId,
) -> bool {
    // Recipe must exist in the catalog
    if !content.recipes.contains_key(recipe_id) {
        return false;
    }
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    let Some(def) = content.module_defs.get(&module.def_id) else {
        return false;
    };
    match (&mut module.kind_state, &def.behavior) {
        (crate::ModuleKindState::Processor(ps), crate::ModuleBehaviorDef::Processor(proc_def)) => {
            if !proc_def.recipes.contains(recipe_id) {
                return false;
            }
            ps.selected_recipe = Some(recipe_id.clone());
        }
        (crate::ModuleKindState::Assembler(asmb), crate::ModuleBehaviorDef::Assembler(asm_def)) => {
            if !asm_def.recipes.contains(recipe_id) {
                return false;
            }
            asmb.selected_recipe = Some(recipe_id.clone());
        }
        _ => return false,
    }
    true
}

/// Set the cap override on an assembler module.
pub(crate) fn handle_set_assembler_cap(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    component_id: &crate::ComponentId,
    max_stock: u32,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    if let crate::ModuleKindState::Assembler(asmb) = &mut module.kind_state {
        asmb.cap_override.insert(component_id.clone(), max_stock);
    }
    true
}

/// Import items into a station via trade.
pub(crate) fn handle_import(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    item_spec: &crate::TradeItemSpec,
    current_tick: u64,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    if current_tick < trade_unlock_tick(&content.constants) {
        return false;
    }
    if !state.stations.contains_key(station_id) {
        return false;
    }

    // Look up pricing and compute cost
    let Some(cost) = trade::compute_import_cost(item_spec, &content.pricing, content) else {
        return false; // not importable or unknown item
    };

    // Check balance
    if state.balance < cost {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::InsufficientFunds {
                station_id: station_id.clone(),
                action: format!("import {}", item_spec.pricing_key()),
                required: cost,
                available: state.balance,
            },
        ));
        return false;
    }

    // Check cargo capacity
    let new_items = trade::create_inventory_items(item_spec, rng);
    let new_volume = inventory_volume_m3(&new_items, content);
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let current_volume = station.used_volume_m3(content);
    let cargo_cap = station.cargo_capacity_m3;
    if current_volume + new_volume > cargo_cap {
        return false; // no room
    }

    // Execute import
    state.balance -= cost;
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    trade::merge_into_inventory(&mut station.inventory, new_items);
    station.invalidate_volume_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ItemImported {
            station_id: station_id.clone(),
            item_spec: item_spec.clone(),
            cost,
            balance_after: state.balance,
        },
    ));
    true
}

/// Export items from a station via trade.
pub(crate) fn handle_export(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    item_spec: &crate::TradeItemSpec,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    if current_tick < trade_unlock_tick(&content.constants) {
        return false;
    }
    let Some(station) = state.stations.get(station_id) else {
        return false;
    };

    // Look up pricing and compute revenue
    let Some(revenue) = trade::compute_export_revenue(item_spec, &content.pricing, content) else {
        return false; // not exportable or unknown item
    };

    // Check station has items
    if !trade::has_enough_for_export(&station.inventory, item_spec) {
        return false;
    }

    // Execute export
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    if !trade::remove_inventory_items(&mut station.inventory, item_spec) {
        return false;
    }
    station.invalidate_volume_cache();
    state.balance += revenue;
    state.export_revenue_total += revenue;
    state.export_count += 1;

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ItemExported {
            station_id: station_id.clone(),
            item_spec: item_spec.clone(),
            revenue,
            balance_after: state.balance,
        },
    ));
    true
}

/// Jettison all slag from a station's inventory.
pub(crate) fn handle_jettison_slag(
    state: &mut GameState,
    station_id: &crate::StationId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let jettisoned_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Slag { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    station
        .inventory
        .retain(|i| !matches!(i, InventoryItem::Slag { .. }));
    station.invalidate_volume_cache();
    if jettisoned_kg > 0.0 {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::SlagJettisoned {
                station_id: station_id.clone(),
                kg: jettisoned_kg,
            },
        ));
    }
    true
}

/// Set the manufacturing priority on a module (processor or assembler).
pub(crate) fn handle_set_manufacturing_priority(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    priority: u32,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    module.manufacturing_priority = priority;
    true
}

/// Recompute ship cached stats (cargo, speed, propellant capacity) from hull + fitted modules.
pub fn recompute_ship_stats(ship: &mut crate::ShipState, content: &GameContent) {
    use crate::modifiers::{ModifierSource, StatId};

    let hull = content
        .hulls
        .get(&ship.hull_id)
        .expect("ship references valid hull_id");

    // Clear all hull + fitted modifiers, then rebuild
    ship.modifiers.remove_where(|s| {
        matches!(
            s,
            ModifierSource::Hull(_) | ModifierSource::FittedModule(_, _)
        )
    });

    // Apply hull bonuses
    for bonus in &hull.bonuses {
        let mut modifier = bonus.clone();
        modifier.source = ModifierSource::Hull(ship.hull_id.clone());
        ship.modifiers.add(modifier);
    }

    // Apply fitted module modifiers
    for fitted in &ship.fitted_modules {
        if let Some(def) = content.module_defs.get(&fitted.module_def_id.0) {
            for ship_modifier in &def.ship_modifiers {
                let mut modifier = ship_modifier.clone();
                modifier.source =
                    ModifierSource::FittedModule(fitted.module_def_id.clone(), fitted.slot_index);
                ship.modifiers.add(modifier);
            }
        }
    }

    // Recompute cached stats
    ship.cargo_capacity_m3 = ship
        .modifiers
        .resolve_f32(StatId::CargoCapacity, hull.cargo_capacity_m3);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamp guards
    {
        ship.speed_ticks_per_au = Some(
            ship.modifiers
                .resolve(StatId::ShipSpeed, hull.base_speed_ticks_per_au as f64)
                .clamp(0.0, u64::MAX as f64) as u64,
        );
    }
    ship.propellant_capacity_kg = ship
        .modifiers
        .resolve_f32(StatId::PropellantCapacity, hull.base_propellant_capacity_kg);
    ship.propellant_kg = ship.propellant_kg.min(ship.propellant_capacity_kg);
}

/// Fit a ship module into a hull slot at the given station.
pub(crate) fn handle_fit_ship_module(
    state: &mut GameState,
    content: &GameContent,
    ship_id: &ShipId,
    slot_index: usize,
    module_def_id: &ModuleDefId,
    station_id: &StationId,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    // Ship must exist and be at the same station location
    let Some(ship) = state.ships.get(ship_id) else {
        return false;
    };
    let Some(station) = state.stations.get(station_id) else {
        return false;
    };
    if ship.position != station.position {
        return false;
    }
    // Ship must be idle (None = freshly constructed, Some(Idle) = completed task)
    if ship
        .task
        .as_ref()
        .is_some_and(|t| !matches!(t.kind, crate::TaskKind::Idle))
    {
        return false;
    }
    // Hull must exist and slot_index must be valid
    let Some(hull) = content.hulls.get(&ship.hull_id) else {
        return false;
    };
    let Some(slot_def) = hull.slots.get(slot_index) else {
        return false;
    };
    // Slot must not already be occupied
    if ship
        .fitted_modules
        .iter()
        .any(|fm| fm.slot_index == slot_index)
    {
        return false;
    }
    // Module def must exist and be compatible with the slot type
    let Some(module_def) = content.module_defs.get(&module_def_id.0) else {
        return false;
    };
    if !module_def.compatible_slots.contains(&slot_def.slot_type) {
        return false;
    }
    // Station must have an InventoryItem::Module with matching module_def_id
    let item_pos = station.inventory.iter().position(|item| {
        matches!(item, InventoryItem::Module { module_def_id: def_id, .. } if *def_id == module_def_id.0)
    });
    let Some(pos) = item_pos else { return false };

    // Execute: remove module from station inventory
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.inventory.remove(pos);
    station.invalidate_volume_cache();

    // Add FittedModule to ship
    let Some(ship) = state.ships.get_mut(ship_id) else {
        return false;
    };
    ship.fitted_modules.push(FittedModule {
        slot_index,
        module_def_id: module_def_id.clone(),
    });
    recompute_ship_stats(ship, content);

    events.push(crate::emit(
        &mut state.counters,
        state.meta.tick,
        crate::Event::ShipModuleFitted {
            ship_id: ship_id.clone(),
            slot_index,
            module_def_id: module_def_id.clone(),
            station_id: station_id.clone(),
        },
    ));
    true
}

/// Unfit a ship module from a hull slot, returning it to station inventory.
pub(crate) fn handle_unfit_ship_module(
    state: &mut GameState,
    content: &GameContent,
    ship_id: &ShipId,
    slot_index: usize,
    station_id: &StationId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    // Ship must exist and be at the same station location
    let Some(ship) = state.ships.get(ship_id) else {
        return false;
    };
    let Some(station) = state.stations.get(station_id) else {
        return false;
    };
    if ship.position != station.position {
        return false;
    }
    // Ship must be idle (None = freshly constructed, Some(Idle) = completed task)
    if ship
        .task
        .as_ref()
        .is_some_and(|t| !matches!(t.kind, crate::TaskKind::Idle))
    {
        return false;
    }
    // Hull must exist in content
    if !content.hulls.contains_key(&ship.hull_id) {
        return false;
    }
    // Slot must have a fitted module
    let Some(fitted_pos) = ship
        .fitted_modules
        .iter()
        .position(|fm| fm.slot_index == slot_index)
    else {
        return false;
    };

    // Execute: remove FittedModule from ship
    let Some(ship) = state.ships.get_mut(ship_id) else {
        return false;
    };
    let removed = ship.fitted_modules.remove(fitted_pos);
    recompute_ship_stats(ship, content);

    // Create a fresh module item and add to station inventory
    let item_id = crate::ModuleItemId(format!(
        "module_item_{:04}",
        state.counters.next_module_instance_id
    ));
    state.counters.next_module_instance_id += 1;

    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.inventory.push(InventoryItem::Module {
        item_id: item_id.clone(),
        module_def_id: removed.module_def_id.0.clone(),
    });
    station.invalidate_volume_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ShipModuleUnfitted {
            ship_id: ship_id.clone(),
            slot_index,
            module_def_id: removed.module_def_id,
            station_id: station_id.clone(),
        },
    ));
    true
}

/// Apply deferred ship task assignments collected during the command loop.
pub(crate) fn apply_ship_assignments(
    state: &mut GameState,
    content: &GameContent,
    assignments: Vec<(ShipId, TaskKind)>,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) {
    for (ship_id, task_kind) in assignments {
        let duration = task_kind.duration(&content.constants);
        let label = task_kind.label().to_string();
        let target = task_kind.target();

        if let Some(ship) = state.ships.get_mut(&ship_id) {
            ship.task = Some(TaskState {
                kind: task_kind,
                started_tick: current_tick,
                eta_tick: current_tick + duration,
            });
        }

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::TaskStarted {
                ship_id,
                task_kind: label,
                target,
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifiers::{ModifierSource, StatId};
    use crate::test_fixtures::{base_content, base_state};
    use crate::{
        FittedModule, HullDef, HullId, InventoryItem, ModuleDefId, ModuleItemId, SlotDef, SlotType,
    };
    use std::collections::BTreeMap;

    fn content_with_hull() -> GameContent {
        let mut content = base_content();
        let mut hulls = BTreeMap::new();
        hulls.insert(
            HullId("hull_general_purpose".to_string()),
            HullDef {
                id: HullId("hull_general_purpose".to_string()),
                name: "General Purpose".to_string(),
                mass_kg: 5000.0,
                cargo_capacity_m3: 50.0,
                base_speed_ticks_per_au: 120,
                base_propellant_capacity_kg: 10000.0,
                slots: vec![
                    SlotDef {
                        slot_type: SlotType("utility".to_string()),
                        label: "Utility 1".to_string(),
                    },
                    SlotDef {
                        slot_type: SlotType("industrial".to_string()),
                        label: "Industrial 1".to_string(),
                    },
                ],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );
        content.hulls = hulls;

        // Add an equipment module def compatible with utility slots
        content.module_defs.insert(
            "module_cargo_expander".to_string(),
            crate::ModuleDef {
                id: "module_cargo_expander".to_string(),
                name: "Cargo Expander".to_string(),
                mass_kg: 500.0,
                volume_m3: 2.0,
                power_consumption_per_run: 0.0,
                wear_per_run: 0.0,
                behavior: crate::ModuleBehaviorDef::Equipment,
                thermal: None,
                compatible_slots: vec![SlotType("utility".to_string())],
                ship_modifiers: vec![crate::modifiers::Modifier::pct_mult(
                    StatId::CargoCapacity,
                    1.3,
                    ModifierSource::Equipment("cargo_expander".to_string()),
                )],
                power_stall_priority: None,
                roles: vec![],
            },
        );
        content
    }

    fn state_with_module_in_inventory(content: &GameContent) -> GameState {
        let mut state = base_state(content);
        let station = state.stations.values_mut().next().unwrap();
        station.inventory.push(InventoryItem::Module {
            item_id: ModuleItemId("mod_item_0001".to_string()),
            module_def_id: "module_cargo_expander".to_string(),
        });
        state
    }

    #[test]
    fn recompute_ship_stats_applies_hull_base() {
        let content = content_with_hull();
        let mut state = base_state(&content);
        let ship = state.ships.values_mut().next().unwrap();
        recompute_ship_stats(ship, &content);

        assert!((ship.cargo_capacity_m3 - 50.0).abs() < 0.01);
        assert_eq!(ship.speed_ticks_per_au, Some(120));
        assert!((ship.propellant_capacity_kg - 10000.0).abs() < 0.01);
    }

    #[test]
    fn fit_ship_module_success() {
        let content = content_with_hull();
        let mut state = state_with_module_in_inventory(&content);
        let ship_id = crate::ShipId("ship_0001".to_string());
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let module_def_id = ModuleDefId("module_cargo_expander".to_string());
        let mut events = vec![];

        let result = handle_fit_ship_module(
            &mut state,
            &content,
            &ship_id,
            0, // utility slot
            &module_def_id,
            &station_id,
            &mut events,
        );

        assert!(result);
        let ship = state.ships.get(&ship_id).unwrap();
        assert_eq!(ship.fitted_modules.len(), 1);
        assert_eq!(ship.fitted_modules[0].slot_index, 0);
        // Cargo should be 50 * 1.3 = 65 from the modifier
        assert!((ship.cargo_capacity_m3 - 65.0).abs() < 0.1);
        // Module removed from station inventory
        let station = state.stations.get(&station_id).unwrap();
        assert!(!station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Module { .. })));
        // Event emitted
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].event,
            crate::Event::ShipModuleFitted { .. }
        ));
    }

    #[test]
    fn fit_ship_module_wrong_slot_type_rejected() {
        let content = content_with_hull();
        let mut state = state_with_module_in_inventory(&content);
        let ship_id = crate::ShipId("ship_0001".to_string());
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let module_def_id = ModuleDefId("module_cargo_expander".to_string());
        let mut events = vec![];

        // Slot 1 is "industrial", module is compatible with "utility" only
        let result = handle_fit_ship_module(
            &mut state,
            &content,
            &ship_id,
            1,
            &module_def_id,
            &station_id,
            &mut events,
        );

        assert!(!result);
        assert!(events.is_empty());
    }

    #[test]
    fn fit_ship_module_occupied_slot_rejected() {
        let content = content_with_hull();
        let mut state = state_with_module_in_inventory(&content);
        let ship_id = crate::ShipId("ship_0001".to_string());
        let station_id = crate::StationId("station_earth_orbit".to_string());
        let module_def_id = ModuleDefId("module_cargo_expander".to_string());

        // Pre-fit a module into slot 0
        let ship = state.ships.get_mut(&ship_id).unwrap();
        ship.fitted_modules.push(FittedModule {
            slot_index: 0,
            module_def_id: ModuleDefId("something_else".to_string()),
        });

        let mut events = vec![];
        let result = handle_fit_ship_module(
            &mut state,
            &content,
            &ship_id,
            0,
            &module_def_id,
            &station_id,
            &mut events,
        );

        assert!(!result);
    }

    #[test]
    fn unfit_ship_module_success() {
        let content = content_with_hull();
        let mut state = base_state(&content);
        let ship_id = crate::ShipId("ship_0001".to_string());
        let station_id = crate::StationId("station_earth_orbit".to_string());

        // Pre-fit a module
        let ship = state.ships.get_mut(&ship_id).unwrap();
        ship.fitted_modules.push(FittedModule {
            slot_index: 0,
            module_def_id: ModuleDefId("module_cargo_expander".to_string()),
        });
        recompute_ship_stats(ship, &content);
        assert!((ship.cargo_capacity_m3 - 65.0).abs() < 0.1);

        let mut events = vec![];
        let result = handle_unfit_ship_module(
            &mut state,
            &content,
            &ship_id,
            0,
            &station_id,
            1,
            &mut events,
        );

        assert!(result);
        let ship = state.ships.get(&ship_id).unwrap();
        assert!(ship.fitted_modules.is_empty());
        // Stats reverted to hull base
        assert!((ship.cargo_capacity_m3 - 50.0).abs() < 0.1);
        // Module returned to station
        let station = state.stations.get(&station_id).unwrap();
        assert!(station.inventory.iter().any(|i| matches!(i, InventoryItem::Module { module_def_id, .. } if module_def_id == "module_cargo_expander")));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].event,
            crate::Event::ShipModuleUnfitted { .. }
        ));
    }

    #[test]
    fn hull_bonus_persists_through_fit_unfit_cycle() {
        let mut content = content_with_hull();
        // Add a mining barge hull with MiningRate +25% bonus
        content.hulls.insert(
            HullId("hull_mining_barge".to_string()),
            HullDef {
                id: HullId("hull_mining_barge".to_string()),
                name: "Mining Barge".to_string(),
                mass_kg: 8000.0,
                cargo_capacity_m3: 80.0,
                base_speed_ticks_per_au: 180,
                base_propellant_capacity_kg: 8000.0,
                slots: vec![
                    SlotDef {
                        slot_type: SlotType("industrial".to_string()),
                        label: "Industrial 1".to_string(),
                    },
                    SlotDef {
                        slot_type: SlotType("utility".to_string()),
                        label: "Utility 1".to_string(),
                    },
                ],
                bonuses: vec![crate::modifiers::Modifier::pct_mult(
                    StatId::MiningRate,
                    1.25,
                    ModifierSource::Hull(HullId("hull_mining_barge".to_string())),
                )],
                required_tech: None,
                tags: vec![],
            },
        );
        // Add a mining laser equipment module
        content.module_defs.insert(
            "module_mining_laser".to_string(),
            crate::ModuleDef {
                id: "module_mining_laser".to_string(),
                name: "Mining Laser".to_string(),
                mass_kg: 800.0,
                volume_m3: 3.0,
                power_consumption_per_run: 0.0,
                wear_per_run: 0.0,
                behavior: crate::ModuleBehaviorDef::Equipment,
                thermal: None,
                compatible_slots: vec![SlotType("industrial".to_string())],
                ship_modifiers: vec![crate::modifiers::Modifier::pct_mult(
                    StatId::MiningRate,
                    1.2,
                    ModifierSource::Equipment("mining_laser".to_string()),
                )],
                power_stall_priority: None,
                roles: vec![],
            },
        );

        let mut state = base_state(&content);
        let ship_id = crate::ShipId("ship_0001".to_string());
        let station_id = crate::StationId("station_earth_orbit".to_string());

        // Set ship hull to mining barge
        let ship = state.ships.get_mut(&ship_id).unwrap();
        ship.hull_id = HullId("hull_mining_barge".to_string());
        recompute_ship_stats(ship, &content);

        // Verify hull bonus is active
        let mining_rate = ship.modifiers.resolve(StatId::MiningRate, 1.0);
        assert!(
            (mining_rate - 1.25).abs() < 0.01,
            "hull bonus should be 1.25"
        );

        // Add module to station inventory
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Module {
            item_id: ModuleItemId("mod_item_laser".to_string()),
            module_def_id: "module_mining_laser".to_string(),
        });

        // Fit mining laser
        let mut events = vec![];
        handle_fit_ship_module(
            &mut state,
            &content,
            &ship_id,
            0,
            &ModuleDefId("module_mining_laser".to_string()),
            &station_id,
            &mut events,
        );
        let ship = state.ships.get(&ship_id).unwrap();
        let mining_rate = ship.modifiers.resolve(StatId::MiningRate, 1.0);
        // Both hull (+25%) and module (+20%) should stack: 1.0 * 1.25 * 1.2 = 1.5
        assert!(
            (mining_rate - 1.5).abs() < 0.01,
            "hull + module should stack to 1.5"
        );

        // Unfit mining laser
        let mut events = vec![];
        handle_unfit_ship_module(
            &mut state,
            &content,
            &ship_id,
            0,
            &station_id,
            2,
            &mut events,
        );
        let ship = state.ships.get(&ship_id).unwrap();
        let mining_rate = ship.modifiers.resolve(StatId::MiningRate, 1.0);
        // Hull bonus should still be active after unfit
        assert!(
            (mining_rate - 1.25).abs() < 0.01,
            "hull bonus should persist after unfit"
        );
    }

    #[test]
    fn modifier_source_hull_serialization_roundtrip() {
        let source = ModifierSource::Hull(HullId("hull_test".to_string()));
        let json = serde_json::to_string(&source).unwrap();
        let deserialized: ModifierSource = serde_json::from_str(&json).unwrap();
        assert_eq!(source, deserialized);
    }

    #[test]
    fn modifier_source_fitted_module_serialization_roundtrip() {
        let source = ModifierSource::FittedModule(ModuleDefId("mod_test".to_string()), 2);
        let json = serde_json::to_string(&source).unwrap();
        let deserialized: ModifierSource = serde_json::from_str(&json).unwrap();
        assert_eq!(source, deserialized);
    }

    #[test]
    fn cargo_capacity_stat_resolution() {
        let content = content_with_hull();
        let mut state = base_state(&content);
        let ship = state.ships.values_mut().next().unwrap();
        recompute_ship_stats(ship, &content);

        // Base cargo is 50 from hull
        assert!((ship.cargo_capacity_m3 - 50.0).abs() < 0.01);

        // Fit cargo expander (+30%)
        ship.fitted_modules.push(FittedModule {
            slot_index: 0,
            module_def_id: ModuleDefId("module_cargo_expander".to_string()),
        });
        recompute_ship_stats(ship, &content);
        // 50 * 1.3 = 65
        assert!((ship.cargo_capacity_m3 - 65.0).abs() < 0.1);
    }
}
