use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

use crate::revive;

const DELAY_SAMPLE_LIMIT: usize = 4096;

pub(crate) struct EngineMetrics {
    started_at: Instant,
    active_rules: AtomicUsize,
    injected_events: AtomicU64,
    scheduler_steps: AtomicU64,
    skipped_pulses: AtomicU64,
    stop_commands: AtomicU64,
    delay_samples_us: Mutex<VecDeque<u64>>,
    hook_samples_us: Mutex<VecDeque<u64>>,
    stop_response_samples_us: Mutex<VecDeque<u64>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineMetricsSnapshot {
    pub active_rules: usize,
    pub injected_events: u64,
    pub injection_rate_per_sec: f64,
    pub scheduler_steps: u64,
    pub skipped_pulses: u64,
    pub stop_commands: u64,
    pub delay_sample_count: usize,
    pub delay_p50_us: u64,
    pub delay_p95_us: u64,
    pub delay_p99_us: u64,
    pub delay_max_us: u64,
    pub hook_sample_count: usize,
    pub hook_p50_us: u64,
    pub hook_p95_us: u64,
    pub hook_p99_us: u64,
    pub hook_max_us: u64,
    pub stop_response_sample_count: usize,
    pub stop_response_p50_us: u64,
    pub stop_response_p95_us: u64,
    pub stop_response_p99_us: u64,
    pub stop_response_max_us: u64,
}

impl EngineMetrics {
    pub(crate) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            active_rules: AtomicUsize::new(0),
            injected_events: AtomicU64::new(0),
            scheduler_steps: AtomicU64::new(0),
            skipped_pulses: AtomicU64::new(0),
            stop_commands: AtomicU64::new(0),
            delay_samples_us: Mutex::new(VecDeque::with_capacity(DELAY_SAMPLE_LIMIT)),
            hook_samples_us: Mutex::new(VecDeque::with_capacity(DELAY_SAMPLE_LIMIT)),
            stop_response_samples_us: Mutex::new(VecDeque::with_capacity(DELAY_SAMPLE_LIMIT)),
        }
    }

    pub(crate) fn set_active_rules(&self, count: usize) {
        self.active_rules.store(count, Ordering::Relaxed);
    }

    pub(crate) fn add_injected_events(&self, count: usize) {
        self.injected_events
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub(crate) fn add_scheduler_step(&self) {
        self.scheduler_steps.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn add_skipped_pulse(&self) {
        self.skipped_pulses.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn add_stop_command(&self) {
        self.stop_commands.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_delay(&self, delay: Duration) {
        push_duration_sample(&self.delay_samples_us, delay);
    }

    #[cfg(windows)]
    pub(crate) fn record_hook_callback(&self, delay: Duration) {
        push_duration_sample(&self.hook_samples_us, delay);
    }

    pub(crate) fn record_stop_response(&self, delay: Duration) {
        push_duration_sample(&self.stop_response_samples_us, delay);
    }

    pub(crate) fn snapshot(&self) -> EngineMetricsSnapshot {
        let delay_samples = sorted_samples(&self.delay_samples_us);
        let hook_samples = sorted_samples(&self.hook_samples_us);
        let stop_response_samples = sorted_samples(&self.stop_response_samples_us);
        let injected_events = self.injected_events.load(Ordering::Relaxed);
        let elapsed = self.started_at.elapsed().as_secs_f64().max(0.001);
        EngineMetricsSnapshot {
            active_rules: self.active_rules.load(Ordering::Relaxed),
            injected_events,
            injection_rate_per_sec: injected_events as f64 / elapsed,
            scheduler_steps: self.scheduler_steps.load(Ordering::Relaxed),
            skipped_pulses: self.skipped_pulses.load(Ordering::Relaxed),
            stop_commands: self.stop_commands.load(Ordering::Relaxed),
            delay_sample_count: delay_samples.len(),
            delay_p50_us: percentile(&delay_samples, 50),
            delay_p95_us: percentile(&delay_samples, 95),
            delay_p99_us: percentile(&delay_samples, 99),
            delay_max_us: delay_samples.last().copied().unwrap_or(0),
            hook_sample_count: hook_samples.len(),
            hook_p50_us: percentile(&hook_samples, 50),
            hook_p95_us: percentile(&hook_samples, 95),
            hook_p99_us: percentile(&hook_samples, 99),
            hook_max_us: hook_samples.last().copied().unwrap_or(0),
            stop_response_sample_count: stop_response_samples.len(),
            stop_response_p50_us: percentile(&stop_response_samples, 50),
            stop_response_p95_us: percentile(&stop_response_samples, 95),
            stop_response_p99_us: percentile(&stop_response_samples, 99),
            stop_response_max_us: stop_response_samples.last().copied().unwrap_or(0),
        }
    }
}

fn push_duration_sample(samples: &Mutex<VecDeque<u64>>, duration: Duration) {
    let delay_us = duration.as_micros().min(u128::from(u64::MAX)) as u64;
    let mut samples = revive(samples.lock());
    if samples.len() >= DELAY_SAMPLE_LIMIT {
        samples.pop_front();
    }
    samples.push_back(delay_us);
}

fn sorted_samples(samples: &Mutex<VecDeque<u64>>) -> Vec<u64> {
    let mut samples: Vec<_> = revive(samples.lock()).iter().copied().collect();
    samples.sort_unstable();
    samples
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let last = samples.len() - 1;
    let idx = (last * percentile).div_ceil(100);
    samples[idx.min(last)]
}
