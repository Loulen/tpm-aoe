//! End-to-end tests for the TPM profile template (Feature 12) and the
//! STATE.md side panel (Feature 4).
//!
//! CLI tests exercise `aoe profile create --template` and verify the resulting
//! config.toml. TUI tests spawn `aoe` inside tmux, pre-seed a session with a
//! `.tpm/STATE.md`, and toggle the state panel with `S`.
//!
//! AC-06 (CJK wrapping) is a supplementary unit test living in
//! `src/tui/home/state_panel.rs` as `test_wrap_line_cjk_nihongo_test`, since
//! it needs access to the crate-private `wrap_line` function.

use serial_test::serial;
use std::path::Path;
use std::time::Duration;

use crate::harness::{require_tmux, TuiTestHarness};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the config dir under the harness's isolated home.
fn config_dir(h: &TuiTestHarness) -> std::path::PathBuf {
    if cfg!(target_os = "linux") {
        h.home_path().join(".config/agent-of-empires")
    } else {
        h.home_path().join(".agent-of-empires")
    }
}

/// Pre-seed a session in the default profile whose project_path points at `dir`.
/// The caller can then put `.tpm/STATE.md` under `dir` to make the state panel
/// available.
fn seed_session(h: &TuiTestHarness, title: &str, dir: &Path) {
    let profile_dir = config_dir(h).join("profiles/default");
    std::fs::create_dir_all(&profile_dir).expect("create default profile dir");

    let session = format!(
        r#"[{{"id":"test_state","title":"{title}","project_path":"{}","group_path":"","command":"","tool":"claude","yolo_mode":false,"status":"idle","created_at":"2026-01-01T00:00:00Z","tpm_managed":true}}]"#,
        dir.display(),
    );
    std::fs::write(profile_dir.join("sessions.json"), session).expect("write sessions.json");
}

// ===========================================================================
// AC-01: aoe profile create --template tpm -> exit 0, config.toml correct
// ===========================================================================

#[test]
#[serial]
fn profile_create_with_tpm_template_writes_correct_config() {
    let h = TuiTestHarness::new("profile_tpm_create");

    let output = h.run_cli(&["profile", "create", "tpm-test", "--template", "tpm"]);
    assert!(
        output.status.success(),
        "aoe profile create --template tpm failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let cfg_path = config_dir(&h).join("profiles/tpm-test/config.toml");
    assert!(
        cfg_path.exists(),
        "config.toml should exist at {}",
        cfg_path.display()
    );

    let raw = std::fs::read_to_string(&cfg_path).expect("read config.toml");

    // Parse as TOML and verify key fields
    let parsed: toml::Value = toml::from_str(&raw).expect("config.toml should be valid TOML");

    // worktree.enabled = true
    let wt_enabled = parsed
        .get("worktree")
        .and_then(|w| w.get("enabled"))
        .and_then(|v| v.as_bool());
    assert_eq!(
        wt_enabled,
        Some(true),
        "worktree.enabled should be true, got: {}",
        raw
    );

    // session.yolo_mode_default = true
    let yolo = parsed
        .get("session")
        .and_then(|s| s.get("yolo_mode_default"))
        .and_then(|v| v.as_bool());
    assert_eq!(
        yolo,
        Some(true),
        "session.yolo_mode_default should be true, got: {}",
        raw
    );
}

// ===========================================================================
// AC-02: aoe profile create --template unknown -> non-zero, stderr mentions it
// ===========================================================================

#[test]
#[serial]
fn profile_create_with_unknown_template_fails() {
    let h = TuiTestHarness::new("profile_unknown_tpl");

    let output = h.run_cli(&["profile", "create", "tpm-test", "--template", "unknown"]);
    assert!(
        !output.status.success(),
        "aoe profile create --template unknown should fail.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // clap validates the --template value against PossibleValuesParser, so the
    // error message comes from clap rather than our Template::from_str. Check
    // for either format.
    assert!(
        stderr.contains("Unknown profile template")
            || stderr.contains("invalid value")
            || stderr.contains("possible values"),
        "stderr should mention the invalid template. stderr: {}",
        stderr
    );
}

// ===========================================================================
// AC-03: config.toml round-trips through TOML parse
// ===========================================================================

#[test]
#[serial]
fn profile_template_config_roundtrips_through_toml() {
    let h = TuiTestHarness::new("profile_tpl_roundtrip");

    let output = h.run_cli(&["profile", "create", "roundtrip", "--template", "tpm"]);
    assert!(
        output.status.success(),
        "profile create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cfg_path = config_dir(&h).join("profiles/roundtrip/config.toml");
    let raw = std::fs::read_to_string(&cfg_path).expect("read config.toml");

    // Parse -> serialize -> parse again -> same values
    let first: toml::Value = toml::from_str(&raw).expect("first parse");
    let reserialized = toml::to_string_pretty(&first).expect("reserialize");
    let second: toml::Value = toml::from_str(&reserialized).expect("second parse");

    assert_eq!(
        first, second,
        "TOML roundtrip should produce identical values.\nFirst: {}\nSecond: {}",
        raw, reserialized
    );
}

// ===========================================================================
// AC-04 + AC-05: TUI state panel toggle with S
// ===========================================================================

#[test]
#[serial]
fn state_panel_toggle_shows_and_hides_content() {
    require_tmux!();

    let mut h = TuiTestHarness::new("state_panel_toggle");

    // Create a project dir with .tpm/STATE.md
    let project = h.project_path();
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).expect("create .tpm dir");
    std::fs::write(
        tpm_dir.join("STATE.md"),
        "# TPM State\n\n## Waves\n\nWave 1 (completed)\n\n## Tasks\n\n| Task | Status |\n|---|---|\n| task-01 | done |\n",
    )
    .expect("write STATE.md");

    // Seed a session pointing at that project
    seed_session(&h, "State Panel Test", &project);

    // Launch TUI
    h.spawn_tui();
    h.wait_for("Agent of Empires");

    // Session should appear in the list
    h.wait_for("State Panel Test");

    // AC-04: Press S to toggle state panel on
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(800));

    // The state panel should show the STATE.md content
    h.wait_for_timeout("Wave 1 (completed)", Duration::from_secs(5));
    // Also verify the panel title appears
    h.assert_screen_contains("TPM State");

    // AC-05: Press S again to toggle off
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(500));

    // Content should no longer be visible
    let screen = h.capture_screen();
    assert!(
        !screen.contains("Wave 1 (completed)"),
        "State panel content should be hidden after second S press.\n--- Screen ---\n{}\n--- End ---",
        screen
    );
}
