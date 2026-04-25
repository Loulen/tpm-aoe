//! E2E test for TPM group-by mode (issue #30, AC-05).
//!
//! Seeds an orchestrator + 2 sub-sessions, presses `g` twice to reach Tpm mode,
//! and verifies the group header is visible with the correct session count.

use serial_test::serial;

use crate::harness::{require_tmux, TuiTestHarness};

/// Return the config dir under the harness's isolated home.
fn config_dir(h: &TuiTestHarness) -> std::path::PathBuf {
    if cfg!(target_os = "linux") {
        h.home_path().join(".config/agent-of-empires")
    } else {
        h.home_path().join(".agent-of-empires")
    }
}

/// Seed an orchestrator session + 2 child sessions with parent_session_id.
fn seed_tpm_sessions(h: &TuiTestHarness) {
    let profile_dir = config_dir(h).join("profiles/default");
    std::fs::create_dir_all(&profile_dir).expect("create default profile dir");

    let sessions = r#"[
        {
            "id": "orch-001",
            "title": "TPM Orchestrator",
            "project_path": "/tmp/tpm-test",
            "command": "",
            "tool": "claude",
            "status": "idle",
            "created_at": "2026-01-01T00:00:00Z"
        },
        {
            "id": "child-001",
            "title": "impl-auth",
            "project_path": "/tmp/tpm-test",
            "command": "",
            "tool": "claude",
            "status": "idle",
            "created_at": "2026-01-01T00:01:00Z",
            "parent_session_id": "orch-001"
        },
        {
            "id": "child-002",
            "title": "impl-db",
            "project_path": "/tmp/tpm-test",
            "command": "",
            "tool": "claude",
            "status": "idle",
            "created_at": "2026-01-01T00:02:00Z",
            "parent_session_id": "orch-001"
        }
    ]"#;
    std::fs::write(profile_dir.join("sessions.json"), sessions).expect("write sessions.json");
}

/// AC-05: press g twice to reach Tpm mode, verify group header visible with count.
#[test]
#[serial]
fn tpm_group_by_shows_orchestrator_group_with_count() {
    require_tmux!();

    let mut h = TuiTestHarness::new("tpm_group_by");
    seed_tpm_sessions(&h);

    h.spawn_tui();
    h.wait_for("Agent of Empires");

    // Default group mode for existing users (has_seen_welcome=true) is Manual.
    // Press g once -> Project, press g again -> Tpm.
    h.send_keys("g");
    std::thread::sleep(std::time::Duration::from_millis(500));
    h.send_keys("g");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // In Tpm mode, the title bar should show "(by TPM)"
    h.wait_for("by TPM");

    // The group header should show the orchestrator's title and child count
    h.assert_screen_contains("TPM Orchestrator");
    // The group header includes the count of children (2)
    h.assert_screen_contains("2");
}
