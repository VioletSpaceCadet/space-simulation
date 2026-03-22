use sim_core::{
    compute_entity_absolute, inventory_volume_m3, is_co_located, mine_duration, trade,
    travel_ticks, AsteroidId, AsteroidState, Command, CommandEnvelope, CommandId, ComponentId,
    DomainProgress, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ModuleBehaviorDef, ModuleKindState, Position, PrincipalId, ShipId, ShipState, SiteId,
    StationState, TaskKind, TechDef, TechId, TradeItemSpec,
};

pub trait CommandSource {
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}

/// Drives ships automatically:
/// 1. Deposit cargo if hold is non-empty.
/// 2. Mine the best available deep-scanned asteroid.
/// 3. Deep-scan `IronRich` asteroids to unlock mining targets.
/// 4. Survey unscanned sites.
pub struct AutopilotController;

const AUTOPILOT_OWNER: &str = "principal_autopilot";

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Wraps `task` in a Transit if `from` and `to` are not co-located; else returns `task` as-is.
fn maybe_transit(
    task: TaskKind,
    from: &Position,
    to: &Position,
    state: &GameState,
    content: &GameContent,
) -> TaskKind {
    if is_co_located(
        from,
        to,
        &state.body_cache,
        content.constants.docking_range_au_um,
    ) {
        return task;
    }
    let from_abs = compute_entity_absolute(from, &state.body_cache);
    let to_abs = compute_entity_absolute(to, &state.body_cache);
    let ticks = travel_ticks(
        from_abs,
        to_abs,
        content.constants.ticks_per_au,
        content.constants.min_transit_ticks,
    );
    TaskKind::Transit {
        destination: to.clone(),
        total_ticks: ticks,
        then: Box::new(task),
    }
}

/// Allocates a command ID and builds a `CommandEnvelope`.
fn make_cmd(
    owner: &PrincipalId,
    tick: u64,
    next_id: &mut u64,
    command: Command,
) -> CommandEnvelope {
    let cmd_id = CommandId(format!("cmd_{:06}", *next_id));
    *next_id += 1;
    CommandEnvelope {
        id: cmd_id,
        issued_by: owner.clone(),
        issued_tick: tick,
        execute_at_tick: tick,
        command,
    }
}

/// Emits commands to install, enable, and configure station modules.
fn station_module_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();
    for station in state.stations.values() {
        for item in &station.inventory {
            if let InventoryItem::Module { item_id, .. } = item {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::InstallModule {
                        station_id: station.id.clone(),
                        module_item_id: item_id.clone(),
                    },
                ));
            }
        }
        for module in &station.modules {
            // Re-enable disabled modules, but not if auto-disabled due to max wear.
            // Skip electrolysis — managed by propellant_pipeline_commands().
            if !module.enabled
                && module.wear.wear < 1.0
                && module.def_id != "module_electrolysis_unit"
            {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::SetModuleEnabled {
                        station_id: station.id.clone(),
                        module_id: module.id.clone(),
                        enabled: true,
                    },
                ));
            }
            if let ModuleKindState::Processor(ps) = &module.kind_state {
                if ps.threshold_kg == 0.0 {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleThreshold {
                            station_id: station.id.clone(),
                            module_id: module.id.clone(),
                            threshold_kg: content.constants.autopilot_refinery_threshold_kg,
                        },
                    ));
                }
            }
        }
    }
    commands
}

/// Propellant pipeline management — no-op if station lacks electrolysis module.
///
/// When LH2 is abundant (> 2x threshold), disable electrolysis to save 25 kW.
/// When LH2 is low (< threshold), ensure electrolysis and heating are enabled.
fn propellant_pipeline_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();
    let lh2_kg = total_lh2_inventory(state);
    let threshold = content.constants.autopilot_lh2_threshold_kg;

    for station in state.stations.values() {
        let has_electrolysis = station
            .modules
            .iter()
            .any(|m| m.def_id == "module_electrolysis_unit");
        if !has_electrolysis {
            continue;
        }

        if lh2_kg > threshold * 2.0 {
            // LH2 abundant — disable electrolysis to save power
            for module in &station.modules {
                if module.def_id == "module_electrolysis_unit"
                    && module.enabled
                    && module.wear.wear < 1.0
                {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleEnabled {
                            station_id: station.id.clone(),
                            module_id: module.id.clone(),
                            enabled: false,
                        },
                    ));
                }
            }
        } else if lh2_kg < threshold {
            // LH2 low — ensure electrolysis and heating are enabled
            for module in &station.modules {
                if (module.def_id == "module_electrolysis_unit"
                    || module.def_id == "module_heating_unit")
                    && !module.enabled
                    && module.wear.wear < 1.0
                {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleEnabled {
                            station_id: station.id.clone(),
                            module_id: module.id.clone(),
                            enabled: true,
                        },
                    ));
                }
            }
        }
    }
    commands
}

/// Returns idle autopilot ships sorted by ID for determinism.
fn collect_idle_ships(state: &GameState, owner: &PrincipalId) -> Vec<ShipId> {
    let mut ships: Vec<ShipId> = state
        .ships
        .values()
        .filter(|ship| {
            ship.owner == *owner
                && ship
                    .task
                    .as_ref()
                    .is_none_or(|t| matches!(t.kind, TaskKind::Idle))
        })
        .map(|ship| ship.id.clone())
        .collect();
    ships.sort_by(|a, b| a.0.cmp(&b.0));
    ships
}

/// Returns asteroid IDs above confidence threshold with unknown composition, sorted by ID.
/// Includes both `IronRich` and `VolatileRich` candidates.
fn collect_deep_scan_candidates(state: &GameState, content: &GameContent) -> Vec<AsteroidId> {
    let mut candidates: Vec<AsteroidId> = state
        .asteroids
        .values()
        .filter(|asteroid| {
            asteroid.knowledge.composition.is_none()
                && asteroid.knowledge.tag_beliefs.iter().any(|(tag, conf)| {
                    (tag.0 == sim_core::TAG_IRON_RICH
                        && *conf > content.constants.autopilot_iron_rich_confidence_threshold)
                        || (tag.0 == sim_core::TAG_VOLATILE_RICH
                            && *conf > content.constants.autopilot_volatile_confidence_threshold)
                })
        })
        .map(|a| a.id.clone())
        .collect();
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates
}

/// Check if any station has a heating module installed.
fn station_has_heating_module(state: &GameState) -> bool {
    state.stations.values().any(|station| {
        station
            .modules
            .iter()
            .any(|module| module.def_id == "module_heating_unit")
    })
}

/// Total H2O material across all station inventories.
fn total_h2o_inventory(state: &GameState) -> f32 {
    state
        .stations
        .values()
        .flat_map(|s| s.inventory.iter())
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "H2O" => Some(*kg),
            _ => None,
        })
        .sum()
}

/// Total LH2 material across all station inventories.
fn total_lh2_inventory(state: &GameState) -> f32 {
    state
        .stations
        .values()
        .flat_map(|s| s.inventory.iter())
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum()
}

/// Mining value for sorting: `mass_kg × H2O_fraction`.
fn h2o_mining_value(asteroid: &AsteroidState) -> f32 {
    asteroid.mass_kg
        * asteroid
            .knowledge
            .composition
            .as_ref()
            .and_then(|c| c.get("H2O"))
            .copied()
            .unwrap_or(0.0)
}

/// Mining value for sorting: `mass_kg × Fe_fraction`.
fn fe_mining_value(asteroid: &AsteroidState) -> f32 {
    asteroid.mass_kg
        * asteroid
            .knowledge
            .composition
            .as_ref()
            .and_then(|c| c.get("Fe"))
            .copied()
            .unwrap_or(0.0)
}

/// Priority 1: if ship has ore, return a Deposit (or Transit→Deposit) task to the nearest station.
fn deposit_priority(
    ship: &ShipState,
    state: &GameState,
    content: &GameContent,
) -> Option<TaskKind> {
    if !ship
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Ore { .. }))
    {
        return None;
    }
    let ship_abs = compute_entity_absolute(&ship.position, &state.body_cache);
    let station = state.stations.values().min_by_key(|s| {
        let s_abs = compute_entity_absolute(&s.position, &state.body_cache);
        ship_abs.distance_squared(s_abs)
    })?;
    Some(maybe_transit(
        TaskKind::Deposit {
            station: station.id.clone(),
            blocked: false,
        },
        &ship.position,
        &station.position,
        state,
        content,
    ))
}

/// Geometric mean of per-domain ratios (accumulated / required), clamped to [0, 1].
fn compute_sufficiency(tech: &TechDef, progress: Option<&DomainProgress>) -> f32 {
    if tech.domain_requirements.is_empty() {
        return 1.0;
    }
    let ratios: Vec<f32> = tech
        .domain_requirements
        .iter()
        .map(|(domain, required)| {
            let accumulated =
                progress.map_or(0.0, |p| p.points.get(domain).copied().unwrap_or(0.0));
            (accumulated / required).min(1.0)
        })
        .collect();
    let product: f32 = ratios.iter().product();
    product.powf(1.0 / ratios.len() as f32)
}

/// Auto-assigns unassigned labs to the highest-priority eligible tech.
fn lab_assignment_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();

    for station in state.stations.values() {
        for module in &station.modules {
            let ModuleKindState::Lab(lab_state) = &module.kind_state else {
                continue;
            };
            // Skip labs that are already assigned to an eligible (non-unlocked) tech
            if let Some(ref tech_id) = lab_state.assigned_tech {
                if !state.research.unlocked.contains(tech_id) {
                    continue;
                }
            }

            // Find lab's domain from def
            let Some(def) = content.module_defs.get(&module.def_id) else {
                continue;
            };
            let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else {
                continue;
            };

            // Find eligible techs that need this lab's domain
            let mut candidates: Vec<(TechId, f32)> = content
                .techs
                .iter()
                .filter(|tech| {
                    !state.research.unlocked.contains(&tech.id)
                        && tech
                            .prereqs
                            .iter()
                            .all(|p| state.research.unlocked.contains(p))
                        && tech.domain_requirements.contains_key(&lab_def.domain)
                })
                .map(|tech| {
                    let sufficiency =
                        compute_sufficiency(tech, state.research.evidence.get(&tech.id));
                    (tech.id.clone(), sufficiency)
                })
                .collect();
            // Highest sufficiency first (closest to unlock), then by ID for determinism
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));

            if let Some((tech_id, _)) = candidates.first() {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::AssignLabTech {
                        station_id: station.id.clone(),
                        module_id: module.id.clone(),
                        tech_id: Some(tech_id.clone()),
                    },
                ));
            }
        }
    }
    commands
}

/// Maximum fleet size the autopilot will build toward.
/// Autopilot won't spend more than this fraction of balance on a single thruster import.
const AUTOPILOT_BUDGET_CAP_FRACTION: f64 = 0.05;

/// Emits Import commands for thrusters when a shipyard is ready and conditions are met.
///
/// Guards (VIO-41):
/// 1. Trade must be unlocked (tick >= `trade_unlock_tick()`).
/// 2. `tech_ship_construction` must be researched.
/// 3. Station must have fewer thrusters than the shipyard recipe requires.
/// 4. Budget cap: import cost must be < `AUTOPILOT_BUDGET_CAP_FRACTION` of current balance.
fn thruster_import_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();

    // Gate 1: Trade unlock
    if state.meta.tick < sim_core::trade_unlock_tick(content.constants.minutes_per_tick) {
        return commands;
    }

    // Gate 2: Tech requirement
    let tech_unlocked = state
        .research
        .unlocked
        .contains(&TechId("tech_ship_construction".to_string()));
    if !tech_unlocked {
        return commands;
    }

    let mut sorted_stations: Vec<_> = state.stations.values().collect();
    sorted_stations.sort_by(|a, b| a.id.0.cmp(&b.id.0));

    // Look up the shipyard recipe's thruster requirement from content.
    let required_thrusters = content
        .module_defs
        .get("module_shipyard")
        .and_then(|def| match &def.behavior {
            ModuleBehaviorDef::Assembler(asm) => asm.recipes.first(),
            _ => None,
        })
        .map_or(4, |recipe| {
            recipe
                .inputs
                .iter()
                .find_map(|input| match (&input.filter, &input.amount) {
                    (InputFilter::Component(cid), InputAmount::Count(n)) if cid.0 == "thruster" => {
                        Some(*n)
                    }
                    _ => None,
                })
                .unwrap_or(4)
        });

    for station in sorted_stations {
        // Find the shipyard module — must be enabled
        let has_shipyard = station
            .modules
            .iter()
            .any(|module| module.def_id == "module_shipyard" && module.enabled);
        if !has_shipyard {
            continue;
        }

        // Count current thrusters in inventory
        let thruster_count: u32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "thruster" => Some(*count),
                _ => None,
            })
            .sum();
        if thruster_count >= required_thrusters {
            continue; // Already have enough for the recipe
        }

        let needed = required_thrusters - thruster_count;
        let item_spec = TradeItemSpec::Component {
            component_id: ComponentId("thruster".to_string()),
            count: needed,
        };

        // Gate 5: Budget cap — cost must be < 5% of current balance
        let Some(cost) = trade::compute_import_cost(&item_spec, &content.pricing, content) else {
            continue;
        };
        if cost > state.balance * AUTOPILOT_BUDGET_CAP_FRACTION {
            continue;
        }

        commands.push(make_cmd(
            owner,
            state.meta.tick,
            next_id,
            Command::Import {
                station_id: station.id.clone(),
                item_spec,
            },
        ));
    }
    commands
}

/// Exports surplus materials and components for revenue.
///
/// Priority order: `repair_kits` > He > Si > Fe.
/// Reserve enforcement is hard — never exports below reserve thresholds.
/// One export per item type per station per tick to prevent stockpile dumping.
fn export_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();

    // Gate: Trade unlock
    if state.meta.tick < sim_core::trade_unlock_tick(content.constants.minutes_per_tick) {
        return commands;
    }

    let reserve_kits = content.constants.autopilot_repair_kit_reserve;
    let reserve_fe_kg = content.constants.autopilot_fe_reserve_kg;
    let batch_size_kg = content.constants.autopilot_export_batch_size_kg;
    let min_revenue = content.constants.autopilot_export_min_revenue;

    let mut sorted_stations: Vec<_> = state.stations.values().collect();
    sorted_stations.sort_by(|a, b| a.id.0.cmp(&b.id.0));

    for station in sorted_stations {
        // Export candidates in priority order
        let candidates =
            build_export_candidates(station, reserve_kits, reserve_fe_kg, batch_size_kg);

        for candidate in candidates {
            let revenue = match trade::compute_export_revenue(&candidate, &content.pricing, content)
            {
                Some(rev) if rev >= min_revenue => rev,
                _ => continue,
            };
            // Verify station actually has the items
            if !trade::has_enough_for_export(&station.inventory, &candidate) {
                continue;
            }
            let _ = revenue; // revenue validated; engine computes final amount
            commands.push(make_cmd(
                owner,
                state.meta.tick,
                next_id,
                Command::Export {
                    station_id: station.id.clone(),
                    item_spec: candidate,
                },
            ));
        }
    }
    commands
}

/// Builds the list of export candidates for a station in priority order.
fn build_export_candidates(
    station: &StationState,
    reserve_kits: u32,
    reserve_fe_kg: f32,
    batch_size_kg: f32,
) -> Vec<TradeItemSpec> {
    let mut candidates = Vec::new();

    // 1. Repair kits — export surplus above reserve
    let kit_count: u32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "repair_kit" => Some(*count),
            _ => None,
        })
        .sum();
    if kit_count > reserve_kits {
        candidates.push(TradeItemSpec::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: kit_count - reserve_kits,
        });
    }

    // 2-4. Materials in priority order: He, Si, Fe
    for (element, reserve_kg) in [("He", 0.0_f32), ("Si", 0.0_f32), ("Fe", reserve_fe_kg)] {
        let available_kg: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material {
                    element: el, kg, ..
                } if el == element => Some(*kg),
                _ => None,
            })
            .sum();
        let surplus_kg = available_kg - reserve_kg;
        if surplus_kg > 0.0 {
            let export_kg = surplus_kg.min(batch_size_kg);
            candidates.push(TradeItemSpec::Material {
                element: element.to_string(),
                kg: export_kg,
            });
        }
    }

    candidates
}

/// Jettisons all slag from stations whose storage usage exceeds the threshold.
fn slag_jettison_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();
    let threshold = content.constants.autopilot_slag_jettison_pct;

    for station in state.stations.values() {
        let used_m3 = inventory_volume_m3(&station.inventory, content);
        let used_pct = used_m3 / station.cargo_capacity_m3;

        if used_pct >= threshold
            && station
                .inventory
                .iter()
                .any(|i| matches!(i, InventoryItem::Slag { .. }))
        {
            commands.push(make_cmd(
                owner,
                state.meta.tick,
                next_id,
                Command::JettisonSlag {
                    station_id: station.id.clone(),
                },
            ));
        }
    }
    commands
}

// ---------------------------------------------------------------------------
// AutopilotController
// ---------------------------------------------------------------------------

impl CommandSource for AutopilotController {
    #[allow(clippy::too_many_lines)]
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut commands = station_module_commands(state, content, &owner, next_command_id);
        commands.extend(lab_assignment_commands(
            state,
            content,
            &owner,
            next_command_id,
        ));
        commands.extend(thruster_import_commands(
            state,
            content,
            &owner,
            next_command_id,
        ));
        commands.extend(slag_jettison_commands(
            state,
            content,
            &owner,
            next_command_id,
        ));
        commands.extend(export_commands(state, content, &owner, next_command_id));
        commands.extend(propellant_pipeline_commands(
            state,
            content,
            &owner,
            next_command_id,
        ));

        let idle_ships = collect_idle_ships(state, &owner);
        let deep_scan_unlocked = state
            .research
            .unlocked
            .contains(&TechId("tech_deep_scan_v1".to_string()));
        let deep_scan_candidates = collect_deep_scan_candidates(state, content);
        let mut next_deep_scan = deep_scan_candidates.iter();
        let mut next_site = state.scan_sites.iter();

        // Determine if we need volatile-rich mining (for H2O or propellant pipeline)
        let has_electrolysis = state.stations.values().any(|s| {
            s.modules
                .iter()
                .any(|m| m.def_id == "module_electrolysis_unit")
        });
        let needs_water = station_has_heating_module(state)
            && (total_h2o_inventory(state) < content.constants.autopilot_volatile_threshold_kg
                || (has_electrolysis
                    && total_lh2_inventory(state) < content.constants.autopilot_lh2_threshold_kg));

        let mut mine_candidates: Vec<&AsteroidState> = state
            .asteroids
            .values()
            .filter(|a| a.mass_kg > 0.0 && a.knowledge.composition.is_some())
            .collect();
        if needs_water {
            // Prioritize H2O-rich asteroids when water inventory is low
            mine_candidates.sort_by(|a, b| {
                h2o_mining_value(b)
                    .total_cmp(&h2o_mining_value(a))
                    .then_with(|| a.id.0.cmp(&b.id.0))
            });
        } else {
            mine_candidates.sort_by(|a, b| {
                fe_mining_value(b)
                    .total_cmp(&fe_mining_value(a))
                    .then_with(|| a.id.0.cmp(&b.id.0))
            });
        }
        let mut next_mine = mine_candidates.iter();

        for ship_id in idle_ships {
            let ship = &state.ships[&ship_id];

            // Priority 1: ship has ore → deposit at nearest station.
            if let Some(task) = deposit_priority(ship, state, content) {
                commands.push(make_cmd(
                    &ship.owner,
                    state.meta.tick,
                    next_command_id,
                    Command::AssignShipTask {
                        ship_id,
                        task_kind: task,
                    },
                ));
                continue;
            }

            // Priority 2: mine best available asteroid.
            if let Some(asteroid) = next_mine.next() {
                let task = maybe_transit(
                    TaskKind::Mine {
                        asteroid: asteroid.id.clone(),
                        duration_ticks: mine_duration(asteroid, ship, content),
                    },
                    &ship.position,
                    &asteroid.position,
                    state,
                    content,
                );
                commands.push(make_cmd(
                    &ship.owner,
                    state.meta.tick,
                    next_command_id,
                    Command::AssignShipTask {
                        ship_id,
                        task_kind: task,
                    },
                ));
                continue;
            }

            // Priority 3: deep scan (enables future mining).
            if deep_scan_unlocked {
                if let Some(asteroid_id) = next_deep_scan.next() {
                    let asteroid_pos = state.asteroids[asteroid_id].position.clone();
                    let task = maybe_transit(
                        TaskKind::DeepScan {
                            asteroid: asteroid_id.clone(),
                        },
                        &ship.position,
                        &asteroid_pos,
                        state,
                        content,
                    );
                    commands.push(make_cmd(
                        &ship.owner,
                        state.meta.tick,
                        next_command_id,
                        Command::AssignShipTask {
                            ship_id,
                            task_kind: task,
                        },
                    ));
                    continue;
                }
            }

            // Priority 4: survey unscanned sites.
            if let Some(site) = next_site.next() {
                let task = maybe_transit(
                    TaskKind::Survey {
                        site: SiteId(site.id.0.clone()),
                    },
                    &ship.position,
                    &site.position,
                    state,
                    content,
                );
                commands.push(make_cmd(
                    &ship.owner,
                    state.meta.tick,
                    next_command_id,
                    Command::AssignShipTask {
                        ship_id,
                        task_kind: task,
                    },
                ));
            }
        }

        commands
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{
        test_fixtures::{base_content, base_state, test_position},
        AnomalyTag, AsteroidId, AsteroidKnowledge, AsteroidState, ComponentDef, InventoryItem,
        LotId, PricingEntry, ShipId, StationId,
    };
    use std::collections::HashMap;

    /// Autopilot tests disable research (no compute/power) and remove scan sites.
    fn autopilot_content() -> sim_core::GameContent {
        let mut content = base_content();
        content.techs.clear();
        content.constants.station_power_available_per_tick = 0.0;
        content
    }

    fn autopilot_state(content: &sim_core::GameContent) -> sim_core::GameState {
        let mut state = base_state(content);
        state.scan_sites.clear();
        // Autopilot tests don't need research compute power on the station.
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.power_available_per_tick = 0.0;
        }
        state
    }

    #[test]
    fn test_autopilot_assigns_mine_when_asteroid_known() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let asteroid_id = AsteroidId("asteroid_0001".to_string());
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id.clone(),
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 1.0)]),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: Some(HashMap::from([("Fe".to_string(), 1.0)])),
                },
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )),
            "autopilot should assign Mine task when deep-scanned asteroid is available"
        );
    }

    #[test]
    fn test_autopilot_assigns_deposit_when_ship_has_cargo() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let ship_id = ShipId("ship_0001".to_string());
        state
            .ships
            .get_mut(&ship_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_test_0001".to_string()),
                asteroid_id: AsteroidId("asteroid_test".to_string()),
                kg: 100.0,
                composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Deposit { .. },
                    ..
                } | sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Transit { .. },
                    ..
                }
            )),
            "autopilot should assign Deposit (or Transit→Deposit) when ship has cargo"
        );
    }

    #[test]
    fn test_autopilot_installs_module_in_station_inventory() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = sim_core::StationId("station_earth_orbit".to_string());
        state.stations.get_mut(&station_id).unwrap().inventory.push(
            sim_core::InventoryItem::Module {
                item_id: sim_core::ModuleItemId("module_item_0001".to_string()),
                module_def_id: "module_basic_iron_refinery".to_string(),
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })),
            "autopilot should issue InstallModule when Module item is in station inventory"
        );
    }

    #[test]
    fn test_autopilot_enables_disabled_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = sim_core::StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_0001".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: false,
                kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                    threshold_kg: 0.0,
                    ticks_since_last_run: 0,
                    stalled: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::SetModuleEnabled { enabled: true, .. }
            )),
            "autopilot should enable a disabled installed module"
        );
    }

    #[test]
    fn test_autopilot_installs_maintenance_bay() {
        let mut content = autopilot_content();
        content.module_defs.insert(
            "module_maintenance_bay".to_string(),
            sim_core::ModuleDef {
                id: "module_maintenance_bay".to_string(),
                name: "Maintenance Bay".to_string(),
                mass_kg: 2000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 5.0,
                wear_per_run: 0.0,
                behavior: sim_core::ModuleBehaviorDef::Maintenance(sim_core::MaintenanceDef {
                    repair_interval_minutes: 30,
                    repair_interval_ticks: 30,
                    wear_reduction_per_run: 0.2,
                    repair_kit_cost: 1,
                    repair_threshold: 0.0,
                    maintenance_component_id: "repair_kit".to_string(),
                }),
                thermal: None,
            },
        );
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        state.stations.get_mut(&station_id).unwrap().inventory.push(
            sim_core::InventoryItem::Module {
                item_id: sim_core::ModuleItemId("module_item_maint".to_string()),
                module_def_id: "module_maintenance_bay".to_string(),
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })),
            "autopilot should install Maintenance Bay module"
        );
    }

    #[test]
    fn test_autopilot_surveys_when_no_asteroids_known() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        // Restore scan sites (autopilot_state clears them).
        state.scan_sites = vec![sim_core::ScanSite {
            id: SiteId("site_0001".to_string()),
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        }];
        // No asteroids (default), no cargo on ship → should fall through to Survey.

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Survey { .. },
                    ..
                }
            )),
            "autopilot should assign Survey when no asteroids are known"
        );
    }

    #[test]
    fn test_autopilot_handles_no_stations() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        // Give ship some cargo so deposit would normally fire.
        let ship_id = ShipId("ship_0001".to_string());
        state
            .ships
            .get_mut(&ship_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_test_0001".to_string()),
                asteroid_id: AsteroidId("asteroid_test".to_string()),
                kg: 100.0,
                composition: HashMap::from([("Fe".to_string(), 1.0_f32)]),
            });

        // Remove all stations — deposit is impossible.
        state.stations.clear();

        // Add a scan site so ship has something to do.
        state.scan_sites = vec![sim_core::ScanSite {
            id: SiteId("site_0001".to_string()),
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        }];

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        // Should NOT crash, and should NOT issue a Deposit command.
        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Deposit { .. },
                    ..
                }
            )),
            "autopilot should not issue Deposit when no stations exist"
        );
    }

    #[test]
    fn test_autopilot_multiple_ships_get_different_assignments() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let owner = PrincipalId("principal_autopilot".to_string());

        // Add a second idle ship.
        let ship_2 = ShipId("ship_0002".to_string());
        state.ships.insert(
            ship_2.clone(),
            ShipState {
                id: ship_2,
                position: test_position(),
                owner,
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
            },
        );

        // Provide two scan sites so each ship can get a different one.
        state.scan_sites = vec![
            sim_core::ScanSite {
                id: SiteId("site_0001".to_string()),
                position: test_position(),
                template_id: "tmpl_iron_rich".to_string(),
            },
            sim_core::ScanSite {
                id: SiteId("site_0002".to_string()),
                position: test_position(),
                template_id: "tmpl_iron_rich".to_string(),
            },
        ];

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        // Collect survey site targets from the commands.
        let survey_targets: Vec<&SiteId> = commands
            .iter()
            .filter_map(|cmd| match &cmd.command {
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Survey { site, .. },
                    ..
                } => Some(site),
                _ => None,
            })
            .collect();

        assert_eq!(
            survey_targets.len(),
            2,
            "both idle ships should receive Survey tasks"
        );
        assert_ne!(
            survey_targets[0], survey_targets[1],
            "each ship should survey a different site"
        );
    }

    #[test]
    fn test_autopilot_does_not_reenable_worn_out_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_0001".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: false,
                kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                    threshold_kg: 500.0,
                    ticks_since_last_run: 0,
                    stalled: false,
                }),
                wear: sim_core::WearState { wear: 1.0 },
                power_stalled: false,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::SetModuleEnabled { enabled: true, .. }
            )),
            "autopilot should NOT re-enable a module at max wear"
        );
    }

    // --- Slag jettison tests ---

    #[test]
    fn test_autopilot_jettisons_slag_above_threshold() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        // Set small capacity so slag easily exceeds 75% threshold
        station.cargo_capacity_m3 = 100.0;
        // Add slag that takes up ~80% of capacity (slag density = 2500 kg/m3, 200kg = 0.08 m3)
        // Actually, let's use a volume that makes sense. We need volume > 75 m3.
        // Slag density is 2500 kg/m3. So 200_000 kg = 80 m3
        station.inventory.push(sim_core::InventoryItem::Slag {
            kg: 200_000.0,
            composition: HashMap::from([("slag".to_string(), 1.0)]),
        });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::JettisonSlag { .. })),
            "autopilot should issue JettisonSlag when storage usage exceeds threshold"
        );
    }

    #[test]
    fn test_autopilot_does_not_jettison_below_threshold() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        // Small amount of slag, well below 75% of 10,000 m3
        station.inventory.push(sim_core::InventoryItem::Slag {
            kg: 10.0,
            composition: HashMap::from([("slag".to_string(), 1.0)]),
        });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::JettisonSlag { .. })),
            "autopilot should NOT jettison slag when storage usage is below threshold"
        );
    }

    // --- Lab assignment tests ---

    fn lab_content_and_state() -> (sim_core::GameContent, sim_core::GameState) {
        let mut content = base_content();
        // Clear default techs and add one with domain requirement
        content.techs.clear();
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_materials_v1".to_string()),
            name: "Materials Research".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Materials, 100.0)]),
            accepted_data: vec![sim_core::DataKind::AssayData],
            difficulty: 10.0,
            effects: vec![],
        });
        // Add lab module def
        content.module_defs.insert(
            "module_materials_lab".to_string(),
            sim_core::ModuleDef {
                id: "module_materials_lab".to_string(),
                name: "Materials Lab".to_string(),
                mass_kg: 1000.0,
                volume_m3: 3.0,
                power_consumption_per_run: 2.0,
                wear_per_run: 0.01,
                behavior: sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain::Materials,
                    data_consumption_per_run: 5.0,
                    research_points_per_run: 10.0,
                    accepted_data: vec![sim_core::DataKind::AssayData],
                    research_interval_minutes: 10,
                    research_interval_ticks: 10,
                }),
                thermal: None,
            },
        );
        content.constants.station_power_available_per_tick = 0.0;
        let mut state = base_state(&content);
        state.scan_sites.clear();
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.power_available_per_tick = 0.0;
        }
        (content, state)
    }

    #[test]
    fn test_autopilot_installs_lab_module() {
        let (content, mut state) = lab_content_and_state();

        let station_id = StationId("station_earth_orbit".to_string());
        state.stations.get_mut(&station_id).unwrap().inventory.push(
            sim_core::InventoryItem::Module {
                item_id: sim_core::ModuleItemId("module_item_lab_001".to_string()),
                module_def_id: "module_materials_lab".to_string(),
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })),
            "autopilot should issue InstallModule for lab module in station inventory"
        );
    }

    #[test]
    fn test_autopilot_assigns_lab_to_eligible_tech() {
        let (content, mut state) = lab_content_and_state();

        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_lab_001".to_string()),
                def_id: "module_materials_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: None,
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech {
                    tech_id: Some(ref t),
                    ..
                } if t.0 == "tech_materials_v1"
            )),
            "autopilot should assign unassigned lab to eligible tech"
        );
    }

    #[test]
    fn test_autopilot_skips_assigned_lab() {
        let (content, mut state) = lab_content_and_state();

        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_lab_001".to_string()),
                def_id: "module_materials_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: Some(TechId("tech_materials_v1".to_string())),
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::AssignLabTech { .. })),
            "autopilot should NOT issue AssignLabTech for already-assigned lab"
        );
    }

    #[test]
    fn test_autopilot_reassigns_lab_from_unlocked_tech() {
        let (mut content, mut state) = lab_content_and_state();

        // Add a second tech so there's something to reassign to
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_materials_v2".to_string()),
            name: "Materials Research v2".to_string(),
            prereqs: vec![TechId("tech_materials_v1".to_string())],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Materials, 200.0)]),
            accepted_data: vec![sim_core::DataKind::AssayData],
            difficulty: 10.0,
            effects: vec![],
        });

        // Mark tech_materials_v1 as unlocked (its prereq for v2)
        state
            .research
            .unlocked
            .insert(TechId("tech_materials_v1".to_string()));

        // Lab is assigned to the already-unlocked tech
        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_lab_001".to_string()),
                def_id: "module_materials_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: Some(TechId("tech_materials_v1".to_string())),
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech {
                    tech_id: Some(ref t),
                    ..
                } if t.0 == "tech_materials_v2"
            )),
            "autopilot should reassign lab from unlocked tech to next eligible tech"
        );
    }

    // --- Engineering lab assignment test ---

    #[test]
    fn test_lab_assignment_assigns_engineering_lab_to_ship_construction() {
        let mut content = base_content();
        content.techs.clear();
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_ship_construction".to_string()),
            name: "Ship Construction".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Manufacturing, 200.0)]),
            accepted_data: vec![sim_core::DataKind::ManufacturingData],
            difficulty: 500.0,
            effects: vec![],
        });
        content.module_defs.insert(
            "module_engineering_lab".to_string(),
            sim_core::ModuleDef {
                id: "module_engineering_lab".to_string(),
                name: "Engineering Lab".to_string(),
                mass_kg: 4000.0,
                volume_m3: 8.0,
                power_consumption_per_run: 12.0,
                wear_per_run: 0.005,
                behavior: sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain::Manufacturing,
                    data_consumption_per_run: 10.0,
                    research_points_per_run: 5.0,
                    accepted_data: vec![sim_core::DataKind::ManufacturingData],
                    research_interval_minutes: 1,
                    research_interval_ticks: 1,
                }),
                thermal: None,
            },
        );
        content.constants.station_power_available_per_tick = 0.0;

        let mut state = base_state(&content);
        state.scan_sites.clear();
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.power_available_per_tick = 0.0;
        }

        // Install engineering lab module on the station
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_eng_lab_001".to_string()),
                def_id: "module_engineering_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: None,
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech {
                    tech_id: Some(ref t),
                    ..
                } if t.0 == "tech_ship_construction"
            )),
            "autopilot should assign engineering lab to tech_ship_construction"
        );
    }

    // --- Thruster import tests ---

    /// Helper to set up state for thruster import tests.
    /// Shipyard recipe requires 4 thrusters, assembly interval is 1440 ticks.
    fn thruster_import_setup() -> (sim_core::GameContent, sim_core::GameState) {
        let mut content = base_content();
        content.techs.clear();
        content.constants.station_power_available_per_tick = 0.0;

        // Add shipyard module def with a recipe requiring 4 thrusters
        content.module_defs.insert(
            "module_shipyard".to_string(),
            sim_core::ModuleDef {
                id: "module_shipyard".to_string(),
                name: "Shipyard".to_string(),
                mass_kg: 5000.0,
                volume_m3: 20.0,
                power_consumption_per_run: 25.0,
                wear_per_run: 0.02,
                behavior: sim_core::ModuleBehaviorDef::Assembler(sim_core::AssemblerDef {
                    assembly_interval_minutes: 1440,
                    assembly_interval_ticks: 1440,
                    recipes: vec![sim_core::RecipeDef {
                        id: "recipe_test_ship".to_string(),
                        inputs: vec![
                            sim_core::RecipeInput {
                                filter: sim_core::InputFilter::Element("Fe".to_string()),
                                amount: sim_core::InputAmount::Kg(5000.0),
                            },
                            sim_core::RecipeInput {
                                filter: sim_core::InputFilter::Component(ComponentId(
                                    "thruster".to_string(),
                                )),
                                amount: sim_core::InputAmount::Count(4),
                            },
                        ],
                        outputs: vec![sim_core::OutputSpec::Ship {
                            cargo_capacity_m3: 50.0,
                        }],
                        efficiency: 1.0,
                        thermal_req: None,
                    }],
                    max_stock: HashMap::new(),
                }),
                thermal: None,
            },
        );

        // Add thruster component def (needed for mass calculation)
        content.component_defs.push(sim_core::ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 200.0,
            volume_m3: 2.0,
        });

        // Set up pricing for thruster
        content.pricing = sim_core::PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: HashMap::from([(
                "thruster".to_string(),
                sim_core::PricingEntry {
                    base_price_per_unit: 50_000.0,
                    importable: true,
                    exportable: true,
                    ..Default::default()
                },
            )]),
        };

        let mut state = base_state(&content);
        state.scan_sites.clear();

        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.power_available_per_tick = 0.0;

        // Install enabled shipyard module
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("module_inst_shipyard_001".to_string()),
            def_id: "module_shipyard".to_string(),
            enabled: true,
            kind_state: sim_core::ModuleKindState::Assembler(sim_core::AssemblerState {
                ticks_since_last_run: 0,
                stalled: false,
                capped: false,
                cap_override: HashMap::new(),
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
            thermal: None,
        });

        // Add 5000 kg Fe to station inventory
        station.inventory.push(sim_core::InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 5000.0,
            quality: 1.0,
            thermal: None,
        });

        // Unlock tech_ship_construction
        state
            .research
            .unlocked
            .insert(TechId("tech_ship_construction".to_string()));

        // Set high balance and advance past trade unlock
        state.balance = 10_000_000.0;
        state.meta.tick = sim_core::trade_unlock_tick(content.constants.minutes_per_tick);

        (content, state)
    }

    #[test]
    fn test_autopilot_imports_thrusters_when_shipyard_ready() {
        let (content, state) = thruster_import_setup();

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let import_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component {
                        component_id,
                        count: 4,
                    },
                    ..
                } if component_id.0 == "thruster"
            )
        });

        assert!(
            import_cmd.is_some(),
            "autopilot should import 4 thrusters when shipyard is ready and past interval"
        );
    }

    #[test]
    fn test_autopilot_no_thruster_import_when_balance_low() {
        let (content, mut state) = thruster_import_setup();
        state.balance = 100.0;

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "autopilot should NOT import thrusters when balance is too low"
        );
    }

    #[test]
    fn test_autopilot_no_thruster_import_when_tech_not_unlocked() {
        let (content, mut state) = thruster_import_setup();
        state.research.unlocked.clear();

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "autopilot should NOT import thrusters when tech_ship_construction is not unlocked"
        );
    }

    #[test]
    fn test_autopilot_no_thruster_import_exceeds_budget_cap() {
        let (content, mut state) = thruster_import_setup();
        // Set balance so that import cost > 5% of balance.
        // 4 thrusters: (50_000 * 4) + (200 * 4 * 100) = 200_000 + 80_000 = 280_000
        // 280_000 / 0.05 = 5_600_000 — so balance below that should block
        state.balance = 5_000_000.0;

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "autopilot should NOT import thrusters when cost exceeds 5% budget cap"
        );
    }

    // -----------------------------------------------------------------------
    // Export tests
    // -----------------------------------------------------------------------

    /// Set up content and state for export tests: pricing entries, component defs,
    /// tick past trade unlock.
    fn export_setup() -> (sim_core::GameContent, sim_core::GameState) {
        let mut content = autopilot_content();
        // Add pricing entries
        content.pricing.items.insert(
            "repair_kit".to_string(),
            PricingEntry {
                base_price_per_unit: 8000.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        content.pricing.items.insert(
            "He".to_string(),
            PricingEntry {
                base_price_per_unit: 200.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        content.pricing.items.insert(
            "Si".to_string(),
            PricingEntry {
                base_price_per_unit: 80.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        content.pricing.items.insert(
            "Fe".to_string(),
            PricingEntry {
                base_price_per_unit: 50.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        // Add He element (not in base_content) for density lookup
        content.elements.push(sim_core::ElementDef {
            id: "He".to_string(),
            density_kg_per_m3: 125.0,
            display_name: "Helium-3".to_string(),
            refined_name: None,
            category: "material".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: None,
            boiling_point_mk: None,
        });
        content.init_caches(); // Rebuild density_map with He
                               // Add component def for repair_kit (needed for mass calculation)
        content.component_defs.push(ComponentDef {
            id: "repair_kit".to_string(),
            name: "Repair Kit".to_string(),
            mass_kg: 50.0,
            volume_m3: 0.05,
        });

        let mut state = autopilot_state(&content);
        // Set tick past trade unlock (minutes_per_tick=1 → unlock at 525,600)
        state.meta.tick = 525_601;
        (content, state)
    }

    #[test]
    fn test_export_surplus_kits_above_reserve() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 15 repair kits (reserve = 10, so 5 surplus)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 15,
                quality: 1.0,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let export_cmds: Vec<_> = commands
            .iter()
            .filter(|cmd| {
                matches!(
                    &cmd.command,
                    Command::Export {
                        item_spec: TradeItemSpec::Component {
                            component_id,
                            count: 5,
                            ..
                        },
                        ..
                    } if component_id.0 == "repair_kit"
                )
            })
            .collect();
        assert_eq!(
            export_cmds.len(),
            1,
            "should export exactly 5 surplus kits (15 - 10 reserve)"
        );
    }

    #[test]
    fn test_export_kits_at_reserve_not_exported() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add exactly 10 repair kits (= reserve, no surplus)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 10,
                quality: 1.0,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Export {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "should NOT export kits when at reserve threshold"
        );
    }

    #[test]
    fn test_export_fe_zero_margin_not_exported() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 20,000 kg Fe (reserve is 12,000, surplus is 8,000)
        // But Fe has $0 margin: base $50/kg - surcharge $50/kg = $0
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 20_000.0,
                quality: 1.0,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Export {
                    item_spec: TradeItemSpec::Material { ref element, .. },
                    ..
                } if element == "Fe"
            )),
            "should NOT export Fe when revenue is $0"
        );
    }

    #[test]
    fn test_export_si_when_present() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 1000 kg Si (no reserve, batch_size=500, so exports 500 kg)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Material {
                element: "Si".to_string(),
                kg: 1000.0,
                quality: 1.0,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let export_cmds: Vec<_> = commands
            .iter()
            .filter(|cmd| {
                matches!(
                    &cmd.command,
                    Command::Export {
                        item_spec: TradeItemSpec::Material { ref element, .. },
                        ..
                    } if element == "Si"
                )
            })
            .collect();
        assert_eq!(export_cmds.len(), 1, "should export Si when present");
        // Verify batch size capping
        if let Command::Export { item_spec, .. } = &export_cmds[0].command {
            if let TradeItemSpec::Material { kg, .. } = item_spec {
                assert!(
                    (*kg - 500.0).abs() < f32::EPSILON,
                    "should cap at batch_size_kg (500)"
                );
            }
        }
    }

    #[test]
    fn test_export_he_when_present() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 200 kg He (no reserve, under batch_size so exports all 200 kg)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Material {
                element: "He".to_string(),
                kg: 200.0,
                quality: 1.0,
                thermal: None,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let export_cmds: Vec<_> = commands
            .iter()
            .filter(|cmd| {
                matches!(
                    &cmd.command,
                    Command::Export {
                        item_spec: TradeItemSpec::Material { ref element, .. },
                        ..
                    } if element == "He"
                )
            })
            .collect();
        assert_eq!(export_cmds.len(), 1, "should export He when present");
        if let Command::Export { item_spec, .. } = &export_cmds[0].command {
            if let TradeItemSpec::Material { kg, .. } = item_spec {
                assert!(
                    (*kg - 200.0).abs() < f32::EPSILON,
                    "should export all 200 kg (under batch_size)"
                );
            }
        }
    }

    #[test]
    fn test_no_exports_before_trade_unlock() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Set tick before trade unlock
        state.meta.tick = 100;

        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 50,
                quality: 1.0,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands
                .iter()
                .any(|cmd| matches!(&cmd.command, Command::Export { .. })),
            "should NOT export anything before trade unlock"
        );
    }

    #[test]
    fn test_autopilot_prefers_volatile_when_water_low() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        // Install a heating module on the station
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("mod_heat_001".to_string()),
            def_id: "module_heating_unit".to_string(),
            enabled: true,
            wear: sim_core::WearState::default(),
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 0.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
            thermal: None,
            power_stalled: false,
        });
        // No H2O in inventory → needs_water = true

        // Add both Fe-rich and H2O-rich asteroids
        let fe_asteroid = AsteroidId("asteroid_fe".to_string());
        state.asteroids.insert(
            fe_asteroid.clone(),
            AsteroidState {
                id: fe_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 0.8)]),
                anomaly_tags: vec![AnomalyTag::new("IronRich")],
                mass_kg: 5000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("IronRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("Fe".to_string(), 0.8),
                        ("Si".to_string(), 0.2),
                    ])),
                },
            },
        );

        let h2o_asteroid = AsteroidId("asteroid_h2o".to_string());
        state.asteroids.insert(
            h2o_asteroid.clone(),
            AsteroidState {
                id: h2o_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 3000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("H2O".to_string(), 0.5),
                        ("Fe".to_string(), 0.1),
                    ])),
                },
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        // The mine command should target the H2O-rich asteroid
        let mine_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )
        });
        assert!(mine_cmd.is_some(), "should assign a mine task");
        if let Some(cmd) = mine_cmd {
            if let Command::AssignShipTask {
                task_kind: TaskKind::Mine { asteroid, .. },
                ..
            } = &cmd.command
            {
                assert_eq!(
                    *asteroid, h2o_asteroid,
                    "should prefer H2O-rich asteroid when water is needed"
                );
            }
        }
    }

    #[test]
    fn test_autopilot_prefers_fe_when_no_heating_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);
        // No heating module → normal Fe-targeting behavior

        let fe_asteroid = AsteroidId("asteroid_fe".to_string());
        state.asteroids.insert(
            fe_asteroid.clone(),
            AsteroidState {
                id: fe_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 0.8)]),
                anomaly_tags: vec![AnomalyTag::new("IronRich")],
                mass_kg: 5000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("IronRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("Fe".to_string(), 0.8),
                        ("Si".to_string(), 0.2),
                    ])),
                },
            },
        );

        let h2o_asteroid = AsteroidId("asteroid_h2o".to_string());
        state.asteroids.insert(
            h2o_asteroid.clone(),
            AsteroidState {
                id: h2o_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 3000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("H2O".to_string(), 0.5),
                        ("Fe".to_string(), 0.1),
                    ])),
                },
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let mine_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )
        });
        assert!(mine_cmd.is_some(), "should assign a mine task");
        if let Some(cmd) = mine_cmd {
            if let Command::AssignShipTask {
                task_kind: TaskKind::Mine { asteroid, .. },
                ..
            } = &cmd.command
            {
                assert_eq!(
                    *asteroid, fe_asteroid,
                    "should prefer Fe-rich asteroid when no heating module"
                );
            }
        }
    }

    #[test]
    fn test_deep_scan_includes_volatile_rich() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let asteroid_id = AsteroidId("asteroid_vol".to_string());
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 2000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: None, // Not deep-scanned yet
                },
            },
        );

        let candidates = collect_deep_scan_candidates(&state, &content);
        assert!(
            candidates.contains(&asteroid_id),
            "VolatileRich asteroids should be deep scan candidates"
        );
    }

    #[test]
    fn test_autopilot_prefers_fe_when_h2o_above_threshold() {
        let mut content = autopilot_content();
        // Add H2O element to content so inventory volume calc works
        content.elements.push(sim_core::ElementDef {
            id: "H2O".to_string(),
            density_kg_per_m3: 1000.0,
            display_name: "Water Ice".to_string(),
            refined_name: Some("Water".to_string()),
            category: "material".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: None,
            boiling_point_mk: None,
        });
        let mut state = autopilot_state(&content);

        // Install heating module
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("mod_heat_001".to_string()),
            def_id: "module_heating_unit".to_string(),
            enabled: true,
            wear: sim_core::WearState::default(),
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 0.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
            thermal: None,
            power_stalled: false,
        });
        // Add H2O above threshold (500 kg) → should NOT trigger volatile targeting
        station.inventory.push(InventoryItem::Material {
            element: "H2O".to_string(),
            kg: 600.0,
            quality: 1.0,
            thermal: None,
        });

        let fe_asteroid = AsteroidId("asteroid_fe".to_string());
        state.asteroids.insert(
            fe_asteroid.clone(),
            AsteroidState {
                id: fe_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 0.8)]),
                anomaly_tags: vec![AnomalyTag::new("IronRich")],
                mass_kg: 5000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("IronRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("Fe".to_string(), 0.8),
                        ("Si".to_string(), 0.2),
                    ])),
                },
            },
        );

        let h2o_asteroid = AsteroidId("asteroid_h2o".to_string());
        state.asteroids.insert(
            h2o_asteroid.clone(),
            AsteroidState {
                id: h2o_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 3000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("H2O".to_string(), 0.5),
                        ("Fe".to_string(), 0.1),
                    ])),
                },
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let mine_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )
        });
        assert!(mine_cmd.is_some(), "should assign a mine task");
        if let Some(cmd) = mine_cmd {
            if let Command::AssignShipTask {
                task_kind: TaskKind::Mine { asteroid, .. },
                ..
            } = &cmd.command
            {
                assert_eq!(
                    *asteroid, fe_asteroid,
                    "should prefer Fe when H2O is above threshold despite heating module"
                );
            }
        }
    }

    // ── Propellant pipeline tests ───────────────────────────────────────

    fn add_electrolysis_module(state: &mut sim_core::GameState, enabled: bool) {
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("electrolysis_inst_001".to_string()),
            def_id: "module_electrolysis_unit".to_string(),
            enabled,
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 200.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
            thermal: None,
        });
    }

    fn add_heating_module(state: &mut sim_core::GameState, enabled: bool) {
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("heating_inst_001".to_string()),
            def_id: "module_heating_unit".to_string(),
            enabled,
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
            thermal: None,
        });
    }

    fn add_lh2_inventory(state: &mut sim_core::GameState, kg: f32) {
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Material {
            element: "LH2".to_string(),
            kg,
            quality: 1.0,
            thermal: None,
        });
    }

    #[test]
    fn test_propellant_noop_without_electrolysis() {
        let content = autopilot_content();
        let state = autopilot_state(&content);
        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;

        let commands = propellant_pipeline_commands(&state, &content, &owner, &mut next_id);
        assert!(
            commands.is_empty(),
            "should emit no commands when station has no electrolysis module"
        );
    }

    #[test]
    fn test_propellant_enables_when_lh2_low() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);
        add_electrolysis_module(&mut state, false);
        add_heating_module(&mut state, false);
        // LH2 = 0 (below threshold of 5000)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = propellant_pipeline_commands(&state, &content, &owner, &mut next_id);

        let enables_electrolysis = commands.iter().any(|cmd| {
            matches!(
                &cmd.command,
                Command::SetModuleEnabled { module_id, enabled: true, .. }
                if module_id.0 == "electrolysis_inst_001"
            )
        });
        let enables_heating = commands.iter().any(|cmd| {
            matches!(
                &cmd.command,
                Command::SetModuleEnabled { module_id, enabled: true, .. }
                if module_id.0 == "heating_inst_001"
            )
        });

        assert!(
            enables_electrolysis,
            "should enable disabled electrolysis when LH2 is low"
        );
        assert!(
            enables_heating,
            "should enable disabled heating when LH2 is low"
        );
    }

    #[test]
    fn test_propellant_disables_when_lh2_abundant() {
        let mut content = autopilot_content();
        content.constants.autopilot_lh2_threshold_kg = 1000.0;
        let mut state = autopilot_state(&content);
        add_electrolysis_module(&mut state, true);
        add_lh2_inventory(&mut state, 3000.0); // > 2x threshold (2000)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = propellant_pipeline_commands(&state, &content, &owner, &mut next_id);

        let disables = commands.iter().any(|cmd| {
            matches!(
                &cmd.command,
                Command::SetModuleEnabled { module_id, enabled: false, .. }
                if module_id.0 == "electrolysis_inst_001"
            )
        });

        assert!(
            disables,
            "should disable electrolysis when LH2 > 2x threshold"
        );
    }

    #[test]
    fn test_propellant_dead_band_no_commands() {
        let mut content = autopilot_content();
        content.constants.autopilot_lh2_threshold_kg = 1000.0;
        let mut state = autopilot_state(&content);
        add_electrolysis_module(&mut state, true);
        add_lh2_inventory(&mut state, 1500.0); // Between threshold (1000) and 2x (2000)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = propellant_pipeline_commands(&state, &content, &owner, &mut next_id);

        assert!(
            commands.is_empty(),
            "should emit no commands in dead band (threshold < LH2 < 2x threshold)"
        );
    }

    #[test]
    fn test_propellant_skips_max_worn_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("electrolysis_inst_001".to_string()),
            def_id: "module_electrolysis_unit".to_string(),
            enabled: false,
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 200.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
            wear: sim_core::WearState { wear: 1.0 },
            power_stalled: false,
            thermal: None,
        });
        // LH2 = 0 (below threshold)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = propellant_pipeline_commands(&state, &content, &owner, &mut next_id);

        assert!(
            commands.is_empty(),
            "should not enable max-worn electrolysis module"
        );
    }
}
