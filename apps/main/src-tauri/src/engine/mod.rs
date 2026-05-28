pub use burst_engine::{start_listener, BurstEngine};
#[cfg(windows)]
pub use win_input::{clear_pending_injections, try_consume_injection, SIM_MARKER};
