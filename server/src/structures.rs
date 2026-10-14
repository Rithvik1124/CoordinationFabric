use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// Metrics submitted by each node. `avg_score` is accepted for compatibility,
/// but the scheduler always recomputes the authoritative capacity score.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CPUPayload {
    #[serde(default)]
    pub node_id: Option<String>,
    /// Base URL where this node's role server is reachable, e.g. http://10.0.0.11:3000.
    #[serde(default)]
    pub advertise_url: Option<String>,
    #[serde(default)]
    pub avg_score: Option<f64>,
    pub cpu_usage: f64,
    pub mem_usage: f64,
    /// Mean event-processing latency in milliseconds. Zero is used when the
    /// telemetry agent has not yet been instrumented to measure this value.
    #[serde(default)]
    pub process_latency_ms: f64,
}

impl CPUPayload {
    pub fn validate(&self) -> Result<(), String> {
        if !self.cpu_usage.is_finite() || !(0.0..=100.0).contains(&self.cpu_usage) {
            return Err("cpu_usage must be a finite percentage from 0 to 100".into());
        }
        if !self.mem_usage.is_finite() || !(0.0..=100.0).contains(&self.mem_usage) {
            return Err("mem_usage must be a finite percentage from 0 to 100".into());
        }
        if !self.process_latency_ms.is_finite() || self.process_latency_ms < 0.0 {
            return Err("process_latency_ms must be finite and non-negative".into());
        }
        if let Some(url) = &self.advertise_url {
            let normalized = url.trim_end_matches('/');
            if !(normalized.starts_with("http://") || normalized.starts_with("https://")) {
                return Err("advertise_url must start with http:// or https://".into());
            }
            if normalized.len() > 512 {
                return Err("advertise_url must be at most 512 characters".into());
            }
        }
        if let Some(id) = &self.node_id {
            if id.trim().is_empty() || id.len() > 128 {
                return Err("node_id must be 1 to 128 characters".into());
            }
            if !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')) {
                return Err("node_id may contain only letters, digits, '-', '_', '.', and ':'".into());
            }
        }
        Ok(())
    }

    /// Capacity score: higher means a node has more headroom and is therefore
    /// a better candidate for a role. Weights are deliberately easy to tune.
    pub fn capacity_score(&self) -> f64 {
        let cpu_headroom = 100.0 - self.cpu_usage.clamp(0.0, 100.0);
        let memory_headroom = 100.0 - self.mem_usage.clamp(0.0, 100.0);
        // A 250 ms processing latency saturates the latency penalty at zero.
        let latency_score = (100.0 * (1.0 - self.process_latency_ms / 250.0))
            .clamp(0.0, 100.0);

        (cpu_headroom * 0.50 + memory_headroom * 0.35 + latency_score * 0.15)
            .clamp(0.0, 100.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Ioc,
    Yara,
    Sigma,
    Consensus,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Yara, Role::Sigma, Role::Ioc, Role::Consensus];

    pub fn key(self) -> &'static str {
        match self {
            Role::Ioc => "ioc",
            Role::Yara => "yara",
            Role::Sigma => "sigma",
            Role::Consensus => "consensus",
        }
    }

    /// Relative load estimate used by the basic greedy allocator.
    pub fn workload_weight(self) -> f64 {
        match self {
            Role::Yara => 3.0,
            Role::Sigma => 2.0,
            Role::Ioc => 1.5,
            Role::Consensus => 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub ioc: String,
    pub yara: String,
    pub sigma: String,
    pub consensus: String,
    pub server: String,
    pub file_hash: String,
}

/// This is the per-node JSON consumed by the role server. The scheduler
/// generates one file for every registered node and also serves it over HTTP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub listen: ListenConfig,
    pub roles: RoleFlags,
    pub peers: PeerConfig,
    pub expected_detectors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleFlags {
    pub ioc: bool,
    pub yara: bool,
    pub sigma: bool,
    pub consensus: bool,
    pub server: bool,
    pub file_hash: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeSnapshot {
    pub node_id: String,
    pub source_ip: IpAddr,
    pub advertise_url: String,
    pub cpu_usage: f64,
    pub mem_usage: f64,
    pub process_latency_ms: f64,
    pub capacity_score: f64,
    pub roles: Vec<Role>,
    pub last_seen_unix_secs: u64,
    pub age_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerSnapshot {
    pub generated_at_unix_secs: u64,
    pub scoring: &'static str,
    pub stale_after_secs: u64,
    pub nodes: Vec<NodeSnapshot>,
    pub role_owners: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestResponse {
    pub status: &'static str,
    pub node_id: String,
    pub capacity_score: f64,
    pub assigned_roles: Vec<Role>,
    pub role_owners: std::collections::BTreeMap<String, String>,
    pub config_url: String,
}
