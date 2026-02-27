use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rayon::prelude::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

mod overrides;
mod run_result;
mod runner;
mod scenario;
mod summary;

#[derive(Parser)]
#[command(
    name = "sim_bench",
    about = "Automated scenario runner for sim benchmarking"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a scenario file across multiple seeds.
    Run {
        /// Path to the scenario JSON file.
        #[arg(long)]
        scenario: String,
        /// Output directory (default: runs/).
        #[arg(long, default_value = "runs")]
        output_dir: String,
    },
}

#[allow(clippy::too_many_lines)]
fn run(scenario_path: &str, output_dir: &str) -> Result<()> {
    let scenario = scenario::load_scenario(Path::new(scenario_path))?;
    let seeds = scenario.seeds.expand();

    println!(
        "Loading scenario '{}': {} seeds × {} ticks",
        scenario.name,
        seeds.len(),
        scenario.ticks
    );

    // Load content and apply overrides.
    let mut content = sim_world::load_content(&scenario.content_dir)?;
    overrides::apply_overrides(&mut content, &scenario.overrides)?;
    // Re-derive tick values after overrides may have changed game-time fields.
    content.constants.derive_tick_values();

    // Load base state file if specified.
    let base_state = if let Some(ref state_path) = scenario.state {
        let json = std::fs::read_to_string(state_path)
            .with_context(|| format!("reading state file: {state_path}"))?;
        let loaded: sim_core::GameState = serde_json::from_str(&json)
            .with_context(|| format!("parsing state file: {state_path}"))?;
        println!("Using state file: {state_path}");
        Some(loaded)
    } else {
        None
    };

    // Build scenario_params for run_result metadata.
    let scenario_params = serde_json::json!({
        "ticks": scenario.ticks,
        "metrics_every": scenario.metrics_every,
        "content_dir": scenario.content_dir,
        "state": scenario.state,
        "overrides": scenario.overrides,
    });

    // Create timestamped output directory.
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let run_dir = PathBuf::from(output_dir).join(format!("{}_{}", scenario.name, timestamp));
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("creating output directory: {}", run_dir.display()))?;

    // Copy scenario file into output dir.
    std::fs::copy(scenario_path, run_dir.join("scenario.json")).context("copying scenario file")?;

    println!("Output: {}", run_dir.display());
    println!("Running {} seeds in parallel...", seeds.len());

    // Run all seeds in parallel.
    let results: Vec<Result<runner::SeedResult>> = seeds
        .par_iter()
        .map(|&seed| {
            let seed_dir = run_dir.join(format!("seed_{seed}"));
            runner::run_seed(
                &content,
                seed,
                scenario.ticks,
                scenario.metrics_every,
                &seed_dir,
                &scenario.name,
                &scenario_params,
                base_state.as_ref(),
            )
        })
        .collect();

    // Collect results, reporting any failures.
    let mut seed_results = Vec::new();
    for result in results {
        match result {
            Ok(seed_result) => seed_results.push(seed_result),
            Err(err) => eprintln!("Seed failed: {err:#}"),
        }
    }

    if seed_results.is_empty() {
        anyhow::bail!("all seeds failed");
    }

    // Compute and print summary.
    let snapshot_refs: Vec<(u64, &sim_core::MetricsSnapshot)> = seed_results
        .iter()
        .map(|r| (r.seed, &r.final_snapshot))
        .collect();

    let stats = summary::compute_summary(&snapshot_refs);
    summary::print_summary(&scenario.name, scenario.ticks, &stats);

    // Write summary.json (legacy format, backward compat)
    let summary_path = run_dir.join("summary.json");
    let summary_json = serde_json::to_string_pretty(&stats).context("serializing summary")?;
    std::fs::write(&summary_path, summary_json)
        .with_context(|| format!("writing {}", summary_path.display()))?;

    // Write batch_summary.json (contract v1 format)
    let batch_id = Uuid::new_v4().to_string();
    let run_ids: Vec<&str> = seed_results.iter().map(|r| r.run_id.as_str()).collect();
    let collapsed_count = seed_results
        .iter()
        .filter(|r| {
            let (collapsed, _) = run_result::detect_collapse(&r.final_snapshot);
            collapsed
        })
        .count();

    let snapshot_only_refs: Vec<&sim_core::MetricsSnapshot> =
        seed_results.iter().map(|r| &r.final_snapshot).collect();
    let aggregated_metrics = summary::build_aggregated_metrics(&snapshot_only_refs);

    let batch_summary = serde_json::json!({
        "batch_schema_version": 1,
        "batch_id": batch_id,
        "scenario_name": scenario.name,
        "scenario_params": scenario_params,
        "seed_count": seed_results.len(),
        "run_ids": run_ids,
        "collapsed_count": collapsed_count,
        "aggregated_metrics": aggregated_metrics,
    });

    let batch_path = run_dir.join("batch_summary.json");
    let batch_tmp = batch_path.with_extension("json.tmp");
    let batch_json =
        serde_json::to_string_pretty(&batch_summary).context("serializing batch summary")?;
    let mut batch_file = std::fs::File::create(&batch_tmp)
        .with_context(|| format!("creating {}", batch_tmp.display()))?;
    batch_file
        .write_all(batch_json.as_bytes())
        .context("writing batch summary")?;
    batch_file.sync_all()?;
    std::fs::rename(&batch_tmp, &batch_path).context("renaming batch summary")?;

    println!("Summary written to {}", summary_path.display());
    println!("Batch summary written to {}", batch_path.display());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            scenario,
            output_dir,
        } => run(&scenario, &output_dir)?,
    }
    Ok(())
}
