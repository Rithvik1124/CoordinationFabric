// use crate::structures::{EmployedEndpoint, CPUPayload};

//if only one endpoint - all 3 tasks
//if 2 endpoints - all 3 tasks for both
// if 3 endpoint or more, then start dividing stuff
use serde::Deserialize;
use std::{
    collections::HashMap,
    net::{IpAddr,Ipv4Addr},
    sync::{LazyLock, Mutex},
};

use crate::Role::{Consensus, IOC, Sigma};
#[derive(Debug, Deserialize, Clone)]
pub struct CPUPayload {
    avg_score: f64,
    cpu_usage: f64,
    mem_usage: f64
}

//somehow these endpoints are more employed than me TwT
#[derive(Debug, Deserialize)]

pub struct EmployedEndpoint{
    roles: Vec<Role>,
    ip_addr: IpAddr,    

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
}

#[derive(Debug, Deserialize, Clone)]

pub struct UnemployedEndpoint{
    ip_addr: IpAddr,
    score: CPUPayload

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

////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

//determine and allocate the roles to the given endpoint
static PRIOR_MAP: LazyLock<Mutex<HashMap<IpAddr, CPUPayload>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));


static ROLE_COMPUTE: LazyLock<Mutex<RoleComputeAverage>> =
    LazyLock::new(|| Mutex::new(RoleComputeAverage::default()));

static ROLE_MAP: LazyLock<Mutex<RoleMap>> =
    LazyLock::new(|| Mutex::new(RoleMap::default()));

fn alloc_role(endpoint: &UnemployedEndpoint)-> Vec<Role>{
    let mut roles:Vec<Role> = Vec::new();
    if PRIOR_MAP.lock().unwrap().len()==1 || PRIOR_MAP.lock().unwrap().len()==2{
        roles.push(Sigma);
        roles.push(IOC);
        roles.push(Consensus);
    let mut avg = ROLE_COMPUTE.lock().unwrap();
    avg.add_role_compute(roles.clone(), endpoint.score.avg_score);
    ROLE_MAP.lock().unwrap().add_role_ip(roles.clone(), endpoint.ip_addr);
    }
    else{
        
    }
    roles

}


//ping the determined result to the endpoint 
fn send_decision(endpoint: UnemployedEndpoint){
    let roles = alloc_role(&endpoint);
    let decision = EmployedEndpoint{
        ip_addr:endpoint.ip_addr,
        roles: roles

    };
    println!("{:?} \n {:?}", ROLE_COMPUTE.lock().unwrap(), ROLE_MAP.lock().unwrap());

    

}

fn main(){
    let mut map:Vec<UnemployedEndpoint>= Vec::new();
    let u1 = UnemployedEndpoint{
            ip_addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            score: CPUPayload{avg_score: 90.77,
            cpu_usage: 89.98,
            mem_usage: 67.67}
        };
    PRIOR_MAP
    .lock()
    .unwrap()
    .insert(u1.clone().ip_addr, u1.clone().score);
    send_decision( u1);
    let u2 = UnemployedEndpoint{
            ip_addr: IpAddr::V4(Ipv4Addr::new(123, 0, 0, 1)),
            score: CPUPayload{avg_score: 90.77,
            cpu_usage: 89.98,
            mem_usage: 67.67}
        };
    PRIOR_MAP
    .lock()
    .unwrap()
    .insert(u2.clone().ip_addr, u2.clone().score);
    send_decision( u2);
    let u3 = UnemployedEndpoint{
            ip_addr: IpAddr::V4(Ipv4Addr::new(128, 0, 0, 1)),
            score: CPUPayload{avg_score: 90.77,
            cpu_usage: 89.98,
            mem_usage: 67.67}
        };
    PRIOR_MAP
    .lock()
    .unwrap()
    .insert(u3.clone().ip_addr, u3.clone().score);
    send_decision( u3);

        
}



// //scored numbers on the basis of their core usages
// fn sigma_score(m: &Metrics) -> f64 {
//     0.6 * m.cpu_benchmark
//     + 0.2 * m.memory_bandwidth
//     + 0.2 * m.hash_throughput
// }

// fn consensus_score(m: &Metrics) -> f64 {
//     0.5 * m.uptime
//     + 0.3 * (1.0 / m.network_latency)
//     + 0.2 * m.cpu_benchmark
// }

// fn ioc_score(m: &Metrics) -> f64 {
//     0.5 * m.disk_iops
//     + 0.3 * m.cpu_benchmark
//     + 0.2 * m.memory_bandwidth
// }