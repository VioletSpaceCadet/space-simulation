use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

mod overrides;
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
    overrides::apply_overrides(&mut content.constants, &scenario.overrides)?;

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

    // Write summary.json
    let summary_path = run_dir.join("summary.json");
    let summary_json = serde_json::to_string_pretty(&stats).context("serializing summary")?;
    std::fs::write(&summary_path, summary_json)
        .with_context(|| format!("writing {}", summary_path.display()))?;

    println!("\nSummary written to {}", summary_path.display());
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
