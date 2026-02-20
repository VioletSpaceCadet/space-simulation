use crate::{
    AnomalyTag, AsteroidId, AsteroidKnowledge, AsteroidState, CompositionVec, Constants, DataKind,
    Event, EventEnvelope, GameContent, GameState, NodeId, ResearchState, ShipId, SiteId, TaskKind,
    TaskState, TechEffect,
};
use rand::Rng;

pub(crate) fn task_duration(kind: &TaskKind, constants: &Constants) -> u64 {
    match kind {
        TaskKind::Transit { total_ticks, .. } => *total_ticks,
        TaskKind::Survey { .. } => constants.survey_scan_ticks,
        TaskKind::DeepScan { .. } => constants.deep_scan_ticks,
        TaskKind::Idle => 0,
    }
}

pub(crate) fn task_kind_label(kind: &TaskKind) -> &'static str {
    match kind {
        TaskKind::Idle => "Idle",
        TaskKind::Transit { .. } => "Transit",
        TaskKind::Survey { .. } => "Survey",
        TaskKind::DeepScan { .. } => "DeepScan",
    }
}

pub(crate) fn task_target(kind: &TaskKind) -> Option<String> {
    match kind {
        TaskKind::Idle => None,
        TaskKind::Transit { destination, .. } => Some(destination.0.clone()),
        TaskKind::Survey { site } => Some(site.0.clone()),
        TaskKind::DeepScan { asteroid } => Some(asteroid.0.clone()),
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
