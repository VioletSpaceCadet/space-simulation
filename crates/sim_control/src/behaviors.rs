use sim_core::{
    compute_entity_absolute, is_co_located, travel_ticks, AsteroidId, AsteroidState, Command,
    CommandEnvelope, CommandId, ComponentId, DomainProgress, GameContent, GameState, InventoryItem,
    Position, PrincipalId, ShipId, ShipState, StationState, TaskKind, TechDef, TradeItemSpec,
};

pub(crate) const AUTOPILOT_OWNER: &str = "principal_autopilot";

// ---------------------------------------------------------------------------
// Shared helpers (used by agents)
// ---------------------------------------------------------------------------

/// Allocates a command ID and builds a `CommandEnvelope`.
pub(crate) fn make_cmd(
    owner: &PrincipalId,
    tick: u64,
    next_id: &mut u64,
    command: Command,
) -> CommandEnvelope {
    let cmd_id = CommandId(*next_id);
    *next_id += 1;
    CommandEnvelope {
        id: cmd_id,
        issued_by: owner.clone(),
        issued_tick: tick,
        execute_at_tick: tick,
        command,
    }
}

/// Wraps `task` in a Transit if `from` and `to` are not co-located; else returns `task` as-is.
pub(crate) fn maybe_transit(
    task: TaskKind,
    from: &Position,
    to: &Position,
    ship_ticks_per_au: u64,
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
    let base_ticks = travel_ticks(
        from_abs,
        to_abs,
        ship_ticks_per_au,
        content.constants.min_transit_ticks,
    );
    // Apply navigation beacon bonus from both origin and destination zones.
    let origin_bonus = sim_core::zone_nav_bonus(&from.parent_body.0, state);
    let dest_bonus = sim_core::zone_nav_bonus(&to.parent_body.0, state);
    let best_bonus = origin_bonus.min(dest_bonus);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks =
        ((base_ticks as f64 * best_bonus).round() as u64).max(content.constants.min_transit_ticks);
    TaskKind::Transit {
        destination: to.clone(),
        total_ticks: ticks,
        then: Box::new(task),
    }
}

/// Check if ship should opportunistically refuel (below threshold, at station with LH2).
pub(crate) fn should_opportunistic_refuel(
    ship: &ShipState,
    state: &GameState,
    content: &GameContent,
) -> bool {
    if content.constants.fuel_cost_per_au <= 0.0 || ship.propellant_capacity_kg <= 0.0 {
        return false;
    }
    let fuel_pct = ship.propellant_kg / ship.propellant_capacity_kg;
    if fuel_pct >= content.autopilot.refuel_threshold_pct {
        return false;
    }
    // Only refuel if at a station with LH2
    try_refuel(ship, state, content).is_some()
}

/// Try to issue a Refuel task if ship is at a station with LH2.
pub(crate) fn try_refuel(
    ship: &ShipState,
    state: &GameState,
    content: &GameContent,
) -> Option<TaskKind> {
    if content.constants.fuel_cost_per_au <= 0.0 {
        return None;
    }
    // Only refuel if below max capacity threshold
    if ship.propellant_kg >= ship.propellant_capacity_kg * content.autopilot.refuel_max_pct {
        return None;
    }
    // Find co-located station with LH2
    let station = state.stations.values().find(|s| {
        is_co_located(
            &ship.position,
            &s.position,
            &state.body_cache,
            content.constants.docking_range_au_um,
        ) && s.core.inventory.iter().any(|item| {
            matches!(item, InventoryItem::Material { element, kg, .. }
                if *element == content.autopilot.propellant_element && *kg > content.constants.min_meaningful_kg)
        })
    })?;
    Some(TaskKind::Refuel {
        station_id: station.id.clone(),
        target_kg: ship.propellant_capacity_kg,
    })
}

/// Returns idle autopilot ships. `BTreeMap` iteration is already sorted by ID.
pub(crate) fn collect_idle_ships(state: &GameState, owner: &PrincipalId) -> Vec<ShipId> {
    state
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
        .collect()
}

/// Returns asteroid IDs above confidence threshold with unknown composition,
/// sorted by distance from `reference_pos` (nearest first), with ID tiebreak for determinism.
/// Targets are read from `content.autopilot.deep_scan_targets`.
pub(crate) fn collect_deep_scan_candidates(
    state: &GameState,
    content: &GameContent,
    reference_pos: &Position,
) -> Vec<AsteroidId> {
    if state.asteroids.is_empty() {
        return Vec::new();
    }
    let targets = &content.autopilot.deep_scan_targets;
    let ref_abs = compute_entity_absolute(reference_pos, &state.body_cache);
    let mut candidates: Vec<(AsteroidId, u128)> = state
        .asteroids
        .values()
        .filter(|asteroid| {
            asteroid.knowledge.composition.is_none()
                && asteroid.knowledge.tag_beliefs.iter().any(|(tag, conf)| {
                    targets
                        .iter()
                        .any(|target| tag.0 == target.tag && *conf > target.min_confidence)
                })
        })
        .map(|a| {
            let dist =
                ref_abs.distance_squared(compute_entity_absolute(&a.position, &state.body_cache));
            (a.id.clone(), dist)
        })
        .collect();
    candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0 .0.cmp(&b.0 .0)));
    candidates.into_iter().map(|(id, _)| id).collect()
}

/// Check if any station has a module with the given role installed.
pub(crate) fn station_has_module_with_role(state: &GameState, role: &str) -> bool {
    state
        .stations
        .values()
        .any(|station| station.has_role(role))
}

/// Total inventory of a specific element across all stations.
pub(crate) fn total_element_inventory(state: &GameState, element: &str) -> f32 {
    state
        .stations
        .values()
        .flat_map(|s| s.core.inventory.iter())
        .filter_map(|item| match item {
            InventoryItem::Material {
                element: el, kg, ..
            } if el == element => Some(*kg),
            _ => None,
        })
        .sum()
}

/// Mining value for sorting: `mass_kg × element_fraction`.
pub(crate) fn element_mining_value(asteroid: &AsteroidState, element: &str) -> f32 {
    asteroid.mass_kg
        * asteroid
            .knowledge
            .composition
            .as_ref()
            .and_then(|c| c.get(element))
            .copied()
            .unwrap_or(0.0)
}

/// Priority 1: if ship has ore, return a Deposit (or Transit→Deposit) task to the nearest station.
pub(crate) fn deposit_priority(
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
        ship.ticks_per_au(content.constants.ticks_per_au),
        state,
        content,
    ))
}

/// Geometric mean of per-domain ratios (accumulated / required), clamped to [0, 1].
pub(crate) fn compute_sufficiency(tech: &TechDef, progress: Option<&DomainProgress>) -> f32 {
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

/// Builds the list of export candidates for a station in priority order.
/// Reads component and element export config from `autopilot`.
pub(crate) fn build_export_candidates(
    station: &StationState,
    autopilot: &sim_core::AutopilotConfig,
    batch_size_kg: f32,
) -> Vec<TradeItemSpec> {
    let mut candidates = Vec::new();

    // 1. Export component surplus above reserve
    let export_comp = &autopilot.export_component;
    let comp_count: u32 = station
        .core
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == export_comp.component_id => Some(*count),
            _ => None,
        })
        .sum();
    if comp_count > export_comp.reserve {
        candidates.push(TradeItemSpec::Component {
            component_id: ComponentId(export_comp.component_id.clone()),
            count: comp_count - export_comp.reserve,
        });
    }

    // 2+. Materials in priority order from config
    for entry in &autopilot.export_elements {
        let available_kg: f32 = station
            .core
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material {
                    element: el, kg, ..
                } if *el == entry.element => Some(*kg),
                _ => None,
            })
            .sum();
        let surplus_kg = available_kg - entry.reserve_kg;
        if surplus_kg > 0.0 {
            let export_kg = surplus_kg.min(batch_size_kg);
            candidates.push(TradeItemSpec::Material {
                element: entry.element.clone(),
                kg: export_kg,
            });
        }
    }

    candidates
}
