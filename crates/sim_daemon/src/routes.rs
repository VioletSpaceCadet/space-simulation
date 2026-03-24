use crate::state::AppState;
use axum::{
    extract::State,
    http::{header, Method, StatusCode},
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::{get, post},
    Router,
};
use sim_core::{
    AbsolutePos, BodyId, CommandEnvelope, CommandId, DataKind, EventEnvelope, ModuleBehaviorDef,
    ModuleKindState, OrbitalBodyDef, PrincipalId, TechDef,
};
use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

#[cfg(test)]
pub fn make_router(state: AppState) -> Router {
    make_router_with_cors(state, "http://localhost:5173").expect("test CORS origin is valid")
}

pub fn make_router_with_cors(state: AppState, cors_origin: &str) -> anyhow::Result<Router> {
    let header_value = cors_origin
        .parse::<axum::http::HeaderValue>()
        .map_err(|_| {
            anyhow::anyhow!(
                "Invalid CORS origin '{cors_origin}': must be a valid HTTP header value"
            )
        })?;
    let cors = CorsLayer::new()
        .allow_origin(header_value)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    Ok(Router::new()
        .route("/api/v1/meta", get(meta_handler))
        .route("/api/v1/snapshot", get(snapshot_handler))
        .route("/api/v1/metrics", get(metrics_handler))
        .route("/api/v1/stream", get(stream_handler))
        .route("/api/v1/save", post(save_handler))
        .route("/api/v1/pause", post(pause_handler))
        .route("/api/v1/resume", post(resume_handler))
        .route("/api/v1/alerts", get(alerts_handler))
        .route("/api/v1/advisor/digest", get(advisor_digest_handler))
        .route("/api/v1/command", post(command_handler))
        .route("/api/v1/pricing", get(pricing_handler))
        .route("/api/v1/spatial-config", get(spatial_config_handler))
        .route("/api/v1/content", get(content_handler))
        .route("/api/v1/speed", post(speed_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state))
}

pub async fn meta_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    let sim = app_state.sim.lock();
    let ticks_per_sec = f64::from_bits(app_state.ticks_per_sec.load(Ordering::Relaxed));
    let paused = app_state.paused.load(Ordering::Relaxed);
    Json(serde_json::json!({
        "tick": sim.game_state.meta.tick,
        "seed": sim.game_state.meta.seed,
        "content_version": sim.game_state.meta.content_version,
        "ticks_per_sec": ticks_per_sec,
        "paused": paused,
        "trade_unlock_tick": sim_core::trade_unlock_tick(&sim.content.constants),
        "minutes_per_tick": sim.content.constants.minutes_per_tick,
    }))
}

pub async fn snapshot_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let sim = app_state.sim.lock();
    match serde_json::to_value(&sim.game_state) {
        Ok(mut val) => {
            // Inject body_absolutes so FE can compute entity absolute positions.
            let body_absolutes: std::collections::HashMap<BodyId, AbsolutePos> = sim
                .game_state
                .body_cache
                .iter()
                .map(|(id, bc)| (id.clone(), bc.absolute))
                .collect();
            if let Some(obj) = val.as_object_mut() {
                if let Ok(ba) = serde_json::to_value(&body_absolutes) {
                    obj.insert("body_absolutes".to_string(), ba);
                }
            }
            drop(sim);
            let json = serde_json::to_string(&val).unwrap_or_default();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                json,
            )
        }
        Err(err) => {
            tracing::error!("snapshot serialization failed: {err}");
            drop(sim);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            )
        }
    }
}

pub async fn metrics_handler(
    State(app_state): State<AppState>,
) -> Json<VecDeque<sim_core::MetricsSnapshot>> {
    let sim = app_state.sim.lock();
    Json(sim.metrics_history.clone())
}

pub async fn save_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let run_dir = match &app_state.run_dir {
        Some(dir) => dir.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "no run directory (started with --no-metrics?)"})),
            );
        }
    };

    let sim = app_state.sim.lock();
    let tick = sim.game_state.meta.tick;
    let body = match serde_json::to_string_pretty(&sim.game_state) {
        Ok(json) => json,
        Err(err) => {
            tracing::error!("save serialization failed: {err}");
            drop(sim);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "serialization failed"})),
            );
        }
    };
    drop(sim);

    let saves_dir = run_dir.join("saves");
    if let Err(err) = std::fs::create_dir_all(&saves_dir) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("create saves dir: {err}")})),
        );
    }

    let filename = format!("save_{tick}.json");
    let path = saves_dir.join(&filename);
    if let Err(err) = std::fs::write(&path, body) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("write save: {err}")})),
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"path": path.display().to_string(), "tick": tick})),
    )
}

async fn alerts_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    let sim = app_state.sim.lock();
    let active_ids: Vec<String> = sim
        .alert_engine
        .as_ref()
        .map(super::alerts::AlertEngine::active_alert_ids)
        .unwrap_or_default();
    Json(serde_json::json!({ "active_alerts": active_ids }))
}

async fn advisor_digest_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let sim = app_state.sim.lock();

    if sim.metrics_history.is_empty() {
        return (
            StatusCode::NO_CONTENT,
            [(header::CONTENT_TYPE, "application/json")],
            String::new(),
        );
    }

    let alert_details = sim
        .alert_engine
        .as_ref()
        .map(super::alerts::AlertEngine::active_alert_details)
        .unwrap_or_default();

    // Safe to unwrap: we checked is_empty() above, so history.back() will return Some.
    let digest = super::analytics::compute_digest(&sim.metrics_history, alert_details).unwrap();
    drop(sim);

    match serde_json::to_string(&digest) {
        Ok(json) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            json,
        ),
        Err(err) => {
            tracing::error!("advisor digest serialization failed: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"error":"serialization failed"}"#.to_string(),
            )
        }
    }
}

pub async fn pause_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    app_state.paused.store(true, Ordering::Relaxed);
    Json(serde_json::json!({"paused": true}))
}

pub async fn resume_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    app_state.paused.store(false, Ordering::Relaxed);
    Json(serde_json::json!({"paused": false}))
}

pub async fn command_handler(
    State(app_state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let command: sim_core::Command = match serde_json::from_value(body["command"].clone()) {
        Ok(cmd) => cmd,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid command: {err}")})),
            );
        }
    };

    let (command_id_str, tick) = {
        let mut sim = app_state.sim.lock();
        let id_num = sim.next_command_id;
        sim.next_command_id += 1;
        let tick = sim.game_state.meta.tick;
        let command_id = format!("cmd_{id_num}");
        (command_id, tick)
    };

    let envelope = CommandEnvelope {
        id: CommandId(command_id_str.clone()),
        issued_by: PrincipalId("principal_player".to_string()),
        issued_tick: tick,
        execute_at_tick: tick,
        command,
    };

    app_state.command_queue.lock().push(envelope);

    (
        StatusCode::OK,
        Json(serde_json::json!({"command_id": command_id_str})),
    )
}

pub async fn pricing_handler(State(app_state): State<AppState>) -> Json<sim_core::PricingTable> {
    let sim = app_state.sim.lock();
    Json(sim.content.pricing.clone())
}

#[derive(serde::Serialize)]
struct SolarSystemConfig {
    bodies: Vec<OrbitalBodyDef>,
    body_absolutes: std::collections::HashMap<BodyId, AbsolutePos>,
    ticks_per_au: u64,
    min_transit_ticks: u64,
    docking_range_au_um: u64,
}

async fn spatial_config_handler(State(app_state): State<AppState>) -> Json<SolarSystemConfig> {
    let sim = app_state.sim.lock();
    let body_absolutes = sim
        .game_state
        .body_cache
        .iter()
        .map(|(id, bc)| (id.clone(), bc.absolute))
        .collect();
    Json(SolarSystemConfig {
        bodies: sim.content.solar_system.bodies.clone(),
        body_absolutes,
        ticks_per_au: sim.content.constants.ticks_per_au,
        min_transit_ticks: sim.content.constants.min_transit_ticks,
        docking_range_au_um: sim.content.constants.docking_range_au_um,
    })
}

fn runs_per_hour(interval_minutes: u64, interval_ticks: u64, minutes_per_tick: u32) -> f64 {
    if interval_minutes > 0 {
        60.0 / interval_minutes as f64
    } else {
        60.0 / (interval_ticks as f64 * f64::from(minutes_per_tick))
    }
}

/// Serves tech definitions needed for the research panel DAG.
pub async fn content_handler(State(app_state): State<AppState>) -> Json<ContentResponse> {
    let sim = app_state.sim.lock();
    let mpt = sim.content.constants.minutes_per_tick;

    let mut lab_rates = Vec::new();
    let mut data_rates: std::collections::HashMap<DataKind, f64> = std::collections::HashMap::new();

    for (station_id, station) in &sim.game_state.stations {
        for module in &station.modules {
            let Some(mod_def) = sim.content.module_defs.get(&module.def_id) else {
                continue;
            };

            match (&module.kind_state, &mod_def.behavior) {
                (ModuleKindState::Lab(lab_state), ModuleBehaviorDef::Lab(def)) => {
                    let rph = runs_per_hour(
                        def.research_interval_minutes,
                        def.research_interval_ticks,
                        mpt,
                    );
                    lab_rates.push(LabRateInfo {
                        station_id: station_id.0.clone(),
                        module_id: module.id.0.clone(),
                        module_name: mod_def.name.clone(),
                        assigned_tech: lab_state.assigned_tech.as_ref().map(|t| t.0.clone()),
                        domain: serde_json::to_value(&def.domain)
                            .ok()
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_default(),
                        points_per_hour: f64::from(def.research_points_per_run) * rph,
                        starved: lab_state.starved,
                        enabled: module.enabled,
                    });

                    // Consumption: only from enabled labs with assigned tech that aren't starved
                    if module.enabled && lab_state.assigned_tech.is_some() && !lab_state.starved {
                        let consumption = f64::from(def.data_consumption_per_run) * rph;
                        for kind in &def.accepted_data {
                            *data_rates.entry(kind.clone()).or_insert(0.0) -=
                                consumption / def.accepted_data.len() as f64;
                        }
                    }
                }
                (_, ModuleBehaviorDef::SensorArray(sensor_def)) if module.enabled => {
                    let rph = runs_per_hour(
                        sensor_def.scan_interval_minutes,
                        sensor_def.scan_interval_ticks,
                        mpt,
                    );
                    let approx_yield = f64::from(sim.content.constants.data_generation_peak) * 0.5;
                    *data_rates
                        .entry(sensor_def.data_kind.clone())
                        .or_insert(0.0) += approx_yield * rph;
                }
                _ => {}
            }
        }
    }

    let event_defs: Vec<EventDefInfo> = sim
        .content
        .events
        .iter()
        .map(|e| EventDefInfo {
            id: e.id.0.clone(),
            name: e.name.clone(),
            category: e.category.clone(),
            description_template: e.description_template.clone(),
        })
        .collect();

    Json(ContentResponse {
        techs: sim.content.techs.clone(),
        lab_rates,
        data_rates,
        minutes_per_tick: mpt,
        recipes: sim.content.recipes.clone(),
        event_defs,
        hulls: sim.content.hulls.clone(),
    })
}

#[derive(serde::Serialize)]
pub struct ContentResponse {
    pub techs: Vec<TechDef>,
    pub lab_rates: Vec<LabRateInfo>,
    pub data_rates: std::collections::HashMap<DataKind, f64>,
    pub minutes_per_tick: u32,
    pub recipes: std::collections::BTreeMap<sim_core::RecipeId, sim_core::RecipeDef>,
    pub event_defs: Vec<EventDefInfo>,
    pub hulls: std::collections::BTreeMap<sim_core::HullId, sim_core::HullDef>,
}

#[derive(serde::Serialize, Clone)]
pub struct EventDefInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description_template: String,
}

#[derive(serde::Serialize)]
pub struct LabRateInfo {
    pub station_id: String,
    pub module_id: String,
    pub module_name: String,
    pub assigned_tech: Option<String>,
    pub domain: String,
    pub points_per_hour: f64,
    pub starved: bool,
    pub enabled: bool,
}

pub async fn speed_handler(
    State(app_state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(tps) = body
        .get("ticks_per_sec")
        .and_then(serde_json::Value::as_f64)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "missing or invalid ticks_per_sec"})),
        );
    };
    if tps < 0.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "ticks_per_sec must be >= 0"})),
        );
    }
    app_state
        .ticks_per_sec
        .store(tps.to_bits(), Ordering::Relaxed);
    (
        StatusCode::OK,
        Json(serde_json::json!({"ticks_per_sec": tps})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, SimState};
    use parking_lot::Mutex;
    use sim_control::AutopilotController;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::sync::Arc;

    fn test_app_state() -> AppState {
        let content_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content")
            .to_string_lossy()
            .to_string();
        let content = sim_world::load_content(&content_dir).expect("load content");
        let (game_state, rng) =
            sim_world::load_or_build_state(&content, Some(1), None).expect("build state");
        let (event_tx, _) = broadcast::channel(16);
        let sim = Arc::new(Mutex::new(SimState {
            game_state,
            content,
            rng,
            autopilot: AutopilotController::new(),
            next_command_id: 1,
            metrics_every: 0,
            metrics_history: VecDeque::new(),
            metrics_writer: None,
            alert_engine: None,
        }));
        AppState {
            sim,
            command_queue: Arc::new(Mutex::new(Vec::new())),
            event_tx,
            ticks_per_sec: Arc::new(AtomicU64::new(0)),
            run_dir: None,
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn valid_cors_origin_succeeds() {
        let state = test_app_state();
        assert!(make_router_with_cors(state, "http://localhost:5173").is_ok());
    }

    #[test]
    fn invalid_cors_origin_returns_error() {
        let state = test_app_state();
        let result = make_router_with_cors(state, "not a valid \x00 header");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid CORS origin"), "got: {err_msg}");
    }
}

pub async fn stream_handler(
    State(app_state): State<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = app_state.event_tx.subscribe();
    let sim = app_state.sim.clone();

    let stream = async_stream::stream! {
        let mut heartbeat = tokio::time::interval(Duration::from_millis(200));
        heartbeat.tick().await; // discard the immediate first tick
        let mut flush = tokio::time::interval(Duration::from_millis(50));
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
                    let tick = sim.lock().game_state.meta.tick;
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
