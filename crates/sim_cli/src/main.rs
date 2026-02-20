use std::path::Path;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;
use sim_control::{AutopilotController, CommandSource};
use sim_core::{
    AsteroidTemplateDef, Constants, Counters, EventLevel, FacilitiesState, GameContent, GameState,
    MetaState, NodeId, PrincipalId, ResearchState, ScanSite, ShipId, ShipState, SiteId,
    SolarSystemDef, StationId, StationState, TechDef,
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
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value = "./content")]
        content_dir: String,
        #[arg(long, default_value_t = 100)]
        print_every: u64,
        #[arg(long, default_value = "normal", value_parser = ["normal", "debug"])]
        event_level: String,
    },
}

// ---------------------------------------------------------------------------
// Content loading
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TechsFile {
    content_version: String,
    techs: Vec<TechDef>,
}

#[derive(Deserialize)]
struct AsteroidTemplatesFile {
    templates: Vec<AsteroidTemplateDef>,
}

fn load_content(content_dir: &str) -> Result<GameContent> {
    let dir = Path::new(content_dir);

    let constants: Constants = serde_json::from_str(
        &std::fs::read_to_string(dir.join("constants.json")).context("reading constants.json")?,
    )
    .context("parsing constants.json")?;

    let techs_file: TechsFile = serde_json::from_str(
        &std::fs::read_to_string(dir.join("techs.json")).context("reading techs.json")?,
    )
    .context("parsing techs.json")?;

    let solar_system: SolarSystemDef = serde_json::from_str(
        &std::fs::read_to_string(dir.join("solar_system.json"))
            .context("reading solar_system.json")?,
    )
    .context("parsing solar_system.json")?;

    let templates_file: AsteroidTemplatesFile = serde_json::from_str(
        &std::fs::read_to_string(dir.join("asteroid_templates.json"))
            .context("reading asteroid_templates.json")?,
    )
    .context("parsing asteroid_templates.json")?;

    Ok(GameContent {
        content_version: techs_file.content_version,
        techs: techs_file.techs,
        solar_system,
        asteroid_templates: templates_file.templates,
        constants,
    })
}

// ---------------------------------------------------------------------------
// World generation
// ---------------------------------------------------------------------------

fn build_initial_state(content: &GameContent, seed: u64, rng: &mut impl rand::Rng) -> GameState {
    let earth_orbit = NodeId("node_earth_orbit".to_string());
    let c = &content.constants;

    // Station
    let station_id = StationId("station_earth_orbit".to_string());
    let station = StationState {
        id: station_id.clone(),
        location_node: earth_orbit.clone(),
        power_available_per_tick: c.station_power_available_per_tick,
        facilities: FacilitiesState {
            compute_units_total: c.station_compute_units_total,
            power_per_compute_unit_per_tick: c.station_power_per_compute_unit_per_tick,
            efficiency: c.station_efficiency,
        },
    };

    // Ship
    let ship_id = ShipId("ship_0001".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());
    let ship = ShipState {
        id: ship_id.clone(),
        location_node: earth_orbit.clone(),
        owner,
        task: None,
    };

    // Scan sites: one per template × count_per_template, random node
    let node_ids: Vec<&NodeId> = content.solar_system.nodes.iter().map(|n| &n.id).collect();
    let mut scan_sites = Vec::new();
    let mut site_counter = 1u64;
    for template in &content.asteroid_templates {
        for _ in 0..c.asteroid_count_per_template {
            let node = node_ids[rng.gen_range(0..node_ids.len())].clone();
            scan_sites.push(ScanSite {
                id: SiteId(format!("site_{site_counter:04}")),
                node,
                template_id: template.id.clone(),
            });
            site_counter += 1;
        }
    }

    GameState {
        meta: MetaState {
            tick: 0,
            seed,
            schema_version: 1,
            content_version: content.content_version.clone(),
        },
        scan_sites,
        asteroids: std::collections::HashMap::new(),
        ships: std::collections::HashMap::from([(ship_id, ship)]),
        stations: std::collections::HashMap::from([(station_id, station)]),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: std::collections::HashMap::new(),
            evidence: std::collections::HashMap::new(),
        },
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
        },
    }
}

// ---------------------------------------------------------------------------
// Run loop
// ---------------------------------------------------------------------------

fn run(
    ticks: u64,
    seed: u64,
    content_dir: &str,
    print_every: u64,
    event_level: EventLevel,
) -> Result<()> {
    let content = load_content(content_dir)?;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut state = build_initial_state(&content, seed, &mut rng);
    let mut autopilot = AutopilotController;
    let mut next_command_id = 0u64;

    println!(
        "Starting simulation: ticks={ticks} seed={seed} sites={} content_version={}",
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
    }

    println!("{}", "-".repeat(80));
    println!("Done. Final state at tick {}:", state.meta.tick);
    print_status(&state);

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
            content_dir,
            print_every,
            event_level,
        } => {
            let level = match event_level.as_str() {
                "debug" => EventLevel::Debug,
                _ => EventLevel::Normal,
            };
            run(ticks, seed, &content_dir, print_every, level)?;
        }
    }
    Ok(())
}
