use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::{AutopilotController, CommandSource};
use sim_core::{EventLevel, GameState};
use sim_world::{build_initial_state, load_content};

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

fn generate_run_id(seed: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Manual UTC time formatting to avoid adding chrono dependency.
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch → year/month/day (simplified Gregorian).
    let (year, month, day) = epoch_days_to_date(days);

    format!("{year:04}{month:02}{day:02}_{hours:02}{minutes:02}{seconds:02}_seed{seed}")
}

fn epoch_days_to_date(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    days += 719_468;
    let era = days / 146_097;
    let day_of_era = days % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

fn create_run_dir(run_id: &str) -> Result<std::path::PathBuf> {
    let dir = std::path::PathBuf::from("runs").join(run_id);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating run directory: {}", dir.display()))?;
    Ok(dir)
}

fn write_run_info(
    dir: &std::path::Path,
    run_id: &str,
    seed: u64,
    ticks: u64,
    content_version: &str,
    metrics_every: u64,
    print_every: u64,
) -> Result<()> {
    let info = serde_json::json!({
        "run_id": run_id,
        "seed": seed,
        "start_time": run_id.split('_').take(2).collect::<Vec<_>>().join("_"),
        "content_version": content_version,
        "metrics_every": metrics_every,
        "runner": "sim_cli",
        "args": {
            "ticks": ticks,
            "print_every": print_every,
        }
    });
    let path = dir.join("run_info.json");
    let file =
        std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    serde_json::to_writer_pretty(file, &info)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

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

    let (mut state, mut rng) = if let Some(path) = state_file {
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("reading state file: {path}"))?;
        let loaded: sim_core::GameState =
            serde_json::from_str(&json).with_context(|| format!("parsing state file: {path}"))?;
        let rng_seed = loaded.meta.seed;
        (loaded, ChaCha8Rng::seed_from_u64(rng_seed))
    } else {
        let resolved_seed = seed.unwrap_or_else(rand::random);
        let mut new_rng = ChaCha8Rng::seed_from_u64(resolved_seed);
        let new_state = build_initial_state(&content, resolved_seed, &mut new_rng);
        (new_state, new_rng)
    };

    // Set up per-run metrics directory.
    let mut metrics_writer: Option<sim_core::MetricsFileWriter> = None;
    if !no_metrics {
        let run_id = generate_run_id(state.meta.seed);
        let run_dir = create_run_dir(&run_id)?;
        write_run_info(
            &run_dir,
            &run_id,
            state.meta.seed,
            ticks,
            &content.content_version,
            metrics_every,
            print_every,
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
    let evidence = state
        .research
        .evidence
        .get(&tech_id)
        .copied()
        .unwrap_or(0.0);

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
