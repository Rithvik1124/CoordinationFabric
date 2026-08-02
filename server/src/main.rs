use axum::{
    extract::{ConnectInfo, Json},
    routing::post,
    Router,
};
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Debug, Deserialize)]
struct CpuPayload {
    cpu_usage: f64,
    mem_usage: f64
}

async fn cpu_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<CpuPayload>,
) {
    println!(
        "[{}] CPU Usage: {:.2}%, Mem Usage: {:.2}%",
        addr.ip(),
        payload.cpu_usage,
        payload.mem_usage
    );
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/cpu", post(cpu_handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .unwrap();

    println!("Listening on 0.0.0.0:8080");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}