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

struct Connection {
    from: NodeId,
    to: NodeId
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
    fn check_overload(&self)-> Vec<NodeId>{
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


    fn simulate(&mut self, steps: usize) {
        println!("=== Scheduler Simulation ===");

        for step in 0..steps {
            println!("\n--- Step {} ---", step + 1);

            // Simulate workload changing on every active node.
            let ids: Vec<NodeId> = self.nodes.keys().collect();

            for id in ids {
                if let Some(node) = self.nodes.get_mut(id) {
                    if node.state == State::Active {
                        // Deterministic workload fluctuation.
                        let change = match step % 5 {
                            0 => 8.0,
                            1 => -3.0,
                            2 => 12.0,
                            3 => -5.0,
                            _ => 6.0,
                        };

                        node.score = (node.score + change).max(0.0);
                    }
                }
            }

            self.rebuild_sorted_nodes();

            // React to the new workload.
            let ids: Vec<NodeId> = self.nodes.keys().collect();

            for id in ids {
                self.decide_need(id);
            }

            self.rebuild_sorted_nodes();

            self.print_status();
        }
    }

    fn print_status(&self) {
        println!("Nodes:");

        for id in &self.sorted_by_score {
            if let Some(node) = self.nodes.get(*id) {
                println!(
                    "  {:?}: score={:.1}, state={:?}, roles={:?}",
                    id,
                    node.score,
                    node.state,
                    node.roles
                );
            }
        }

        println!("\nRole averages:");

        for (role, average) in self.get_role_computes() {
            println!("  {:?}: {:.2}", role, average);
        }

        let overloaded = self.check_overload();

        println!(
            "\nOverloaded nodes: {}",
            overloaded.len()
        );
    }

}

fn main() {
    let mut scheduler = Scheduler {
        nodes: SlotMap::with_key(),
        by_ip: HashMap::new(),
        by_role: HashMap::new(),
        sorted_by_score: Vec::new(),
    };

    scheduler.add_node(Node {
        ip: "10.0.0.1".parse().unwrap(),
        score: 70.0,
        state: State::Idle,
        roles: vec![],
    });

    scheduler.add_node(Node {
        ip: "10.0.0.2".parse().unwrap(),
        score: 80.0,
        state: State::Idle,
        roles: vec![],
    });

    scheduler.add_node(Node {
        ip: "10.0.0.3".parse().unwrap(),
        score: 95.0,
        state: State::Idle,
        roles: vec![],
    });

    scheduler.add_node(Node {
        ip: "10.0.0.4".parse().unwrap(),
        score: 40.0,
        state: State::Idle,
        roles: vec![],
    });

    scheduler.simulate(20);
}


