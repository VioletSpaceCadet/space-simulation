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
    requested_slot: Option<usize>,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let item_pos = station.core.inventory.iter().position(
        |i| matches!(i, InventoryItem::Module { item_id, .. } if item_id == module_item_id),
    );
    let Some(pos) = item_pos else { return false };
    let InventoryItem::Module {
        item_id,
        module_def_id,
    } = station.core.inventory.remove(pos)
    else {
        return false;
    };
    station.invalidate_volume_cache();

    let Some(def) = content.module_defs.get(&module_def_id) else {
        return false;
    };
    if !tech_gate_passed(
        state,
        station_id,
        def,
        item_id.clone(),
        module_def_id.clone(),
        current_tick,
        events,
    ) {
        return false;
    }
    // Safe to re-borrow after tech gate check released the mutable borrow.
    let station = state.stations.get_mut(station_id).expect("station exists");

    // Resolve the target slot against the station frame, if any. Frameless
    // stations keep legacy behavior (slot_index stays None). Framed stations
    // either validate an explicit slot or auto-find the first compatible
    // unoccupied slot. Failures return the module to inventory and emit a
    // ModuleNoCompatibleSlot event.
    let resolved_slot: Option<usize> =
        match resolve_install_slot(station, def, requested_slot, content) {
            SlotResolution::Frameless => None,
            SlotResolution::Slot(idx) => Some(idx),
            SlotResolution::NoCompatibleSlot => {
                return_module_on_slot_failure(
                    state,
                    station_id,
                    item_id,
                    module_def_id,
                    current_tick,
                    events,
                );
                return false;
            }
        };

    let module_id_str = format!("module_inst_{:04}", state.counters.next_module_instance_id);
    state.counters.next_module_instance_id += 1;
    let module_id = crate::ModuleInstanceId(module_id_str);
    let (kind_state, behavior_type, thermal) = default_module_state(def, content);

    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.core.modules.push(crate::ModuleState {
        id: module_id.clone(),
        def_id: module_def_id.clone(),
        enabled: false,
        kind_state,
        wear: crate::WearState::default(),
        thermal,
        slot_index: resolved_slot,
        power_stalled: false,
        module_priority: 0,
        assigned_crew: std::collections::BTreeMap::new(),
        efficiency: if def.crew_requirement.is_empty() {
            1.0
        } else {
            0.0
        },
        prev_crew_satisfied: def.crew_requirement.is_empty(),
    });
    station.rebuild_module_index(content);
    station.invalidate_power_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleInstalled {
            station_id: station_id.clone(),
            module_id,
            module_item_id: item_id,
            module_def_id,
            behavior_type,
            slot_index: resolved_slot,
        },
    ));
    true
}

/// Check the tech gate for an install. Returns `true` if the install may
/// proceed, `false` if the module was put back into inventory and a
/// `ModuleAwaitingTech` event was emitted. Extracted so
/// `handle_install_module` stays under the clippy line budget.
fn tech_gate_passed(
    state: &mut GameState,
    station_id: &crate::StationId,
    def: &crate::ModuleDef,
    item_id: crate::ModuleItemId,
    module_def_id: String,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(tech_id) = def.required_tech.as_ref() else {
        return true;
    };
    if state.research.unlocked.contains(tech_id) {
        return true;
    }
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.core.inventory.push(InventoryItem::Module {
        item_id,
        module_def_id,
    });
    station.invalidate_volume_cache();
    let module_id = crate::ModuleInstanceId(format!("pending_{}", tech_id.0));
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleAwaitingTech {
            station_id: station_id.clone(),
            module_id,
            tech_id: tech_id.clone(),
        },
    ));
    false
}

/// Put the module back into inventory and emit a `ModuleNoCompatibleSlot`
/// event when slot resolution fails on a framed station.
fn return_module_on_slot_failure(
    state: &mut GameState,
    station_id: &crate::StationId,
    item_id: crate::ModuleItemId,
    module_def_id: String,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    station.core.inventory.push(InventoryItem::Module {
        item_id,
        module_def_id: module_def_id.clone(),
    });
    station.invalidate_volume_cache();
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleNoCompatibleSlot {
            station_id: station_id.clone(),
            module_def_id,
        },
    ));
}

/// Outcome of slot resolution when installing a module on a station.
enum SlotResolution {
    /// Station has no frame — legacy path, `slot_index` stays `None`.
    Frameless,
    /// A specific slot index was either validated or auto-discovered.
    Slot(usize),
    /// Framed station but no compatible, unoccupied slot found.
    NoCompatibleSlot,
}

/// Resolve the target slot for an `InstallModule` command.
///
/// - Frameless station → `Frameless` (no validation).
/// - `requested_slot: Some(idx)` → validate index is in range, slot type
///   is in the module's `compatible_slots`, and no existing module is
///   in that slot. Any failure returns `NoCompatibleSlot`.
/// - `requested_slot: None` → return the first slot whose type matches
///   `compatible_slots` and is not occupied. None available → `NoCompatibleSlot`.
fn resolve_install_slot(
    station: &crate::StationState,
    def: &crate::ModuleDef,
    requested_slot: Option<usize>,
    content: &GameContent,
) -> SlotResolution {
    let Some(frame_id) = station.frame_id.as_ref() else {
        return SlotResolution::Frameless;
    };
    let Some(frame) = content.frames.get(frame_id) else {
        // Frame id references a missing catalog entry — treat as frameless
        // for forward-compat with saves that predate a removed frame.
        return SlotResolution::Frameless;
    };

    // Collect currently-occupied slot indices for the fast contains check.
    let occupied: std::collections::HashSet<usize> = station
        .core
        .modules
        .iter()
        .filter_map(|m| m.slot_index)
        .collect();

    if let Some(idx) = requested_slot {
        let Some(slot) = frame.slots.get(idx) else {
            return SlotResolution::NoCompatibleSlot;
        };
        if !def.compatible_slots.contains(&slot.slot_type) {
            return SlotResolution::NoCompatibleSlot;
        }
        if occupied.contains(&idx) {
            return SlotResolution::NoCompatibleSlot;
        }
        return SlotResolution::Slot(idx);
    }
    for (idx, slot) in frame.slots.iter().enumerate() {
        if occupied.contains(&idx) {
            continue;
        }
        if def.compatible_slots.contains(&slot.slot_type) {
            return SlotResolution::Slot(idx);
        }
    }
    SlotResolution::NoCompatibleSlot
}

/// Default assembly duration for on-site station construction (VIO-592).
///
/// Ticket-defined bounds: 48–168 ticks (roughly 2–7 game-hours at the
/// standard 60 minutes-per-tick), scaled by kit mass so heavier frames
/// take longer. The formula is deliberately simple — a future balance
/// pass can move these knobs into content/constants.json.
fn assembly_ticks_for_kit(kit_def: &crate::ComponentDef) -> u64 {
    // ~1 tick per 300 kg, clamped to [48, 168].
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let raw = (kit_def.mass_kg / 300.0) as u64;
    raw.clamp(48, 168)
}

/// Handle a `DeployStation` command. Validates the kit + ship, removes
/// the kit from cargo, and queues a Transit → ConstructStation chain on
/// the ship so assembly begins once the ship reaches the target position.
pub(crate) fn handle_deploy_station(
    state: &mut GameState,
    content: &GameContent,
    ship_id: &ShipId,
    kit_item_index: usize,
    target_position: &crate::Position,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(ship) = state.ships.get(ship_id) else {
        return false;
    };
    // Kit lookup: index must point at a Component whose def declares
    // deploys_frame. Anything else is rejected.
    let Some(InventoryItem::Component { component_id, .. }) = ship.inventory.get(kit_item_index)
    else {
        return false;
    };
    let kit_def = match content
        .component_defs
        .iter()
        .find(|c| c.id == component_id.0)
    {
        Some(def) => def,
        None => return false,
    };
    let Some(frame_id) = kit_def.deploys_frame.clone() else {
        return false;
    };
    // Frame must still exist in the catalog (protects against stale saves).
    if !content.frames.contains_key(&frame_id) {
        return false;
    }

    let kit_component_id_str = component_id.0.clone();
    let assembly_ticks = assembly_ticks_for_kit(kit_def);
    let ship_pos = ship.position.clone();

    // Consume the kit from the ship inventory. Components carry a count,
    // so decrement and only remove the entry when it reaches zero.
    let Some(ship_mut) = state.ships.get_mut(ship_id) else {
        return false;
    };
    if let Some(InventoryItem::Component { count, .. }) = ship_mut.inventory.get_mut(kit_item_index)
    {
        if *count == 0 {
            return false;
        }
        *count -= 1;
        if *count == 0 {
            ship_mut.inventory.remove(kit_item_index);
        }
    } else {
        return false;
    }

    // Build the Transit → ConstructStation task chain. Travel ticks come
    // through the existing spatial helper so the ship still honors
    // tech_efficient_transit and related modifiers via its speed cache.
    let travel_ticks = if crate::is_co_located(
        &ship_pos,
        target_position,
        &state.body_cache,
        content.constants.docking_range_au_um,
    ) {
        0
    } else {
        let from_abs = crate::compute_entity_absolute(&ship_pos, &state.body_cache);
        let to_abs = crate::compute_entity_absolute(target_position, &state.body_cache);
        let ticks_per_au = ship_mut
            .speed_ticks_per_au
            .unwrap_or(content.constants.ticks_per_au);
        crate::travel_ticks(
            from_abs,
            to_abs,
            ticks_per_au,
            content.constants.min_transit_ticks,
        )
    };
    let construct_task = TaskKind::ConstructStation {
        frame_id: frame_id.clone(),
        position: target_position.clone(),
        assembly_ticks,
        kit_component_id: kit_component_id_str,
    };

    // Skip the transit step entirely if the ship is already on site.
    let (final_task, total_duration) = if travel_ticks == 0 {
        let duration = construct_task.duration(&content.constants);
        (construct_task, duration)
    } else {
        let transit = TaskKind::Transit {
            destination: target_position.clone(),
            total_ticks: travel_ticks,
            then: Box::new(construct_task),
        };
        let duration = transit.duration(&content.constants);
        (transit, duration)
    };

    let Some(ship_mut) = state.ships.get_mut(ship_id) else {
        return false;
    };
    ship_mut.task = Some(crate::TaskState {
        kind: final_task.clone(),
        started_tick: current_tick,
        eta_tick: current_tick + total_duration,
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::TaskStarted {
            ship_id: ship_id.clone(),
            task_kind: final_task.label().to_string(),
            target: final_task.target(),
        },
    ));

    // When the ship is already on-site, fire StationConstructionStarted
    // immediately. In the transit case, resolve_transit emits it once the
    // transit step finishes.
    if let TaskKind::ConstructStation {
        frame_id: task_frame,
        position,
        assembly_ticks: ticks,
        ..
    } = &final_task
    {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::StationConstructionStarted {
                ship_id: ship_id.clone(),
                frame_id: task_frame.clone(),
                position: position.clone(),
                assembly_ticks: *ticks,
            },
        ));
    }

    true
}

/// Uninstall a module from the station and return it to inventory.
pub(crate) fn handle_uninstall_module(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(pos) = station.module_index_by_id(module_id) else {
        return false;
    };
    let module = station.core.modules.remove(pos);

    let item_id = crate::ModuleItemId(format!(
        "module_item_{:04}",
        state.counters.next_module_instance_id
    ));
    state.counters.next_module_instance_id += 1;

    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.core.inventory.push(InventoryItem::Module {
        item_id: item_id.clone(),
        module_def_id: module.def_id.clone(),
    });
    station.invalidate_volume_cache();
    station.rebuild_module_index(content);
    station.invalidate_power_cache();

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
    let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    module.enabled = enabled;
    station.invalidate_power_cache();
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
    let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) else {
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
    let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) else {
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
    let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) else {
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
    let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) else {
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
    if !state
        .progression
        .trade_tier_unlocked(crate::TradeTier::BasicImport)
    {
        return false;
    }
    if !state.stations.contains_key(station_id) {
        return false;
    }
    // Zone must have comm relay coverage for trade.
    let zone_id = &state.stations[station_id].position.parent_body.0;
    if crate::satellite::zone_comm_tier(zone_id, state, content) < crate::CommTier::Basic {
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

    // Crew import: add to station crew roster (no inventory/cargo involved)
    if let crate::TradeItemSpec::Crew { role, count } = item_spec {
        state.balance -= cost;
        let Some(station) = state.stations.get_mut(station_id) else {
            return false;
        };
        *station.core.crew.entry(role.clone()).or_insert(0) += count;
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
        return true;
    }

    // Check cargo capacity
    let new_items = trade::create_inventory_items(item_spec, rng);
    let new_volume = inventory_volume_m3(&new_items, content);
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let current_volume = station.used_volume_m3(content);
    let cargo_cap = station.core.cargo_capacity_m3;
    if current_volume + new_volume > cargo_cap {
        return false; // no room
    }

    // Execute import
    state.balance -= cost;
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    trade::merge_into_inventory(&mut station.core.inventory, new_items);
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
    if !state
        .progression
        .trade_tier_unlocked(crate::TradeTier::Export)
    {
        return false;
    }
    let Some(station) = state.stations.get(station_id) else {
        return false;
    };
    // Zone must have comm relay coverage for trade.
    if crate::satellite::zone_comm_tier(&station.position.parent_body.0, state, content)
        < crate::CommTier::Basic
    {
        return false;
    }

    // Look up pricing and compute revenue
    let Some(revenue) = trade::compute_export_revenue(item_spec, &content.pricing, content) else {
        return false; // not exportable or unknown item
    };

    // Check station has items
    if !trade::has_enough_for_export(&station.core.inventory, item_spec) {
        return false;
    }

    // Execute export
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    if !trade::remove_inventory_items(&mut station.core.inventory, item_spec) {
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

/// Import items into a ground facility. Bypasses milestone trade gating
/// (ground facilities on Earth have direct trade access).
pub(crate) fn handle_ground_import(
    state: &mut GameState,
    content: &GameContent,
    gf_id: &crate::GroundFacilityId,
    item_spec: &crate::TradeItemSpec,
    current_tick: u64,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    if !state.ground_facilities.contains_key(gf_id) {
        return false;
    }

    let Some(cost) = trade::compute_import_cost(item_spec, &content.pricing, content) else {
        return false;
    };

    if state.balance < cost {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::InsufficientFunds {
                station_id: crate::StationId(gf_id.0.clone()),
                action: format!("import {}", item_spec.pricing_key()),
                required: cost,
                available: state.balance,
            },
        ));
        return false;
    }

    // Crew import
    if let crate::TradeItemSpec::Crew { role, count } = item_spec {
        state.balance -= cost;
        let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
            return false;
        };
        *gf.core.crew.entry(role.clone()).or_insert(0) += count;
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::ItemImported {
                station_id: crate::StationId(gf_id.0.clone()),
                item_spec: item_spec.clone(),
                cost,
                balance_after: state.balance,
            },
        ));
        return true;
    }

    // Check cargo capacity
    let new_items = trade::create_inventory_items(item_spec, rng);
    let new_volume = inventory_volume_m3(&new_items, content);
    let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
        return false;
    };
    let current_volume = gf.core.used_volume_m3(content);
    if current_volume + new_volume > gf.core.cargo_capacity_m3 {
        return false;
    }

    state.balance -= cost;
    let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
        return false;
    };
    trade::merge_into_inventory(&mut gf.core.inventory, new_items);
    gf.core.invalidate_volume_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ItemImported {
            station_id: crate::StationId(gf_id.0.clone()),
            item_spec: item_spec.clone(),
            cost,
            balance_after: state.balance,
        },
    ));
    true
}

/// Export items from a ground facility. Bypasses milestone trade gating.
pub(crate) fn handle_ground_export(
    state: &mut GameState,
    content: &GameContent,
    gf_id: &crate::GroundFacilityId,
    item_spec: &crate::TradeItemSpec,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(gf) = state.ground_facilities.get(gf_id) else {
        return false;
    };

    let Some(revenue) = trade::compute_export_revenue(item_spec, &content.pricing, content) else {
        return false;
    };

    if !trade::has_enough_for_export(&gf.core.inventory, item_spec) {
        return false;
    }

    let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
        return false;
    };
    if !trade::remove_inventory_items(&mut gf.core.inventory, item_spec) {
        return false;
    }
    gf.core.invalidate_volume_cache();
    state.balance += revenue;
    state.export_revenue_total += revenue;
    state.export_count += 1;

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ItemExported {
            station_id: crate::StationId(gf_id.0.clone()),
            item_spec: item_spec.clone(),
            revenue,
            balance_after: state.balance,
        },
    ));
    true
}

/// Install a module from ground facility inventory into the facility's active modules.
pub(crate) fn handle_ground_install_module(
    state: &mut GameState,
    content: &GameContent,
    gf_id: &crate::GroundFacilityId,
    module_item_id: &crate::ModuleItemId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
        return false;
    };
    let item_pos = gf.core.inventory.iter().position(
        |i| matches!(i, InventoryItem::Module { item_id, .. } if item_id == module_item_id),
    );
    let Some(pos) = item_pos else { return false };
    let InventoryItem::Module {
        item_id,
        module_def_id,
    } = gf.core.inventory.remove(pos)
    else {
        return false;
    };
    gf.core.invalidate_volume_cache();

    let Some(def) = content.module_defs.get(&module_def_id) else {
        return false;
    };
    if let Some(ref tech_id) = def.required_tech {
        if !state.research.unlocked.contains(tech_id) {
            let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
                return false;
            };
            gf.core.inventory.push(InventoryItem::Module {
                item_id,
                module_def_id,
            });
            gf.core.invalidate_volume_cache();
            return false;
        }
    }

    let module_id_str = format!("module_inst_{:04}", state.counters.next_module_instance_id);
    state.counters.next_module_instance_id += 1;
    let module_id = crate::ModuleInstanceId(module_id_str);
    let (kind_state, behavior_type, thermal) = default_module_state(def, content);

    let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
        return false;
    };
    // Ground facility modules auto-enable and auto-assign crew on install.
    let crew_satisfied = def
        .crew_requirement
        .iter()
        .all(|(role, &needed)| gf.core.crew.get(role).copied().unwrap_or(0) >= needed);
    gf.core.modules.push(crate::ModuleState {
        id: module_id.clone(),
        def_id: module_def_id.clone(),
        enabled: true,
        kind_state,
        wear: crate::WearState::default(),
        thermal,
        slot_index: None,
        power_stalled: false,
        module_priority: 0,
        assigned_crew: def.crew_requirement.clone(),
        efficiency: if crew_satisfied { 1.0 } else { 0.0 },
        prev_crew_satisfied: crew_satisfied,
    });
    gf.core.rebuild_module_index(content);
    gf.core.invalidate_power_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleInstalled {
            station_id: crate::StationId(gf_id.0.clone()),
            module_id,
            module_item_id: item_id,
            module_def_id,
            behavior_type,
            slot_index: None,
        },
    ));
    true
}

/// Toggle the enabled flag on a ground facility module.
pub(crate) fn handle_ground_set_module_enabled(
    state: &mut GameState,
    gf_id: &crate::GroundFacilityId,
    module_id: &crate::ModuleInstanceId,
    enabled: bool,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(gf) = state.ground_facilities.get_mut(gf_id) else {
        return false;
    };
    let Some(module) = gf.core.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    module.enabled = enabled;
    gf.core.invalidate_power_cache();
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleToggled {
            station_id: crate::StationId(gf_id.0.clone()),
            module_id: module_id.clone(),
            enabled,
        },
    ));
    true
}

/// Consume fuel from a ground facility's inventory.
fn consume_fuel(core: &mut crate::FacilityCore, fuel_element: &str, fuel_kg: f32) {
    let mut remaining = fuel_kg;
    for item in &mut core.inventory {
        if remaining <= 0.0 {
            break;
        }
        if let InventoryItem::Material { element, kg, .. } = item {
            if element == fuel_element {
                let consumed = kg.min(remaining);
                *kg -= consumed;
                remaining -= consumed;
            }
        }
    }
    core.inventory
        .retain(|item| !matches!(item, InventoryItem::Material { kg, .. } if *kg <= 0.0));
    core.invalidate_volume_cache();
}

/// Remove `count` units of a component from a facility's inventory.
/// Only removes from the first matching slot. Callers must validate that
/// the slot has sufficient count before calling.
fn remove_component(core: &mut crate::FacilityCore, component_id: &str, count: u32) {
    for item in &mut core.inventory {
        if let InventoryItem::Component {
            component_id: cid,
            count: ref mut c,
            ..
        } = item
        {
            if cid.0 == component_id {
                let removed = (*c).min(count);
                *c -= removed;
                break;
            }
        }
    }
    core.inventory
        .retain(|item| !matches!(item, InventoryItem::Component { count, .. } if *count == 0));
    core.invalidate_volume_cache();
}

/// Find an available launch pad with sufficient capacity.
/// Returns `(module_index, recovery_ticks)`.
fn find_available_pad(
    facility: &crate::GroundFacilityState,
    content: &GameContent,
    min_payload_kg: f32,
) -> Option<(usize, u64)> {
    facility
        .core
        .modules
        .iter()
        .enumerate()
        .find_map(|(index, module)| {
            if !module.enabled {
                return None;
            }
            let def = content.module_defs.get(&module.def_id)?;
            let crate::ModuleBehaviorDef::LaunchPad(pad_def) = &def.behavior else {
                return None;
            };
            let crate::ModuleKindState::LaunchPad(pad_state) = &module.kind_state else {
                return None;
            };
            if !pad_state.available || pad_def.max_payload_kg < min_payload_kg {
                return None;
            }
            Some((index, pad_def.recovery_ticks))
        })
}

/// For `Satellite` payloads, validate the def exists, tech is unlocked, and the
/// satellite component is in the facility's inventory. Returns `true` if valid
/// (or if the payload is not a satellite).
fn validate_satellite_payload(
    payload: &crate::LaunchPayload,
    facility: &crate::GroundFacilityState,
    state: &GameState,
    content: &GameContent,
) -> bool {
    let crate::LaunchPayload::Satellite { satellite_def_id } = payload else {
        return true;
    };
    let Some(sat_def) = content.satellite_defs.get(satellite_def_id.as_str()) else {
        return false;
    };
    if let Some(ref required_tech) = sat_def.required_tech {
        if !state.research.unlocked.contains(required_tech) {
            return false;
        }
    }
    facility.core.inventory.iter().any(|item| {
        if let InventoryItem::Component {
            component_id,
            count,
            ..
        } = item
        {
            component_id.0 == *satellite_def_id && *count > 0
        } else {
            false
        }
    })
}

/// Compute the mass of a launch payload in kg.
fn compute_payload_mass(payload: &crate::LaunchPayload, content: &GameContent) -> f32 {
    match payload {
        crate::LaunchPayload::Supplies(items) => crate::tasks::inventory_mass_kg(items),
        crate::LaunchPayload::StationKit => content
            .component_defs
            .iter()
            .find(|d| d.id == "station_kit")
            .map_or(5000.0, |d| d.mass_kg),
        crate::LaunchPayload::Satellite { satellite_def_id } => content
            .satellite_defs
            .get(satellite_def_id.as_str())
            .map_or(0.0, |def| def.mass_kg),
    }
}

/// Launch a rocket from a ground facility. Validates pad availability,
/// payload weight, fuel, and balance. Deducts cost and fuel, marks pad
/// as recovering, and creates a `LaunchTransitState`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_launch(
    state: &mut GameState,
    content: &GameContent,
    facility_id: &crate::GroundFacilityId,
    rocket_def_id: &str,
    payload: &crate::LaunchPayload,
    destination: &crate::Position,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    // Look up rocket definition.
    let Some(rocket_def) = content.rocket_defs.get(rocket_def_id) else {
        return false;
    };

    // Check tech gate.
    if let Some(ref tech_id) = rocket_def.required_tech {
        if !state.research.unlocked.contains(tech_id) {
            return false;
        }
    }

    let Some(facility) = state.ground_facilities.get(facility_id) else {
        return false;
    };

    let Some((pad_index, recovery_ticks)) =
        find_available_pad(facility, content, rocket_def.payload_capacity_kg)
    else {
        return false;
    };

    if !validate_satellite_payload(payload, facility, state, content) {
        return false;
    }

    let payload_mass_kg = compute_payload_mass(payload, content);
    if payload_mass_kg > rocket_def.payload_capacity_kg {
        return false;
    }

    // Check fuel availability in facility inventory.
    let fuel_element = &content.constants.launch_fuel_element;
    let available_fuel: f32 = facility
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == fuel_element => Some(*kg),
            _ => None,
        })
        .sum();
    if available_fuel < rocket_def.fuel_kg {
        return false;
    }

    // Compute total cost: base + fuel.
    let fuel_cost = f64::from(rocket_def.fuel_kg) * content.constants.launch_fuel_cost_per_kg;
    let total_cost = rocket_def.base_launch_cost + fuel_cost;
    if state.balance < total_cost {
        return false;
    }

    // Commit: deduct cost and consume fuel.
    state.balance -= total_cost;
    let Some(facility) = state.ground_facilities.get_mut(facility_id) else {
        return false;
    };
    consume_fuel(&mut facility.core, fuel_element, rocket_def.fuel_kg);

    // For Satellite payloads, consume the satellite component from inventory.
    if let crate::LaunchPayload::Satellite { satellite_def_id } = payload {
        remove_component(&mut facility.core, satellite_def_id, 1);
    }

    // Mark pad as recovering and create launch transit.
    let transit_ticks = content
        .constants
        .game_minutes_to_ticks(rocket_def.transit_minutes);
    let arrival_tick = current_tick + transit_ticks;

    let Some(facility) = state.ground_facilities.get_mut(facility_id) else {
        return false;
    };
    if let crate::ModuleKindState::LaunchPad(ref mut pad_state) =
        facility.core.modules[pad_index].kind_state
    {
        pad_state.available = false;
        pad_state.recovery_ticks_remaining = recovery_ticks;
        pad_state.launches_count += 1;
    }
    facility.launch_transits.push(crate::LaunchTransitState {
        rocket_def_id: rocket_def_id.to_string(),
        payload: payload.clone(),
        destination: destination.clone(),
        arrival_tick,
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::PayloadLaunched {
            facility_id: facility_id.clone(),
            rocket_def_id: rocket_def_id.to_string(),
            payload: payload.clone(),
            destination: destination.clone(),
            cost: total_cost,
            fuel_cost,
            fuel_consumed_kg: rocket_def.fuel_kg,
            arrival_tick,
        },
    ));
    true
}

/// Deploy a satellite from an orbital station's inventory.
/// Removes the satellite component, creates a `SatelliteState` at the station's
/// position, and emits `SatelliteDeployed`.
pub(crate) fn handle_deploy_satellite(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    satellite_def_id: &str,
    current_tick: u64,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    // Validate satellite def exists.
    let Some(sat_def) = content.satellite_defs.get(satellite_def_id) else {
        return false;
    };

    // Validate tech requirement.
    if let Some(ref required_tech) = sat_def.required_tech {
        if !state.research.unlocked.contains(required_tech) {
            return false;
        }
    }

    // Validate station exists and has the satellite component.
    let Some(station) = state.stations.get(station_id) else {
        return false;
    };
    let has_component = station.core.inventory.iter().any(|item| {
        matches!(item, InventoryItem::Component { component_id, count, .. }
            if component_id.0 == satellite_def_id && *count > 0)
    });
    if !has_component {
        return false;
    }

    let position = station.position.clone();

    // Remove component from inventory.
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    remove_component(&mut station.core, satellite_def_id, 1);

    // Create satellite at station's position using the shared constructor.
    let Some(satellite) = crate::engine::create_satellite(
        satellite_def_id,
        position.clone(),
        current_tick,
        content,
        rng,
    ) else {
        return false;
    };
    let satellite_id = satellite.id.clone();
    let satellite_type = satellite.satellite_type.clone();
    state.satellites.insert(satellite_id.clone(), satellite);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::SatelliteDeployed {
            satellite_id,
            position,
            satellite_type,
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
        .core
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
        .core
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

/// Set the priority on a module. Controls inventory consumption order, crew
/// assignment, and power allocation within each behavior class.
pub(crate) fn handle_set_module_priority(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    priority: u32,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    module.module_priority = priority;
    true
}

/// Assign crew of a given role to a module. Validates available crew, role requirement, and cap.
pub(crate) fn handle_assign_crew(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    role: &crate::CrewRole,
    count: u32,
    events: &mut Vec<crate::EventEnvelope>,
) -> bool {
    if count == 0 {
        return false;
    }
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get(station_id) else {
        return false;
    };
    let Some(module_index) = station.module_index_by_id(module_id) else {
        return false;
    };
    let def_id = &station.core.modules[module_index].def_id;
    let Some(def) = content.module_defs.get(def_id) else {
        return false;
    };
    let Some(&needed) = def.crew_requirement.get(role) else {
        return false;
    };
    // Cap: don't assign more than the requirement
    let already_assigned = station.core.modules[module_index]
        .assigned_crew
        .get(role)
        .copied()
        .unwrap_or(0);
    let max_assignable = needed.saturating_sub(already_assigned);
    let actual_count = count.min(max_assignable);
    if actual_count == 0 {
        return false;
    }
    // Check available crew
    let available = station.available_crew(role);
    if available < actual_count {
        return false;
    }
    let count = actual_count;

    let station = state
        .stations
        .get_mut(station_id)
        .expect("station checked above");
    let was_satisfied = crate::is_crew_satisfied(
        &station.core.modules[module_index].assigned_crew,
        &def.crew_requirement,
    );
    let entry = station.core.modules[module_index]
        .assigned_crew
        .entry(role.clone())
        .or_insert(0);
    *entry += count;
    let now_satisfied = crate::is_crew_satisfied(
        &station.core.modules[module_index].assigned_crew,
        &def.crew_requirement,
    );
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::CrewAssigned {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            role: role.clone(),
            count,
        },
    ));
    if !was_satisfied && now_satisfied {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::ModuleFullyStaffed {
                station_id: station_id.clone(),
                module_id: module_id.clone(),
            },
        ));
    }
    true
}

/// Unassign crew of a given role from a module.
pub(crate) fn handle_unassign_crew(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    role: &crate::CrewRole,
    count: u32,
    events: &mut Vec<crate::EventEnvelope>,
) -> bool {
    if count == 0 {
        return false;
    }
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    let assigned = module.assigned_crew.get(role).copied().unwrap_or(0);
    if assigned < count {
        return false;
    }
    let def_id = module.def_id.clone();
    let was_satisfied = content
        .module_defs
        .get(&def_id)
        .is_none_or(|def| crate::is_crew_satisfied(&module.assigned_crew, &def.crew_requirement));
    let new_assigned = assigned - count;
    if new_assigned == 0 {
        module.assigned_crew.remove(role);
    } else {
        module.assigned_crew.insert(role.clone(), new_assigned);
    }
    let now_satisfied = content
        .module_defs
        .get(&def_id)
        .is_none_or(|def| crate::is_crew_satisfied(&module.assigned_crew, &def.crew_requirement));
    let module_id = module.id.clone();
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::CrewUnassigned {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            role: role.clone(),
            count,
        },
    ));
    if was_satisfied && !now_satisfied {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::ModuleUnderstaffed {
                station_id: station_id.clone(),
                module_id,
            },
        ));
    }
    true
}

/// Apply frame bonuses to a station via the modifier pipeline, mirroring
/// [`recompute_ship_stats`] for ships.
///
/// - Clears any existing `ModifierSource::Frame(_)` entries from
///   `station.core.modifiers` so the set matches the current frame exactly.
/// - If the station has a frame, copies the frame's `bonuses` into the
///   modifier set tagged with `ModifierSource::Frame(frame_id)` and recomputes
///   cached base stats (currently just `cargo_capacity_m3`).
/// - Frameless stations (`frame_id == None`) are left alone — no frame
///   modifiers, no stat recompute. Their `cargo_capacity_m3` remains whatever
///   the caller set directly.
///
/// **Power note:** `base_power_capacity_kw` on `FrameDef` is reserved for
/// future use. Phase 1 power resolution stays solar-array driven and is not
/// wired through frames here.
pub fn recompute_station_stats(station: &mut crate::StationState, content: &GameContent) {
    use crate::modifiers::{ModifierSource, StatId};

    // Always clear old frame modifiers so the set stays in sync with the
    // current frame, even when the station has been de-framed.
    station
        .core
        .modifiers
        .remove_where(|s| matches!(s, ModifierSource::Frame(_)));

    let Some(frame_id) = station.frame_id.clone() else {
        return;
    };
    let Some(frame) = content.frames.get(&frame_id) else {
        return;
    };

    for bonus in &frame.bonuses {
        let mut modifier = bonus.clone();
        modifier.source = ModifierSource::Frame(frame_id.clone());
        station.core.modifiers.add(modifier);
    }

    // Recompute cached cargo capacity from the frame base plus modifiers.
    station.core.cargo_capacity_m3 = station
        .core
        .modifiers
        .resolve_f32(StatId::CargoCapacity, frame.base_cargo_capacity_m3);
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
    let item_pos = station.core.inventory.iter().position(|item| {
        matches!(item, InventoryItem::Module { module_def_id: def_id, .. } if *def_id == module_def_id.0)
    });
    let Some(pos) = item_pos else { return false };

    // Execute: remove module from station inventory
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.core.inventory.remove(pos);
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
    station.core.inventory.push(InventoryItem::Module {
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

/// Create a thermal link between two module ports on a station.
pub(crate) fn handle_create_thermal_link(
    state: &mut GameState,
    content: &GameContent,
    link: &crate::ThermalLink,
    station_id: &StationId,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Validate both modules exist and look up their defs
    let Some(from_idx) = station.module_index_by_id(&link.from_module_id) else {
        return;
    };
    let Some(to_idx) = station.module_index_by_id(&link.to_module_id) else {
        return;
    };
    let from_module = &station.core.modules[from_idx];
    let to_module = &station.core.modules[to_idx];
    let Some(from_def) = content.module_defs.get(&from_module.def_id) else {
        return;
    };
    let Some(to_def) = content.module_defs.get(&to_module.def_id) else {
        return;
    };

    // Validate ports exist and have correct directions
    let from_port = from_def.ports.iter().find(|p| p.id == link.from_port_id);
    let to_port = to_def.ports.iter().find(|p| p.id == link.to_port_id);
    match (from_port, to_port) {
        (Some(fp), Some(tp))
            if fp.direction == crate::PortDirection::Output
                && tp.direction == crate::PortDirection::Input => {}
        _ => return,
    }

    // Check for duplicate
    let station = state
        .stations
        .get_mut(station_id)
        .expect("station verified above");
    if station.core.thermal_links.contains(link) {
        return;
    }

    station.core.thermal_links.push(link.clone());
    events.push(crate::emit(
        &mut state.counters,
        state.meta.tick,
        crate::Event::ThermalLinkCreated {
            station_id: station_id.clone(),
            from_module_id: link.from_module_id.clone(),
            from_port_id: link.from_port_id.clone(),
            to_module_id: link.to_module_id.clone(),
            to_port_id: link.to_port_id.clone(),
        },
    ));
}

/// Remove a thermal link between two module ports on a station.
pub(crate) fn handle_remove_thermal_link(
    state: &mut GameState,
    link: &crate::ThermalLink,
    station_id: &StationId,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };

    let before_len = station.core.thermal_links.len();
    station.core.thermal_links.retain(|l| l != link);
    if station.core.thermal_links.len() < before_len {
        events.push(crate::emit(
            &mut state.counters,
            state.meta.tick,
            crate::Event::ThermalLinkRemoved {
                station_id: station_id.clone(),
                from_module_id: link.from_module_id.clone(),
                from_port_id: link.from_port_id.clone(),
                to_module_id: link.to_module_id.clone(),
                to_port_id: link.to_port_id.clone(),
            },
        ));
    }
}

/// Transfer molten material between two thermal container modules along a link.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(crate) fn handle_transfer_molten(
    state: &mut GameState,
    content: &GameContent,
    station_id: &StationId,
    from_module_id: &crate::ModuleInstanceId,
    to_module_id: &crate::ModuleInstanceId,
    element: &str,
    kg: f32,
    events: &mut Vec<EventEnvelope>,
) {
    if kg <= 0.0 {
        return;
    }

    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Verify a thermal link exists between these modules
    let has_link =
        station.core.thermal_links.iter().any(|link| {
            link.from_module_id == *from_module_id && link.to_module_id == *to_module_id
        });
    if !has_link {
        return;
    }

    // Find source and destination module indices
    let (Some(from_idx), Some(to_idx)) = (
        station.module_index_by_id(from_module_id),
        station.module_index_by_id(to_module_id),
    ) else {
        return;
    };

    // Verify both are thermal containers
    let is_from_container = matches!(
        station.core.modules[from_idx].kind_state,
        crate::ModuleKindState::ThermalContainer(_)
    );
    let is_to_container = matches!(
        station.core.modules[to_idx].kind_state,
        crate::ModuleKindState::ThermalContainer(_)
    );
    if !is_from_container || !is_to_container {
        return;
    }

    // Check destination capacity
    let to_def = content
        .module_defs
        .get(&station.core.modules[to_idx].def_id);
    let capacity_kg = to_def
        .and_then(|d| match &d.behavior {
            crate::ModuleBehaviorDef::ThermalContainer(tc) => Some(tc.capacity_kg),
            _ => None,
        })
        .unwrap_or(0.0);

    let station = state
        .stations
        .get_mut(station_id)
        .expect("station verified");

    // Extract material from source container
    let crate::ModuleKindState::ThermalContainer(ref mut from_container) =
        station.core.modules[from_idx].kind_state
    else {
        return;
    };

    // Find liquid material of the requested element
    let item_idx = from_container.held_items.iter().position(|item| {
        matches!(
            item,
            crate::InventoryItem::Material {
                element: e,
                thermal: Some(props),
                ..
            } if e == element && props.phase == crate::Phase::Liquid
        )
    });
    let Some(item_idx) = item_idx else {
        return; // no liquid material of this element
    };

    // Extract the transfer amount from the source container
    let source_item = &from_container.held_items[item_idx];
    let (source_kg_val, quality_val, thermal_props) = match source_item {
        crate::InventoryItem::Material {
            kg: source_kg,
            quality,
            thermal,
            ..
        } => (*source_kg, *quality, thermal.clone()),
        _ => return,
    };

    let transfer_kg = kg.min(source_kg_val);

    let transferred_item = crate::InventoryItem::Material {
        element: element.to_string(),
        kg: transfer_kg,
        quality: quality_val,
        thermal: thermal_props,
    };

    // Update or remove source item
    if transfer_kg >= source_kg_val {
        from_container.held_items.remove(item_idx);
    } else if let crate::InventoryItem::Material {
        kg: ref mut src_kg, ..
    } = from_container.held_items[item_idx]
    {
        *src_kg -= transfer_kg;
    }

    let actual_kg = transfer_kg;

    // Check if material freezes during transfer (below solidification point)
    let froze = if let crate::InventoryItem::Material {
        thermal: Some(ref props),
        ..
    } = transferred_item
    {
        if let Some(element_def) = content.elements.iter().find(|e| e.id == element) {
            if let Some(melting_point) = element_def.melting_point_mk {
                let solidification_point =
                    melting_point.saturating_sub(crate::thermal::SOLIDIFICATION_HYSTERESIS_MK);
                props.temp_mk <= solidification_point
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    if froze {
        // Material solidified — put it back in source and emit PipeFreeze
        let crate::ModuleKindState::ThermalContainer(ref mut from_container) =
            station.core.modules[from_idx].kind_state
        else {
            return;
        };
        from_container.held_items.push(transferred_item);
        events.push(crate::emit(
            &mut state.counters,
            state.meta.tick,
            crate::Event::PipeFreeze {
                station_id: station_id.clone(),
                from_module_id: from_module_id.clone(),
                to_module_id: to_module_id.clone(),
                element: element.to_string(),
            },
        ));
        return;
    }

    // Check destination capacity
    let crate::ModuleKindState::ThermalContainer(ref dest_container) =
        station.core.modules[to_idx].kind_state
    else {
        return;
    };
    let current_dest_kg: f32 = crate::tasks::inventory_mass_kg(&dest_container.held_items);
    if current_dest_kg + actual_kg > capacity_kg {
        // Over capacity — put material back in source
        let crate::ModuleKindState::ThermalContainer(ref mut from_container) =
            station.core.modules[from_idx].kind_state
        else {
            return;
        };
        from_container.held_items.push(transferred_item);
        return;
    }

    // Place in destination
    let crate::ModuleKindState::ThermalContainer(ref mut dest_container) =
        station.core.modules[to_idx].kind_state
    else {
        return;
    };
    dest_container.held_items.push(transferred_item);

    events.push(crate::emit(
        &mut state.counters,
        state.meta.tick,
        crate::Event::MoltenTransferred {
            station_id: station_id.clone(),
            from_module_id: from_module_id.clone(),
            to_module_id: to_module_id.clone(),
            element: element.to_string(),
            kg: actual_kg,
        },
    ));
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
        // Deduct propellant on Transit start
        if let TaskKind::Transit {
            ref destination, ..
        } = &task_kind
        {
            if !deduct_transit_fuel(state, content, &ship_id, destination, current_tick, events) {
                continue; // insufficient fuel — assignment rejected
            }
        }

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

/// Attempt to deduct transit fuel from a ship. Returns `true` if successful.
/// Skips fuel deduction when propulsion is not configured (`fuel_cost_per_au` == 0).
fn deduct_transit_fuel(
    state: &mut GameState,
    content: &GameContent,
    ship_id: &ShipId,
    destination: &crate::Position,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    // Skip fuel deduction when propulsion is not configured
    if content.constants.fuel_cost_per_au <= 0.0 {
        return true;
    }
    let Some(ship) = state.ships.get(ship_id) else {
        return false;
    };
    let position = ship.position.clone();
    let base_fuel_cost = crate::propulsion::compute_transit_fuel(
        ship,
        &position,
        destination,
        content,
        &state.body_cache,
    );
    // Apply global fuel efficiency modifier (e.g. tech_efficient_propulsion).
    let fuel_cost = base_fuel_cost
        * state
            .modifiers
            .resolve_f32(crate::modifiers::StatId::FuelEfficiency, 1.0);

    if fuel_cost <= 0.0 {
        return true; // co-located, no fuel needed
    }

    if ship.propellant_kg < fuel_cost {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::InsufficientPropellant {
                ship_id: ship_id.clone(),
                destination: destination.clone(),
            },
        ));
        return false;
    }

    let ship = state.ships.get_mut(ship_id).expect("ship exists");
    ship.propellant_kg -= fuel_cost;
    state.propellant_consumed_total += f64::from(fuel_cost);
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::PropellantConsumed {
            ship_id: ship_id.clone(),
            kg_consumed: fuel_cost,
            destination: destination.clone(),
        },
    ));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifiers::{ModifierSource, StatId};
    use crate::test_fixtures::{base_content, base_state, ModuleDefBuilder};
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
            ModuleDefBuilder::new("module_cargo_expander")
                .name("Cargo Expander")
                .mass(500.0)
                .volume(2.0)
                .behavior(crate::ModuleBehaviorDef::Equipment)
                .compatible_slots(vec![SlotType("utility".to_string())])
                .ship_modifiers(vec![crate::modifiers::Modifier::pct_mult(
                    StatId::CargoCapacity,
                    1.3,
                    ModifierSource::Equipment("cargo_expander".to_string()),
                )])
                .build(),
        );
        content
    }

    fn state_with_module_in_inventory(content: &GameContent) -> GameState {
        let mut state = base_state(content);
        let station = state.stations.values_mut().next().unwrap();
        station.core.inventory.push(InventoryItem::Module {
            item_id: ModuleItemId("mod_item_0001".to_string()),
            module_def_id: "module_cargo_expander".to_string(),
        });
        state
    }

    // ------------------------------------------------------------------
    // SF-04: recompute_station_stats — frame bonus application
    // ------------------------------------------------------------------

    fn content_with_frame() -> GameContent {
        let mut content = base_content();
        use crate::modifiers::{Modifier, ModifierOp, ModifierSource, StatId};
        let frame_id = crate::FrameId("frame_test_research".to_string());
        content.frames.insert(
            frame_id.clone(),
            crate::FrameDef {
                id: frame_id.clone(),
                name: "Test Research".to_string(),
                base_cargo_capacity_m3: 1000.0,
                base_power_capacity_kw: 80.0,
                slots: vec![],
                bonuses: vec![Modifier {
                    stat: StatId::ResearchSpeed,
                    op: ModifierOp::PctAdditive,
                    value: 0.15,
                    source: ModifierSource::Frame(frame_id.clone()),
                    condition: None,
                }],
                required_tech: None,
                tags: vec![],
            },
        );
        content
    }

    #[test]
    fn recompute_station_stats_frameless_leaves_modifiers_untouched() {
        let content = base_content();
        let mut state = base_state(&content);
        let station = state.stations.values_mut().next().unwrap();
        station.frame_id = None;
        // Prime the modifier set with a non-frame modifier to prove it is
        // not swept away by the recompute.
        use crate::modifiers::{Modifier, ModifierOp, ModifierSource, StatId};
        station.core.modifiers.add(Modifier {
            stat: StatId::ResearchSpeed,
            op: ModifierOp::Flat,
            value: 1.0,
            source: ModifierSource::Tech("tech_research".to_string()),
            condition: None,
        });
        let before = station.core.modifiers.len();
        recompute_station_stats(station, &content);
        assert_eq!(station.core.modifiers.len(), before);
    }

    #[test]
    fn recompute_station_stats_applies_frame_bonus_and_cargo() {
        let content = content_with_frame();
        let mut state = base_state(&content);
        let station = state.stations.values_mut().next().unwrap();
        station.frame_id = Some(crate::FrameId("frame_test_research".to_string()));

        recompute_station_stats(station, &content);

        // Frame adds +15% ResearchSpeed tagged with ModifierSource::Frame
        use crate::modifiers::{ModifierSource, StatId};
        let resolved = station.core.modifiers.resolve(StatId::ResearchSpeed, 100.0);
        assert!(
            (resolved - 115.0).abs() < 1e-6,
            "expected ResearchSpeed = 115, got {resolved}"
        );
        let has_frame_modifier = station
            .core
            .modifiers
            .iter()
            .any(|m| matches!(m.source, ModifierSource::Frame(_)));
        assert!(
            has_frame_modifier,
            "frame bonus should be tagged with ModifierSource::Frame"
        );

        // Cached cargo capacity should match the frame base (no CargoCapacity
        // modifier on this frame, so resolve returns the base).
        assert!((station.core.cargo_capacity_m3 - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn recompute_station_stats_is_idempotent() {
        let content = content_with_frame();
        let mut state = base_state(&content);
        let station = state.stations.values_mut().next().unwrap();
        station.frame_id = Some(crate::FrameId("frame_test_research".to_string()));

        recompute_station_stats(station, &content);
        let after_first = station.core.modifiers.len();
        recompute_station_stats(station, &content);
        let after_second = station.core.modifiers.len();
        assert_eq!(
            after_first, after_second,
            "calling recompute twice should not stack modifiers"
        );
    }

    #[test]
    fn recompute_station_stats_clears_stale_frame_modifiers_when_unframed() {
        let content = content_with_frame();
        let mut state = base_state(&content);
        let station = state.stations.values_mut().next().unwrap();
        station.frame_id = Some(crate::FrameId("frame_test_research".to_string()));
        recompute_station_stats(station, &content);
        assert!(!station.core.modifiers.is_empty());

        // De-frame the station and recompute — all frame modifiers should go.
        station.frame_id = None;
        recompute_station_stats(station, &content);
        use crate::modifiers::ModifierSource;
        let lingering_frame = station
            .core
            .modifiers
            .iter()
            .any(|m| matches!(m.source, ModifierSource::Frame(_)));
        assert!(
            !lingering_frame,
            "de-framed station should have no Frame-sourced modifiers"
        );
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
            .core
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
        assert!(station.core.inventory.iter().any(|i| matches!(i, InventoryItem::Module { module_def_id, .. } if module_def_id == "module_cargo_expander")));
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
            ModuleDefBuilder::new("module_mining_laser")
                .name("Mining Laser")
                .mass(800.0)
                .volume(3.0)
                .behavior(crate::ModuleBehaviorDef::Equipment)
                .compatible_slots(vec![SlotType("industrial".to_string())])
                .ship_modifiers(vec![crate::modifiers::Modifier::pct_mult(
                    StatId::MiningRate,
                    1.2,
                    ModifierSource::Equipment("mining_laser".to_string()),
                )])
                .build(),
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
        station.core.inventory.push(InventoryItem::Module {
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

    // -- Mass helper tests --

    #[test]
    fn dry_mass_hull_only() {
        let content = content_with_hull();
        let state = base_state(&content);
        let ship = state.ships.values().next().unwrap();
        // Hull mass_kg = 5000, no fitted modules
        assert!((ship.dry_mass_kg(&content) - 5000.0).abs() < 0.01);
    }

    #[test]
    fn dry_mass_with_fitted_module() {
        let content = content_with_hull();
        let mut state = base_state(&content);
        let ship_id = crate::ShipId("ship_0001".to_string());
        let ship = state.ships.get_mut(&ship_id).unwrap();
        ship.fitted_modules.push(FittedModule {
            slot_index: 0,
            module_def_id: ModuleDefId("module_cargo_expander".to_string()),
        });
        // Hull 5000 + cargo_expander 500 = 5500
        assert!((ship.dry_mass_kg(&content) - 5500.0).abs() < 0.01);
    }

    #[test]
    fn total_mass_includes_propellant_and_cargo() {
        let content = content_with_hull();
        let mut state = base_state(&content);
        let ship_id = crate::ShipId("ship_0001".to_string());
        let ship = state.ships.get_mut(&ship_id).unwrap();
        ship.propellant_kg = 8000.0;
        ship.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 2000.0,
            quality: 1.0,
            thermal: None,
        });
        // dry=5000 + propellant=8000 + cargo=2000 = 15000
        assert!((ship.total_mass_kg(&content) - 15000.0).abs() < 0.01);
    }

    #[test]
    fn inventory_mass_kg_sums_correctly() {
        let inventory = vec![
            InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 1500.0,
                quality: 1.0,
                thermal: None,
            },
            InventoryItem::Ore {
                lot_id: crate::LotId("lot1".to_string()),
                asteroid_id: crate::AsteroidId("ast1".to_string()),
                kg: 500.0,
                composition: std::collections::HashMap::new(),
            },
            InventoryItem::Component {
                component_id: crate::ComponentId("repair_kit".to_string()),
                count: 5,
                quality: 1.0,
            },
        ];
        // Fe 1500 + Ore 500 + Component 0 = 2000
        assert!((crate::tasks::inventory_mass_kg(&inventory) - 2000.0).abs() < 0.01);
    }
}
