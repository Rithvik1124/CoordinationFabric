use serde::Serialize;
use std::{
    env,
    fs,
    io,
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
struct CpuCounters {
    total: u64,
    idle: u64,
}

#[derive(Debug, Serialize)]
struct MetricReport {
    node_id: String,
    advertise_url: Option<String>,
    avg_score: f64,
    cpu_usage: f64,
    mem_usage: f64,
    process_latency_ms: f64,
}

impl MetricReport {
    fn collect(node_id: String, advertise_url: Option<String>) -> io::Result<Self> {
        let cpu_usage = read_cpu_usage()?;
        let mem_usage = read_memory_usage()?;
        // The archived project does not include the EDR event-processing hook.
        // Wire EDR_PROCESS_LATENCY_MS to that hook when integrating; zero is a
        // neutral prototype default, not a claim about event processing latency.
        let process_latency_ms = env::var("EDR_PROCESS_LATENCY_MS")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(0.0);
        let avg_score = capacity_score(cpu_usage, mem_usage, process_latency_ms);

        Ok(Self {
            node_id,
            advertise_url,
            avg_score,
            cpu_usage,
            mem_usage,
            process_latency_ms,
        })
    }
}

fn read_cpu_counters() -> io::Result<CpuCounters> {
    let contents = fs::read_to_string("/proc/stat")?;
    let line = contents
        .lines()
        .find(|line| line.starts_with("cpu "))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "aggregate CPU line missing"))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .map(|v| v.parse::<u64>().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)))
        .collect::<Result<_, _>>()?;
    if values.len() < 8 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "incomplete /proc/stat CPU counters"));
    }
    // Linux guest/guest_nice are already included in user/nice, so only sum
    // the first eight counters to avoid double-counting them.
    let total = values.iter().take(8).sum();
    let idle = values[3].saturating_add(values[4]); // idle + iowait
    Ok(CpuCounters { total, idle })
}

fn read_cpu_usage() -> io::Result<f64> {
    let sample_ms = env::var("CPU_SAMPLE_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1000)
        .clamp(100, 5000);
    let first = read_cpu_counters()?;
    thread::sleep(Duration::from_millis(sample_ms));
    let second = read_cpu_counters()?;
    let total_delta = second.total.saturating_sub(first.total);
    let idle_delta = second.idle.saturating_sub(first.idle);
    if total_delta == 0 {
        return Ok(0.0);
    }
    let busy = total_delta.saturating_sub(idle_delta.min(total_delta));
    Ok((busy as f64 / total_delta as f64 * 100.0).clamp(0.0, 100.0))
}

fn read_memory_usage() -> io::Result<f64> {
    let contents = fs::read_to_string("/proc/meminfo")?;
    let parse_kib = |key: &str| -> io::Result<u64> {
        let line = contents
            .lines()
            .find(|line| line.starts_with(key))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("{key} missing")))?;
        line.split_whitespace()
            .nth(1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("{key} value missing")))?
            .parse::<u64>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    };

    let total = parse_kib("MemTotal:")?;
    let available = parse_kib("MemAvailable:")?;
    if total == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "MemTotal is zero"));
    }
    let used = total.saturating_sub(available.min(total));
    Ok((used as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

fn capacity_score(cpu: f64, memory: f64, latency_ms: f64) -> f64 {
    let cpu_headroom = 100.0 - cpu.clamp(0.0, 100.0);
    let memory_headroom = 100.0 - memory.clamp(0.0, 100.0);
    let latency_score = (100.0 * (1.0 - latency_ms / 250.0)).clamp(0.0, 100.0);
    (cpu_headroom * 0.50 + memory_headroom * 0.35 + latency_score * 0.15).clamp(0.0, 100.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scheduler_url = env::var("SCHEDULER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080/cpu".to_string());
    let node_id = env::var("NODE_ID").unwrap_or_else(|_| {
        env::var("HOSTNAME").unwrap_or_else(|_| "node-unknown".to_string())
    });
    let advertise_url = env::var("NODE_ADVERTISE_URL").ok();
    let interval_secs = env::var("METRIC_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(15)
        .clamp(5, 3600);
    let http_timeout_secs = env::var("METRIC_HTTP_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5)
        .clamp(1, 30);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(http_timeout_secs))
        .build()?;

    println!("Metric agent for node '{node_id}' reporting to {scheduler_url} every {interval_secs}s");
    println!("Higher capacity score means more available headroom; role assignments are decided by the scheduler.");

    loop {
        let started = Instant::now();
        match MetricReport::collect(node_id.clone(), advertise_url.clone()) {
            Ok(report) => {
                println!(
                    "node={} score={:.2} cpu={:.2}% memory={:.2}% process_latency={:.2}ms",
                    report.node_id, report.avg_score, report.cpu_usage, report.mem_usage, report.process_latency_ms
                );
                match client.post(&scheduler_url).json(&report).send() {
                    Ok(response) => {
                        let status = response.status();
                        match response.error_for_status() {
                            Ok(response) => match response.text() {
                                Ok(body) => println!("scheduler accepted metrics ({status}): {body}"),
                                Err(err) => eprintln!("could not read scheduler response: {err}"),
                            },
                            Err(err) => eprintln!("scheduler rejected metrics ({status}): {err}"),
                        }
                    }
                    Err(err) => eprintln!("could not send metrics to scheduler: {err}"),
                }
            }
            Err(err) => eprintln!("could not collect node metrics: {err}"),
        }

        let elapsed = started.elapsed();
        thread::sleep(Duration::from_secs(interval_secs).saturating_sub(elapsed));
    }
}
