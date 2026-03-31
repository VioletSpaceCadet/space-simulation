//! Run scoring types and configuration.
//!
//! `ScoringConfig` is loaded from `content/scoring.json` as part of `GameContent`.
//! `RunScore` is the output of `compute_run_score()` (implemented in VIO-521).
//! All types are pure data — no computation logic in this module yet.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Content configuration (loaded from scoring.json)
// ---------------------------------------------------------------------------

/// A single scoring dimension definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionDef {
    /// Unique identifier (e.g., "industrial_output").
    pub id: String,
    /// Display name (e.g., "Industrial Output").
    pub name: String,
    /// Weight in composite score (all weights must sum to 1.0).
    pub weight: f64,
    /// Normalization ceiling — the raw value at which the dimension scores 1.0.
    pub ceiling: f64,
}

/// A named score threshold (e.g., "Enterprise" at 500 points).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdDef {
    /// Display name (e.g., "Enterprise").
    pub name: String,
    /// Minimum composite score to enter this threshold.
    pub min_score: f64,
}

/// Scoring configuration loaded from `content/scoring.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    let weight_sum: f64 = config.dimensions.iter().map(|d| d.weight).sum();
    if (weight_sum - 1.0).abs() > 1e-6 {
        return Err(format!(
            "dimension weights must sum to 1.0, got {weight_sum:.6}"
        ));
    }

    for dim in &config.dimensions {
        if dim.ceiling <= 0.0 {
            return Err(format!(
                "dimension '{}' has non-positive ceiling {}",
                dim.id, dim.ceiling
            ));
        }
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

    Ok(())
}

// ---------------------------------------------------------------------------
// Score output
// ---------------------------------------------------------------------------

/// Score for a single dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunScore {
    /// Per-dimension breakdown, keyed by dimension id.
    pub dimensions: BTreeMap<String, DimensionScore>,
    /// Composite score (sum of all weighted contributions).
    pub composite: f64,
    /// Named threshold (e.g., "Enterprise"). The highest threshold whose
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
            threshold: "Startup".to_string(),
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
}
