use std::sync::atomic::Ordering;

#[cfg(windows)]
use win_input::{clear_pending_injections, clear_relay_injections};

use crate::{release_simulated_keys, BurstEngine};

pub(crate) const LIFECYCLE_PAUSED: u8 = 0;
const LIFECYCLE_RUNNING: u8 = 1;
const LIFECYCLE_STOPPING: u8 = 2;
const LIFECYCLE_SHUTTING_DOWN: u8 = 3;
const LIFECYCLE_SHUTDOWN: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineLifecycle {
    Paused,
    Running,
    Stopping,
    ShuttingDown,
    Shutdown,
}

impl EngineLifecycle {
    fn from_raw(raw: u8) -> Self {
        match raw {
            LIFECYCLE_RUNNING => Self::Running,
            LIFECYCLE_STOPPING => Self::Stopping,
            LIFECYCLE_SHUTTING_DOWN => Self::ShuttingDown,
            LIFECYCLE_SHUTDOWN => Self::Shutdown,
            _ => Self::Paused,
        }
    }
}

impl BurstEngine {
    pub fn lifecycle(&self) -> EngineLifecycle {
        EngineLifecycle::from_raw(self.lifecycle_state.load(Ordering::SeqCst))
    }

    pub fn enable_runtime(&self) -> bool {
        let mut current = self.lifecycle_state.load(Ordering::SeqCst);
        loop {
            match current {
                LIFECYCLE_RUNNING => {
                    self.global_enabled.store(true, Ordering::SeqCst);
                    return true;
                }
                LIFECYCLE_PAUSED => match self.lifecycle_state.compare_exchange(
                    LIFECYCLE_PAUSED,
                    LIFECYCLE_RUNNING,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        self.global_enabled.store(true, Ordering::SeqCst);
                        return true;
                    }
                    Err(next) => current = next,
                },
                LIFECYCLE_STOPPING | LIFECYCLE_SHUTTING_DOWN | LIFECYCLE_SHUTDOWN => return false,
                _ => return false,
            }
        }
    }

    pub fn pause_runtime(&self) {
        self.global_enabled.store(false, Ordering::SeqCst);
        let mut current = self.lifecycle_state.load(Ordering::SeqCst);
        loop {
            match current {
                LIFECYCLE_SHUTTING_DOWN | LIFECYCLE_SHUTDOWN => return,
                LIFECYCLE_STOPPING => return,
                LIFECYCLE_PAUSED | LIFECYCLE_RUNNING => {
                    match self.lifecycle_state.compare_exchange(
                        current,
                        LIFECYCLE_STOPPING,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(_) => break,
                        Err(next) => current = next,
                    }
                }
                _ => return,
            }
        }

        let _gate = self.begin_stop_all();
        self.cancel_all_loops_inner();
        let _ = self.lifecycle_state.compare_exchange(
            LIFECYCLE_STOPPING,
            LIFECYCLE_PAUSED,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }

    pub fn shutdown(&self) {
        self.shutdown_inner();
    }

    pub(crate) fn shutdown_inner(&self) {
        let previous = self
            .lifecycle_state
            .swap(LIFECYCLE_SHUTTING_DOWN, Ordering::SeqCst);
        if previous == LIFECYCLE_SHUTDOWN {
            // 已经完整关机：还原为 Shutdown，防止重复调用时状态退化为 ShuttingDown。
            self.lifecycle_state
                .store(LIFECYCLE_SHUTDOWN, Ordering::SeqCst);
            return;
        }
        if previous == LIFECYCLE_SHUTTING_DOWN {
            // 另一线程正在执行关机流程，放行即可。
            return;
        }

        self.global_enabled.store(false, Ordering::SeqCst);
        {
            let _gate = self.begin_stop_all();
            self.cancel_all_loops_inner();
        }

        let _ = self.scheduler_tx.send(crate::SchedulerCommand::Shutdown);
        if let Some(handle) = crate::revive(self.scheduler_handle.lock()).take() {
            let _ = handle.join();
        }
        release_simulated_keys(&self.pressed_keys, &self.simulated_keys);
        #[cfg(windows)]
        {
            clear_pending_injections();
            clear_relay_injections();
        }
        self.lifecycle_state
            .store(LIFECYCLE_SHUTDOWN, Ordering::SeqCst);
    }
}
