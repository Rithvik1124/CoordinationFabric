use serde::Deserialize;
#[derive(Debug, Deserialize)]
pub struct CPUPayload {
    avg_score: f64,
    cpu_usage: f64,
    mem_usage: f64
}