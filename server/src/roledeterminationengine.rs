use serde::Deserialize;
use std::{
    collections::HashMap,
    net::{IpAddr,Ipv4Addr},
    sync::{LazyLock, Mutex, RwLock},
};
use crate::Role::{Consensus, IOC, Sigma};
use rand::{Rng, rng, RngExt};
////////////////////////////////////////////////////STRUCTURES//////////////////////////////////////////////////////////////

//New Node
#[derive(Debug, Deserialize, Clone, PartialEq)]
struct Node{
    ip_addr: IpAddr,
    score: f64,
    state: State,
}

#[derive(Debug, Clone)]
pub struct NodeCompute {
    pub score: f64,
    pub state: State,
}

//Employed Node
#[derive(Debug, Deserialize)]
pub struct EmployedNode{
    roles: Vec<Role>,
    ip_addr: IpAddr,
    score: f64,
}


#[derive(Default, Debug)]
struct RoleComputeAverage {
    sigma_total: f64,
    sigma_count: usize,

    ioc_total: f64,
    ioc_count: usize,

    consensus_total: f64,
    consensus_count: usize,
}



#[derive(Eq, Hash, PartialEq, Clone, Debug, Deserialize)]
enum Role {
    Sigma,
    IOC,
    Consensus,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]

enum State{
    Active,
    Idle,
    Overloaded,
    Cooldown,
}

impl RoleComputeAverage {
    fn add_role_compute(&mut self, role: Vec<Role>, compute: f64) {
        for i in role{
            match i {
            Role::Sigma => {
                self.sigma_total += compute;
                self.sigma_count += 1;
            }
            Role::IOC => {
                self.ioc_total += compute;
                self.ioc_count += 1;
            }
            Role::Consensus => {
                self.consensus_total += compute;
                self.consensus_count += 1;
            }
        }
        }
    }

    fn sigma_average(&self) -> f64 {
        if self.sigma_count == 0 {
            0.0
        } else {
            self.sigma_total / self.sigma_count as f64
        }
    }
    fn consensus_average(&self) -> f64 {
        if self.consensus_count == 0 {
            0.0
        } else {
            self.consensus_total / self.consensus_count as f64
        }
    }
    fn ioc_average(&self) -> f64 {
        if self.ioc_count == 0 {
            0.0
        } else {
            self.ioc_total / self.ioc_count as f64
        }
    }
}

#[derive(Eq, PartialEq, Default, Debug)]
pub struct RoleMap{
    map: HashMap<Role,Vec<IpAddr>>,
}
impl RoleMap {
    fn add_role_ip(&mut self, role: Vec<Role>, ip: IpAddr) {
        for i in role{
            self.map.entry(i).or_insert_with(Vec::new).push(ip);
            }
        }


}




static ROLE_COMPUTE: LazyLock<Mutex<RoleComputeAverage>> =
    LazyLock::new(|| Mutex::new(RoleComputeAverage::default()));


static ROLE_MAP: LazyLock<Mutex<RoleMap>> =
    LazyLock::new(|| Mutex::new(RoleMap::default()));

// Hashmap of nodes and their compute scores, for easy updation every 2-3 minutes

static NODES_COMPUTE_MAP: LazyLock<RwLock<HashMap<IpAddr, NodeCompute>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// Vector of nodes sorted in ascending order based on their compute scores, for easy lookup when allocating roles

static SORTED_NODES_COMPUTE_MAP: LazyLock<RwLock<Vec<(IpAddr, NodeCompute)>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

static UNEMPLOYED_NODES_SORTED_MAP: LazyLock<RwLock<Vec<(IpAddr, NodeCompute)>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));


////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////


//Check whether a node is Employed: In RoleMap
fn is_employed(ip: IpAddr) -> bool {
    let role_map = ROLE_MAP.lock().unwrap();

    role_map
        .map
        .values()
        .any(|ips| ips.contains(&ip))
}

//determine and allocate the roles to the given endpoint 
fn realloc_roles(endpoint: &Node) -> Vec<Role> {
    let avg = ROLE_COMPUTE.lock().unwrap();

    let sigma_avg = avg.sigma_average();
    let consensus_avg = avg.consensus_average();
    let ioc_avg = avg.consensus_average();

    let max = sigma_avg.max(consensus_avg).max(ioc_avg);

    let role = if max <= 70.0 {
        // Everything is below the threshold.
        // Default to Sigma.
        Role::Sigma
    } else if sigma_avg >= consensus_avg && sigma_avg >= ioc_avg {
        // Sigma is the highest.
        Role::Sigma
    } else if consensus_avg >= ioc_avg {
        // Consensus is the highest.
        Role::Consensus
    } else {
        // IOC is the highest.
        Role::IOC
    };

    drop(avg);

    ROLE_COMPUTE
        .lock()
        .unwrap()
        .add_role_compute(vec![role.clone()], endpoint.score);

    ROLE_MAP
        .lock()
        .unwrap()
        .add_role_ip(vec![role.clone()], endpoint.ip_addr);

    vec![role]
}

fn alloc_role(endpoint: &Node) -> Vec<Role> {
    if NODES_COMPUTE_MAP.read().unwrap().len() <= 2 {
        let roles = vec![Role::Sigma, Role::IOC, Role::Consensus];

        ROLE_COMPUTE
            .lock()
            .unwrap()
            .add_role_compute(roles.clone(), endpoint.score);

        ROLE_MAP
            .lock()
            .unwrap()
            .add_role_ip(roles.clone(), endpoint.ip_addr);


        roles
    } else {
        realloc_roles(endpoint)
    }
}

// decide if you need to allocate role currently or if you should just send the node an unemployed section
fn decide_need(node: Node) {
    let has_roles = {
        let avg = ROLE_COMPUTE.lock().unwrap();

        avg.sigma_count > 0
            || avg.ioc_count > 0
            || avg.consensus_count > 0
    };

    if !has_roles {
        alloc_role(&node);
        return;
    }

    let (sigma_avg, consensus_avg, ioc_avg) = {
        let avg = ROLE_COMPUTE.lock().unwrap();

        (
            avg.sigma_average(),
            avg.consensus_average(),
            avg.ioc_average(),
        )
    };

    let employed_nodes = {
    let role_map = ROLE_MAP.lock().unwrap();

        role_map
            .map
            .values()
            .flat_map(|ips| ips.iter())
            .collect::<std::collections::HashSet<_>>()
            .len()
    };

    if sigma_avg <= 70.0
        && ioc_avg < 70.0
        && consensus_avg < 70.0
        && employed_nodes > 3
    {
        add_to_backup(node);
    } else {
        alloc_role(&node);
    }
}

fn add_to_backup(node: Node) {
    let mut backup_list = UNEMPLOYED_NODES_SORTED_MAP.write().unwrap();

    if let Some((_, node_compute)) = backup_list
        .iter_mut()
        .find(|(ip, _)| *ip == node.ip_addr)
    {
        // IP already exists → update its NodeCompute
        node_compute.score = node.score;
        node_compute.state = State::Idle;
    } else {
        // IP doesn't exist → add it
        backup_list.push((
            node.ip_addr,
            NodeCompute {
                score: node.score,
                state: State::Idle,
            },
        ));
    }
}

fn update_nodes(node: Node){
    {let mut nodes = NODES_COMPUTE_MAP.write().unwrap();
    nodes.insert(node.ip_addr, NodeCompute { score:node.score, state: node.state});}
    update_sorted_nodes();
}

fn update_sorted_nodes() {
    // Read the hashmap
    let map = NODES_COMPUTE_MAP.read().unwrap();

    // Copy into a vector
    let mut sorted: Vec<(IpAddr, NodeCompute)> = map
        .iter()
        .map(|(ip, compute)| (*ip, compute.clone()))
        .collect();

    // Sort ascending by score
    sorted.sort_by(|a, b| a.1.score.total_cmp(&b.1.score));

    // Replace the old vector
    *SORTED_NODES_COMPUTE_MAP.write().unwrap() = sorted;
}

//ping the determined result to the endpoint 
fn send_decision(endpoint: Node){
    let roles = alloc_role(&endpoint);
    let decision = EmployedNode{
        ip_addr:endpoint.ip_addr,
        roles: roles,
        score: endpoint.score,

    };
    println!("{:?} \n {:?}\n\n", ROLE_COMPUTE.lock().unwrap(), ROLE_MAP.lock().unwrap());
}

fn simulate_nodes(count: usize) {
    let mut rng = rng();

    for i in 1..=count {
        let node = Node {
            ip_addr: IpAddr::V4(Ipv4Addr::new(
                10,
                0,
                0,
                i as u8,
            )),
            score: rng.random_range(0.0..100.0),
            state: State::Active,
        };

        // Update the node database
        update_nodes(node.clone());

        // Decide whether to employ or keep as backup
        decide_need(node);
    }

    print_all();
}

fn simulate_updates(rounds: usize) {
    let mut rng = rng();

    for _ in 0..rounds {
        let ips: Vec<IpAddr> = {
            let map = NODES_COMPUTE_MAP.read().unwrap();
            map.keys().copied().collect()
        };

        for ip in ips {
            let node = Node {
                ip_addr: ip,
                score: rng.random_range(0.0..100.0),
                state: State::Active,
            };

            update_nodes(node.clone());
            decide_need(node);
        }

        update_sorted_nodes();
    }

    print_all();
}

fn print_all() {
    println!("\n================ NODES_COMPUTE_MAP ================");
    println!("{:#?}", *NODES_COMPUTE_MAP.read().unwrap());

    println!("\n============ SORTED_NODES_COMPUTE_MAP =============");
    println!("{:#?}", *SORTED_NODES_COMPUTE_MAP.read().unwrap());

    println!("\n========== UNEMPLOYED_NODES_SORTED_MAP ============");
    println!("{:#?}", *UNEMPLOYED_NODES_SORTED_MAP.read().unwrap());

    println!("\n================ ROLE_COMPUTE =====================");
    println!("{:#?}", *ROLE_COMPUTE.lock().unwrap());

    println!("\n=================== ROLE_MAP ======================");
    println!("{:#?}", *ROLE_MAP.lock().unwrap());
}

fn main(){
    simulate_nodes(15);
    simulate_updates(20);
}
