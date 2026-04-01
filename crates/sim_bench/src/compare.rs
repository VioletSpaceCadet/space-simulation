use crate::overrides;
use crate::runner;
use crate::scenario;
use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ComparisonReport {
    pub scenario_name: String,
    pub config_a_path: String,
    pub config_b_path: String,
    pub seed_count: usize,
    pub ticks: u64,
    pub composite_delta: DeltaSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composite_t_test: Option<TTestResult>,
    pub dimension_deltas: Vec<DimensionDelta>,
    pub per_seed: Vec<SeedComparison>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeltaSummary {
    pub mean: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TTestResult {
    pub t_statistic: f64,
    pub degrees_of_freedom: usize,
    pub significant_at_05: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DimensionDelta {
    pub dimension_id: String,
    pub dimension_name: String,
    pub delta: DeltaSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeedComparison {
    pub seed: u64,
    pub composite_a: f64,
    pub composite_b: f64,
    pub composite_delta: f64,
    pub threshold_a: String,
    pub threshold_b: String,
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

fn delta_summary(values: &[f64]) -> DeltaSummary {
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    let stddev = variance.sqrt();
    DeltaSummary {
        mean,
        stddev,
        min,
        max,
    }
}

/// Paired t-test on the deltas (B - A) for each seed.
/// Returns None if fewer than 2 samples.
fn paired_t_test(deltas: &[f64]) -> Option<TTestResult> {
    let sample_count = deltas.len();
    if sample_count < 2 {
        return None;
    }
    let count = sample_count as f64;
    let mean = deltas.iter().sum::<f64>() / count;
    // Sample standard deviation (Bessel's correction)
    let sample_variance = deltas.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (count - 1.0);
    let sample_stddev = sample_variance.sqrt();

    if sample_stddev < 1e-15 {
        // All deltas identical — t is infinite (or 0/0 if mean is also 0)
        return Some(TTestResult {
            t_statistic: if mean.abs() < 1e-15 {
                0.0
            } else {
                f64::INFINITY
            },
            degrees_of_freedom: sample_count - 1,
            significant_at_05: mean.abs() > 1e-15,
        });
    }

    let t_statistic = mean / (sample_stddev / count.sqrt());
    let degrees_of_freedom = sample_count - 1;

    // Compare |t| against two-tailed 0.05 critical values.
    let significant = t_statistic.abs() > t_critical_05(degrees_of_freedom);

    Some(TTestResult {
        t_statistic,
        degrees_of_freedom,
        significant_at_05: significant,
    })
}

/// Two-tailed t critical value at alpha=0.05 for given degrees of freedom.
/// Uses a lookup table for df 1..30, then the normal approximation (1.96) for df > 30.
fn t_critical_05(degrees_of_freedom: usize) -> f64 {
    // Standard two-tailed 0.05 critical values from t-distribution tables.
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, // df 1-5
        2.447, 2.365, 2.306, 2.262, 2.228, // df 6-10
        2.201, 2.179, 2.160, 2.145, 2.131, // df 11-15
        2.120, 2.110, 2.101, 2.093, 2.086, // df 16-20
        2.080, 2.074, 2.069, 2.064, 2.060, // df 21-25
        2.056, 2.052, 2.048, 2.045, 2.042, // df 26-30
    ];
    if degrees_of_freedom == 0 {
        return f64::INFINITY;
    }
    if degrees_of_freedom <= 30 {
        TABLE[degrees_of_freedom - 1]
    } else {
        1.96 // normal approximation
    }
}

// ---------------------------------------------------------------------------
// Report building
// ---------------------------------------------------------------------------

fn build_comparison_report(
    scenario_name: &str,
    config_a_path: &str,
    config_b_path: &str,
    ticks: u64,
    results_a: &[runner::SeedResult],
    results_b: &[runner::SeedResult],
) -> ComparisonReport {
    // Pair results by seed
    let map_b: HashMap<u64, &runner::SeedResult> = results_b.iter().map(|r| (r.seed, r)).collect();

    let mut per_seed = Vec::new();
    let mut composite_deltas = Vec::new();
    // dimension_id → vec of deltas
    let mut dim_deltas: HashMap<String, Vec<f64>> = HashMap::new();
    let mut dim_names: HashMap<String, String> = HashMap::new();

    for baseline_result in results_a {
        let Some(variant_result) = map_b.get(&baseline_result.seed) else {
            continue;
        };
        let delta = variant_result.final_score.composite - baseline_result.final_score.composite;
        composite_deltas.push(delta);

        per_seed.push(SeedComparison {
            seed: baseline_result.seed,
            composite_a: baseline_result.final_score.composite,
            composite_b: variant_result.final_score.composite,
            composite_delta: delta,
            threshold_a: baseline_result.final_score.threshold.clone(),
            threshold_b: variant_result.final_score.threshold.clone(),
        });

        for (dim_id, score_a) in &baseline_result.final_score.dimensions {
            let score_b_normalized = variant_result
                .final_score
                .dimensions
                .get(dim_id)
                .map_or(0.0, |d| d.normalized);
            let dim_delta = score_b_normalized - score_a.normalized;
            dim_deltas
                .entry(dim_id.clone())
                .or_default()
                .push(dim_delta);
            dim_names
                .entry(dim_id.clone())
                .or_insert_with(|| score_a.name.clone());
        }
    }

    per_seed.sort_by_key(|s| s.seed);

    let composite_delta = delta_summary(&composite_deltas);
    let composite_t_test = paired_t_test(&composite_deltas);

    let mut dimension_deltas: Vec<DimensionDelta> = dim_deltas
        .iter()
        .map(|(dim_id, deltas)| DimensionDelta {
            dimension_id: dim_id.clone(),
            dimension_name: dim_names.get(dim_id).cloned().unwrap_or_default(),
            delta: delta_summary(deltas),
        })
        .collect();
    dimension_deltas.sort_by(|a, b| a.dimension_id.cmp(&b.dimension_id));

    ComparisonReport {
        scenario_name: scenario_name.to_string(),
        config_a_path: config_a_path.to_string(),
        config_b_path: config_b_path.to_string(),
        seed_count: per_seed.len(),
        ticks,
        composite_delta,
        composite_t_test,
        dimension_deltas,
        per_seed,
    }
}

fn print_comparison_summary(report: &ComparisonReport) {
    println!(
        "\n=== Comparison: {} ({} seeds, {} ticks) ===",
        report.scenario_name, report.seed_count, report.ticks
    );
    println!("  Config A: {}", report.config_a_path);
    println!("  Config B: {}", report.config_b_path);
    println!(
        "\nComposite delta (B - A): {:+.2} (stddev {:.2}, range [{:.2}, {:+.2}])",
        report.composite_delta.mean,
        report.composite_delta.stddev,
        report.composite_delta.min,
        report.composite_delta.max,
    );
    if let Some(ref t_test) = report.composite_t_test {
        let sig = if t_test.significant_at_05 {
            "SIGNIFICANT"
        } else {
            "not significant"
        };
        println!(
            "  t={:.3}, df={}, {} at p<0.05",
            t_test.t_statistic, t_test.degrees_of_freedom, sig
        );
    }
    println!(
        "\n{:<25} {:>10} {:>10}",
        "Dimension", "Mean Delta", "StdDev"
    );
    println!("{}", "-".repeat(50));
    for dim in &report.dimension_deltas {
        println!(
            "{:<25} {:>+10.4} {:>10.4}",
            dim.dimension_name, dim.delta.mean, dim.delta.stddev
        );
    }
    println!(
        "\n{:<8} {:>12} {:>12} {:>10} {:>14} {:>14}",
        "Seed", "Composite A", "Composite B", "Delta", "Threshold A", "Threshold B"
    );
    println!("{}", "-".repeat(75));
    for seed in &report.per_seed {
        println!(
            "{:<8} {:>12.2} {:>12.2} {:>+10.2} {:>14} {:>14}",
            seed.seed,
            seed.composite_a,
            seed.composite_b,
            seed.composite_delta,
            seed.threshold_a,
            seed.threshold_b,
        );
    }
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

struct ArmConfig<'a> {
    label: &'a str,
    content: &'a sim_core::GameContent,
    seeds: &'a [u64],
    scenario_name: &'a str,
    scenario_params: &'a serde_json::Value,
    base_state: Option<&'a sim_core::GameState>,
    ticks: u64,
    metrics_every: u64,
}

fn run_arm(config: &ArmConfig<'_>, arm_dir: &Path) -> Result<Vec<runner::SeedResult>> {
    println!(
        "Running arm {} ({} seeds)...",
        config.label,
        config.seeds.len()
    );
    let results: Vec<Result<runner::SeedResult>> = config
        .seeds
        .par_iter()
        .map(|&seed| {
            let seed_dir = arm_dir.join(format!("seed_{seed}"));
            runner::run_seed(
                config.content,
                seed,
                config.ticks,
                config.metrics_every,
                &seed_dir,
                config.scenario_name,
                config.scenario_params,
                config.base_state,
            )
        })
        .collect();

    let mut seed_results = Vec::new();
    for result in results {
        match result {
            Ok(seed_result) => seed_results.push(seed_result),
            Err(err) => eprintln!("  Seed failed ({}): {err:#}", config.label),
        }
    }
    if seed_results.is_empty() {
        anyhow::bail!("all seeds failed for arm {}", config.label);
    }
    println!(
        "  Arm {}: {}/{} seeds completed",
        config.label,
        seed_results.len(),
        config.seeds.len()
    );
    Ok(seed_results)
}

fn load_autopilot_config(path: &str) -> Result<sim_core::AutopilotConfig> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("reading autopilot config: {path}"))?;
    serde_json::from_str(&json).with_context(|| format!("parsing autopilot config: {path}"))
}

fn load_base_content(scenario: &scenario::Scenario) -> Result<sim_core::GameContent> {
    let mut content = sim_world::load_content(&scenario.content_dir)?;
    let non_autopilot_overrides: HashMap<String, serde_json::Value> = scenario
        .overrides
        .iter()
        .filter(|(key, _)| !key.starts_with("autopilot."))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    overrides::apply_overrides(&mut content, &non_autopilot_overrides)?;
    content.constants.derive_tick_values();
    sim_core::derive_module_tick_values(&mut content.module_defs, &content.constants);
    Ok(content)
}

fn write_report_atomic(report: &ComparisonReport, path: &Path) -> Result<()> {
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(report).context("serializing comparison report")?;
    let mut file = std::fs::File::create(&tmp_path)
        .with_context(|| format!("creating {}", tmp_path.display()))?;
    file.write_all(json.as_bytes())
        .context("writing comparison report")?;
    file.sync_all()?;
    std::fs::rename(&tmp_path, path).context("renaming comparison report")
}

pub fn run_compare(
    scenario_path: &str,
    config_a_path: &str,
    config_b_path: &str,
    output_dir: &str,
) -> Result<()> {
    let scenario = scenario::load_scenario(Path::new(scenario_path))?;
    let seeds = scenario.seeds.expand();

    println!(
        "Compare: '{}' | {} seeds x {} ticks | A={} B={}",
        scenario.name,
        seeds.len(),
        scenario.ticks,
        config_a_path,
        config_b_path
    );

    let base_content = load_base_content(&scenario)?;

    let mut content_a = base_content.clone();
    content_a.autopilot = load_autopilot_config(config_a_path)?;
    let mut content_b = base_content;
    content_b.autopilot = load_autopilot_config(config_b_path)?;

    let base_state = if let Some(ref state_path) = scenario.state {
        let json = std::fs::read_to_string(state_path)
            .with_context(|| format!("reading state file: {state_path}"))?;
        let mut loaded: sim_core::GameState = serde_json::from_str(&json)
            .with_context(|| format!("parsing state file: {state_path}"))?;
        loaded.body_cache = sim_core::build_body_cache(&content_a.solar_system.bodies);
        Some(loaded)
    } else {
        None
    };

    let scenario_params = serde_json::json!({
        "ticks": scenario.ticks,
        "metrics_every": scenario.metrics_every,
    });

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let run_dir =
        PathBuf::from(output_dir).join(format!("compare_{}_{}", scenario.name, timestamp));
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("creating output directory: {}", run_dir.display()))?;

    let arm_cfg = |label, content| ArmConfig {
        label,
        content,
        seeds: &seeds,
        scenario_name: &scenario.name,
        scenario_params: &scenario_params,
        base_state: base_state.as_ref(),
        ticks: scenario.ticks,
        metrics_every: scenario.metrics_every,
    };

    let results_a = run_arm(&arm_cfg("A", &content_a), &run_dir.join("arm_a"))?;
    let results_b = run_arm(&arm_cfg("B", &content_b), &run_dir.join("arm_b"))?;

    let report = build_comparison_report(
        &scenario.name,
        config_a_path,
        config_b_path,
        scenario.ticks,
        &results_a,
        &results_b,
    );

    print_comparison_summary(&report);

    let report_path = run_dir.join("comparison_report.json");
    write_report_atomic(&report, &report_path)?;
    println!("\nReport: {}", report_path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_summary_basic() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let summary = delta_summary(&values);
        assert!((summary.mean - 3.0).abs() < 1e-10);
        assert!((summary.min - 1.0).abs() < 1e-10);
        assert!((summary.max - 5.0).abs() < 1e-10);
        assert!(summary.stddev > 0.0);
    }

    #[test]
    fn delta_summary_identical() {
        let values = vec![5.0, 5.0, 5.0];
        let summary = delta_summary(&values);
        assert!((summary.mean - 5.0).abs() < 1e-10);
        assert!(summary.stddev.abs() < 1e-10);
    }

    #[test]
    fn t_test_too_few_samples() {
        assert!(paired_t_test(&[]).is_none());
        assert!(paired_t_test(&[1.0]).is_none());
    }

    #[test]
    fn t_test_identical_zero_deltas() {
        let deltas = vec![0.0, 0.0, 0.0, 0.0, 0.0];
        let result = paired_t_test(&deltas).unwrap();
        assert!((result.t_statistic).abs() < 1e-10);
        assert!(!result.significant_at_05);
    }

    #[test]
    fn t_test_large_effect() {
        // 10 seeds, all showing positive delta of ~100 with small variance
        let deltas = vec![
            100.0, 101.0, 99.0, 100.5, 100.2, 99.8, 100.1, 99.9, 100.3, 99.7,
        ];
        let result = paired_t_test(&deltas).unwrap();
        assert!(result.t_statistic > 0.0);
        assert!(result.significant_at_05);
    }

    #[test]
    fn t_test_no_effect_high_variance() {
        // Deltas centered on 0 with high variance — should not be significant
        let deltas = vec![50.0, -50.0, 30.0, -30.0, 10.0, -10.0];
        let result = paired_t_test(&deltas).unwrap();
        assert!((result.t_statistic).abs() < 1.0);
        assert!(!result.significant_at_05);
    }

    #[test]
    fn t_critical_values_known() {
        assert!((t_critical_05(1) - 12.706).abs() < 0.001);
        assert!((t_critical_05(10) - 2.228).abs() < 0.001);
        assert!((t_critical_05(30) - 2.042).abs() < 0.001);
        assert!((t_critical_05(100) - 1.96).abs() < 0.001);
    }

    #[test]
    fn comparison_report_from_seed_results() {
        use sim_core::{DimensionScore, RunScore};
        use std::collections::BTreeMap;

        let make_score = |composite: f64, threshold: &str| -> RunScore {
            let mut dimensions = BTreeMap::new();
            dimensions.insert(
                "industrial_output".to_string(),
                DimensionScore {
                    id: "industrial_output".to_string(),
                    name: "Industrial Output".to_string(),
                    raw_value: composite / 10.0,
                    normalized: composite / 2500.0,
                    weighted: composite * 0.25,
                },
            );
            RunScore {
                dimensions,
                composite,
                threshold: threshold.to_string(),
                tick: 100,
            }
        };

        use rand::SeedableRng;
        let content = sim_world::load_content("../../content").unwrap();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0);
        let state = sim_world::build_initial_state(&content, 0, &mut rng);
        let snapshot = sim_core::compute_metrics(&state, &content);

        let results_a = vec![
            runner::SeedResult {
                seed: 1,
                final_snapshot: snapshot.clone(),
                final_score: make_score(300.0, "Contractor"),
                wall_time_ms: 10,
                run_id: "a1".to_string(),
            },
            runner::SeedResult {
                seed: 2,
                final_snapshot: snapshot.clone(),
                final_score: make_score(400.0, "Contractor"),
                wall_time_ms: 10,
                run_id: "a2".to_string(),
            },
        ];
        let results_b = vec![
            runner::SeedResult {
                seed: 1,
                final_snapshot: snapshot.clone(),
                final_score: make_score(350.0, "Contractor"),
                wall_time_ms: 10,
                run_id: "b1".to_string(),
            },
            runner::SeedResult {
                seed: 2,
                final_snapshot: snapshot,
                final_score: make_score(500.0, "Enterprise"),
                wall_time_ms: 10,
                run_id: "b2".to_string(),
            },
        ];

        let report = build_comparison_report(
            "test",
            "config_a.json",
            "config_b.json",
            100,
            &results_a,
            &results_b,
        );

        assert_eq!(report.seed_count, 2);
        // Delta: seed1 = 50, seed2 = 100 → mean = 75
        assert!((report.composite_delta.mean - 75.0).abs() < 1e-6);
        assert_eq!(report.per_seed.len(), 2);
        assert_eq!(report.per_seed[0].seed, 1);
        assert!((report.per_seed[0].composite_delta - 50.0).abs() < 1e-6);
        assert_eq!(report.per_seed[1].threshold_b, "Enterprise");
        assert_eq!(report.dimension_deltas.len(), 1);
    }
}
