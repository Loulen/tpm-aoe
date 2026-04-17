//! Event bus for AoE session lifecycle and orchestration events.
//!
//! Events are written as JSON lines to `<profile_dir>/events.jsonl` and can be
//! consumed via `aoe events watch` (live tail) or `aoe events history`.
//!
//! Designed for orchestration use cases like the TPM workflow: the orchestrator
//! tails events to know when child sessions complete, fail, or need attention.

pub mod log;
pub mod sweeper;
pub mod types;

pub use log::{events_log_path, read_history, tail_events, write_event};
pub use sweeper::{run_sweeper, sweep_once};
pub use types::Event;
