use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sim_control::{AutopilotController, CommandSource};
use sim_core::{EventLevel, GameState};
use sim_world::{
    create_run_dir, generate_run_id, load_content, load_or_build_state, write_run_info,
};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "sim_cli", about = "Space Industry Sim CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the simulation for a fixed number of ticks.
    Run {
        #[arg(long)]
        ticks: u64,
        /// Generate world procedurally with this seed. Mutually exclusive with --state.
        #[arg(long, conflicts_with = "state_file")]
        seed: Option<u64>,
        /// Load initial GameState from a JSON file. Mutually exclusive with --seed.
        #[arg(long = "state", conflicts_with = "seed")]
        state_file: Option<String>,
        #[arg(long, default_value = "./content")]
        content_dir: String,
        #[arg(long, default_value_t = 100)]
        print_every: u64,
        #[arg(long, default_value = "normal", value_parser = ["normal", "debug"])]
        event_level: String,
        /// Sample metrics every N ticks (default 60).
        #[arg(long, default_value_t = 60)]
        metrics_every: u64,
        /// Disable automatic metrics collection to runs/ directory.
        #[arg(long)]
        no_metrics: bool,
    },
}

// ---------------------------------------------------------------------------
// Run loop
// ---------------------------------------------------------------------------

fn run(
    ticks: u64,
    seed: Option<u64>,
    state_file: Option<String>,
    content_dir: &str,
    print_every: u64,
    event_level: EventLevel,
    metrics_every: u64,
    no_metrics: bool,
) -> Result<()> {
    let content = load_content(content_dir)?;

    let (mut state, mut rng) = load_or_build_state(&content, seed, state_file.as_deref())?;

    // Set up per-run metrics directory.
    let mut metrics_writer: Option<sim_core::MetricsFileWriter> = None;
    if !no_metrics {
        let run_id = generate_run_id(state.meta.seed);
        let run_dir = create_run_dir(&run_id)?;
        write_run_info(
            &run_dir,
            &run_id,
            state.meta.seed,
            &content.content_version,
            metrics_every,
            serde_json::json!({
                "runner": "sim_cli",
                "ticks": ticks,
                "print_every": print_every,
            }),
        )?;
        let writer = sim_core::MetricsFileWriter::new(run_dir.clone())
            .with_context(|| format!("opening metrics CSV in {}", run_dir.display()))?;
        metrics_writer = Some(writer);
        println!("Run directory: {}", run_dir.display());
    }

    let mut autopilot = AutopilotController;
    let mut next_command_id = 0u64;

    println!(
        "Starting simulation: ticks={ticks} seed={} sites={} content_version={}",
        state.meta.seed,
        state.scan_sites.len(),
        content.content_version,
    );
    println!("{}", "-".repeat(80));

    for _ in 0..ticks {
        let commands = autopilot.generate_commands(&state, &content, &mut next_command_id);

        let events = sim_core::tick(&mut state, &commands, &content, &mut rng, event_level);

        // Print notable events regardless of print_every.
        for event in &events {
            if let sim_core::Event::TechUnlocked { tech_id } = &event.event {
                println!(
                    "*** TECH UNLOCKED: {tech_id} at tick={:04} ***",
                    state.meta.tick
                );
            }
        }

        if state.meta.tick % print_every == 0 {
            print_status(&state);
        }

        if let Some(ref mut writer) = metrics_writer {
            if state.meta.tick % metrics_every == 0 {
                let snapshot = sim_core::compute_metrics(&state, &content);
                writer.write_row(&snapshot).context("writing metrics row")?;
            }
        }
    }

    println!("{}", "-".repeat(80));
    println!("Done. Final state at tick {}:", state.meta.tick);
    print_status(&state);

    if let Some(ref mut writer) = metrics_writer {
        writer.flush().context("final metrics flush")?;
        println!("Metrics written to runs/ directory.");
    }

    Ok(())
}

fn print_status(state: &GameState) {
    let tick = state.meta.tick;
    let day = tick / 1440;
    let hour = (tick % 1440) / 60;

    let unlocked: Vec<String> = state
        .research
        .unlocked
        .iter()
        .map(|t| t.0.clone())
        .collect();
    let unlocked_str = if unlocked.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}]", unlocked.join(", "))
    };

    let scan_data = state
        .research
        .data_pool
        .get(&sim_core::DataKind::ScanData)
        .copied()
        .unwrap_or(0.0);

    let tech_id = sim_core::TechId("tech_deep_scan_v1".to_string());
    let evidence: f32 = state
        .research
        .evidence
        .get(&tech_id)
        .map_or(0.0, |dp| dp.points.values().sum());

    println!(
        "[tick={tick:04}  day={day}  hour={hour:02}]  \
         sites={sites:3}  asteroids={asteroids:3}  \
         unlocked={unlocked_str}  data_pool={scan_data:.1}  evidence={evidence:.1}",
        sites = state.scan_sites.len(),
        asteroids = state.asteroids.len(),
    );
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            ticks,
            seed,
            state_file,
            content_dir,
            print_every,
            event_level,
            metrics_every,
            no_metrics,
        } => {
            let level = match event_level.as_str() {
                "debug" => EventLevel::Debug,
                _ => EventLevel::Normal,
            };
            run(
                ticks,
                seed,
                state_file,
                &content_dir,
                print_every,
                level,
                metrics_every,
                no_metrics,
            )?;
        }
    }
    Ok(())
}
