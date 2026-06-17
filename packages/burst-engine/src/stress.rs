use crate::scheduler::{EventDispatcher, SchedulerHandle, SchedulerStats};
use qzh_profile::key_id::KeyId;
use qzh_profile::profile::{BurstMode, BurstRule};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use win_input::{DispatchResult, InputEvent};

#[derive(Debug, Clone)]
pub struct StressConfig {
    pub rules: usize,
    pub interval_ms: u32,
    pub duration: Duration,
    pub same_target: bool,
}

impl Default for StressConfig {
    fn default() -> Self {
        Self {
            rules: 64,
            interval_ms: 10,
            duration: Duration::from_secs(5),
            same_target: false,
        }
    }
}

#[derive(Debug)]
pub struct StressReport {
    pub rules: usize,
    pub interval_ms: u32,
    pub duration_ms: u128,
    pub same_target: bool,
    pub scheduler_threads: usize,
    pub hp_degraded: bool,
    pub dispatch_batches: u64,
    pub sent_events: u64,
    pub failed_events: u64,
    pub injection_rate_per_sec: f64,
    pub delay_samples: usize,
    pub delay_p50_us: u128,
    pub delay_p95_us: u128,
    pub delay_p99_us: u128,
    pub delay_max_us: u128,
    pub stop_ack_us: u128,
    pub process_cpu_ms: Option<u128>,
}

struct DryRunDispatcher;

impl EventDispatcher for DryRunDispatcher {
    fn dispatch(&self, events: &[InputEvent]) -> Vec<DispatchResult> {
        vec![DispatchResult::Sent; events.len()]
    }
}

pub fn run_stress(config: StressConfig) -> StressReport {
    let stats = Arc::new(SchedulerStats::default());
    let scheduler = SchedulerHandle::start_with_dispatcher(
        Arc::new(DryRunDispatcher),
        Some(stats.clone()),
        Arc::new(Mutex::new(HashSet::new())),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(|_| {}),
    );
    let cpu_start = process_cpu_time_100ns();
    let started = Instant::now();
    let rules = make_rules(&config);

    for rule in rules {
        scheduler.start_rule(rule, 0);
    }

    thread::sleep(config.duration);

    let stop_started = Instant::now();
    let _ = scheduler.stop_all_blocking(1);
    let stop_ack = stop_started.elapsed();
    let elapsed = started.elapsed();
    let cpu_end = process_cpu_time_100ns();
    let hp_degraded = scheduler.hp_degraded();
    let _ = scheduler.shutdown_blocking(2);
    let snapshot = stats.snapshot();
    let delays = sorted(snapshot.delay_ns);

    StressReport {
        rules: config.rules,
        interval_ms: config.interval_ms,
        duration_ms: elapsed.as_millis(),
        same_target: config.same_target,
        scheduler_threads: 1,
        hp_degraded,
        dispatch_batches: snapshot.batches,
        sent_events: snapshot.sent_events,
        failed_events: snapshot.failed_events,
        injection_rate_per_sec: snapshot.sent_events as f64 / elapsed.as_secs_f64().max(0.001),
        delay_samples: delays.len(),
        delay_p50_us: percentile_us(&delays, 50),
        delay_p95_us: percentile_us(&delays, 95),
        delay_p99_us: percentile_us(&delays, 99),
        delay_max_us: delays.last().copied().unwrap_or(0) / 1_000,
        stop_ack_us: stop_ack.as_micros(),
        process_cpu_ms: cpu_start
            .zip(cpu_end)
            .map(|(start, end)| end.saturating_sub(start) as u128 / 10_000),
    }
}

impl StressReport {
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"rules\":{},\"interval_ms\":{},\"duration_ms\":{},\"same_target\":{},\"scheduler_threads\":{},\"hp_degraded\":{},\"dispatch_batches\":{},\"sent_events\":{},\"failed_events\":{},\"injection_rate_per_sec\":{:.2},\"delay_samples\":{},\"delay_p50_us\":{},\"delay_p95_us\":{},\"delay_p99_us\":{},\"delay_max_us\":{},\"stop_ack_us\":{},\"process_cpu_ms\":{}}}",
            self.rules,
            self.interval_ms,
            self.duration_ms,
            self.same_target,
            self.scheduler_threads,
            self.hp_degraded,
            self.dispatch_batches,
            self.sent_events,
            self.failed_events,
            self.injection_rate_per_sec,
            self.delay_samples,
            self.delay_p50_us,
            self.delay_p95_us,
            self.delay_p99_us,
            self.delay_max_us,
            self.stop_ack_us,
            self.process_cpu_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string()),
        )
    }
}

fn make_rules(config: &StressConfig) -> Vec<Arc<BurstRule>> {
    (0..config.rules)
        .map(|i| {
            Arc::new(BurstRule {
                id: format!("stress-{i}"),
                enabled: true,
                trigger_key: KeyId::Keyboard(0x1000 + i as u32),
                target_key: if config.same_target {
                    KeyId::Keyboard(0x2000)
                } else {
                    KeyId::Keyboard(0x2000 + i as u32)
                },
                mode: BurstMode::Toggle,
                stop_key: None,
                interval_ms: config.interval_ms,
                group: None,
            })
        })
        .collect()
}

fn sorted(mut values: Vec<u128>) -> Vec<u128> {
    values.sort_unstable();
    values
}

fn percentile_us(values: &[u128], percentile: usize) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let idx = ((values.len() - 1) * percentile) / 100;
    values[idx] / 1_000
}

#[cfg(windows)]
fn process_cpu_time_100ns() -> Option<u64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    unsafe {
        let mut creation = std::mem::zeroed::<FILETIME>();
        let mut exit = std::mem::zeroed::<FILETIME>();
        let mut kernel = std::mem::zeroed::<FILETIME>();
        let mut user = std::mem::zeroed::<FILETIME>();
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        ) == 0
        {
            return None;
        }
        Some(filetime_to_u64(kernel).saturating_add(filetime_to_u64(user)))
    }
}

#[cfg(windows)]
fn filetime_to_u64(ft: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

#[cfg(not(windows))]
fn process_cpu_time_100ns() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stress_report_emits_json_line_with_core_fields() {
        let report = StressReport {
            rules: 1,
            interval_ms: 10,
            duration_ms: 100,
            same_target: false,
            scheduler_threads: 1,
            hp_degraded: false,
            dispatch_batches: 2,
            sent_events: 4,
            failed_events: 0,
            injection_rate_per_sec: 40.0,
            delay_samples: 2,
            delay_p50_us: 1,
            delay_p95_us: 2,
            delay_p99_us: 2,
            delay_max_us: 2,
            stop_ack_us: 100,
            process_cpu_ms: Some(5),
        };

        let line = report.to_json_line();
        assert!(line.contains("\"rules\":1"));
        assert!(line.contains("\"process_cpu_ms\":5"));
    }
}
