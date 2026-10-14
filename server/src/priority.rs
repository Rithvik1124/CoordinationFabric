use crate::structures::{
    CPUPayload, ListenConfig, NodeConfig, NodeSnapshot, PeerConfig, Role, RoleFlags,
    SchedulerSnapshot,
};
use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    io,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
struct NodeRecord {
    node_id: String,
    source_ip: IpAddr,
    advertise_url: String,
    cpu_usage: f64,
    mem_usage: f64,
    process_latency_ms: f64,
    capacity_score: f64,
    roles: Vec<Role>,
    last_seen: Instant,
    last_seen_unix_secs: u64,
}

#[derive(Debug, Clone)]
pub struct SchedulerSettings {
    pub stale_after: Duration,
    pub role_port: u16,
    pub listen_host: String,
    pub telemetry_sink_url: String,
    pub file_hash_url: String,
    pub server_role_node_id: Option<String>,
    pub file_hash_role_node_id: Option<String>,
    pub config_dir: PathBuf,
}

impl SchedulerSettings {
    pub fn from_env() -> Self {
        let stale_secs = env::var("NODE_STALE_AFTER_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .max(5);
        let role_port = env::var("NODE_ROLE_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3000);
        let config_dir = env::var("NODE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("generated-configs"));
        let telemetry_sink_url = env::var("SERVER_SINK_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let file_hash_url = env::var("FILE_HASH_URL")
            .unwrap_or_else(|_| telemetry_sink_url.clone());
        let server_role_node_id = env::var("SERVER_ROLE_NODE_ID").ok().filter(|s| !s.trim().is_empty());
        let file_hash_role_node_id = env::var("FILE_HASH_ROLE_NODE_ID").ok().filter(|s| !s.trim().is_empty());

        Self {
            stale_after: Duration::from_secs(stale_secs),
            role_port,
            listen_host: "0.0.0.0".to_string(),
            telemetry_sink_url: telemetry_sink_url.trim_end_matches('/').to_string(),
            file_hash_url: file_hash_url.trim_end_matches('/').to_string(),
            server_role_node_id,
            file_hash_role_node_id,
            config_dir,
        }
    }
}

pub struct Scheduler {
    nodes: BTreeMap<String, NodeRecord>,
    role_owners: BTreeMap<String, String>,
    settings: SchedulerSettings,
}

impl Scheduler {
    pub fn new(settings: SchedulerSettings) -> Self {
        Self {
            nodes: BTreeMap::new(),
            role_owners: BTreeMap::new(),
            settings,
        }
    }

    pub fn update_node(
        &mut self,
        source: SocketAddr,
        payload: CPUPayload,
    ) -> Result<String, String> {
        payload.validate()?;
        let previous_layout = self.layout_signature();
        let node_id = payload
            .node_id
            .clone()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| source.ip().to_string());
        let advertise_url = match payload.advertise_url.as_deref() {
            Some(url) => url.trim_end_matches('/').to_string(),
            None => format_base_url(source.ip(), self.settings.role_port),
        };
        let raw_score = payload.capacity_score();
        // Smooth score jitter so tiny changes in one sample do not move roles
        // between otherwise similar nodes on every heartbeat.
        const NEW_SAMPLE_WEIGHT: f64 = 0.35;
        let score = self
            .nodes
            .get(&node_id)
            .map(|existing| existing.capacity_score * (1.0 - NEW_SAMPLE_WEIGHT) + raw_score * NEW_SAMPLE_WEIGHT)
            .unwrap_or(raw_score);
        let now = Instant::now();
        let now_epoch = unix_now_secs();

        self.nodes.insert(
            node_id.clone(),
            NodeRecord {
                node_id: node_id.clone(),
                source_ip: source.ip(),
                advertise_url,
                cpu_usage: payload.cpu_usage,
                mem_usage: payload.mem_usage,
                process_latency_ms: payload.process_latency_ms,
                capacity_score: score,
                roles: Vec::new(),
                last_seen: now,
                last_seen_unix_secs: now_epoch,
            },
        );

        self.prune_stale_nodes();
        self.rebalance_roles();
        let layout_changed = previous_layout != self.layout_signature();
        self.persist_generated_files(layout_changed)
            .map_err(|e| format!("could not write generated configs: {e}"))?;
        Ok(node_id)
    }

    pub fn expire_stale(&mut self) -> io::Result<bool> {
        let previous_layout = self.layout_signature();
        let before = self.nodes.len();
        self.prune_stale_nodes();
        let changed = self.nodes.len() != before;
        if changed {
            self.rebalance_roles();
            self.persist_generated_files(previous_layout != self.layout_signature())?;
        }
        Ok(changed)
    }

    fn prune_stale_nodes(&mut self) {
        let stale_after = self.settings.stale_after;
        self.nodes.retain(|_, node| node.last_seen.elapsed() <= stale_after);
    }

    fn rebalance_roles(&mut self) {
        self.role_owners.clear();
        for node in self.nodes.values_mut() {
            node.roles.clear();
        }

        // Give each role a distinct owner first whenever there are enough nodes.
        // If there are fewer nodes than roles, assign remaining roles using a
        // workload-to-capacity ratio so one small node does not collect every role.
        let mut ranked: Vec<(String, f64)> = self
            .nodes
            .iter()
            .map(|(id, node)| (id.clone(), node.capacity_score))
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        if ranked.is_empty() {
            return;
        }

        let mut assigned_load: HashMap<String, f64> = HashMap::new();
        for role in Role::ALL {
            let unassigned = ranked
                .iter()
                .filter(|(id, _)| !assigned_load.contains_key(id))
                .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)));

            let owner_id = if let Some((id, _)) = unassigned {
                id.clone()
            } else {
                ranked
                    .iter()
                    .min_by(|a, b| {
                        let load_a = assigned_load.get(&a.0).copied().unwrap_or(0.0);
                        let load_b = assigned_load.get(&b.0).copied().unwrap_or(0.0);
                        let cost_a = (load_a + role.workload_weight()) / a.1.max(1.0);
                        let cost_b = (load_b + role.workload_weight()) / b.1.max(1.0);
                        cost_a.total_cmp(&cost_b).then_with(|| a.0.cmp(&b.0))
                    })
                    .map(|(id, _)| id.clone())
                    .expect("non-empty ranked list")
            };

            let load = assigned_load.entry(owner_id.clone()).or_insert(0.0);
            *load += role.workload_weight();
            if let Some(node) = self.nodes.get_mut(&owner_id) {
                node.roles.push(role);
            }
            self.role_owners.insert(role.key().to_string(), owner_id);
        }
    }

    pub fn snapshot(&self) -> SchedulerSnapshot {
        let mut nodes: Vec<NodeSnapshot> = self
            .nodes
            .values()
            .map(|node| NodeSnapshot {
                node_id: node.node_id.clone(),
                source_ip: node.source_ip,
                advertise_url: node.advertise_url.clone(),
                cpu_usage: node.cpu_usage,
                mem_usage: node.mem_usage,
                process_latency_ms: node.process_latency_ms,
                capacity_score: node.capacity_score,
                roles: node.roles.clone(),
                last_seen_unix_secs: node.last_seen_unix_secs,
                age_secs: node.last_seen.elapsed().as_secs(),
            })
            .collect();
        nodes.sort_by(|a, b| b.capacity_score.total_cmp(&a.capacity_score).then_with(|| a.node_id.cmp(&b.node_id)));

        SchedulerSnapshot {
            generated_at_unix_secs: unix_now_secs(),
            scoring: "capacity = 0.50*(100-CPU%) + 0.35*(100-memory%) + 0.15*latency_score; latency_score falls linearly to 0 at 250ms",
            stale_after_secs: self.settings.stale_after.as_secs(),
            nodes,
            role_owners: self.role_owners.clone(),
        }
    }

    pub fn node_config(&self, node_id: &str) -> Option<NodeConfig> {
        let node = self.nodes.get(node_id)?;
        let is_owner = |role: Role| self.role_owners.get(role.key()).is_some_and(|id| id.as_str() == node_id);
        let peer_url = |role: Role| {
            self.role_owners
                .get(role.key())
                .and_then(|owner| self.nodes.get(owner))
                .map(|owner| owner.advertise_url.clone())
                .unwrap_or_default()
        };

        Some(NodeConfig {
            node_id: node.node_id.clone(),
            listen: ListenConfig {
                host: self.settings.listen_host.clone(),
                port: self.settings.role_port,
            },
            roles: RoleFlags {
                ioc: is_owner(Role::Ioc),
                yara: is_owner(Role::Yara),
                sigma: is_owner(Role::Sigma),
                consensus: is_owner(Role::Consensus),
                server: self.settings.server_role_node_id.as_deref() == Some(node_id),
                file_hash: self.settings.file_hash_role_node_id.as_deref() == Some(node_id),
            },
            peers: PeerConfig {
                ioc: peer_url(Role::Ioc),
                yara: peer_url(Role::Yara),
                sigma: peer_url(Role::Sigma),
                consensus: peer_url(Role::Consensus),
                server: self.settings.telemetry_sink_url.clone(),
                file_hash: self.settings.file_hash_url.clone(),
            },
            expected_detectors: vec!["ioc".into(), "yara".into(), "sigma".into()],
        })
    }

    pub fn role_owners(&self) -> BTreeMap<String, String> {
        self.role_owners.clone()
    }

    pub fn score_for(&self, node_id: &str) -> Option<f64> {
        self.nodes.get(node_id).map(|n| n.capacity_score)
    }

    pub fn roles_for(&self, node_id: &str) -> Option<Vec<Role>> {
        self.nodes.get(node_id).map(|n| n.roles.clone())
    }

    pub fn contains_node(&self, node_id: &str) -> bool {
        self.nodes.contains_key(node_id)
    }

    fn layout_signature(&self) -> Vec<(String, String, Vec<Role>)> {
        let mut layout: Vec<(String, String, Vec<Role>)> = self
            .nodes
            .iter()
            .map(|(id, node)| (id.clone(), node.advertise_url.clone(), node.roles.clone()))
            .collect();
        layout.sort_by(|a, b| a.0.cmp(&b.0));
        layout
    }

    fn persist_generated_files(&self, write_node_configs: bool) -> io::Result<()> {
        fs::create_dir_all(&self.settings.config_dir)?;
        let snapshot = self.snapshot();
        let state_json = serde_json::to_vec_pretty(&snapshot)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(self.settings.config_dir.join("scheduler-state.json"), state_json)?;

        if !write_node_configs {
            return Ok(());
        }

        let nodes_dir = self.settings.config_dir.join("nodes");
        fs::create_dir_all(&nodes_dir)?;
        let mut live_dirs = Vec::new();
        for node_id in self.nodes.keys() {
            if let Some(config) = self.node_config(node_id) {
                let dir_name = safe_filename(node_id);
                let node_dir = nodes_dir.join(&dir_name);
                fs::create_dir_all(&node_dir)?;
                let bytes = serde_json::to_vec_pretty(&config)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                fs::write(node_dir.join("node.config.json"), bytes)?;
                live_dirs.push(dir_name);
            }
        }

        // This is a dedicated generated output directory; remove only stale
        // per-node directories within its nodes/ subdirectory.
        for entry in fs::read_dir(&nodes_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !live_dirs.iter().any(|f| f == &name) && entry.file_type()?.is_dir() {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
        Ok(())
    }
}

fn safe_filename(value: &str) -> String {
    let safe: String = value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(100)
        .collect();
    if safe.is_empty() { "node".to_string() } else { safe }
}

fn format_base_url(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(v4) => format!("http://{v4}:{port}"),
        IpAddr::V6(v6) => format!("http://[{v6}]:{port}"),
    }
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn settings() -> SchedulerSettings {
        SchedulerSettings {
            stale_after: Duration::from_secs(60),
            role_port: 3000,
            listen_host: "0.0.0.0".into(),
            telemetry_sink_url: "http://127.0.0.1:3000".into(),
            file_hash_url: "http://127.0.0.1:3000".into(),
            server_role_node_id: None,
            file_hash_role_node_id: None,
            config_dir: std::env::temp_dir().join(format!("scheduler-test-{}", unix_now_secs())),
        }
    }

    fn payload(id: &str, cpu: f64, mem: f64) -> CPUPayload {
        CPUPayload {
            node_id: Some(id.into()),
            advertise_url: Some(format!("http://{id}:3000")),
            avg_score: None,
            cpu_usage: cpu,
            mem_usage: mem,
            process_latency_ms: 10.0,
        }
    }

    #[test]
    fn assigns_all_roles_and_spreads_when_four_nodes_exist() {
        let mut scheduler = Scheduler::new(settings());
        for (idx, (id, cpu, mem)) in [
            ("n1", 10.0, 20.0),
            ("n2", 20.0, 25.0),
            ("n3", 30.0, 30.0),
            ("n4", 40.0, 35.0),
        ].into_iter().enumerate() {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000 + idx as u16);
            scheduler.update_node(addr, payload(id, cpu, mem)).unwrap();
        }
        assert_eq!(scheduler.role_owners().len(), 4);
        let owners: std::collections::HashSet<_> = scheduler.role_owners().values().collect();
        assert_eq!(owners.len(), 4);
        let cfg = scheduler.node_config("n1").unwrap();
        assert!(cfg.roles.yara);
        assert!(!cfg.roles.ioc);
    }

    #[test]
    fn fewer_nodes_can_hold_multiple_roles() {
        let mut scheduler = Scheduler::new(settings());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);
        scheduler.update_node(addr, payload("solo", 15.0, 30.0)).unwrap();
        assert_eq!(scheduler.roles_for("solo").unwrap().len(), 4);
    }
}
