mod alerts;
mod routes;
mod state;
mod tick_loop;

use routes::make_router;
use sim_world::{
    create_run_dir, generate_run_id, load_content, load_or_build_state, write_run_info,
};
use state::{AppState, SimState};
use tick_loop::run_tick_loop;

use anyhow::{Context, Result};

use clap::{Parser, Subcommand};
use sim_control::AutopilotController;
use sim_core::EventEnvelope;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
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
            let (game_state, rng) = load_or_build_state(&content, seed, state_file.as_deref())?;

            // Set up per-run metrics directory.
            let (metrics_writer, run_dir) = if no_metrics {
                (None, None)
            } else {
                let run_id = generate_run_id(game_state.meta.seed);
                let run_dir = create_run_dir(&run_id)?;
                write_run_info(
                    &run_dir,
                    &run_id,
                    game_state.meta.seed,
                    &content.content_version,
                    metrics_every,
                    serde_json::json!({
                        "runner": "sim_daemon",
                        "max_ticks": max_ticks,
                    }),
                )?;
                let writer = sim_core::MetricsFileWriter::new(run_dir.clone())
                    .with_context(|| format!("opening metrics CSV in {}", run_dir.display()))?;
                println!("Run directory: {}", run_dir.display());
                (Some(writer), Some(run_dir))
            };

            let alert_engine = if no_metrics {
                None
            } else {
                Some(alerts::AlertEngine::new(content.techs.len()))
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
                    metrics_history: VecDeque::new(),
                    metrics_writer,
                    alert_engine,
                })),
                event_tx: event_tx.clone(),
                ticks_per_sec,
                run_dir,
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
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use sim_core::test_fixtures::base_content;
    use sim_world::build_initial_state;
    use tower::ServiceExt;

    fn make_test_state() -> AppState {
        let content = base_content();
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
                metrics_history: VecDeque::new(),
                metrics_writer: None,
                alert_engine: None,
            })),
            event_tx,
            ticks_per_sec: 10.0,
            run_dir: None,
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

    fn make_test_state_with_run_dir(run_dir: std::path::PathBuf) -> AppState {
        let mut state = make_test_state();
        state.run_dir = Some(run_dir);
        state
    }

    #[tokio::test]
    async fn test_save_returns_200_with_run_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let app = make_router(make_test_state_with_run_dir(tmp.path().to_path_buf()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/save")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["tick"], 0);
        assert!(json["path"].as_str().unwrap().contains("save_0.json"));

        // Verify file was actually written and contains valid GameState
        let save_path = json["path"].as_str().unwrap();
        let contents = std::fs::read_to_string(save_path).unwrap();
        let _state: sim_core::GameState = serde_json::from_str(&contents).unwrap();
    }

    #[tokio::test]
    async fn test_save_returns_503_without_run_dir() {
        let app = make_router(make_test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/save")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
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
