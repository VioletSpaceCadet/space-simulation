mod routes;
mod state;
mod tick_loop;

use routes::make_router;
use sim_world::{build_initial_state, load_content};
use state::{AppState, SimState};
use tick_loop::run_tick_loop;

use anyhow::Context;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::{Parser, Subcommand};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::AutopilotController;
use sim_core::EventEnvelope;
use tokio::sync::broadcast;

#[derive(Parser)]
#[command(name = "sim_daemon", about = "Space Industry Sim Daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        /// Generate world procedurally with this seed. Mutually exclusive with --state.
        #[arg(long, conflicts_with = "state_file")]
        seed: Option<u64>,
        /// Load initial GameState from a JSON file. Mutually exclusive with --seed.
        #[arg(long = "state", conflicts_with = "seed")]
        state_file: Option<String>,
        #[arg(long, default_value = "./content")]
        content_dir: String,
        #[arg(long, default_value_t = 3001)]
        port: u16,
        /// Ticks per second. 0 = as fast as possible.
        #[arg(long, default_value_t = 10.0)]
        ticks_per_sec: f64,
        #[arg(long)]
        max_ticks: Option<u64>,
        /// Sample metrics every N ticks (default 60). 0 = disabled.
        #[arg(long, default_value_t = 60)]
        metrics_every: u64,
        /// Disable automatic metrics collection to runs/ directory.
        #[arg(long)]
        no_metrics: bool,
    },
}

fn generate_run_id(seed: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = epoch_days_to_date(secs / 86400);

    format!("{year:04}{month:02}{day:02}_{hours:02}{minutes:02}{seconds:02}_seed{seed}")
}

fn epoch_days_to_date(mut days: u64) -> (u64, u64, u64) {
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
    content_version: &str,
    metrics_every: u64,
    max_ticks: Option<u64>,
) -> Result<()> {
    let info = serde_json::json!({
        "run_id": run_id,
        "seed": seed,
        "content_version": content_version,
        "metrics_every": metrics_every,
        "runner": "sim_daemon",
        "args": {
            "max_ticks": max_ticks,
        }
    });
    let path = dir.join("run_info.json");
    let file =
        std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    serde_json::to_writer_pretty(file, &info)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            seed,
            state_file,
            content_dir,
            port,
            ticks_per_sec,
            max_ticks,
            metrics_every,
            no_metrics,
        } => {
            let content = load_content(&content_dir)?;
            let (game_state, rng) = if let Some(path) = state_file {
                let json = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading state file: {path}"))?;
                let loaded: sim_core::GameState = serde_json::from_str(&json)
                    .with_context(|| format!("parsing state file: {path}"))?;
                let rng_seed = loaded.meta.seed;
                (loaded, ChaCha8Rng::seed_from_u64(rng_seed))
            } else {
                let resolved_seed = seed.unwrap_or_else(rand::random);
                let mut rng = ChaCha8Rng::seed_from_u64(resolved_seed);
                let new_state = build_initial_state(&content, resolved_seed, &mut rng);
                (new_state, rng)
            };

            // Set up per-run metrics directory.
            let metrics_writer = if no_metrics {
                None
            } else {
                let run_id = generate_run_id(game_state.meta.seed);
                let run_dir = create_run_dir(&run_id)?;
                write_run_info(
                    &run_dir,
                    &run_id,
                    game_state.meta.seed,
                    &content.content_version,
                    metrics_every,
                    max_ticks,
                )?;
                let writer = sim_core::MetricsFileWriter::new(run_dir.clone())
                    .with_context(|| format!("opening metrics CSV in {}", run_dir.display()))?;
                println!("Run directory: {}", run_dir.display());
                Some(writer)
            };

            let (event_tx, _) = broadcast::channel::<Vec<EventEnvelope>>(256);
            let app_state = AppState {
                sim: Arc::new(Mutex::new(SimState {
                    game_state,
                    content,
                    rng,
                    autopilot: AutopilotController,
                    next_command_id: 0,
                    metrics_every,
                    metrics_history: Vec::new(),
                    metrics_writer,
                })),
                event_tx: event_tx.clone(),
                ticks_per_sec,
            };
            let router = make_router(app_state.clone());
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let speed = if ticks_per_sec == 0.0 {
                "max".to_string()
            } else {
                format!("{ticks_per_sec} ticks/sec")
            };
            println!("sim_daemon listening on http://localhost:{port}  speed={speed}");
            tokio::spawn(run_tick_loop(
                app_state.sim,
                event_tx,
                ticks_per_sec,
                max_ticks,
            ));
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, router).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, http::StatusCode};
    use http_body_util::BodyExt;
    use sim_core::{Constants, ElementDef, GameContent, NodeDef, NodeId, SolarSystemDef};
    use tower::ServiceExt;

    fn make_test_state() -> AppState {
        let content = GameContent {
            content_version: "test".to_string(),
            techs: vec![],
            solar_system: SolarSystemDef {
                nodes: vec![NodeDef {
                    id: NodeId("node_test".to_string()),
                    name: "Test".to_string(),
                }],
                edges: vec![],
            },
            asteroid_templates: vec![],
            elements: vec![
                ElementDef {
                    id: "Fe".to_string(),
                    density_kg_per_m3: 7874.0,
                    display_name: "Iron".to_string(),
                    refined_name: None,
                },
                ElementDef {
                    id: "Si".to_string(),
                    density_kg_per_m3: 2329.0,
                    display_name: "Silicon".to_string(),
                    refined_name: None,
                },
            ],
            module_defs: vec![],
            constants: Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_scan_data_amount: 1.0,
                survey_scan_data_quality: 1.0,
                deep_scan_data_amount: 1.0,
                deep_scan_data_quality: 1.0,
                survey_tag_detection_probability: 0.5,
                asteroid_count_per_template: 0,
                asteroid_mass_min_kg: 500.0,
                asteroid_mass_max_kg: 500.0,
                ship_cargo_capacity_m3: 20.0,
                station_cargo_capacity_m3: 10_000.0,
                station_compute_units_total: 10,
                station_power_per_compute_unit_per_tick: 1.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 100.0,
                mining_rate_kg_per_tick: 50.0,
                deposit_ticks: 1,
                autopilot_iron_rich_confidence_threshold: 0.7,
                autopilot_refinery_threshold_kg: 500.0,
            },
        };
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let game_state = build_initial_state(&content, 0, &mut rng);
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        AppState {
            sim: std::sync::Arc::new(std::sync::Mutex::new(SimState {
                game_state,
                content,
                rng,
                autopilot: AutopilotController,
                next_command_id: 0,
                metrics_every: 60,
                metrics_history: Vec::new(),
                metrics_writer: None,
            })),
            event_tx,
            ticks_per_sec: 10.0,
        }
    }

    #[tokio::test]
    async fn test_meta_returns_200() {
        let app = make_router(make_test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/meta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_meta_contains_tick() {
        let app = make_router(make_test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/meta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["tick"], 0);
    }

    #[tokio::test]
    async fn test_snapshot_returns_200() {
        let app = make_router(make_test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_meta_contains_ticks_per_sec() {
        let app = make_router(make_test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/meta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ticks_per_sec"], 10.0);
    }

    #[tokio::test]
    async fn test_snapshot_is_valid_json() {
        let app = make_router(make_test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let result: Result<serde_json::Value, _> = serde_json::from_slice(&body);
        assert!(result.is_ok(), "snapshot was not valid JSON: {:?}", body);
    }
}
