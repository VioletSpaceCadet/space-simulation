use std::convert::Infallible;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::get,
    Router,
};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;
use sim_control::AutopilotController;
use sim_core::{
    AsteroidTemplateDef, Constants, Counters, EventEnvelope, FacilitiesState, GameContent,
    GameState, MetaState, NodeDef, NodeId, PrincipalId, ResearchState, ScanSite, ShipId,
    ShipState, SiteId, SolarSystemDef, StationId, StationState, TechDef,
};
use tokio::sync::broadcast;

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

fn build_initial_state(content: &GameContent, seed: u64, rng: &mut impl Rng) -> GameState {
    let earth_orbit = NodeId("node_earth_orbit".to_string());
    let c = &content.constants;
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
    let ship_id = ShipId("ship_0001".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());
    let ship = ShipState {
        id: ship_id.clone(),
        location_node: earth_orbit.clone(),
        owner,
        task: None,
    };
    let node_ids: Vec<&NodeId> = content.solar_system.nodes.iter().map(|n| &n.id).collect();
    let mut scan_sites = Vec::new();
    let mut site_counter = 1u64;
    for template in &content.asteroid_templates {
        for _ in 0..c.asteroid_count_per_template {
            let node = node_ids[rng.gen_range(0..node_ids.len())].clone();
            scan_sites.push(ScanSite {
                id: SiteId(format!("site_{:04}", site_counter)),
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

struct SimState {
    game_state: GameState,
    content: GameContent,
    rng: ChaCha8Rng,
    autopilot: AutopilotController,
    next_command_id: u64,
}

type SharedSim = Arc<Mutex<SimState>>;
type EventTx = broadcast::Sender<Vec<EventEnvelope>>;

#[derive(Clone)]
struct AppState {
    sim: SharedSim,
    event_tx: EventTx,
}

fn make_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/meta", get(meta_handler))
        .route("/api/v1/snapshot", get(snapshot_handler))
        .route("/api/v1/stream", get(stream_handler))
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
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(events) if !events.is_empty() => {
                            let data = serde_json::to_string(&events).unwrap_or_default();
                            yield Ok(Event::default().data(data));
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    let tick = sim.lock().unwrap().game_state.meta.tick;
                    let heartbeat = serde_json::json!({"heartbeat": true, "tick": tick});
                    yield Ok(Event::default().data(heartbeat.to_string()));
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

fn main() {
    println!("sim_daemon stub");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, http::StatusCode};
    use http_body_util::BodyExt;
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
