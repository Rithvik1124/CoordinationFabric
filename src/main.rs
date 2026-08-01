use std::fs;
use std::{thread, time};

use reqwest::blocking::Client;
use serde::Serialize;




pub struct CPUMetrics{
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq:u64,
    pub softirq: u64,
    pub steal: u64

}

#[derive(Serialize)]
struct CpuPayload {
    cpu_usage: f64,
}

fn main() {
    println!("Started!");
   
    let client = Client::new();

    let server_url = "http://127.0.0.1:8080/cpu";
    
    loop{

        //Delta 1
        let before_30_read = match fs::read("/proc/stat"){
            Ok(v)=>v,
            Err(_) => return , 
        };
        let y = String::from_utf8_lossy(&before_30_read).into_owned();
        let cpu_values_1:Vec<&str> = y.split_whitespace().collect();

        let mut cpu1 = CPUMetrics{
            user:cpu_values_1[1].parse().unwrap(),
            nice:cpu_values_1[2].parse().unwrap(),
            system:cpu_values_1[3].parse().unwrap(),
            idle:cpu_values_1[4].parse().unwrap(),
            iowait:cpu_values_1[5].parse().unwrap(),
            irq:cpu_values_1[6].parse().unwrap(),
            softirq:cpu_values_1[7].parse().unwrap(),
            steal:cpu_values_1[8].parse().unwrap(),
        };

        let ten_millis = time::Duration::from_millis(30000);
        thread::sleep(ten_millis);
        
        let after_30_read = match fs::read("/proc/stat"){
            Ok(v)=>v,
            Err(_) => return , 
        };
        let y = String::from_utf8_lossy(&after_30_read).into_owned();
        let cpu_values_2:Vec<&str> = y.split_whitespace().collect();
        let mut cpu2 = CPUMetrics{
            user:cpu_values_2[1].parse().unwrap(),
            nice:cpu_values_2[2].parse().unwrap(),
            system:cpu_values_2[3].parse().unwrap(),
            idle:cpu_values_2[4].parse().unwrap(),
            iowait:cpu_values_2[5].parse().unwrap(),
            irq:cpu_values_2[6].parse().unwrap(),
            softirq:cpu_values_2[7].parse().unwrap(),
            steal:cpu_values_2[8].parse().unwrap(),
        };

        let total_jiffies = (cpu2.user-cpu1.user)+
                         (cpu2.nice-cpu1.nice)+
                         (cpu2.system-cpu1.system)+
                         (cpu2.idle-cpu1.idle)+
                         (cpu2.iowait-cpu1.iowait)+
                         (cpu2.irq-cpu1.irq)+
                         (cpu2.softirq-cpu1.softirq)+
                         (cpu2.steal-cpu1.steal)        ;
        let busy_jiffies = total_jiffies-(cpu2.idle-cpu1.idle);
        let curr_compute: f64 = (busy_jiffies as f64/total_jiffies as f64)*100.0;
        println!("total_jiffies:{} busy_jiffies:{} curr_compute: {}",total_jiffies, busy_jiffies,curr_compute);      


        let payload = CpuPayload {
            cpu_usage: curr_compute,
        };

        match client.post(server_url).json(&payload).send() {
            Ok(resp) => println!("Sent ({})", resp.status()),
            Err(e) => println!("Failed to send: {}", e),
        }



    }
   


    
}
