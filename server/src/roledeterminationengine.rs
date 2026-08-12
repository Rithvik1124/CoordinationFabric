use serde::Deserialize;
use slotmap::{DefaultKey, SlotMap};
use std::{
    collections::{HashMap, HashSet}, hash::Hash, net::{IpAddr,Ipv4Addr}, sync::{LazyLock, Mutex, RwLock},
};
use crate::{Role::{Consensus, IOC, Sigma}, State::Idle};
use rand::{Rng, rng, RngExt};
////////////////////////////////////////////////////STRUCTURES//////////////////////////////////////////////////////////////

//New Node
const THRESHOLD: f64 = 60.0;


#[derive(Eq, Hash, PartialEq, Clone, Debug, Deserialize)]
enum Role {
    Sigma,
    IOC,
    Consensus,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
enum State {
    Active,
    #[default]
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


type NodeId = DefaultKey;

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct Node {
    ip: IpAddr,
    score: f64,
    state: State,
    roles: Vec<Role>,
}

impl Node{

}

struct Scheduler {
    nodes: SlotMap<NodeId, Node>,
    by_ip: HashMap<IpAddr, NodeId>,
    by_role: HashMap<Role, HashSet<NodeId>>,
    sorted_by_score: Vec<NodeId>,
}

impl Scheduler {
    // add node to nodes
    fn add_node(&mut self, node: Node)-> NodeId {
        let ip_addr = node.ip;
        let node_id = self.nodes.insert(node);
        self.by_ip.insert(ip_addr, node_id);
        self.decide_need(node_id);
        self.rebuild_sorted_nodes();
        node_id
    }  

    fn get_role_computes(&self) -> HashMap<Role, f64> {
        HashMap::from([
            (Sigma, self.role_average(Sigma)),
            (IOC, self.role_average(IOC)),
            (Consensus, self.role_average(Consensus)),
        ])
    }

    // sort sorted_by_scores
    fn rebuild_sorted_nodes(&mut self) {
        let mut ids: Vec<NodeId> = self.nodes.keys().collect();

        ids.sort_by(|a, b| {
            let score_a = self.nodes[*a].score;
            let score_b = self.nodes[*b].score;

            score_a.total_cmp(&score_b)
        });

        self.sorted_by_score = ids;
    }
    // 
    fn change_role(&mut self, node_id: NodeId, roles: &Vec<Role>) {
        // get old roles first
        let old_roles = match self.nodes.get(node_id) {
            Some(node) => node.roles.clone(),
            None => return,
        };

        // removing node from old role indexes
        for role in old_roles {
            if let Some(nodes) = self.by_role.get_mut(&role) {
                nodes.remove(&node_id);
            }
        }

        // update the actual Node
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.roles = roles.clone();
        }

        // add node to new role indexes
        for role in roles {
            self.by_role
                .entry(role.clone())
                .or_default()
                .insert(node_id);
        }
    }
    
    fn remove_from_by_role(&mut self, node_id: NodeId){
        for i in &self.by_role[&Sigma]{
            if *i== node_id{
                self.by_role[&Sigma].clone();

            }

        } 
        for i in &self.by_role[&Consensus]{
            if *i== node_id{
                self.by_role[&Consensus].clone();

            }

        } 
        for i in &self.by_role[&IOC]{
            if *i== node_id{
                self.by_role[&IOC].clone();

            }

        } 
    }
    fn check_overload(&mut self)-> Vec<NodeId>{
        let mut overloaded_nodes: Vec<NodeId> = Vec::new();
        for i in self.nodes.keys(){
            if self.nodes[i].score>THRESHOLD{
                overloaded_nodes.push(i);
            }

        }
        overloaded_nodes
}

    fn remove_node(&mut self, nodeId: NodeId){
        let ip_addr = self.nodes.get(nodeId).unwrap().ip;
        self.nodes.remove(nodeId);
        self.by_ip.remove(&ip_addr);

        self.remove_from_by_role(nodeId);
        self.rebuild_sorted_nodes();

    }

    fn get_node_by_ip(&mut self, ip_addr: IpAddr)-> Option<NodeId>{
    self.by_ip.get(&ip_addr).copied()
    }

    fn get_sorted_unemployed_nodes(&mut self,)-> Vec<NodeId>{
        let mut unemployed_nodes: Vec<NodeId> = Vec::new();
        for &i in &self.sorted_by_score{
            if self.nodes.get(i).unwrap().state == Idle{
                unemployed_nodes.push(i);

            }
            else{

            }            
        }
        unemployed_nodes
    }

    fn role_average(&self, role: Role) -> f64 {
        let nodes = match self.by_role.get(&role) {
            Some(nodes) => nodes,
            None => return 0.0,
        };

        if nodes.is_empty() {
            return 0.0;
        }

        let total: f64 = nodes
            .iter()
            .filter_map(|id| self.nodes.get(*id))
            .map(|node| node.score)
            .sum();

        total / nodes.len() as f64
    }

    fn get_lowest_node_for_role(&self, role: &Role, exclude_node: NodeId,) -> Option<NodeId> {
        self.by_role
            .get(&role)?
            .iter()
            .filter_map(|&id| {
                if id == exclude_node {
                    return None;
                }

                let node = self.nodes.get(id)?;
                Some((id, node.score))
            })
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(id, _)| id)
    }

    fn decide_need(&mut self, node_id: NodeId) {
        let role_computes = self.get_role_computes();

        let sigma_avg = role_computes[&Sigma];
        let consensus_avg = role_computes[&Consensus];
        let ioc_avg = role_computes[&IOC];

        let overloaded_nodes = self.check_overload();

        // 1. Deal with currently overloaded nodes first.
        if !overloaded_nodes.is_empty() {
            for overloaded_node in overloaded_nodes {
                self.share_load(overloaded_node);
            }

            return;
        }

        // 2. No node is individually overloaded.
        //    Check whether any role has a high average compute.
        let role_to_allocate = if sigma_avg >= THRESHOLD {
            Some(Sigma)
        } else if ioc_avg >= THRESHOLD {
            Some(IOC)
        } else if consensus_avg >= THRESHOLD {
            Some(Consensus)
        } else {
            None
        };

        match role_to_allocate {
            Some(role) => {
                self.alloc_role(node_id, vec![role]);
            }

            None => {
                // No role currently needs another node.
                if let Some(node) = self.nodes.get_mut(node_id) {
                    node.state = Idle;
                    node.roles.clear();
                }
            }
        }
    }


    fn share_load(&mut self, overloaded_node: NodeId) {
        let roles = match self.nodes.get(overloaded_node) {
            Some(node) => node.roles.clone(),
            None => return,
        };

        if roles.is_empty() {
            return;
        }

        let role_computes = self.get_role_computes();

        let overloaded_role = roles
            .into_iter()
            .max_by(|a, b| {
                role_computes[a]
                    .total_cmp(&role_computes[b])
            });

        let role = match overloaded_role {
            Some(role) => role,
            None => return,
        };

        // First preference: an unemployed node.
        let unemployed_node = self
            .get_sorted_unemployed_nodes()
            .into_iter()
            .next();

        if let Some(node_id) = unemployed_node {
            self.alloc_role(node_id, vec![role]);

            return;
        }

        // No unemployed node exists.
        // Find the lowest-compute node already serving this role.
        if let Some(node_id) = self.get_lowest_node_for_role(&role, overloaded_node) {
            // For now, this is where the actual workload redistribution will happen.
            // e.g. reduce overloaded_node.score increase node_id.score
            // Actual work redistribution can be implemented later.
            println!(
                "Share {:?} load between {:?} and {:?}",
                role,
                overloaded_node,
                node_id
            );
        }
    }

        fn alloc_role(&mut self, node_id: NodeId, roles: Vec<Role>) {
            self.change_role(node_id, &roles);

            if let Some(node) = self.nodes.get_mut(node_id) {
                node.state = State::Active;
            }
}


}



////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

fn make_nodes(){

}



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
    let mut nodes_map = NODES_COMPUTE_MAP.write().unwrap();
    if let Some(node) = nodes_map.get_mut(&endpoint.ip_addr) {
        node.state = State::Active;
    }
    let avg = ROLE_COMPUTE.lock().unwrap();

    let sigma_avg = avg.sigma_average();
    let consensus_avg = avg.consensus_average();
    let ioc_avg = avg.ioc_average();

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


fn get_node_roles(ip: IpAddr) -> Vec<Role> {
    let role_map = ROLE_MAP.lock().unwrap();

    role_map
        .map
        .iter()
        .filter(|(_, ips)| ips.contains(&ip))
        .map(|(role, _)| role.clone())
        .collect()
}

fn get_node_state(ip: IpAddr)-> State{
    let role_map = NODES_COMPUTE_MAP.read().unwrap();

    return role_map[&ip].state.clone();
}

//give it a better name please
fn check_overload(){
    let sorted_nodes_map: std::sync::RwLockReadGuard<'_, Vec<(IpAddr, NodeCompute)>> = SORTED_NODES_COMPUTE_MAP.read().unwrap();
    for i in (0..sorted_nodes_map.len()).rev(){
        if sorted_nodes_map[i].1.score>= THRESHOLD && get_node_state(sorted_nodes_map[i].0)==State::Active{
            let roles = get_node_roles(sorted_nodes_map[i].0);
            let overloaded_ip = sorted_nodes_map[i].0;
            share_load(overloaded_ip, roles);
        }
        else if sorted_nodes_map[i].1.score<= THRESHOLD{
            return 
        }
    }


}


//FIND OUT A NODE THAT IS IDLE WITH LOW COMPUT - IF NOT THEN AN ACTIVE NODE WITH THE LOWEST COMPUT AND SHARE IT WITH THE OVERLOADED NODE
fn share_load(overloaded_ip:IpAddr, roles: Vec<Role>){
    //get unemployed nodes
    //check if unemployed nodes exist
    //if not then use the active nodes within our network with the lowest computes - from sorted_nodes_compute
    //make a remove_roles function, and add the role(s) to the unemployed or low compute guy else

}

fn alloc_role(endpoint: &Node) -> Vec<Role> {
    if NODES_COMPUTE_MAP.read().unwrap().len() <= 2 {
        let mut nodes_map = NODES_COMPUTE_MAP.write().unwrap();

        if let Some(node) = nodes_map.get_mut(&endpoint.ip_addr) {
            node.state = State::Active;
        }
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
fn decide_need(mut node: Node) {
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

    let nodes = NODES_COMPUTE_MAP.read().unwrap();
    if sigma_avg <= 70.0
        && ioc_avg < 70.0
        && consensus_avg < 70.0
        && employed_nodes > 3 
        && nodes.get(&node.ip_addr).unwrap().state==State::Idle
    {
        drop(nodes);
        add_to_backup(node);
    } 
    else if nodes.get(&node.ip_addr).unwrap().state==State::Active{
        drop(nodes);
        realloc_roles(&node);
        
    }
    else {
        drop(nodes);
        alloc_role(&node);
    }
}


fn add_to_backup(node: Node) {
    {let mut backup_list = UNEMPLOYED_NODES_SORTED_MAP.write().unwrap();

    backup_list.insert(
            node.ip_addr
            );}
}

fn remove_from_backup(node: Node){
    {let mut backup_list = UNEMPLOYED_NODES_SORTED_MAP.write().unwrap();

    backup_list.remove(
            &node.ip_addr
            );}

}
fn update_nodes(node: Node) {
    let mut nodes = NODES_COMPUTE_MAP.write().unwrap();

    let current_state = nodes
        .get(&node.ip_addr)
        .map(|node| node.state.clone())
        .unwrap_or(State::Idle);

    nodes.insert(
        node.ip_addr,
        NodeCompute {
            score: node.score,
            state: current_state,
        },
    );

    drop(nodes);

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
        host_ip_addr:endpoint.ip_addr,
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
