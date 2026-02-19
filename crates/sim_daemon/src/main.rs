mod state;
mod world;

use state::{AppState, EventTx, SharedSim, SimState};
use world::{build_initial_state, load_content};

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use axum::{
    extract::State,
    http::{header, Method, StatusCode},
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::get,
    Router,
};
use clap::{Parser, Subcommand};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::{AutopilotController, CommandSource};
use sim_core::{Constants, EventEnvelope, EventLevel, GameContent, NodeId, SolarSystemDef};
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

fn make_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            "http://localhost:5173"
                .parse::<axum::http::HeaderValue>()
                .unwrap(),
        )
        .allow_methods([Method::GET])
        .allow_headers(Any);

    Router::new()
        .route("/api/v1/meta", get(meta_handler))
        .route("/api/v1/snapshot", get(snapshot_handler))
        .route("/api/v1/stream", get(stream_handler))
        .layer(cors)
        .with_state(state)
}

async fn meta_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    let sim = app_state.sim.lock().unwrap();
    Json(serde_json::json!({
        "tick": sim.game_state.meta.tick,
        "seed": sim.game_state.meta.seed,
        "content_version": sim.game_state.meta.content_version,
    }))
}

async fn snapshot_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let sim = app_state.sim.lock().unwrap();
    let body = serde_json::to_string(&sim.game_state).unwrap_or_default();
    drop(sim);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
}

async fn stream_handler(
    State(app_state): State<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = app_state.event_tx.subscribe();
    let sim = app_state.sim.clone();

    let stream = async_stream::stream! {
        let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
        heartbeat.tick().await; // discard the immediate first tick
        let mut flush = tokio::time::interval(Duration::from_secs(1));
        flush.tick().await; // discard the immediate first tick
        let mut pending: Vec<EventEnvelope> = Vec::new();
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(events) => pending.extend(events),
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = flush.tick() => {
                    if !pending.is_empty() {
                        let data = serde_json::to_string(&pending).unwrap_or_default();
                        pending.clear();
                        yield Ok(Event::default().data(data));
                    }
                }
                _ = heartbeat.tick() => {
                    let tick = sim.lock().unwrap().game_state.meta.tick;
                    let hb = serde_json::json!({"heartbeat": true, "tick": tick});
                    yield Ok(Event::default().data(hb.to_string()));
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    )
}

async fn run_tick_loop(
    sim: SharedSim,
    event_tx: EventTx,
    ticks_per_sec: f64,
    max_ticks: Option<u64>,
) {
    let sleep_duration = if ticks_per_sec > 0.0 {
        Some(Duration::from_secs_f64(1.0 / ticks_per_sec))
    } else {
        None
    };

    loop {
        let start = std::time::Instant::now();

        let (events, done) = {
            let mut guard = sim.lock().unwrap();
            // Split borrow: autopilot needs &mut self, plus we need &game_state, &content, &mut next_command_id.
            // Extract the fields we need as separate borrows to satisfy the borrow checker.
            let SimState {
                ref game_state,
                ref content,
                rng: _,
                ref mut autopilot,
                ref mut next_command_id,
                ..
            } = *guard;
            let commands = autopilot.generate_commands(game_state, content, next_command_id);
            let SimState {
                ref mut game_state,
                ref content,
                ref mut rng,
                ..
            } = *guard;
            let events = sim_core::tick(game_state, &commands, content, rng, EventLevel::Normal);
            let done = max_ticks.map_or(false, |max| guard.game_state.meta.tick >= max);
            (events, done)
        };

        let _ = event_tx.send(events);

        if done {
            break;
        }

        if let Some(duration) = sleep_duration {
            let elapsed = start.elapsed();
            if elapsed < duration {
                tokio::time::sleep(duration - elapsed).await;
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
}

#[derive(Parser)]
#[command(name = "sim_daemon", about = "Space Industry Sim Daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value = "./content")]
        content_dir: String,
        #[arg(long, default_value_t = 3001)]
        port: u16,
        /// Ticks per second. 0 = as fast as possible.
        #[arg(long, default_value_t = 10.0)]
        ticks_per_sec: f64,
        #[arg(long)]
        max_ticks: Option<u64>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            seed,
            content_dir,
            port,
            ticks_per_sec,
            max_ticks,
        } => {
            let content = load_content(&content_dir)?;
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let game_state = build_initial_state(&content, seed, &mut rng);
            let (event_tx, _) = broadcast::channel::<Vec<EventEnvelope>>(256);
            let app_state = AppState {
                sim: Arc::new(Mutex::new(SimState {
                    game_state,
                    content,
                    rng,
                    autopilot: AutopilotController,
                    next_command_id: 0,
                })),
                event_tx: event_tx.clone(),
            };
            let router = make_router(app_state.clone());
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let speed = if ticks_per_sec == 0.0 {
                "max".to_string()
            } else {
                format!("{} ticks/sec", ticks_per_sec)
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
    use sim_core::NodeDef;
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
                station_compute_units_total: 10,
                station_power_per_compute_unit_per_tick: 1.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 100.0,
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
            })),
            event_tx,
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
