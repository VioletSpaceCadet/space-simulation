//! Run scoring — types, configuration, and computation.
//!
//! `ScoringConfig` is loaded from `content/scoring.json` as part of `GameContent`.
//! `compute_run_score()` is a pure function producing a `RunScore` from game state.
//!
//! Signal sources are resolved by code (they encode game mechanics), but signal
//! combination (blending, saturation, transforms) is config-driven via `scoring.json`.

use crate::{GameContent, GameState, MetricsSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Content configuration (loaded from scoring.json)
// ---------------------------------------------------------------------------

/// Transform applied to a signal source value before blending.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SignalTransform {
    /// Pass-through: value used as-is (assumed already in useful range).
    #[default]
    Identity,
    /// `(value / saturation).min(1.0)` — linear ramp to 1.0 at saturation.
    LinearSaturate,
    /// `(value.sqrt() / saturation).min(1.0)` — diminishing returns.
    SqrtSaturate,
    /// `(1.0 - value).clamp(0.0, 1.0)` — inversion (e.g. wear → health).
    Inverse,
    /// Piecewise: 1.0 in `[band_low, band_high]`, linear penalty outside.
    Band,
    /// `(value / saturation).clamp(0.0, clamp_max) / clamp_max` — bounded ratio.
    ClampSaturate,
}

/// A single signal within a scoring dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalDef {
    /// Named signal source (resolved by `resolve_signal_source`).
    pub source: String,
    /// Contribution weight within this dimension.
    pub blend: f64,
    /// Transform applied to the raw source value.
    #[serde(default)]
    pub transform: SignalTransform,
    /// Denominator for saturation transforms (`linear_saturate`, `sqrt_saturate`, `clamp_saturate`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturation: Option<f64>,
    /// Lower boundary for `band` transform (value below this is penalized).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band_low: Option<f64>,
    /// Upper boundary for `band` transform (value above this is penalized).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band_high: Option<f64>,
    /// Upper clamp for `clamp_saturate` (value is clamped to `[0, clamp_max]` then normalized).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clamp_max: Option<f64>,
}

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
    /// Signal definitions that compose this dimension's raw value.
    /// `raw = Σ(transform(resolve(source)) * blend)`
    #[serde(default)]
    pub signals: Vec<SignalDef>,
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
    /// The scoring dimensions with weights and normalization ceilings.
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

/// All recognized signal source names. Used for content validation.
pub const KNOWN_SIGNAL_SOURCES: &[&str] = &[
    "assembler_active",
    "avg_module_wear",
    "balance",
    "base_count",
    "extra_bases",
    "fleet_total",
    "fleet_utilization",
    "grant_rate",
    "industrial_throughput",
    "power_utilization",
    "revenue_rate",
    "satellites_active",
    "satellite_utilization",
    "science_satellites",
    "ships_constructed",
    "station_storage_used_pct",
    "tech_fraction",
    "total_launches",
    "total_raw_data",
];

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
        validate_dimension_signals(dim)?;
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

fn validate_dimension_signals(dim: &DimensionDef) -> Result<(), String> {
    for signal in &dim.signals {
        if signal.blend <= 0.0 {
            return Err(format!(
                "dimension '{}' signal '{}' has non-positive blend {}",
                dim.id, signal.source, signal.blend
            ));
        }
        if !KNOWN_SIGNAL_SOURCES.contains(&signal.source.as_str()) {
            return Err(format!(
                "dimension '{}' signal '{}' has unknown source",
                dim.id, signal.source
            ));
        }
        match signal.transform {
            SignalTransform::LinearSaturate
            | SignalTransform::SqrtSaturate
            | SignalTransform::ClampSaturate => {
                let sat = signal.saturation.unwrap_or(0.0);
                if sat <= 0.0 {
                    return Err(format!(
                        "dimension '{}' signal '{}' requires saturation > 0 for {:?}",
                        dim.id, signal.source, signal.transform
                    ));
                }
            }
            SignalTransform::Band => {
                let low = signal.band_low.unwrap_or(0.0);
                let high = signal.band_high.unwrap_or(0.0);
                if low >= high {
                    return Err(format!(
                        "dimension '{}' signal '{}' band requires band_low < band_high",
                        dim.id, signal.source
                    ));
                }
                if high >= 1.0 {
                    return Err(format!(
                        "dimension '{}' signal '{}' band requires band_high < 1.0 (got {})",
                        dim.id, signal.source, high
                    ));
                }
            }
            SignalTransform::Identity | SignalTransform::Inverse => {}
        }
        if signal.transform == SignalTransform::ClampSaturate {
            let max = signal.clamp_max.unwrap_or(0.0);
            if max <= 0.0 {
                return Err(format!(
                    "dimension '{}' signal '{}' clamp_saturate requires clamp_max > 0",
                    dim.id, signal.source
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Score computation
// ---------------------------------------------------------------------------

/// Compute a run score from the current metrics snapshot and game state.
///
/// Pure function — no state mutation, no IO. Deterministic for the same inputs.
/// Each dimension produces a raw value from its config-driven signals, which is
/// normalized to [0.0, 1.0] by dividing by the dimension's ceiling (clamped).
/// The composite score is the weighted sum scaled by `scale_factor`.
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
        let raw_value = compute_dimension_raw(dim, metrics, state, content, tick);
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

/// Compute the raw value for a single dimension from its signal config.
///
/// `raw = Σ(apply_transform(resolve_source(signal.source), signal) * signal.blend)`
fn compute_dimension_raw(
    dim: &DimensionDef,
    metrics: &MetricsSnapshot,
    state: &GameState,
    content: &GameContent,
    tick: f64,
) -> f64 {
    dim.signals
        .iter()
        .map(|signal| {
            let raw = resolve_signal_source(&signal.source, metrics, state, content, tick);
            apply_transform(raw, signal) * signal.blend
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Signal source resolution
// ---------------------------------------------------------------------------

/// Resolve a named signal source to its raw value.
///
/// Signal sources encode game mechanics — adding a new source requires code.
/// The signal *combination* (blending, saturation, transforms) is config-driven.
fn resolve_signal_source(
    source: &str,
    metrics: &MetricsSnapshot,
    state: &GameState,
    content: &GameContent,
    tick: f64,
) -> f64 {
    match source {
        // -- Industrial --
        "industrial_throughput" => {
            let throughput = f64::from(metrics.total_material_kg) / tick;
            let productive_bases = count_productive_bases(state) as f64;
            let extra_bases = (productive_bases - 1.0).clamp(0.0, 3.0);
            let diversification_multiplier = 1.0 + extra_bases * 0.10;
            throughput * diversification_multiplier
        }
        "assembler_active" => f64::from(
            metrics
                .per_module_metrics
                .get("assembler")
                .map_or(0, |m| m.active),
        ),

        // -- Research --
        "tech_fraction" => {
            let total_techs = content.techs.len().max(1) as f64;
            f64::from(metrics.techs_unlocked) / total_techs
        }
        "total_raw_data" => {
            let mut data_values: Vec<f64> = state
                .research
                .data_pool
                .values()
                .map(|v| f64::from(*v))
                .collect();
            data_values.sort_by(f64::total_cmp);
            data_values.iter().sum()
        }
        "science_satellites" => state
            .satellites
            .values()
            .filter(|s| s.enabled && s.satellite_type == "science_platform")
            .count() as f64,

        // -- Economic --
        "balance" => state.balance,
        "revenue_rate" => metrics.export_revenue_total / tick,
        "grant_rate" => {
            let grant_total: f64 = state
                .progression
                .grant_history
                .iter()
                .map(|g| g.amount)
                .sum();
            grant_total / tick
        }

        // -- Fleet --
        "fleet_utilization" => {
            if metrics.fleet_total > 0 {
                let total = f64::from(metrics.fleet_total);
                let active = total - f64::from(metrics.fleet_idle);
                active / total
            } else {
                0.0
            }
        }
        "ships_constructed" => (state.ships.len() as f64 - 1.0).max(0.0),
        "satellites_active" => f64::from(metrics.satellites_active),
        "total_launches" => {
            state.counters.stations_deployed as f64
                + state
                    .ground_facilities
                    .values()
                    .flat_map(|f| f.core.modules.iter())
                    .filter_map(|m| match &m.kind_state {
                        crate::ModuleKindState::LaunchPad(pad) => Some(pad.launches_count as f64),
                        _ => None,
                    })
                    .sum::<f64>()
        }

        // -- Efficiency --
        "avg_module_wear" => f64::from(metrics.avg_module_wear),
        "power_utilization" => {
            if metrics.power_generated_kw > 0.0 {
                (f64::from(metrics.power_consumed_kw) / f64::from(metrics.power_generated_kw))
                    .min(1.0)
            } else {
                0.0
            }
        }
        "station_storage_used_pct" => f64::from(metrics.station_storage_used_pct),
        "satellite_utilization" => {
            let total_sats = f64::from(metrics.satellites_active + metrics.satellites_failed);
            if total_sats > 0.0 {
                f64::from(metrics.satellites_active) / total_sats
            } else {
                1.0
            }
        }

        // -- Expansion --
        "base_count" => (state.stations.len() + state.ground_facilities.len()) as f64,
        "fleet_total" => f64::from(metrics.fleet_total),
        "extra_bases" => {
            let base_count = (state.stations.len() + state.ground_facilities.len()) as f64;
            (base_count - 1.0).clamp(0.0, 3.0)
        }

        _ => 0.0,
    }
}

/// Count productive bases: stations + ground facilities that host at
/// least one Assembler or Processor module.
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

// ---------------------------------------------------------------------------
// Signal transforms
// ---------------------------------------------------------------------------

/// Apply the configured transform to a raw signal value.
fn apply_transform(raw: f64, signal: &SignalDef) -> f64 {
    match signal.transform {
        SignalTransform::Identity => raw,
        SignalTransform::LinearSaturate => (raw / signal.saturation.unwrap_or(1.0)).min(1.0),
        SignalTransform::SqrtSaturate => (raw.sqrt() / signal.saturation.unwrap_or(1.0)).min(1.0),
        SignalTransform::Inverse => (1.0 - raw).clamp(0.0, 1.0),
        SignalTransform::Band => {
            let low = signal.band_low.unwrap_or(0.0);
            let high = signal.band_high.unwrap_or(1.0);
            let raw = raw.max(0.0); // Guard: signal sources should not be negative
            if raw < low {
                if low > 0.0 {
                    raw / low
                } else {
                    1.0
                }
            } else if raw > high {
                let range = 1.0 - high;
                if range > f64::EPSILON {
                    (1.0 - raw) / range
                } else {
                    0.0
                }
            } else {
                1.0
            }
        }
        SignalTransform::ClampSaturate => {
            let sat = signal.saturation.unwrap_or(1.0);
            let max = signal.clamp_max.unwrap_or(1.0);
            (raw / sat).clamp(0.0, max) / max
        }
    }
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
                    signals: vec![
                        SignalDef {
                            source: "industrial_throughput".into(),
                            blend: 1.0,
                            transform: SignalTransform::Identity,
                            saturation: None,
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "assembler_active".into(),
                            blend: 0.1,
                            transform: SignalTransform::Identity,
                            saturation: None,
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                    ],
                },
                DimensionDef {
                    id: "research_progress".into(),
                    name: "Research Progress".into(),
                    weight: 0.20,
                    ceiling: 1.0,
                    signals: vec![
                        SignalDef {
                            source: "tech_fraction".into(),
                            blend: 0.6,
                            transform: SignalTransform::Identity,
                            saturation: None,
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "total_raw_data".into(),
                            blend: 0.25,
                            transform: SignalTransform::LinearSaturate,
                            saturation: Some(1000.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "science_satellites".into(),
                            blend: 0.15,
                            transform: SignalTransform::LinearSaturate,
                            saturation: Some(3.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                    ],
                },
                DimensionDef {
                    id: "economic_health".into(),
                    name: "Economic Health".into(),
                    weight: 0.20,
                    ceiling: 1.0,
                    signals: vec![
                        SignalDef {
                            source: "balance".into(),
                            blend: 0.3,
                            transform: SignalTransform::ClampSaturate,
                            saturation: Some(1_000_000_000.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: Some(2.0),
                        },
                        SignalDef {
                            source: "revenue_rate".into(),
                            blend: 0.35,
                            transform: SignalTransform::LinearSaturate,
                            saturation: Some(10_000.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "grant_rate".into(),
                            blend: 0.35,
                            transform: SignalTransform::LinearSaturate,
                            saturation: Some(50_000.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                    ],
                },
                DimensionDef {
                    id: "fleet_operations".into(),
                    name: "Fleet Operations".into(),
                    weight: 0.15,
                    ceiling: 1.0,
                    signals: vec![
                        SignalDef {
                            source: "fleet_utilization".into(),
                            blend: 0.35,
                            transform: SignalTransform::Identity,
                            saturation: None,
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "ships_constructed".into(),
                            blend: 0.25,
                            transform: SignalTransform::LinearSaturate,
                            saturation: Some(5.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "satellites_active".into(),
                            blend: 0.2,
                            transform: SignalTransform::SqrtSaturate,
                            saturation: Some(3.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "total_launches".into(),
                            blend: 0.2,
                            transform: SignalTransform::SqrtSaturate,
                            saturation: Some(3.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                    ],
                },
                DimensionDef {
                    id: "efficiency".into(),
                    name: "Efficiency".into(),
                    weight: 0.10,
                    ceiling: 1.0,
                    signals: vec![
                        SignalDef {
                            source: "avg_module_wear".into(),
                            blend: 0.25,
                            transform: SignalTransform::Inverse,
                            saturation: None,
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "power_utilization".into(),
                            blend: 0.25,
                            transform: SignalTransform::Identity,
                            saturation: None,
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "station_storage_used_pct".into(),
                            blend: 0.25,
                            transform: SignalTransform::Band,
                            saturation: None,
                            band_low: Some(0.1),
                            band_high: Some(0.95),
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "satellite_utilization".into(),
                            blend: 0.25,
                            transform: SignalTransform::Identity,
                            saturation: None,
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                    ],
                },
                DimensionDef {
                    id: "expansion".into(),
                    name: "Expansion".into(),
                    weight: 0.10,
                    ceiling: 1.0,
                    signals: vec![
                        SignalDef {
                            source: "base_count".into(),
                            blend: 0.4,
                            transform: SignalTransform::LinearSaturate,
                            saturation: Some(3.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "fleet_total".into(),
                            blend: 0.3,
                            transform: SignalTransform::SqrtSaturate,
                            saturation: Some(3.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "satellites_active".into(),
                            blend: 0.3,
                            transform: SignalTransform::SqrtSaturate,
                            saturation: Some(3.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                        SignalDef {
                            source: "extra_bases".into(),
                            blend: 0.15,
                            transform: SignalTransform::LinearSaturate,
                            saturation: Some(3.0),
                            band_low: None,
                            band_high: None,
                            clamp_max: None,
                        },
                    ],
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
    fn unknown_signal_source_rejected() {
        let mut config = sample_config();
        config.dimensions[0].signals[0].source = "nonexistent".into();
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("unknown source"), "{err}");
    }

    #[test]
    fn missing_saturation_rejected() {
        let mut config = sample_config();
        // research_progress has linear_saturate signals with saturation
        config.dimensions[1].signals[1].saturation = None;
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("saturation"), "{err}");
    }

    #[test]
    fn invalid_band_rejected() {
        let mut config = sample_config();
        // efficiency has a band signal
        config.dimensions[4].signals[2].band_low = Some(0.9);
        config.dimensions[4].signals[2].band_high = Some(0.1); // low > high
        let err = validate_scoring_config(&config).unwrap_err();
        assert!(err.contains("band_low < band_high"), "{err}");
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
        // Verify signals survived round-trip
        assert_eq!(roundtrip.dimensions[1].signals.len(), 3);
        assert_eq!(roundtrip.dimensions[1].signals[0].source, "tech_fraction");
    }

    #[test]
    fn default_scoring_config_has_defaults() {
        let config = ScoringConfig::default();
        assert_eq!(config.computation_interval_ticks, 24);
        assert!((config.scale_factor - 2500.0).abs() < f64::EPSILON);
    }

    // -- Transform unit tests --

    #[test]
    fn transform_identity() {
        let signal = SignalDef {
            source: String::new(),
            blend: 1.0,
            transform: SignalTransform::Identity,
            saturation: None,
            band_low: None,
            band_high: None,
            clamp_max: None,
        };
        assert!((apply_transform(0.75, &signal) - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn transform_linear_saturate() {
        let signal = SignalDef {
            source: String::new(),
            blend: 1.0,
            transform: SignalTransform::LinearSaturate,
            saturation: Some(100.0),
            band_low: None,
            band_high: None,
            clamp_max: None,
        };
        assert!((apply_transform(50.0, &signal) - 0.5).abs() < f64::EPSILON);
        assert!((apply_transform(200.0, &signal) - 1.0).abs() < f64::EPSILON); // clamped
    }

    #[test]
    fn transform_sqrt_saturate() {
        let signal = SignalDef {
            source: String::new(),
            blend: 1.0,
            transform: SignalTransform::SqrtSaturate,
            saturation: Some(3.0),
            band_low: None,
            band_high: None,
            clamp_max: None,
        };
        // sqrt(4) / 3 = 2/3
        assert!((apply_transform(4.0, &signal) - 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn transform_inverse() {
        let signal = SignalDef {
            source: String::new(),
            blend: 1.0,
            transform: SignalTransform::Inverse,
            saturation: None,
            band_low: None,
            band_high: None,
            clamp_max: None,
        };
        assert!((apply_transform(0.2, &signal) - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn transform_band() {
        let signal = SignalDef {
            source: String::new(),
            blend: 1.0,
            transform: SignalTransform::Band,
            saturation: None,
            band_low: Some(0.1),
            band_high: Some(0.95),
            clamp_max: None,
        };
        // In band → 1.0
        assert!((apply_transform(0.5, &signal) - 1.0).abs() < f64::EPSILON);
        // Below band → linear
        assert!((apply_transform(0.05, &signal) - 0.5).abs() < f64::EPSILON);
        // Above band → linear penalty
        assert!((apply_transform(0.975, &signal) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn transform_clamp_saturate() {
        let signal = SignalDef {
            source: String::new(),
            blend: 1.0,
            transform: SignalTransform::ClampSaturate,
            saturation: Some(1_000_000_000.0),
            band_low: None,
            band_high: None,
            clamp_max: Some(2.0),
        };
        // 1B / 1B = 1.0, clamp(0, 2) / 2 = 0.5
        assert!((apply_transform(1_000_000_000.0, &signal) - 0.5).abs() < f64::EPSILON);
        // 3B / 1B = 3.0, clamp(0, 2) / 2 = 1.0
        assert!((apply_transform(3_000_000_000.0, &signal) - 1.0).abs() < f64::EPSILON);
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
        let content = scored_content();
        let metrics = make_metrics(100);

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
        let content = scored_content();
        let metrics = make_metrics(100);

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

        let mut metrics_all_active = make_metrics(100);
        metrics_all_active.satellites_active = 4;
        metrics_all_active.satellites_failed = 0;
        let score_healthy = compute_run_score(&metrics_all_active, &state, &content);

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
