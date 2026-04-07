//! Run scoring — types, configuration, and computation.
//!
//! `ScoringConfig` is loaded from `content/scoring.json` as part of `GameContent`.
//! `compute_run_score()` is a pure function producing a `RunScore` from game state.

use crate::{GameContent, GameState, MetricsSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Content configuration (loaded from scoring.json)
// ---------------------------------------------------------------------------

/// A single scoring dimension definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionDef {
    /// Unique identifier (e.g., `"industrial_output"`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Weight in composite score (all weights must sum to 1.0).
    pub weight: f64,
    /// Normalization ceiling — the raw value at which the dimension scores 1.0.
    pub ceiling: f64,
}

/// A named score threshold (e.g., "Enterprise" at 500 points).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdDef {
    /// Display name.
    pub name: String,
    /// Minimum composite score to enter this threshold.
    pub min_score: f64,
}

/// Scoring configuration loaded from `content/scoring.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// The 6 scoring dimensions with weights and normalization ceilings.
    pub dimensions: Vec<DimensionDef>,
    /// Named score thresholds, ordered ascending by `min_score`.
    pub thresholds: Vec<ThresholdDef>,
    /// How often to recompute the score (in ticks). Default: 24.
    #[serde(default = "default_computation_interval")]
    pub computation_interval_ticks: u64,
    /// Multiplier applied to the weighted sum to produce the composite score.
    #[serde(default = "default_scale_factor")]
    pub scale_factor: f64,
}

fn default_computation_interval() -> u64 {
    24
}

fn default_scale_factor() -> f64 {
    2500.0
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            dimensions: Vec::new(),
            thresholds: Vec::new(),
            computation_interval_ticks: default_computation_interval(),
            scale_factor: default_scale_factor(),
        }
    }
}

/// Validate a scoring config. Returns an error message if invalid.
pub fn validate_scoring_config(config: &ScoringConfig) -> Result<(), String> {
    if config.dimensions.is_empty() {
        return Err("scoring config must have at least one dimension".into());
    }

    // Check for duplicate dimension IDs
    let mut seen_ids = std::collections::HashSet::new();
    for dim in &config.dimensions {
        if !seen_ids.insert(&dim.id) {
            return Err(format!("duplicate dimension id: '{}'", dim.id));
        }
    }

    for dim in &config.dimensions {
        if dim.weight <= 0.0 {
            return Err(format!(
                "dimension '{}' has non-positive weight {}",
                dim.id, dim.weight
            ));
        }
        if dim.ceiling <= 0.0 {
            return Err(format!(
                "dimension '{}' has non-positive ceiling {}",
                dim.id, dim.ceiling
            ));
        }
    }

    let weight_sum: f64 = config.dimensions.iter().map(|d| d.weight).sum();
    if (weight_sum - 1.0).abs() > 1e-6 {
        return Err(format!(
            "dimension weights must sum to 1.0, got {weight_sum:.6}"
        ));
    }

    // Thresholds must be ascending by min_score
    for window in config.thresholds.windows(2) {
        if window[1].min_score <= window[0].min_score {
            return Err(format!(
                "thresholds must be ascending: '{}' ({}) should be > '{}' ({})",
                window[1].name, window[1].min_score, window[0].name, window[0].min_score,
            ));
        }
    }

    if config.computation_interval_ticks == 0 {
        return Err("computation_interval_ticks must be > 0".into());
    }

    if config.scale_factor <= 0.0 {
        return Err(format!(
            "scale_factor must be positive, got {}",
            config.scale_factor
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Score computation
// ---------------------------------------------------------------------------

/// Compute a run score from the current metrics snapshot and game state.
///
/// Pure function — no state mutation, no IO. Deterministic for the same inputs.
/// Each dimension produces a raw value, which is normalized to [0.0, 1.0] by
/// dividing by the dimension's ceiling (clamped). The composite score is the
/// weighted sum scaled by `scale_factor`.
pub fn compute_run_score(
    metrics: &MetricsSnapshot,
    state: &GameState,
    content: &GameContent,
) -> RunScore {
    let config = &content.scoring;
    if config.dimensions.is_empty() {
        return RunScore::default();
    }

    let tick = metrics.tick.max(1) as f64; // avoid division by zero

    let mut dimensions = BTreeMap::new();
    let mut composite = 0.0;

    for dim in &config.dimensions {
        let raw_value = compute_dimension_raw(&dim.id, metrics, state, content, tick);
        let normalized = (raw_value / dim.ceiling).clamp(0.0, 1.0);
        let weighted = normalized * dim.weight * config.scale_factor;
        composite += weighted;

        dimensions.insert(
            dim.id.clone(),
            DimensionScore {
                id: dim.id.clone(),
                name: dim.name.clone(),
                raw_value,
                normalized,
                weighted,
            },
        );
    }

    let threshold = resolve_threshold(&config.thresholds, composite);

    RunScore {
        dimensions,
        composite,
        threshold,
        tick: metrics.tick,
    }
}

/// Compute the raw value for a single dimension by its ID.
fn compute_dimension_raw(
    dimension_id: &str,
    metrics: &MetricsSnapshot,
    state: &GameState,
    content: &GameContent,
    tick: f64,
) -> f64 {
    match dimension_id {
        "industrial_output" => compute_industrial(metrics, state, tick),
        "research_progress" => compute_research(metrics, state, content),
        "economic_health" => compute_economic(metrics, state, tick),
        "fleet_operations" => compute_fleet(metrics, state),
        "efficiency" => compute_efficiency(metrics),
        "expansion" => compute_expansion(metrics, state),
        _ => 0.0, // unknown dimension — content-defined, scores zero
    }
}

/// Industrial Output: material throughput per tick + assembler activity.
///
/// VIO-603: adds a multi-base diversification multiplier — running N
/// productive bases amplifies throughput credit, rewarding distributed
/// production over concentrating everything on one base. A "productive
/// base" is a station or ground facility with at least one Assembler
/// or Processor module (module `enabled` flag is ignored — structural
/// diversification is what we want to reward, not current uptime).
///
/// The multiplier is proportional (not additive) so the bonus scales
/// with actual throughput rather than being lost against the ceiling.
/// Plateaus at 3 extra productive bases (+30% max) to avoid rewarding
/// sprawl beyond what players can realistically manage.
fn compute_industrial(metrics: &MetricsSnapshot, state: &GameState, tick: f64) -> f64 {
    let throughput = f64::from(metrics.total_material_kg) / tick;
    let assembler_active = metrics
        .per_module_metrics
        .get("assembler")
        .map_or(0, |m| m.active);
    let productive_bases = count_productive_bases(state) as f64;
    // +10% throughput per extra productive base, capped at +30% (4 bases).
    let extra_bases = (productive_bases - 1.0).clamp(0.0, 3.0);
    let diversification_multiplier = 1.0 + extra_bases * 0.10;
    throughput * diversification_multiplier + f64::from(assembler_active) * 0.1
}

/// Count productive bases: stations + ground facilities that host at
/// least one Assembler or Processor module. Treats stations and ground
/// facilities symmetrically so ground-start runs have access to the
/// same diversification rewards as orbital-start runs.
fn count_productive_bases(state: &GameState) -> usize {
    let station_count = state
        .stations
        .values()
        .filter(|s| has_productive_module(&s.core.modules))
        .count();
    let ground_count = state
        .ground_facilities
        .values()
        .filter(|g| has_productive_module(&g.core.modules))
        .count();
    station_count + ground_count
}

fn has_productive_module(modules: &[crate::ModuleState]) -> bool {
    modules.iter().any(|m| {
        matches!(
            m.kind_state,
            crate::ModuleKindState::Assembler(_) | crate::ModuleKindState::Processor(_)
        )
    })
}

/// Research Progress: fraction of techs unlocked + scan data growth + science satellites.
fn compute_research(metrics: &MetricsSnapshot, state: &GameState, content: &GameContent) -> f64 {
    let total_techs = content.techs.len().max(1) as f64;
    let tech_fraction = f64::from(metrics.techs_unlocked) / total_techs;
    // Use all raw data kinds (SURVEY, OpticalData, RadioData, etc.) — not just SURVEY.
    // Collect and sort for deterministic floating-point sum across AHashMap iterations.
    let mut data_values: Vec<f64> = state
        .research
        .data_pool
        .values()
        .map(|v| f64::from(*v))
        .collect();
    data_values.sort_by(f64::total_cmp);
    let total_raw_data: f64 = data_values.iter().sum();
    let data_signal = (total_raw_data / 1000.0).min(1.0);
    // Science satellites contribute to research capability.
    let science_count = state
        .satellites
        .values()
        .filter(|s| s.enabled && s.satellite_type == "science_platform")
        .count() as f64;
    let science_signal = (science_count / 3.0).min(1.0);
    // Blend: 60% tech unlocks, 25% data accumulation, 15% science infrastructure
    tech_fraction * 0.6 + data_signal * 0.25 + science_signal * 0.15
}

/// Economic Health: balance trend + export/grant revenue per tick.
fn compute_economic(metrics: &MetricsSnapshot, state: &GameState, tick: f64) -> f64 {
    // Fixed reference balance for normalization. Ground starts ($50M) will have a
    // lower ratio than orbital starts ($1B), but grant income compensates.
    let reference_balance = 1_000_000_000.0_f64;
    let balance_ratio = (state.balance / reference_balance).clamp(0.0, 2.0) / 2.0;
    let revenue_rate = metrics.export_revenue_total / tick;
    let revenue_signal = (revenue_rate / 10_000.0).min(1.0);
    // Grant income signals economic health for ground-start (no exports yet)
    let grant_total: f64 = state
        .progression
        .grant_history
        .iter()
        .map(|g| g.amount)
        .sum();
    let grant_rate = grant_total / tick;
    let grant_signal = (grant_rate / 50_000.0).min(1.0);
    // Blend: 30% balance, 35% export revenue, 35% grant income
    balance_ratio * 0.3 + revenue_signal * 0.35 + grant_signal * 0.35
}

/// Fleet Operations: utilization + fleet size + satellite deployments + launches.
fn compute_fleet(metrics: &MetricsSnapshot, state: &GameState) -> f64 {
    let has_fleet = metrics.fleet_total > 0;
    let utilization = if has_fleet {
        let total = f64::from(metrics.fleet_total);
        let active = total - f64::from(metrics.fleet_idle);
        active / total
    } else {
        0.0
    };
    // ships_built counter: total ships minus any that existed at start (max 1 starting ship)
    let ships_constructed = (state.ships.len() as f64 - 1.0).max(0.0);
    let construction_signal = (ships_constructed / 5.0).min(1.0);
    // Satellite deployments count as completed missions (sqrt diminishing returns).
    let satellite_signal = (f64::from(metrics.satellites_active).sqrt() / 3.0).min(1.0);
    // Ground launches count as operational activity.
    let total_launches = state.counters.stations_deployed as f64
        + state
            .ground_facilities
            .values()
            .flat_map(|f| f.core.modules.iter())
            .filter_map(|m| match &m.kind_state {
                crate::ModuleKindState::LaunchPad(pad) => Some(pad.launches_count as f64),
                _ => None,
            })
            .sum::<f64>();
    let launch_signal = (total_launches.sqrt() / 3.0).min(1.0);
    // Blend: 35% utilization, 25% fleet growth, 20% satellites, 20% launches
    utilization * 0.35 + construction_signal * 0.25 + satellite_signal * 0.2 + launch_signal * 0.2
}

/// Efficiency: inverted wear + power utilization + storage balance + satellite utilization.
fn compute_efficiency(metrics: &MetricsSnapshot) -> f64 {
    let wear_score = 1.0 - f64::from(metrics.avg_module_wear);
    let power_util = if metrics.power_generated_kw > 0.0 {
        (f64::from(metrics.power_consumed_kw) / f64::from(metrics.power_generated_kw)).min(1.0)
    } else {
        0.0
    };
    let storage_pct = f64::from(metrics.station_storage_used_pct);
    // Penalize extremes: empty (<10%) or overflowing (>95%)
    let storage_score = if storage_pct < 0.1 {
        storage_pct / 0.1
    } else if storage_pct > 0.95 {
        (1.0 - storage_pct) / 0.05
    } else {
        1.0
    };
    // Satellite utilization: active / total (1.0 if no satellites yet).
    let total_sats = f64::from(metrics.satellites_active + metrics.satellites_failed);
    let sat_util = if total_sats > 0.0 {
        f64::from(metrics.satellites_active) / total_sats
    } else {
        1.0
    };
    // Blend: 25% each of four efficiency signals
    (wear_score + power_util + storage_score + sat_util) / 4.0
}

/// Expansion: bases (stations + ground facilities) + fleet + satellites.
///
/// VIO-603: adds a multi-base bonus *on top of* the original blend so
/// single-base play is NOT regressed. Bases include both stations and
/// ground facilities, keeping orbital- and ground-start runs symmetric.
/// Plateaus at 4 total bases. Raw values can temporarily exceed 1.0;
/// the outer normalizer clamps to the unit range (still within ceiling).
fn compute_expansion(metrics: &MetricsSnapshot, state: &GameState) -> f64 {
    // Both station and ground facility count as operational bases.
    let base_count = (state.stations.len() + state.ground_facilities.len()) as f64;
    let base_signal = (base_count / 3.0).min(1.0);
    let fleet_signal = (f64::from(metrics.fleet_total).sqrt() / 3.0).min(1.0);
    // Satellites contribute to expansion (orbital infrastructure).
    let satellite_signal = (f64::from(metrics.satellites_active).sqrt() / 3.0).min(1.0);
    // Multi-base bonus: additive on top of the original blend so
    // single-base runs are unchanged. Plateaus at 4 total bases
    // (3 extras beyond the starting one).
    let extra_bases = (base_count - 1.0).clamp(0.0, 3.0);
    let multi_base_bonus = extra_bases / 3.0 * 0.15;
    // Original 40/30/30 blend preserved. Bonus stacks on top; ceiling
    // normalization clamps the final normalized value to [0, 1].
    base_signal * 0.4 + fleet_signal * 0.3 + satellite_signal * 0.3 + multi_base_bonus
}

/// Resolve the highest threshold name for a given composite score.
fn resolve_threshold(thresholds: &[ThresholdDef], composite: f64) -> String {
    thresholds
        .iter()
        .rev()
        .find(|t| composite >= t.min_score)
        .map_or_else(String::new, |t| t.name.clone())
}

// ---------------------------------------------------------------------------
// Score output
// ---------------------------------------------------------------------------

/// Score for a single dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionScore {
    /// Dimension id (matches `DimensionDef::id`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Raw computed value before normalization.
    pub raw_value: f64,
    /// Normalized score in [0.0, 1.0].
    pub normalized: f64,
    /// Weighted contribution to composite (normalized * weight * `scale_factor`).
    pub weighted: f64,
}

/// Complete run score computed by `compute_run_score()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunScore {
    /// Per-dimension breakdown, keyed by dimension id.
    pub dimensions: BTreeMap<String, DimensionScore>,
    /// Composite score (sum of all weighted contributions).
    pub composite: f64,
    /// Named threshold. The highest threshold whose
    /// `min_score` is <= `composite`.
    pub threshold: String,
    /// The tick at which this score was computed.
    pub tick: u64,
}

impl Default for RunScore {
    fn default() -> Self {
        Self {
            dimensions: BTreeMap::new(),
            composite: 0.0,
            threshold: String::new(),
            tick: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ScoringConfig {
        ScoringConfig {
            dimensions: vec![
                DimensionDef {
                    id: "industrial_output".into(),
                    name: "Industrial Output".into(),
                    weight: 0.25,
                    ceiling: 1000.0,
                },
                DimensionDef {
                    id: "research_progress".into(),
                    name: "Research Progress".into(),
                    weight: 0.20,
                    ceiling: 1.0,
                },
                DimensionDef {
                    id: "economic_health".into(),
                    name: "Economic Health".into(),
                    weight: 0.20,
                    ceiling: 1.0,
                },
                DimensionDef {
                    id: "fleet_operations".into(),
                    name: "Fleet Operations".into(),
                    weight: 0.15,
                    ceiling: 1.0,
                },
                DimensionDef {
                    id: "efficiency".into(),
                    name: "Efficiency".into(),
                    weight: 0.10,
                    ceiling: 1.0,
                },
                DimensionDef {
                    id: "expansion".into(),
                    name: "Expansion".into(),
                    weight: 0.10,
                    ceiling: 1.0,
                },
            ],
            thresholds: vec![
                ThresholdDef {
                    name: "Startup".into(),
                    min_score: 0.0,
                },
                ThresholdDef {
                    name: "Contractor".into(),
                    min_score: 200.0,
                },
                ThresholdDef {
                    name: "Enterprise".into(),
                    min_score: 500.0,
                },
                ThresholdDef {
                    name: "Industrial Giant".into(),
                    min_score: 1000.0,
                },
                ThresholdDef {
                    name: "Space Magnate".into(),
                    min_score: 2000.0,
                },
            ],
            computation_interval_ticks: 24,
            scale_factor: 2500.0,
        }
    }

    #[test]
    fn valid_config_passes_validation() {
        let config = sample_config();
        assert!(validate_scoring_config(&config).is_ok());
    }

    #[test]
    fn weights_not_summing_to_one_rejected() {
        let mut config = sample_config();
        config.dimensions[0].weight = 0.50; // now sums to 1.25
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("weights must sum to 1.0"), "{err}");
    }

    #[test]
    fn empty_dimensions_rejected() {
        let mut config = sample_config();
        config.dimensions.clear();
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("at least one dimension"), "{err}");
    }

    #[test]
    fn non_ascending_thresholds_rejected() {
        let mut config = sample_config();
        config.thresholds[2].min_score = 100.0; // Enterprise < Contractor
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("ascending"), "{err}");
    }

    #[test]
    fn non_positive_ceiling_rejected() {
        let mut config = sample_config();
        config.dimensions[0].ceiling = 0.0;
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("non-positive ceiling"), "{err}");
    }

    #[test]
    fn zero_computation_interval_rejected() {
        let mut config = sample_config();
        config.computation_interval_ticks = 0;
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("computation_interval_ticks"), "{err}");
    }

    #[test]
    fn duplicate_dimension_ids_rejected() {
        let mut config = sample_config();
        config.dimensions[1].id = "industrial_output".into(); // duplicate
        config.dimensions[1].weight = config.dimensions[0].weight; // fix weights
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("duplicate dimension id"), "{err}");
    }

    #[test]
    fn negative_weight_rejected() {
        let mut config = sample_config();
        config.dimensions[0].weight = -0.25;
        config.dimensions[1].weight = 0.70; // sums to 1.0 but negative weight
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("non-positive weight"), "{err}");
    }

    #[test]
    fn non_positive_scale_factor_rejected() {
        let mut config = sample_config();
        config.scale_factor = 0.0;
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("scale_factor"), "{err}");
    }

    #[test]
    fn run_score_serde_roundtrip() {
        let score = RunScore {
            dimensions: BTreeMap::from([(
                "industrial_output".to_string(),
                DimensionScore {
                    id: "industrial_output".into(),
                    name: "Industrial Output".into(),
                    raw_value: 500.0,
                    normalized: 0.5,
                    weighted: 312.5,
                },
            )]),
            composite: 847.0,
            threshold: "Enterprise".into(),
            tick: 500,
        };
        let json = serde_json::to_string(&score).unwrap();
        let roundtrip: RunScore = serde_json::from_str(&json).unwrap();
        assert!((roundtrip.composite - 847.0).abs() < f64::EPSILON);
        assert_eq!(roundtrip.threshold, "Enterprise");
        assert_eq!(roundtrip.tick, 500);
    }

    #[test]
    fn scoring_config_serde_roundtrip() {
        let config = sample_config();
        let json = serde_json::to_string(&config).unwrap();
        let roundtrip: ScoringConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.dimensions.len(), 6);
        assert_eq!(roundtrip.thresholds.len(), 5);
        assert_eq!(roundtrip.computation_interval_ticks, 24);
    }

    #[test]
    fn default_scoring_config_has_defaults() {
        let config = ScoringConfig::default();
        assert_eq!(config.computation_interval_ticks, 24);
        assert!((config.scale_factor - 2500.0).abs() < f64::EPSILON);
    }

    // -- compute_run_score tests --

    fn make_metrics(tick: u64) -> MetricsSnapshot {
        crate::MetricsSnapshot {
            tick,
            metrics_version: crate::METRICS_VERSION,
            total_ore_kg: 0.0,
            total_material_kg: 500.0,
            total_slag_kg: 0.0,
            per_element_material_kg: BTreeMap::new(),
            station_storage_used_pct: 0.5,
            ship_cargo_used_pct: 0.0,
            per_element_ore_stats: BTreeMap::new(),
            ore_lot_count: 0,
            avg_material_quality: 0.8,
            per_module_metrics: BTreeMap::from([(
                "assembler".to_string(),
                crate::ModuleStatusMetrics {
                    active: 1,
                    stalled: 0,
                    starved: 0,
                },
            )]),
            fleet_total: 3,
            fleet_idle: 1,
            fleet_mining: 1,
            fleet_transiting: 1,
            fleet_surveying: 0,
            fleet_depositing: 0,
            fleet_refueling: 0,
            fleet_propellant_kg: 100.0,
            fleet_propellant_pct: 0.8,
            propellant_consumed_total: 50.0,
            scan_sites_remaining: 5,
            asteroids_discovered: 3,
            asteroids_depleted: 0,
            techs_unlocked: 2,
            total_scan_data: 500.0,
            max_tech_evidence: 0.5,
            avg_module_wear: 0.2,
            max_module_wear: 0.5,
            repair_kits_remaining: 5,
            balance: 999_000_000.0,
            crew_salary_per_hour: 0.0,
            thruster_count: 3,
            export_revenue_total: 50_000.0,
            export_count: 5,
            power_generated_kw: 10.0,
            power_consumed_kw: 8.0,
            power_deficit_kw: 0.0,
            battery_charge_pct: 0.9,
            station_max_temp_mk: 300_000,
            station_avg_temp_mk: 293_000,
            overheat_warning_count: 0,
            overheat_critical_count: 0,
            heat_wear_multiplier_avg: 1.0,
            satellites_active: 0,
            satellites_failed: 0,
            transfer_volume_kg: 0.0,
            transfer_count: 0,
            milestones_completed: 0,
            game_phase: 0,
        }
    }

    fn scored_content() -> crate::GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.scoring = sample_config();
        content
    }

    #[test]
    fn compute_run_score_deterministic() {
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);
        let metrics = make_metrics(100);

        let score1 = compute_run_score(&metrics, &state, &content);
        let score2 = compute_run_score(&metrics, &state, &content);
        assert_eq!(score1.composite, score2.composite);
        assert_eq!(score1.dimensions, score2.dimensions);
    }

    #[test]
    fn all_dimensions_in_unit_range() {
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);
        let metrics = make_metrics(100);

        let score = compute_run_score(&metrics, &state, &content);
        for (dim_id, dim_score) in &score.dimensions {
            assert!(
                (0.0..=1.0).contains(&dim_score.normalized),
                "dimension '{dim_id}' normalized={} out of [0.0, 1.0]",
                dim_score.normalized
            );
        }
    }

    #[test]
    fn tick_zero_scores_startup() {
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);
        let mut metrics = make_metrics(1);
        metrics.total_material_kg = 0.0;
        metrics.techs_unlocked = 0;
        metrics.export_revenue_total = 0.0;
        metrics.fleet_total = 1;
        metrics.fleet_idle = 1;

        let score = compute_run_score(&metrics, &state, &content);
        // Minimal production activity, but balance alone gives economic score
        assert!(
            score.composite < 500.0,
            "minimal activity should be below Enterprise (500), got {}",
            score.composite
        );
    }

    #[test]
    fn empty_scoring_config_returns_default() {
        let mut content = scored_content();
        content.scoring = ScoringConfig::default(); // empty dimensions
        let state = crate::test_fixtures::base_state(&content);
        let metrics = make_metrics(100);

        let score = compute_run_score(&metrics, &state, &content);
        assert_eq!(score.composite, 0.0);
        assert!(score.dimensions.is_empty());
    }

    #[test]
    fn composite_scales_with_activity() {
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);

        let low_metrics = make_metrics(1000);
        let mut high_metrics = make_metrics(1000);
        high_metrics.total_material_kg = 50_000.0;
        high_metrics.techs_unlocked = 5;
        high_metrics.export_revenue_total = 500_000.0;

        let low_score = compute_run_score(&low_metrics, &state, &content);
        let high_score = compute_run_score(&high_metrics, &state, &content);
        assert!(
            high_score.composite > low_score.composite,
            "more activity should produce higher score: high={} vs low={}",
            high_score.composite,
            low_score.composite
        );
    }

    #[test]
    fn resolve_threshold_finds_highest() {
        let thresholds = sample_config().thresholds;
        assert_eq!(resolve_threshold(&thresholds, 0.0), "Startup");
        assert_eq!(resolve_threshold(&thresholds, 199.0), "Startup");
        assert_eq!(resolve_threshold(&thresholds, 200.0), "Contractor");
        assert_eq!(resolve_threshold(&thresholds, 999.0), "Enterprise");
        assert_eq!(resolve_threshold(&thresholds, 2500.0), "Space Magnate");
    }

    /// Build an empty station (no modules) with the given id.
    /// Used by VIO-603 multi-station scoring tests.
    fn test_empty_station(id: &str) -> crate::StationState {
        crate::StationState {
            id: crate::StationId(id.to_string()),
            position: crate::test_fixtures::test_position(),
            core: crate::FacilityCore {
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                modifiers: crate::modifiers::ModifierSet::default(),
                crew: Default::default(),
                thermal_links: Vec::new(),
                power: crate::PowerState::default(),
                cached_inventory_volume_m3: None,
                module_type_index: crate::ModuleTypeIndex::default(),
                module_id_index: std::collections::HashMap::new(),
                power_budget_cache: crate::PowerBudgetCache::default(),
            },
            leaders: Vec::new(),
            frame_id: None,
        }
    }

    /// Build a station with a single Processor module (a "productive" base).
    fn test_productive_station(id: &str) -> crate::StationState {
        let mut station = test_empty_station(id);
        station.core.modules.push(crate::ModuleState {
            id: crate::ModuleInstanceId(format!("mod_proc_{id}")),
            def_id: "module_basic_smelter".to_string(),
            enabled: true,
            kind_state: crate::ModuleKindState::Processor(crate::ProcessorState {
                threshold_kg: 500.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
            wear: crate::WearState::default(),
            thermal: None,
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            slot_index: None,
        });
        station
    }

    fn expansion_raw(score: &RunScore) -> f64 {
        score.dimensions.get("expansion").unwrap().raw_value
    }

    fn industrial_raw(score: &RunScore) -> f64 {
        score.dimensions.get("industrial_output").unwrap().raw_value
    }

    #[test]
    fn satellites_improve_expansion_score() {
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);
        let metrics_no_sats = make_metrics(100);

        let mut metrics_with_sats = make_metrics(100);
        metrics_with_sats.satellites_active = 4;

        let score_no_sats = compute_run_score(&metrics_no_sats, &state, &content);
        let score_with_sats = compute_run_score(&metrics_with_sats, &state, &content);

        let expansion_no = score_no_sats.dimensions.get("expansion").unwrap().raw_value;
        let expansion_yes = score_with_sats
            .dimensions
            .get("expansion")
            .unwrap()
            .raw_value;
        assert!(
            expansion_yes > expansion_no,
            "active satellites should increase expansion score: {} vs {}",
            expansion_yes,
            expansion_no
        );
    }

    #[test]
    fn multi_base_improves_expansion_score() {
        // VIO-603: deploying a second base should produce a visible
        // expansion score bump via the multi_base_bonus.
        let content = scored_content();
        let metrics = make_metrics(100);

        let state_one = crate::test_fixtures::base_state(&content);
        let score_one = compute_run_score(&metrics, &state_one, &content);

        let mut state_two = crate::test_fixtures::base_state(&content);
        state_two.stations.insert(
            crate::StationId("station_mars_orbit".into()),
            test_empty_station("station_mars_orbit"),
        );
        let score_two = compute_run_score(&metrics, &state_two, &content);

        assert!(
            expansion_raw(&score_two) > expansion_raw(&score_one),
            "second base should increase expansion score: {} vs {}",
            expansion_raw(&score_two),
            expansion_raw(&score_one)
        );
    }

    #[test]
    fn single_base_expansion_unchanged_by_vio_603() {
        // VIO-603 regression guard: the multi_base_bonus must be purely
        // additive — a single-base run must score *exactly* the
        // pre-VIO-603 expansion raw_value:
        //   base_signal * 0.4 + fleet_signal * 0.3 + satellite_signal * 0.3
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);
        let metrics = make_metrics(100);

        // Hand-compute expected raw from the original blend to pin it.
        let base_signal = (1.0_f64 / 3.0).min(1.0);
        let fleet_signal = (f64::from(metrics.fleet_total).sqrt() / 3.0).min(1.0);
        let satellite_signal = (f64::from(metrics.satellites_active).sqrt() / 3.0).min(1.0);
        let expected = base_signal * 0.4 + fleet_signal * 0.3 + satellite_signal * 0.3;

        let actual = expansion_raw(&compute_run_score(&metrics, &state, &content));
        assert!(
            (actual - expected).abs() < 1e-10,
            "single-base expansion raw_value changed (regression): expected {expected}, got {actual}"
        );
    }

    #[test]
    fn multi_base_bonus_plateaus_at_four_bases() {
        // VIO-603: the multi_base_bonus must saturate at 4 total bases.
        // Adding a 5th or 10th base must produce the same expansion
        // raw_value as 4 bases — no reward for sprawl.
        let content = scored_content();
        let metrics = make_metrics(100);

        let score_at = |n: usize| {
            let mut state = crate::test_fixtures::base_state(&content);
            for i in 1..n {
                let id = format!("station_extra_{i}");
                state
                    .stations
                    .insert(crate::StationId(id.clone()), test_empty_station(&id));
            }
            expansion_raw(&compute_run_score(&metrics, &state, &content))
        };

        let at_four = score_at(4);
        let at_five = score_at(5);
        let at_ten = score_at(10);

        assert!(
            (at_four - at_five).abs() < 1e-10,
            "multi_base_bonus should plateau at 4 bases: n=4 {} vs n=5 {}",
            at_four,
            at_five
        );
        assert!(
            (at_four - at_ten).abs() < 1e-10,
            "multi_base_bonus should plateau at 4 bases: n=4 {} vs n=10 {}",
            at_four,
            at_ten
        );
    }

    #[test]
    fn multi_productive_bases_improve_industrial_score() {
        // VIO-603: running N productive bases (assembler/processor)
        // should produce a diversification multiplier on throughput,
        // rewarding distributed production over concentrating everything
        // on one base.
        let content = scored_content();
        let metrics = make_metrics(100);

        // Baseline: single station with a processor.
        let mut state_one = crate::test_fixtures::base_state(&content);
        {
            let station = state_one
                .stations
                .get_mut(&crate::test_fixtures::test_station_id())
                .unwrap();
            *station = {
                let mut s = test_productive_station("earth_orbit");
                s.id = crate::test_fixtures::test_station_id();
                s
            };
        }
        let score_one = compute_run_score(&metrics, &state_one, &content);

        // Two productive bases.
        let mut state_two = state_one.clone();
        state_two.stations.insert(
            crate::StationId("station_mars_orbit".into()),
            test_productive_station("station_mars_orbit"),
        );
        let score_two = compute_run_score(&metrics, &state_two, &content);

        assert!(
            industrial_raw(&score_two) > industrial_raw(&score_one),
            "second productive base should increase industrial score: {} vs {}",
            industrial_raw(&score_two),
            industrial_raw(&score_one)
        );
    }

    #[test]
    fn unproductive_second_base_no_industrial_bonus() {
        // VIO-603: a base with no assembler/processor should NOT
        // contribute to the industrial diversification multiplier —
        // only productive bases count.
        let content = scored_content();
        let metrics = make_metrics(100);

        let state_one = crate::test_fixtures::base_state(&content);
        let score_one = compute_run_score(&metrics, &state_one, &content);

        let mut state_two = crate::test_fixtures::base_state(&content);
        state_two.stations.insert(
            crate::StationId("station_mars_orbit".into()),
            test_empty_station("station_mars_orbit"),
        );
        let score_two = compute_run_score(&metrics, &state_two, &content);

        assert!(
            (industrial_raw(&score_one) - industrial_raw(&score_two)).abs() < f64::EPSILON,
            "empty second base should not change industrial score: {} vs {}",
            industrial_raw(&score_one),
            industrial_raw(&score_two)
        );
    }

    #[test]
    fn ground_facility_counts_as_productive_base() {
        // VIO-603: ground facilities with productive modules must count
        // toward the diversification multiplier so ground-start runs
        // have access to the same rewards as orbital-start runs.
        let content = scored_content();
        let metrics = make_metrics(100);

        // Baseline: one productive orbital station.
        let mut state_orbital = crate::test_fixtures::base_state(&content);
        {
            let station = state_orbital
                .stations
                .get_mut(&crate::test_fixtures::test_station_id())
                .unwrap();
            *station = {
                let mut s = test_productive_station("earth_orbit");
                s.id = crate::test_fixtures::test_station_id();
                s
            };
        }
        let score_orbital = compute_run_score(&metrics, &state_orbital, &content);

        // Add a productive ground facility.
        let mut state_hybrid = state_orbital.clone();
        let gf_id = crate::GroundFacilityId("gf_earth_kennedy".into());
        state_hybrid.ground_facilities.insert(
            gf_id.clone(),
            crate::GroundFacilityState {
                id: gf_id,
                name: "Kennedy Launch Complex".into(),
                position: crate::test_fixtures::test_position(),
                core: crate::FacilityCore {
                    inventory: vec![],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![crate::ModuleState {
                        id: crate::ModuleInstanceId("mod_gf_proc".into()),
                        def_id: "module_basic_smelter".into(),
                        enabled: true,
                        kind_state: crate::ModuleKindState::Processor(crate::ProcessorState {
                            threshold_kg: 500.0,
                            ticks_since_last_run: 0,
                            stalled: false,
                            selected_recipe: None,
                        }),
                        wear: crate::WearState::default(),
                        thermal: None,
                        power_stalled: false,
                        module_priority: 0,
                        assigned_crew: Default::default(),
                        efficiency: 1.0,
                        prev_crew_satisfied: true,
                        slot_index: None,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    crew: Default::default(),
                    thermal_links: Vec::new(),
                    power: crate::PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                    module_id_index: std::collections::HashMap::new(),
                    power_budget_cache: crate::PowerBudgetCache::default(),
                },
                launch_transits: Vec::new(),
            },
        );
        let score_hybrid = compute_run_score(&metrics, &state_hybrid, &content);

        assert!(
            industrial_raw(&score_hybrid) > industrial_raw(&score_orbital),
            "ground facility should count as productive base (industrial): {} vs {}",
            industrial_raw(&score_hybrid),
            industrial_raw(&score_orbital)
        );
        assert!(
            expansion_raw(&score_hybrid) > expansion_raw(&score_orbital),
            "ground facility should count toward expansion multi-base bonus: {} vs {}",
            expansion_raw(&score_hybrid),
            expansion_raw(&score_orbital)
        );
    }

    #[test]
    fn satellites_improve_fleet_score() {
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);
        let mut metrics = make_metrics(100);
        metrics.fleet_total = 2;
        metrics.fleet_idle = 0;
        let score_base = compute_run_score(&metrics, &state, &content);

        metrics.satellites_active = 3;
        let score_sats = compute_run_score(&metrics, &state, &content);

        let fleet_base = score_base
            .dimensions
            .get("fleet_operations")
            .unwrap()
            .raw_value;
        let fleet_sats = score_sats
            .dimensions
            .get("fleet_operations")
            .unwrap()
            .raw_value;
        assert!(
            fleet_sats > fleet_base,
            "satellites should increase fleet ops score: {} vs {}",
            fleet_sats,
            fleet_base
        );
    }

    #[test]
    fn satellite_failures_reduce_efficiency() {
        let content = scored_content();
        let state = crate::test_fixtures::base_state(&content);

        // All active — sat_util = 1.0
        let mut metrics_all_active = make_metrics(100);
        metrics_all_active.satellites_active = 4;
        metrics_all_active.satellites_failed = 0;
        let score_healthy = compute_run_score(&metrics_all_active, &state, &content);

        // Half failed — sat_util = 0.5
        let mut metrics_half_failed = make_metrics(100);
        metrics_half_failed.satellites_active = 2;
        metrics_half_failed.satellites_failed = 2;
        let score_degraded = compute_run_score(&metrics_half_failed, &state, &content);

        let eff_healthy = score_healthy
            .dimensions
            .get("efficiency")
            .unwrap()
            .raw_value;
        let eff_degraded = score_degraded
            .dimensions
            .get("efficiency")
            .unwrap()
            .raw_value;
        assert!(
            eff_healthy > eff_degraded,
            "satellite failures should reduce efficiency: {} vs {}",
            eff_healthy,
            eff_degraded
        );
    }

    #[test]
    fn science_satellites_improve_research() {
        let content = scored_content();
        let state_no_sats = crate::test_fixtures::base_state(&content);
        let metrics = make_metrics(100);
        let score_no_sats = compute_run_score(&metrics, &state_no_sats, &content);

        let mut state_with_sats = crate::test_fixtures::base_state(&content);
        for i in 0..2 {
            state_with_sats.satellites.insert(
                crate::SatelliteId(format!("sat_{i}")),
                crate::SatelliteState {
                    id: crate::SatelliteId(format!("sat_{i}")),
                    def_id: "sat_science_platform".into(),
                    name: format!("Science {i}"),
                    position: crate::test_fixtures::test_position(),
                    deployed_tick: 0,
                    wear: 0.0,
                    enabled: true,
                    satellite_type: "science_platform".into(),
                    payload_config: None,
                },
            );
        }
        let score_with_sats = compute_run_score(&metrics, &state_with_sats, &content);

        let research_no = score_no_sats
            .dimensions
            .get("research_progress")
            .unwrap()
            .raw_value;
        let research_yes = score_with_sats
            .dimensions
            .get("research_progress")
            .unwrap()
            .raw_value;
        assert!(
            research_yes > research_no,
            "science satellites should increase research score: {} vs {}",
            research_yes,
            research_no
        );
    }
}
