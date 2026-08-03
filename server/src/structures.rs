use serde::Deserialize;
use std::net::IpAddr;

#[derive(Debug, Deserialize)]
pub struct CPUPayload {
    avg_score: f64,
    cpu_usage: f64,
    mem_usage: f64
}

//somehow these endpoints are more employed than me TwT
pub struct EmployedEndpoint{
    roles: Vec<String>,
    ip_addr: IpAddr,
}

pub struct UnemployedEndpoint{
    ip_addr: IpAddr,
    avg_score: f64,
    cpu_usage: f64,
    mem_usage: f64

}