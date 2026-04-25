//! E2E tests for idle session highlighting (issue #21).
//!
//! Verifies the "needs attention" dot indicator appears for sessions that went
//! idle after the user last viewed them, and disappears once the session is
//! accessed.

use serial_test::serial;
use std::path::Path;
use std::process::Command;

use crate::harness::{require_tmux, TuiTestHarness};

/// The session ID used in all tests. Chosen to be a valid 16-char hex string.
const SESSION_ID: &str = "aabbccdd11223344";

/// Compute the tmux session name AoE would use for the test session.
/// Format: aoe_{sanitized_title}_{first 8 chars of id}
fn tmux_session_name() -> String {
    format!("aoe_idle-session_{}", &SESSION_ID[..8])
}

/// Pre-create a tmux session on the harness's socket so AoE's status poller
/// sees it as alive. Runs `sleep 300` so the pane stays alive.
fn create_backing_tmux_session(socket: &Path) {
    let name = tmux_session_name();
    let _ = Command::new("tmux")
        .args(["-S"])
        .arg(socket)
        .args(["kill-session", "-t", &name])
        .output();
    let output = Command::new("tmux")
        .args(["-S"])
        .arg(socket)
        .args([
            "new-session",
            "-d",
            "-s",
            &name,
            "-x",
            "80",
            "-y",
            "24",
            "sleep",
            "300",
        ])
        .output()
        .expect("create backing tmux session");
    assert!(
        output.status.success(),
        "Failed to create backing tmux session: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn kill_backing_tmux_session(socket: &Path) {
    let name = tmux_session_name();
    let _ = Command::new("tmux")
        .args(["-S"])
        .arg(socket)
        .args(["kill-session", "-t", &name])
        .output();
}

/// Write a sessions.json with one session that has idle_since set but no
/// last_accessed_at, so the TUI should show the attention indicator.
fn seed_idle_session(h: &TuiTestHarness) {
    let config_dir = if cfg!(target_os = "linux") {
        h.home_path()
            .join(".config/agent-of-empires/profiles/default")
    } else {
        h.home_path().join(".agent-of-empires/profiles/default")
    };
    std::fs::create_dir_all(&config_dir).expect("create profile dir");

    let sessions_json = format!(
        r#"[
        {{
            "id": "{SESSION_ID}",
            "title": "idle-session",
            "project_path": "/tmp/test",
            "command": "",
            "tool": "claude",
            "status": "idle",
            "created_at": "2026-01-01T00:00:00Z",
            "idle_since": "2026-04-25T10:00:00Z"
        }}
    ]"#
    );
    std::fs::write(config_dir.join("sessions.json"), sessions_json).expect("write sessions.json");
}

/// Write a sessions.json where last_accessed_at > idle_since, so the indicator
/// should NOT appear.
fn seed_accessed_session(h: &TuiTestHarness) {
    let config_dir = if cfg!(target_os = "linux") {
        h.home_path()
            .join(".config/agent-of-empires/profiles/default")
    } else {
        h.home_path().join(".agent-of-empires/profiles/default")
    };

    let sessions_json = format!(
        r#"[
        {{
            "id": "{SESSION_ID}",
            "title": "idle-session",
            "project_path": "/tmp/test",
            "command": "",
            "tool": "claude",
            "status": "idle",
            "created_at": "2026-01-01T00:00:00Z",
            "idle_since": "2026-04-25T10:00:00Z",
            "last_accessed_at": "2026-04-25T11:00:00Z"
        }}
    ]"#
    );
    std::fs::write(config_dir.join("sessions.json"), sessions_json).expect("write sessions.json");
}

/// AC-04 part 1: idle session shows the attention indicator dot.
/// Pre-creates a backing tmux session on the harness's socket so the status
/// poller sees it as alive and keeps the Idle status (and idle_since) intact.
#[test]
#[serial]
fn test_idle_session_shows_attention_indicator() {
    require_tmux!();

    let mut h = TuiTestHarness::new("idle_indicator");
    seed_idle_session(&h);

    // Must create backing session on the harness socket AFTER spawn_tui(),
    // because spawn_tui() starts the tmux server on that socket.
    h.spawn_tui();
    create_backing_tmux_session(h.socket_path());

    h.wait_for("Agent of Empires");
    h.wait_for("idle-session");
    h.assert_screen_contains("●");

    kill_backing_tmux_session(h.socket_path());
}

/// AC-04 part 2: once last_accessed_at > idle_since, the indicator is gone.
#[test]
#[serial]
fn test_accessed_session_hides_attention_indicator() {
    require_tmux!();

    let mut h = TuiTestHarness::new("idle_no_indicator");
    seed_accessed_session(&h);

    h.spawn_tui();
    create_backing_tmux_session(h.socket_path());

    h.wait_for("Agent of Empires");
    h.wait_for("idle-session");
    h.assert_screen_not_contains("●");

    kill_backing_tmux_session(h.socket_path());
}
