use axum::{
    extract::{ConnectInfo, Json},
    routing::post,
    Router,
};
mod priority;
mod structures;
use crate::structures::CPUPayload;
//use priority;
use serde::Deserialize;
use std::net::SocketAddr;

// #[derive(Debug, Deserialize)]
// struct CPUPayload {
//     avg_score: f64,
//     cpu_usage: f64,
//     mem_usage: f64
// }

// println!(
//         "[{:?}] Overall: {:.2}%, CPU Usage: {:.2}%, Mem Usage: {:.2}%",
//         addr.ip(),
//         payload.avg_score,
//         payload.cpu_usage,
//         payload.mem_usage
//     );
async fn cpu_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<CPUPayload>,
) {
    priority::add_node_to_list(addr.ip(),payload)    
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