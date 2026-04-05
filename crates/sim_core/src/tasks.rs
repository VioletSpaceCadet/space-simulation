use crate::{
    AnomalyTag, AsteroidId, AsteroidKnowledge, AsteroidState, CompositionVec, DataKind, Event,
    EventEnvelope, GameContent, GameState, InventoryItem, LotId, ResearchState, ShipId, ShipState,
    SiteId, StationId, TaskKind, TaskState, TechEffect,
};
use rand::Rng;

/// Resolve a completed ship task — dispatches to the appropriate handler.
pub(crate) fn resolve_task(
    task_kind: &TaskKind,
    state: &mut GameState,
    ship_id: &ShipId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    match task_kind {
        TaskKind::Transit {
            ref destination,
            ref then,
            ..
        } => resolve_transit(state, ship_id, destination, then, content, events),
        TaskKind::Survey { ref site } => {
            resolve_survey(state, ship_id, site, content, rng, events);
        }
        TaskKind::DeepScan { ref asteroid } => {
            resolve_deep_scan(state, ship_id, asteroid, content, rng, events);
        }
        TaskKind::Mine { ref asteroid, .. } => {
            resolve_mine(state, ship_id, asteroid, content, events);
        }
        TaskKind::Deposit { ref station, .. } => {
            resolve_deposit(state, ship_id, station, content, events);
        }
        TaskKind::Pickup {
            ref from_station,
            ref items,
            ref then,
        } => {
            resolve_pickup(state, ship_id, from_station, items, then, content, events);
        }
        TaskKind::ConstructStation {
            ref frame_id,
            ref position,
            ref kit_component_id,
            ..
        } => {
            resolve_construct_station(
                state,
                ship_id,
                frame_id,
                position,
                kit_component_id,
                content,
                events,
            );
        }
        TaskKind::Idle | TaskKind::Refuel { .. } => {}
    }
}

/// Finalize a `ConstructStation` task: create a fresh, empty `StationState`
/// at the build position with the kit's frame, apply frame bonuses via the
/// modifier pipeline, and idle the ship. The kit was already consumed by
/// the `DeployStation` command handler before the Transit started.
pub(crate) fn resolve_construct_station(
    state: &mut GameState,
    ship_id: &ShipId,
    frame_id: &crate::FrameId,
    position: &crate::Position,
    kit_component_id: &str,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Allocate a deterministic station id off the existing deploy counter.
    // This matches the P4 launch-delivered station naming convention
    // (station_{counter}) so save files and scoring treat them uniformly.
    state.counters.stations_deployed += 1;
    let station_id = crate::StationId(format!(
        "station_deployed_{:04}",
        state.counters.stations_deployed
    ));

    // VIO-594: Seed the new station's inventory from the kit def so it
    // has a buffer of raw materials + repair kits to survive until the
    // first module deliveries arrive.
    let seed_inventory = build_seed_inventory(kit_component_id, content);

    let mut station = crate::StationState {
        id: station_id.clone(),
        position: position.clone(),
        core: crate::FacilityCore {
            inventory: seed_inventory,
            cargo_capacity_m3: content
                .frames
                .get(frame_id)
                .map_or(500.0, |f| f.base_cargo_capacity_m3),
            ..Default::default()
        },
        frame_id: Some(frame_id.clone()),
        leaders: Vec::new(),
    };
    crate::recompute_station_stats(&mut station, content);
    state.stations.insert(station_id.clone(), station);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::StationDeployed {
            station_id,
            position: position.clone(),
            ship_id: Some(ship_id.clone()),
            frame_id: Some(frame_id.clone()),
            kit_component_id: Some(kit_component_id.to_string()),
        },
    ));

    set_ship_idle(state, ship_id, current_tick);
}

/// Build the initial inventory for a station deployed from a kit
/// (VIO-594). Reads `ComponentDef.deploys_seed_materials` and
/// `deploys_seed_components`. Empty kits or missing kit defs yield an
/// empty inventory — the station starts bare, matching pre-VIO-594
/// behavior.
fn build_seed_inventory(kit_component_id: &str, content: &GameContent) -> Vec<InventoryItem> {
    let Some(kit_def) = content
        .component_defs
        .iter()
        .find(|c| c.id == kit_component_id)
    else {
        return Vec::new();
    };
    let mut inventory = Vec::new();

    for seed in &kit_def.deploys_seed_materials {
        inventory.push(InventoryItem::Material {
            element: seed.element.clone(),
            kg: seed.kg,
            quality: seed.quality,
            thermal: None,
        });
    }

    for seed in &kit_def.deploys_seed_components {
        inventory.push(InventoryItem::Component {
            component_id: crate::ComponentId(seed.id.clone()),
            count: seed.count,
            quality: seed.quality,
        });
    }
    inventory
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

/// True if any unlocked tech grants the `EnableShipConstruction` effect.
pub(crate) fn ship_construction_enabled(research: &ResearchState, content: &GameContent) -> bool {
    content
        .techs
        .iter()
        .filter(|tech| research.unlocked.contains(&tech.id))
        .flat_map(|tech| &tech.effects)
        .any(|effect| matches!(effect, TechEffect::EnableShipConstruction))
}

/// Returns the `TechId` of the first tech with `EnableShipConstruction` effect, if any.
pub(crate) fn ship_construction_tech_id(content: &GameContent) -> Option<&crate::TechId> {
    content.techs.iter().find_map(|tech| {
        if tech
            .effects
            .iter()
            .any(|e| matches!(e, TechEffect::EnableShipConstruction))
        {
            Some(&tech.id)
        } else {
            None
        }
    })
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
            _ => None,
        })
        .unwrap_or(0.0)
}

/// Returns the density (kg/m³) for the given element id.
///
/// Panics if the element is not found in content — that is a content authoring error.
pub(crate) fn element_density(content: &GameContent, element_id: &str) -> f32 {
    content
        .elements
        .iter()
        .find(|e| e.id == element_id)
        .unwrap_or_else(|| panic!("element '{element_id}' not found in content"))
        .density_kg_per_m3
}

/// Volume (m³) of a single inventory item. Uses `density_map` for O(1) lookups.
pub(crate) fn item_volume_m3(item: &InventoryItem, content: &GameContent) -> f32 {
    match item {
        InventoryItem::Ore { kg, .. } => {
            kg / content
                .density_map
                .get(crate::ELEMENT_ORE)
                .copied()
                .unwrap_or_else(|| element_density(content, crate::ELEMENT_ORE))
        }
        InventoryItem::Slag { kg, .. } => {
            kg / content
                .density_map
                .get(crate::ELEMENT_SLAG)
                .copied()
                .unwrap_or_else(|| element_density(content, crate::ELEMENT_SLAG))
        }
        InventoryItem::Material { element, kg, .. } => {
            kg / content
                .density_map
                .get(element.as_str())
                .copied()
                .unwrap_or_else(|| element_density(content, element))
        }
        InventoryItem::Component { count, .. } => *count as f32 * 1.0,
        InventoryItem::Module { module_def_id, .. } => content
            .module_defs
            .get(module_def_id.as_str())
            .map_or(0.0, |m| m.volume_m3),
    }
}

/// Volume (m³) currently occupied by the inventory items.
pub fn inventory_volume_m3(inventory: &[InventoryItem], content: &GameContent) -> f32 {
    inventory
        .iter()
        .map(|item| item_volume_m3(item, content))
        .sum()
}

/// Total mass (kg) of inventory items (ore, slag, materials).
/// Components and modules are treated as massless for propulsion calculations.
pub fn inventory_mass_kg(inventory: &[InventoryItem]) -> f32 {
    inventory.iter().map(InventoryItem::mass_kg).sum()
}

/// Pre-compute how many ticks a mining run will take.
///
/// Stops when the cargo hold fills OR the asteroid is depleted, whichever comes first.
pub fn mine_duration(asteroid: &AsteroidState, ship: &ShipState, content: &GameContent) -> u64 {
    let ore_density = element_density(content, crate::ELEMENT_ORE);
    let effective_m3_per_kg = 1.0 / ore_density;

    let volume_used = inventory_volume_m3(&ship.inventory, content);
    let free_volume = (ship.cargo_capacity_m3 - volume_used).max(0.0);
    let rate = content.constants.mining_rate_kg_per_tick;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamp guards
    let ticks_to_fill = (free_volume / (rate * effective_m3_per_kg))
        .ceil()
        .clamp(0.0, u64::MAX as f32) as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamp guards
    let ticks_to_deplete = (asteroid.mass_kg / rate).ceil().clamp(0.0, u64::MAX as f32) as u64;

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
    destination: &crate::Position,
    then: &TaskKind,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.position = destination.clone();
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ShipArrived {
            ship_id: ship_id.clone(),
            position: destination.clone(),
        },
    ));

    // Generate transit data from completed flight
    let data_amount = crate::research::generate_data(
        &mut state.research,
        DataKind::new(DataKind::TRANSIT),
        "transit",
        &content.constants,
    );

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::new(DataKind::TRANSIT),
            amount: data_amount,
        },
    ));

    // Start the follow-on task immediately.
    let duration = then.duration(&content.constants);
    let label = then.label().to_string();
    let target = then.target();

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

    // VIO-592: if the follow-on task is a station construction, fire the
    // dedicated StationConstructionStarted event so downstream consumers
    // can surface it in the timeline without parsing TaskStarted labels.
    if let TaskKind::ConstructStation {
        frame_id,
        position,
        assembly_ticks,
        ..
    } = then
    {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::StationConstructionStarted {
                ship_id: ship_id.clone(),
                frame_id: frame_id.clone(),
                position: position.clone(),
                assembly_ticks: *assembly_ticks,
            },
        ));
    }
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
            position: site.position.clone(),
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
            position: site.position.clone(),
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

    let data_amount = crate::research::generate_data(
        &mut state.research,
        DataKind::new(DataKind::SURVEY),
        "survey",
        &content.constants,
    );

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::new(DataKind::SURVEY),
            amount: data_amount,
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

    let ore_density = element_density(content, crate::ELEMENT_ORE);
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

    crate::research::generate_data(
        &mut state.research,
        DataKind::new(DataKind::ASSAY),
        "mine",
        &content.constants,
    );

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

#[allow(clippy::too_many_lines)]
pub(crate) fn resolve_deposit(
    state: &mut GameState,
    ship_id: &ShipId,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Check if the task is already in the blocked state before we take inventory.
    let was_blocked = state
        .ships
        .get(ship_id)
        .and_then(|s| s.task.as_ref())
        .is_some_and(|t| matches!(&t.kind, TaskKind::Deposit { blocked: true, .. }));

    let items = if let Some(ship) = state.ships.get_mut(ship_id) {
        std::mem::take(&mut ship.inventory)
    } else {
        return;
    };

    if items.is_empty() {
        set_ship_idle(state, ship_id, current_tick);
        return;
    }

    // Warm the station-level volume cache.
    if let Some(station) = state.stations.get_mut(station_id) {
        let _ = station.used_volume_m3(content);
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
    // SAFETY: cache warmed via used_volume_m3() above; no intervening invalidation.
    let mut station_volume = station
        .core
        .cached_inventory_volume_m3
        .unwrap_or_else(|| inventory_volume_m3(&station.core.inventory, content));
    let station_capacity = station.core.cargo_capacity_m3;
    let mut to_deposit = Vec::new();
    let mut to_return = Vec::new();
    for item in items {
        let item_volume = item_volume_m3(&item, content);
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
        // Nothing fits — keep ship in Deposit task, bump eta to retry next tick.
        if let Some(ship) = state.ships.get_mut(ship_id) {
            if let Some(task) = &mut ship.task {
                if let TaskKind::Deposit { blocked, .. } = &mut task.kind {
                    *blocked = true;
                }
                task.eta_tick = current_tick + 1;
            }
        }

        if !was_blocked {
            // Compute shortfall for the event: volume of items on ship minus free space.
            let ship_volume = state
                .ships
                .get(ship_id)
                .map_or(0.0, |s| inventory_volume_m3(&s.inventory, content));
            let station_vol = state
                .stations
                .get_mut(station_id)
                .map_or(0.0, |s| s.used_volume_m3(content));
            let free_space = (station_capacity - station_vol).max(0.0);
            let shortfall = (ship_volume - free_space).max(0.0);

            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::DepositBlocked {
                    ship_id: ship_id.clone(),
                    station_id: station_id.clone(),
                    shortfall_m3: shortfall,
                },
            ));
        }
        return;
    }

    if let Some(station) = state.stations.get_mut(station_id) {
        station.core.inventory.extend(to_deposit.clone());
        station.invalidate_volume_cache();
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

    if was_blocked {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::DepositUnblocked {
                ship_id: ship_id.clone(),
                station_id: station_id.clone(),
            },
        ));
    }

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

/// VIO-595: load inventory items from a station into the ship, then
/// start the chained `then` task (typically `Transit { then: Deposit }`
/// to complete an inter-station transfer). Best-effort: picks up what
/// the station has up to the ship's remaining cargo capacity. Items
/// that are not available or do not fit are silently dropped from the
/// request — the ship proceeds with whatever it managed to load.
pub(crate) fn resolve_pickup(
    state: &mut GameState,
    ship_id: &ShipId,
    from_station: &StationId,
    items: &[crate::TradeItemSpec],
    then: &TaskKind,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Compute remaining ship capacity before any pickups.
    let mut remaining_capacity = {
        let Some(ship) = state.ships.get(ship_id) else {
            return;
        };
        let used = inventory_volume_m3(&ship.inventory, content);
        (ship.cargo_capacity_m3 - used).max(0.0)
    };

    // Pull matching items out of the station inventory, one TradeItemSpec
    // at a time. Track what we successfully transferred for the event.
    let mut picked_up: Vec<InventoryItem> = Vec::new();

    for spec in items {
        let Some(station) = state.stations.get_mut(from_station) else {
            break; // Station disappeared — abort further pickups.
        };
        let taken = take_items_for_spec(station, spec, remaining_capacity, content);
        for item in taken {
            remaining_capacity = (remaining_capacity - item_volume_m3(&item, content)).max(0.0);
            picked_up.push(item);
        }
    }

    // Move the loaded items into the ship inventory.
    if !picked_up.is_empty() {
        if let Some(station) = state.stations.get_mut(from_station) {
            station.invalidate_volume_cache();
        }
        if let Some(ship) = state.ships.get_mut(ship_id) {
            ship.inventory.extend(picked_up.clone());
        }
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ItemsPickedUp {
            ship_id: ship_id.clone(),
            station_id: from_station.clone(),
            items: picked_up,
        },
    ));

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Pickup".to_string(),
            target: Some(from_station.0.clone()),
        },
    ));

    // Chain into the follow-on task (mirrors resolve_transit's handoff).
    let duration = then.duration(&content.constants);
    let label = then.label().to_string();
    let target = then.target();
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

/// Remove items from `station.core.inventory` that match `spec`, up to
/// the available remaining ship capacity. Returns the extracted items.
/// Best-effort on both sides: partial materials are split, partial
/// component stacks are split, missing items are skipped. Volume check
/// is strict — items whose `item_volume_m3` exceeds remaining capacity
/// are not partially loaded (components/modules are atomic).
fn take_items_for_spec(
    station: &mut crate::StationState,
    spec: &crate::TradeItemSpec,
    remaining_capacity: f32,
    content: &GameContent,
) -> Vec<InventoryItem> {
    match spec {
        crate::TradeItemSpec::Material {
            element,
            kg: requested_kg,
        } => take_material(
            &mut station.core.inventory,
            element,
            *requested_kg,
            remaining_capacity,
            content,
        ),
        crate::TradeItemSpec::Component {
            component_id,
            count,
        } => take_components(
            &mut station.core.inventory,
            component_id,
            *count,
            remaining_capacity,
            content,
        ),
        crate::TradeItemSpec::Module { module_def_id } => take_module(
            &mut station.core.inventory,
            module_def_id,
            remaining_capacity,
            content,
        ),
        crate::TradeItemSpec::Crew { .. } => Vec::new(), // Crew transfer not supported.
    }
}

fn take_material(
    inventory: &mut Vec<InventoryItem>,
    element: &str,
    requested_kg: f32,
    remaining_capacity: f32,
    content: &GameContent,
) -> Vec<InventoryItem> {
    // Material volume is density-based: volume = kg / density.
    let density = element_density(content, element);
    if density <= 0.0 {
        return Vec::new();
    }
    let max_kg_by_capacity = (remaining_capacity * density).max(0.0);
    let mut to_take = requested_kg.min(max_kg_by_capacity);
    if to_take <= 0.0 {
        return Vec::new();
    }

    let mut taken: Vec<InventoryItem> = Vec::new();
    // Iterate in place; split materials as needed.
    let mut index = 0;
    while index < inventory.len() && to_take > 0.0 {
        let matches_element = matches!(
            &inventory[index],
            InventoryItem::Material { element: e, .. } if e == element
        );
        if !matches_element {
            index += 1;
            continue;
        }
        let InventoryItem::Material {
            element: e,
            kg,
            quality,
            thermal,
        } = &mut inventory[index]
        else {
            unreachable!();
        };
        if *kg <= to_take {
            to_take -= *kg;
            taken.push(inventory.remove(index));
        } else {
            // Split: extract `to_take` kg into a new InventoryItem.
            *kg -= to_take;
            taken.push(InventoryItem::Material {
                element: e.clone(),
                kg: to_take,
                quality: *quality,
                thermal: thermal.clone(),
            });
            to_take = 0.0;
        }
    }
    taken
}

fn take_components(
    inventory: &mut Vec<InventoryItem>,
    component_id: &crate::ComponentId,
    requested_count: u32,
    remaining_capacity: f32,
    _content: &GameContent,
) -> Vec<InventoryItem> {
    // Per-unit volume matches `item_volume_m3`: hardcoded 1.0 m^3 per
    // component (engine-wide convention for components in cargo holds).
    // Using `ComponentDef.volume_m3` here would create an asymmetry with
    // the rest of the cargo system and allow overloading by a factor of
    // 1/def.volume_m3. When item_volume_m3 is fixed to honor def values,
    // this helper should follow suit (single source of truth).
    const PER_UNIT_VOLUME_M3: f32 = 1.0;
    // Safe: non-negative (remaining_capacity >= 0) and clamped below
    // u16::MAX well below u32::MAX before cast.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_count_by_capacity = (remaining_capacity / PER_UNIT_VOLUME_M3)
        .floor()
        .clamp(0.0, f32::from(u16::MAX)) as u32;
    let mut to_take = requested_count.min(max_count_by_capacity);
    if to_take == 0 {
        return Vec::new();
    }

    let mut taken: Vec<InventoryItem> = Vec::new();
    let mut index = 0;
    while index < inventory.len() && to_take > 0 {
        let matches_id = matches!(
            &inventory[index],
            InventoryItem::Component { component_id: cid, .. } if cid == component_id
        );
        if !matches_id {
            index += 1;
            continue;
        }
        let InventoryItem::Component {
            component_id: cid,
            count,
            quality,
        } = &mut inventory[index]
        else {
            unreachable!();
        };
        if *count <= to_take {
            to_take -= *count;
            taken.push(inventory.remove(index));
        } else {
            *count -= to_take;
            taken.push(InventoryItem::Component {
                component_id: cid.clone(),
                count: to_take,
                quality: *quality,
            });
            to_take = 0;
        }
    }
    taken
}

fn take_module(
    inventory: &mut Vec<InventoryItem>,
    module_def_id: &str,
    remaining_capacity: f32,
    content: &GameContent,
) -> Vec<InventoryItem> {
    // Find the module def to look up its volume (module_defs is keyed by id).
    let Some(def) = content.module_defs.get(module_def_id) else {
        return Vec::new();
    };
    if def.volume_m3 > remaining_capacity {
        return Vec::new();
    }
    // Take the first matching module item.
    let position = inventory.iter().position(|item| {
        matches!(item, InventoryItem::Module { module_def_id: mdid, .. } if mdid == module_def_id)
    });
    match position {
        Some(pos) => vec![inventory.remove(pos)],
        None => Vec::new(),
    }
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

    let data_amount = crate::research::generate_data(
        &mut state.research,
        DataKind::new(DataKind::SURVEY),
        "deep_scan",
        &content.constants,
    );

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::new(DataKind::SURVEY),
            amount: data_amount,
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

// ---------------------------------------------------------------------------
// Refuel resolution (ongoing task — runs every tick)
// ---------------------------------------------------------------------------

/// Resolve all ongoing refuel tasks. Runs every tick before scheduled tasks.
/// Uses pro-rata allocation when multiple ships refuel at the same station.
pub(crate) fn resolve_refuels(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    use std::collections::BTreeMap;

    // Group refueling ships by station
    let mut station_groups: BTreeMap<StationId, Vec<ShipId>> = BTreeMap::new();
    for ship in state.ships.values() {
        if let Some(TaskState {
            kind: TaskKind::Refuel { ref station_id, .. },
            ..
        }) = &ship.task
        {
            station_groups
                .entry(station_id.clone())
                .or_default()
                .push(ship.id.clone());
        }
    }

    for (station_id, ship_ids) in &station_groups {
        resolve_station_refuels(state, content, events, station_id, ship_ids);
    }
}

/// Get total LH2 (kg) in a station's inventory.
fn station_lh2_kg(state: &GameState, station_id: &StationId) -> f32 {
    state.stations.get(station_id).map_or(0.0, |s| {
        s.core
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
                _ => None,
            })
            .sum()
    })
}

/// Process refueling for all ships at a single station.
fn resolve_station_refuels(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
    station_id: &StationId,
    ship_ids: &[ShipId],
) {
    let current_tick = state.meta.tick;
    let rate = content.constants.refuel_kg_per_tick;
    let available_lh2 = station_lh2_kg(state, station_id);

    // Each ship requests min(rate, remaining_need)
    let requests: Vec<(ShipId, f32)> = ship_ids
        .iter()
        .filter_map(|sid| {
            let ship = state.ships.get(sid)?;
            let target = match &ship.task {
                Some(TaskState {
                    kind: TaskKind::Refuel { target_kg, .. },
                    ..
                }) => *target_kg,
                _ => return None,
            };
            let need = (target - ship.propellant_kg).max(0.0);
            Some((sid.clone(), need.min(rate)))
        })
        .collect();

    let total_requested: f32 = requests.iter().map(|(_, r)| r).sum();

    if total_requested <= 0.0 {
        complete_all(state, events, current_tick, station_id, ship_ids);
        return;
    }

    if available_lh2 <= content.constants.min_meaningful_kg {
        abort_all(state, events, current_tick, station_id, ship_ids);
        return;
    }

    // Pro-rata allocation and LH2 transfer
    let total_consumed = allocate_and_transfer(
        state,
        events,
        current_tick,
        station_id,
        &requests,
        total_requested,
        available_lh2,
    );

    // Deduct LH2 from station inventory
    if total_consumed > 0.0 {
        deduct_station_lh2(state, content, station_id, total_consumed);
    }
}

fn complete_all(
    state: &mut GameState,
    events: &mut Vec<EventEnvelope>,
    tick: u64,
    station_id: &StationId,
    ship_ids: &[ShipId],
) {
    for ship_id in ship_ids {
        if let Some(ship) = state.ships.get_mut(ship_id) {
            ship.task = None;
        }
        events.push(crate::emit(
            &mut state.counters,
            tick,
            Event::RefuelComplete {
                ship_id: ship_id.clone(),
                station_id: station_id.clone(),
                kg_transferred: 0.0,
            },
        ));
    }
}

fn abort_all(
    state: &mut GameState,
    events: &mut Vec<EventEnvelope>,
    tick: u64,
    station_id: &StationId,
    ship_ids: &[ShipId],
) {
    for ship_id in ship_ids {
        if let Some(ship) = state.ships.get_mut(ship_id) {
            ship.task = None;
        }
        events.push(crate::emit(
            &mut state.counters,
            tick,
            Event::RefuelAborted {
                ship_id: ship_id.clone(),
                station_id: station_id.clone(),
                reason: "station_empty".to_string(),
            },
        ));
    }
}

fn allocate_and_transfer(
    state: &mut GameState,
    events: &mut Vec<EventEnvelope>,
    tick: u64,
    station_id: &StationId,
    requests: &[(ShipId, f32)],
    total_requested: f32,
    available_lh2: f32,
) -> f32 {
    let mut total_consumed = 0.0_f32;
    for (ship_id, requested) in requests {
        let allocated = if total_requested <= available_lh2 {
            *requested
        } else {
            requested / total_requested * available_lh2
        };
        if allocated <= 0.0 {
            continue;
        }

        let Some(ship) = state.ships.get_mut(ship_id) else {
            continue;
        };
        ship.propellant_kg += allocated;
        total_consumed += allocated;

        let target = match &ship.task {
            Some(TaskState {
                kind: TaskKind::Refuel { target_kg, .. },
                ..
            }) => *target_kg,
            _ => continue,
        };
        if ship.propellant_kg >= target {
            ship.propellant_kg = ship.propellant_kg.min(ship.propellant_capacity_kg);
            ship.task = None;
            events.push(crate::emit(
                &mut state.counters,
                tick,
                Event::RefuelComplete {
                    ship_id: ship_id.clone(),
                    station_id: station_id.clone(),
                    kg_transferred: allocated,
                },
            ));
        }
    }
    total_consumed
}

fn deduct_station_lh2(
    state: &mut GameState,
    content: &GameContent,
    station_id: &StationId,
    amount: f32,
) {
    if let Some(station) = state.stations.get_mut(station_id) {
        let mut remaining = amount;
        for item in &mut station.core.inventory {
            if remaining <= 0.0 {
                break;
            }
            if let InventoryItem::Material { element, kg, .. } = item {
                if element == "LH2" {
                    let deduct = remaining.min(*kg);
                    *kg -= deduct;
                    remaining -= deduct;
                }
            }
        }
        station
            .core
            .inventory
            .retain(|item| item.mass_kg() > content.constants.min_meaningful_kg);
    }
}
