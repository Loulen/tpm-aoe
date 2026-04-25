//! End-to-end tests for Agent of Empires.
//!
//! These tests exercise the full `aoe` binary -- both TUI mode (via tmux) and
//! CLI subcommands (via subprocess). They catch startup failures, rendering
//! bugs, config resolution errors, and full-flow regressions that unit and
//! integration tests miss.
//!
//! # Running
//!
//! ```sh
//! cargo test --test e2e              # run all e2e tests
//! cargo test --test e2e -- --nocapture  # with screen dumps on failure
//! ```
//!
//! TUI tests require tmux and are skipped automatically if it is not installed.
//! Docker-dependent tests are `#[ignore]` and require a running Docker daemon.

mod harness;
pub(crate) mod helpers;

mod cli;
mod errors;
mod events;
mod idle_highlight;
mod new_session;
mod profile_picker;
mod profile_template;
mod sandbox;
mod send_keys;
mod tpm;
mod tpm_artifacts;
mod tpm_config;
mod tpm_prompt_injection;
mod tpm_tier;
mod tpm_tui_create;
mod tpm_tui_delete;
mod tui_config_settings;
mod tui_full_lifecycle;
mod tui_launch;
mod tui_state_panel;
mod unified_view;
mod worktree_from;
