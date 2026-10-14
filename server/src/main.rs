use axum::{
    extract::{ConnectInfo, Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use crate::structures::{CPUPayload, IngestResponse, NodeConfig, SchedulerSnapshot};
use priority::{Scheduler, SchedulerSettings};
use std::{net::SocketAddr, sync::{Arc, Mutex}, time::Duration};

mod priority;
mod structures;

#[derive(Clone)]
struct AppState {
    scheduler: Arc<Mutex<Scheduler>>,
}

async fn cpu_handler(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<CPUPayload>,
) -> Result<Json<IngestResponse>, (StatusCode, String)> {
    payload.validate().map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let mut scheduler = state.scheduler.lock().unwrap_or_else(|e| e.into_inner());
    let node_id = scheduler
        .update_node(addr, payload)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let score = scheduler.score_for(&node_id).unwrap_or(0.0);
    let assigned_roles = scheduler.roles_for(&node_id).unwrap_or_default();
    let role_owners = scheduler.role_owners();

    Ok(Json(IngestResponse {
        status: "ok",
        config_url: format!("/node-config/{node_id}"),
        node_id,
        capacity_score: score,
        assigned_roles,
        role_owners,
    }))
}

async fn scheduler_state(State(state): State<AppState>) -> Json<SchedulerSnapshot> {
    let mut scheduler = state.scheduler.lock().unwrap_or_else(|e| e.into_inner());
    let _ = scheduler.expire_stale();
    Json(scheduler.snapshot())
}

async fn node_config(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> Result<Json<NodeConfig>, (StatusCode, String)> {
    let mut scheduler = state.scheduler.lock().unwrap_or_else(|e| e.into_inner());
    let _ = scheduler.expire_stale();
    scheduler
        .node_config(&node_id)
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("node '{node_id}' is not registered or its heartbeat expired")))
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let settings = SchedulerSettings::from_env();
    println!("Starting metric scheduler");
    println!("Node heartbeat TTL: {} seconds", settings.stale_after.as_secs());
    println!("Generated configs: {}", settings.config_dir.display());

    let state = AppState {
        scheduler: Arc::new(Mutex::new(Scheduler::new(settings.clone()))),
    };

    // Expire dead nodes even if no new metric report arrives.
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let mut scheduler = cleanup_state.scheduler.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(err) = scheduler.expire_stale() {
                eprintln!("scheduler cleanup/persist error: {err}");
            }
        }
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/cpu", axum::routing::post(cpu_handler))
        .route("/scheduler/state", get(scheduler_state))
        .route("/node-config/{node_id}", get(node_config))
        .with_state(state);

    let bind_addr = std::env::var("SCHEDULER_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind scheduler at {bind_addr}: {e}"));

    println!("Listening on {bind_addr}");
    println!("POST /cpu, GET /scheduler/state, GET /node-config/{{node_id}}, GET /health");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap_or_else(|e| panic!("scheduler server failed: {e}"));
}
