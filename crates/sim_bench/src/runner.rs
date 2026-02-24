use crate::run_result::{self, RunResult, SummaryMetrics};
use anyhow::{Context, Result};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::{AutopilotController, CommandSource};
use sim_core::{EventLevel, GameContent, MetricsSnapshot};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use uuid::Uuid;

pub struct SeedResult {
    pub seed: u64,
    pub final_snapshot: MetricsSnapshot,
    #[allow(dead_code)]
    pub wall_time_ms: u64,
    pub run_id: String,
}

pub fn run_seed(
    content: &GameContent,
    seed: u64,
    ticks: u64,
    metrics_every: u64,
    seed_dir: &Path,
    scenario_name: &str,
    scenario_params: &serde_json::Value,
) -> Result<SeedResult> {
    let run_id = Uuid::new_v4().to_string();
    let start = Instant::now();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut state = sim_world::build_initial_state(content, seed, &mut rng);
    let mut autopilot = AutopilotController;
    let mut next_command_id = 0u64;

    std::fs::create_dir_all(seed_dir)
        .with_context(|| format!("creating seed directory: {}", seed_dir.display()))?;

    // Write run_info.json
    sim_world::write_run_info(
        seed_dir,
        &format!("seed_{seed}"),
        seed,
        &content.content_version,
        metrics_every,
        serde_json::json!({
            "runner": "sim_bench",
            "ticks": ticks,
        }),
    )?;

    let mut metrics_writer = sim_core::MetricsFileWriter::new(seed_dir.to_path_buf())
        .with_context(|| format!("opening metrics CSV in {}", seed_dir.display()))?;

    for _ in 0..ticks {
        let commands = autopilot.generate_commands(&state, content, &mut next_command_id);
        sim_core::tick(&mut state, &commands, content, &mut rng, EventLevel::Normal);

        if state.meta.tick % metrics_every == 0 {
            let snapshot = sim_core::compute_metrics(&state, content);
            metrics_writer
                .write_row(&snapshot)
                .context("writing metrics row")?;
        }
    }

    // Always capture final snapshot
    let final_snapshot = sim_core::compute_metrics(&state, content);
    if state.meta.tick % metrics_every != 0 {
        metrics_writer
            .write_row(&final_snapshot)
            .context("writing final metrics row")?;
    }
    metrics_writer.flush().context("flushing metrics")?;

    #[allow(clippy::cast_possible_truncation)]
    let wall_time_ms = start.elapsed().as_millis() as u64;
    let sim_ticks_per_second = if wall_time_ms > 0 {
        (ticks as f64) / (wall_time_ms as f64 / 1000.0)
    } else {
        0.0
    };

    let (collapse_occurred, collapse_reason) = run_result::detect_collapse(&final_snapshot);

    let run_result = RunResult {
        run_schema_version: 1,
        run_status: "completed".to_string(),
        run_id: run_id.clone(),
        git_sha: run_result::git_sha(),
        git_dirty: run_result::git_dirty(),
        seed,
        scenario_name: scenario_name.to_string(),
        scenario_params: scenario_params.clone(),
        tick_start: 0,
        tick_end: final_snapshot.tick,
        total_ticks: ticks,
        wall_time_ms,
        sim_ticks_per_second,
        summary_metrics: Some(SummaryMetrics::from_snapshot(&final_snapshot)),
        alert_counts_by_type: HashMap::new(),
        alert_first_tick_by_type: HashMap::new(),
        alert_last_tick_by_type: HashMap::new(),
        collapse_occurred,
        collapse_tick: if collapse_occurred {
            Some(final_snapshot.tick)
        } else {
            None
        },
        collapse_reason,
        metrics_path: "metrics_000.csv".to_string(),
        alerts_path: None,
        events_path: None,
        error_message: None,
    };

    run_result
        .write_atomic(&seed_dir.join("run_result.json"))
        .context("writing run_result.json")?;

    Ok(SeedResult {
        seed,
        final_snapshot,
        wall_time_ms,
        run_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_run_seed_produces_output() {
        let content = sim_world::load_content("../../content").unwrap();
        let temp_dir = TempDir::new().unwrap();
        let seed_dir = temp_dir.path().join("seed_42");
        let params = serde_json::json!({"ticks": 120});

        let result = run_seed(&content, 42, 120, 60, &seed_dir, "test_scenario", &params).unwrap();

        assert_eq!(result.seed, 42);
        assert_eq!(result.final_snapshot.tick, 120);
        assert!(result.wall_time_ms > 0 || result.wall_time_ms == 0); // just exists
        assert!(!result.run_id.is_empty());
        assert!(seed_dir.join("run_info.json").exists());
        assert!(seed_dir.join("metrics_000.csv").exists());
        assert!(seed_dir.join("run_result.json").exists());

        // Verify run_result.json content
        let content_str = std::fs::read_to_string(seed_dir.join("run_result.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content_str).unwrap();
        assert_eq!(parsed["run_schema_version"], 1);
        assert_eq!(parsed["run_status"], "completed");
        assert_eq!(parsed["seed"], 42);
        assert!(parsed["summary_metrics"].is_object());
    }

    #[test]
    fn test_run_seed_determinism() {
        let content = sim_world::load_content("../../content").unwrap();
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        let params = serde_json::json!({"ticks": 120});

        let result1 = run_seed(
            &content,
            42,
            120,
            60,
            &dir1.path().join("seed_42"),
            "test",
            &params,
        )
        .unwrap();
        let result2 = run_seed(
            &content,
            42,
            120,
            60,
            &dir2.path().join("seed_42"),
            "test",
            &params,
        )
        .unwrap();

        assert_eq!(result1.final_snapshot.tick, result2.final_snapshot.tick);
        assert_eq!(
            result1.final_snapshot.techs_unlocked,
            result2.final_snapshot.techs_unlocked
        );
        assert_eq!(
            result1.final_snapshot.fleet_total,
            result2.final_snapshot.fleet_total
        );
    }
}
