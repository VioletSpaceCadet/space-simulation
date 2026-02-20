use crate::{
    AnomalyTag, AsteroidId, AsteroidKnowledge, AsteroidState, CompositionVec, Constants, DataKind,
    Event, EventEnvelope, GameContent, GameState, InventoryItem, LotId, NodeId, ResearchState,
    ShipId, ShipState, SiteId, StationId, TaskKind, TaskState, TechEffect,
};
use rand::Rng;

pub(crate) fn task_duration(kind: &TaskKind, constants: &Constants) -> u64 {
    match kind {
        TaskKind::Transit { total_ticks, .. } => *total_ticks,
        TaskKind::Survey { .. } => constants.survey_scan_ticks,
        TaskKind::DeepScan { .. } => constants.deep_scan_ticks,
        TaskKind::Mine { duration_ticks, .. } => *duration_ticks,
        TaskKind::Deposit { .. } => constants.deposit_ticks,
        TaskKind::Idle => 0,
    }
}

pub(crate) fn task_kind_label(kind: &TaskKind) -> &'static str {
    match kind {
        TaskKind::Idle => "Idle",
        TaskKind::Transit { .. } => "Transit",
        TaskKind::Survey { .. } => "Survey",
        TaskKind::DeepScan { .. } => "DeepScan",
        TaskKind::Mine { .. } => "Mine",
        TaskKind::Deposit { .. } => "Deposit",
    }
}

pub(crate) fn task_target(kind: &TaskKind) -> Option<String> {
    match kind {
        TaskKind::Idle => None,
        TaskKind::Transit { destination, .. } => Some(destination.0.clone()),
        TaskKind::Survey { site } => Some(site.0.clone()),
        TaskKind::DeepScan { asteroid } | TaskKind::Mine { asteroid, .. } => {
            Some(asteroid.0.clone())
        }
        TaskKind::Deposit { station } => Some(station.0.clone()),
    }
}

/// True if any unlocked tech grants the `EnableDeepScan` effect.
pub(crate) fn deep_scan_enabled(research: &ResearchState, content: &GameContent) -> bool {
    content
        .techs
        .iter()
        .filter(|tech| research.unlocked.contains(&tech.id))
        .flat_map(|tech| &tech.effects)
        .any(|effect| matches!(effect, TechEffect::EnableDeepScan))
}

/// Composition noise sigma from unlocked tech effects, defaulting to 0.0.
fn composition_noise_sigma(research: &ResearchState, content: &GameContent) -> f32 {
    content
        .techs
        .iter()
        .filter(|tech| research.unlocked.contains(&tech.id))
        .flat_map(|tech| &tech.effects)
        .find_map(|effect| match effect {
            TechEffect::DeepScanCompositionNoise { sigma } => Some(*sigma),
            TechEffect::EnableDeepScan => None,
        })
        .unwrap_or(0.0)
}

/// Returns the density (kg/m³) for the given element id.
///
/// Panics if the element is not found in content — that is a content authoring error.
fn element_density(content: &GameContent, element_id: &str) -> f32 {
    content
        .elements
        .iter()
        .find(|e| e.id == element_id)
        .unwrap_or_else(|| panic!("element '{element_id}' not found in content"))
        .density_kg_per_m3
}

/// Volume (m³) currently occupied by the inventory items.
pub fn inventory_volume_m3(inventory: &[InventoryItem], content: &GameContent) -> f32 {
    inventory
        .iter()
        .map(|item| match item {
            InventoryItem::Ore { kg, .. } => kg / element_density(content, "ore"),
            InventoryItem::Slag { kg, .. } => kg / element_density(content, "slag"),
            InventoryItem::Material { element, kg, .. } => kg / element_density(content, element),
            InventoryItem::Component { count, .. } => *count as f32 * 1.0, // 1.0 m³ per unit; replace with ComponentDef.volume_m3 when defs exist
            InventoryItem::Module { module_def_id, .. } => content
                .module_defs
                .iter()
                .find(|m| m.id == *module_def_id)
                .map_or(0.0, |m| m.volume_m3), // TODO: unknown module def — should not occur in valid state
        })
        .sum()
}

/// Pre-compute how many ticks a mining run will take.
///
/// Stops when the cargo hold fills OR the asteroid is depleted, whichever comes first.
pub fn mine_duration(asteroid: &AsteroidState, ship: &ShipState, content: &GameContent) -> u64 {
    let ore_density = element_density(content, "ore");
    let effective_m3_per_kg = 1.0 / ore_density;

    let volume_used = inventory_volume_m3(&ship.inventory, content);
    let free_volume = (ship.cargo_capacity_m3 - volume_used).max(0.0);
    let rate = content.constants.mining_rate_kg_per_tick;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks_to_fill = (free_volume / (rate * effective_m3_per_kg)).ceil() as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks_to_deplete = (asteroid.mass_kg / rate).ceil() as u64;

    ticks_to_fill.min(ticks_to_deplete).max(1)
}

/// Normalise a composition map so values sum to 1.0. No-op if sum is zero.
fn normalise(composition: &mut CompositionVec) {
    let total: f32 = composition.values().sum();
    if total > 0.0 {
        for value in composition.values_mut() {
            *value /= total;
        }
    }
}

pub(crate) fn set_ship_idle(state: &mut GameState, ship_id: &ShipId, current_tick: u64) {
    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.task = Some(TaskState {
            kind: TaskKind::Idle,
            started_tick: current_tick,
            eta_tick: current_tick,
        });
    }
}

pub(crate) fn resolve_transit(
    state: &mut GameState,
    ship_id: &ShipId,
    destination: &NodeId,
    then: &TaskKind,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.location_node = destination.clone();
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ShipArrived {
            ship_id: ship_id.clone(),
            node: destination.clone(),
        },
    ));

    // Start the follow-on task immediately.
    let duration = task_duration(then, &content.constants);
    let label = task_kind_label(then).to_string();
    let target = task_target(then);

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.task = Some(TaskState {
            kind: then.clone(),
            started_tick: current_tick,
            eta_tick: current_tick + duration,
        });
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskStarted {
            ship_id: ship_id.clone(),
            task_kind: label,
            target,
        },
    ));
}

pub(crate) fn resolve_survey(
    state: &mut GameState,
    ship_id: &ShipId,
    site_id: &SiteId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(site_pos) = state.scan_sites.iter().position(|s| &s.id == site_id) else {
        return; // Site already consumed — shouldn't happen with valid state.
    };
    let site = state.scan_sites.remove(site_pos);

    let Some(template) = content
        .asteroid_templates
        .iter()
        .find(|t| t.id == site.template_id)
    else {
        return; // Unknown template — content error.
    };

    // Roll composition from ranges, then normalise.
    let mut composition: CompositionVec = template
        .composition_ranges
        .iter()
        .map(|(element, &(min, max))| (element.clone(), rng.gen_range(min..=max)))
        .collect();
    normalise(&mut composition);

    let mass_kg = rng
        .gen_range(content.constants.asteroid_mass_min_kg..=content.constants.asteroid_mass_max_kg);

    let asteroid_id = AsteroidId(format!("asteroid_{:04}", state.counters.next_asteroid_id));
    state.counters.next_asteroid_id += 1;

    let anomaly_tags = template.anomaly_tags.clone();
    state.asteroids.insert(
        asteroid_id.clone(),
        AsteroidState {
            id: asteroid_id.clone(),
            location_node: site.node.clone(),
            true_composition: composition,
            anomaly_tags: anomaly_tags.clone(),
            mass_kg,
            knowledge: AsteroidKnowledge {
                tag_beliefs: vec![],
                composition: None,
            },
        },
    );

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::AsteroidDiscovered {
            asteroid_id: asteroid_id.clone(),
            location_node: site.node.clone(),
        },
    ));

    // Detect anomaly tags probabilistically.
    let detection_prob = content.constants.survey_tag_detection_probability;
    let detected_tags: Vec<(AnomalyTag, f32)> = anomaly_tags
        .iter()
        .filter(|_| rng.gen::<f32>() < detection_prob)
        .map(|tag| (tag.clone(), detection_prob))
        .collect();

    if let Some(asteroid) = state.asteroids.get_mut(&asteroid_id) {
        asteroid.knowledge.tag_beliefs.clone_from(&detected_tags);
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ScanResult {
            asteroid_id: asteroid_id.clone(),
            tags: detected_tags,
        },
    ));

    let amount = content.constants.survey_scan_data_amount;
    let quality = content.constants.survey_scan_data_quality;
    *state
        .research
        .data_pool
        .entry(DataKind::ScanData)
        .or_insert(0.0) += amount * quality;

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::ScanData,
            amount,
            quality,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Survey".to_string(),
            target: Some(site_id.0.clone()),
        },
    ));
}

pub(crate) fn resolve_mine(
    state: &mut GameState,
    ship_id: &ShipId,
    asteroid_id: &AsteroidId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(asteroid) = state.asteroids.get(asteroid_id) else {
        set_ship_idle(state, ship_id, current_tick);
        return;
    };

    let Some(ship) = state.ships.get(ship_id) else {
        return;
    };

    let ore_density = element_density(content, "ore");
    let effective_m3_per_kg = 1.0 / ore_density;

    let volume_used = inventory_volume_m3(&ship.inventory, content);
    let free_volume = (ship.cargo_capacity_m3 - volume_used).max(0.0);
    let max_kg_by_volume = free_volume / effective_m3_per_kg;
    let extracted_total_kg = asteroid.mass_kg.min(max_kg_by_volume);

    // Snapshot composition at mine-time (known composition if deep-scanned, else true composition).
    let composition = asteroid
        .knowledge
        .composition
        .clone()
        .unwrap_or_else(|| asteroid.true_composition.clone());

    let lot_id = LotId(format!("lot_{:04}", state.counters.next_lot_id));
    state.counters.next_lot_id += 1;

    let asteroid_remaining_kg = asteroid.mass_kg - extracted_total_kg;
    if asteroid_remaining_kg <= 0.0 {
        state.asteroids.remove(asteroid_id);
    } else if let Some(asteroid) = state.asteroids.get_mut(asteroid_id) {
        asteroid.mass_kg = asteroid_remaining_kg;
    }

    let ore_item = InventoryItem::Ore {
        lot_id,
        asteroid_id: asteroid_id.clone(),
        kg: extracted_total_kg,
        composition,
    };

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.inventory.push(ore_item.clone());
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::OreMined {
            ship_id: ship_id.clone(),
            asteroid_id: asteroid_id.clone(),
            ore_lot: ore_item,
            asteroid_remaining_kg: asteroid_remaining_kg.max(0.0),
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Mine".to_string(),
            target: Some(asteroid_id.0.clone()),
        },
    ));
}

pub(crate) fn resolve_deposit(
    state: &mut GameState,
    ship_id: &ShipId,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let items = if let Some(ship) = state.ships.get_mut(ship_id) {
        std::mem::take(&mut ship.inventory)
    } else {
        return;
    };

    if items.is_empty() {
        set_ship_idle(state, ship_id, current_tick);
        return;
    }

    let Some(station) = state.stations.get(station_id) else {
        // Station gone — return items to ship and idle.
        if let Some(ship) = state.ships.get_mut(ship_id) {
            ship.inventory = items;
        }
        set_ship_idle(state, ship_id, current_tick);
        return;
    };

    // Split items into those that fit in the station and those that don't.
    let mut station_volume = inventory_volume_m3(&station.inventory, content);
    let station_capacity = station.cargo_capacity_m3;
    let mut to_deposit = Vec::new();
    let mut to_return = Vec::new();
    for item in items {
        let item_volume = inventory_volume_m3(std::slice::from_ref(&item), content);
        if station_volume + item_volume <= station_capacity {
            station_volume += item_volume;
            to_deposit.push(item);
        } else {
            to_return.push(item);
        }
    }

    // Return overflow items to the ship.
    if !to_return.is_empty() {
        if let Some(ship) = state.ships.get_mut(ship_id) {
            ship.inventory = to_return;
        }
    }

    if to_deposit.is_empty() {
        set_ship_idle(state, ship_id, current_tick);
        return;
    }

    if let Some(station) = state.stations.get_mut(station_id) {
        station.inventory.extend(to_deposit.clone());
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::OreDeposited {
            ship_id: ship_id.clone(),
            station_id: station_id.clone(),
            items: to_deposit,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Deposit".to_string(),
            target: Some(station_id.0.clone()),
        },
    ));
}

pub(crate) fn resolve_deep_scan(
    state: &mut GameState,
    ship_id: &ShipId,
    asteroid_id: &AsteroidId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let sigma = composition_noise_sigma(&state.research, content);

    let Some(true_composition) = state
        .asteroids
        .get(asteroid_id)
        .map(|a| a.true_composition.clone())
    else {
        return; // Asteroid not found — shouldn't happen with valid state.
    };

    // Map composition: true value + uniform noise in [-sigma, sigma], clamped and normalised.
    let mut mapped: CompositionVec = true_composition
        .iter()
        .map(|(element, &true_value)| {
            let noise = if sigma > 0.0 {
                rng.gen_range(-sigma..=sigma)
            } else {
                0.0
            };
            (element.clone(), (true_value + noise).clamp(0.0, 1.0))
        })
        .collect();
    normalise(&mut mapped);

    if let Some(asteroid) = state.asteroids.get_mut(asteroid_id) {
        asteroid.knowledge.composition = Some(mapped.clone());
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::CompositionMapped {
            asteroid_id: asteroid_id.clone(),
            composition: mapped,
        },
    ));

    let amount = content.constants.deep_scan_data_amount;
    let quality = content.constants.deep_scan_data_quality;
    *state
        .research
        .data_pool
        .entry(DataKind::ScanData)
        .or_insert(0.0) += amount * quality;

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::ScanData,
            amount,
            quality,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "DeepScan".to_string(),
            target: Some(asteroid_id.0.clone()),
        },
    ));
}
