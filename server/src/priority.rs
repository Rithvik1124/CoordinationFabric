use crate::structures::CPUPayload;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{LazyLock, Mutex},
};

static PRIOR_MAP: LazyLock<Mutex<HashMap<IpAddr, CPUPayload>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct AvgRolePerf{
    sigma: f64,
    ioc: f64,
    consensus:f64
}

// impl AvgRolePerf{
//     fn update(&mut self){
//         self.sigma = 
//     }
// }

pub fn add_node_to_list(ip_addr: IpAddr, perf_vals: CPUPayload) {
    let mut map = PRIOR_MAP.lock().unwrap();
    map.insert(ip_addr, perf_vals);

    println!("{:?}", *map);
}