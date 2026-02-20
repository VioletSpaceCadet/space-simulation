use crate::{
    AnomalyTag, AsteroidId, AsteroidKnowledge, AsteroidState, CompositionVec, Constants, DataKind,
    ElementId, Event, EventEnvelope, GameContent, GameState, NodeId, ResearchState, ShipId,
    ShipState, SiteId, StationId, TaskKind, TaskState, TechEffect,
};
use rand::Rng;
use std::collections::HashMap;

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

/// Volume (m³) currently occupied by the ship's cargo.
/// Unknown elements default to 1.0 kg/m³ (safe fallback).
pub fn cargo_volume_used<S: std::hash::BuildHasher>(
    cargo: &HashMap<ElementId, f32, S>,
    content: &GameContent,
) -> f32 {
    cargo
        .iter()
        .map(|(element_id, &mass_kg)| {
            // Ore is keyed as "ore:{asteroid_id}"; all ore variants share the "ore" density.
            let lookup_id: &str = if element_id.starts_with("ore:") {
                "ore"
            } else {
                element_id
            };
            let density = content
                .elements
                .iter()
                .find(|e| e.id == lookup_id)
                .map_or(1.0, |e| e.density_kg_per_m3);
            mass_kg / density
        })
        .sum()
}

/// Pre-compute how many ticks a mining run will take.
///
/// Stops when the cargo hold fills OR the asteroid is depleted, whichever comes first.
pub fn mine_duration(asteroid: &AsteroidState, ship: &ShipState, content: &GameContent) -> u64 {
    // Mining extracts raw ore (unrefined bulk rock). Use the ore element's density
    // for volume calculations; fall back to 3000 kg/m³ if ore is not in the element list.
    let ore_density = content
        .elements
        .iter()
        .find(|e| e.id == "ore")
        .map_or(3000.0, |e| e.density_kg_per_m3);
    let effective_m3_per_kg = 1.0 / ore_density;

    let volume_used = cargo_volume_used(&ship.cargo, content);
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
        return; // Asteroid already gone — nothing to do.
    };

    let Some(ship) = state.ships.get(ship_id) else {
        return;
    };

    // How much volume is free in the hold?
    let volume_used = cargo_volume_used(&ship.cargo, content);
    let free_volume = (ship.cargo_capacity_m3 - volume_used).max(0.0);

    // Mining extracts raw ore. Use ore density for volume calculation.
    let ore_density = content
        .elements
        .iter()
        .find(|e| e.id == "ore")
        .map_or(3000.0, |e| e.density_kg_per_m3);
    let effective_m3_per_kg = 1.0 / ore_density;

    // Total kg we can extract: limited by asteroid mass and cargo space.
    let max_kg_by_volume = free_volume / effective_m3_per_kg;
    let extracted_total_kg = asteroid.mass_kg.min(max_kg_by_volume);

    // Extract as raw ore keyed by source asteroid — different asteroids produce
    // distinct ore lots that must be tracked and refined separately.
    let ore_key = format!("ore:{}", asteroid_id.0);
    let extracted: HashMap<ElementId, f32> = HashMap::from([(ore_key, extracted_total_kg)]);

    // Update asteroid mass; remove if depleted.
    let asteroid_remaining_kg = asteroid.mass_kg - extracted_total_kg;
    if asteroid_remaining_kg <= 0.0 {
        state.asteroids.remove(asteroid_id);
    } else if let Some(asteroid) = state.asteroids.get_mut(asteroid_id) {
        asteroid.mass_kg = asteroid_remaining_kg;
    }

    // Add extracted ore to ship cargo.
    if let Some(ship) = state.ships.get_mut(ship_id) {
        for (element_id, kg) in &extracted {
            *ship.cargo.entry(element_id.clone()).or_insert(0.0) += kg;
        }
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::OreMined {
            ship_id: ship_id.clone(),
            asteroid_id: asteroid_id.clone(),
            extracted: extracted.clone(),
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
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(ship) = state.ships.get(ship_id) else {
        return;
    };
    let cargo = ship.cargo.clone();

    if cargo.is_empty() {
        set_ship_idle(state, ship_id, current_tick);
        return;
    }

    let Some(station) = state.stations.get_mut(station_id) else {
        set_ship_idle(state, ship_id, current_tick);
        return;
    };

    // Transfer all ship cargo to the station.
    for (element_id, kg) in &cargo {
        *station.cargo.entry(element_id.clone()).or_insert(0.0) += kg;
    }

    // Clear ship cargo.
    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.cargo.clear();
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::OreDeposited {
            ship_id: ship_id.clone(),
            station_id: station_id.clone(),
            deposited: cargo,
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
