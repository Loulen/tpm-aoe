//! E2E tests for the settings TUI's TPM category (Journey 11) and CLI config
//! flag propagation with TUI badge verification (Journeys 12-15).
//!
//! AC-01: Navigate settings TUI to TPM category, verify field labels.
//! AC-02: `--tpm --tpm-review-passes 7` → config.json + TUI badge.
//! AC-03: `--tpm-disable-agent blind-hunter --tpm-disable-agent end-user-simulator`.
//! AC-04: `--tpm-disable-agent implementer` silently stripped + TUI badge.
//! AC-05: Default `--tpm` (no extra flags) writes standard tier.

use serial_test::serial;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

use crate::harness::{require_tmux, TuiTestHarness};

// ---------------------------------------------------------------------------
// Helpers (self-contained to avoid cross-file merge conflicts)
// ---------------------------------------------------------------------------

/// Drop a fake `agents/orchestrator.md` under `root`.
fn write_fake_orchestrator(root: &Path) {
    let agents = root.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(agents.join("orchestrator.md"), "# Fake Orchestrator\n")
        .expect("write orchestrator.md");
}

/// Create a harness with a git-initialized project and a fake plugin dir.
fn setup_tpm_harness(name: &str) -> (TuiTestHarness, TempDir) {
    let h = TuiTestHarness::new(name);
    let project = h.project_path();

    let git_init = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(&project)
        .output()
        .expect("git init");
    assert!(
        git_init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&git_init.stderr)
    );

    let plugin_dir = TempDir::new().expect("plugin tempdir");
    write_fake_orchestrator(plugin_dir.path());

    (h, plugin_dir)
}

/// Read `.tpm/config.json` from the project directory inside the harness.
fn read_tpm_config(h: &TuiTestHarness) -> serde_json::Value {
    let path = h.project_path().join(".tpm/config.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("invalid .tpm/config.json")
}

// ---------------------------------------------------------------------------
// AC-01 (Journey 11): Settings TUI → navigate to TPM category
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn settings_tui_shows_tpm_category_and_fields() {
    require_tmux!();

    let mut h = TuiTestHarness::new("tui_cfg_tpm_nav");

    h.spawn_tui();
    h.wait_for("Agent of Empires");

    // Open settings with 's'
    h.send_keys("s");
    std::thread::sleep(Duration::from_millis(500));

    // Settings starts at Categories panel, first category (Theme).
    // TPM is the last category (index 8). Navigate down 8 times.
    // Categories order: Theme, Session, Hooks, Sandbox, Worktree, Updates, Tmux, Sound, Tpm
    for _ in 0..8 {
        h.send_keys("Down");
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));

    let screen = h.capture_screen();

    // The selected category should be "TPM"
    assert!(
        screen.contains("TPM"),
        "Screen should contain 'TPM' category header.\n--- Screen ---\n{}\n--- End ---",
        screen
    );

    // At least one TPM field label must be visible. The fields panel renders
    // automatically when the category is selected (focus is Categories, but
    // fields show on the right). Check for known field labels.
    let has_tier = screen.contains("Default Tier") || screen.contains("Tier");
    let has_review = screen.contains("Max Review Cycles") || screen.contains("Review Cycles");
    let has_disabled = screen.contains("Disabled Agents");

    assert!(
        has_tier || has_review || has_disabled,
        "Screen should contain at least one TPM field label (Tier, Review Cycles, or Disabled Agents).\n--- Screen ---\n{}\n--- End ---",
        screen
    );
}

// ---------------------------------------------------------------------------
// AC-02 (Journey 12): --tpm --tpm-review-passes 7 → config + badge
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_review_passes_seven_config_and_badge() {
    require_tmux!();

    let (mut h, plugin_dir) = setup_tpm_harness("tui_cfg_review7");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-review-passes",
            "7",
            "-t",
            "Review 7",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm --tpm-review-passes 7 failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify config.json
    let config = read_tpm_config(&h);
    assert_eq!(
        config["max_review_cycles"].as_u64(),
        Some(7),
        "config.json should have max_review_cycles: 7, got: {}",
        config
    );

    // Spawn TUI and verify badge
    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("Review 7");

    let screen = h.capture_screen();
    let session_line = screen
        .lines()
        .find(|line| line.contains("Review 7"))
        .unwrap_or_else(|| panic!("'Review 7' not found in screen:\n{}", screen));
    assert!(
        session_line.contains("TPM"),
        "The line with 'Review 7' should contain TPM badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        session_line,
        screen
    );
}

// ---------------------------------------------------------------------------
// AC-03 (Journey 13): --tpm-disable-agent blind-hunter + end-user-simulator
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_disable_agents_config() {
    let (h, plugin_dir) = setup_tpm_harness("tui_cfg_disable");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-disable-agent",
            "blind-hunter",
            "--tpm-disable-agent",
            "end-user-simulator",
            "-t",
            "Disabled",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm --tpm-disable-agent failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = read_tpm_config(&h);
    let disabled = config["disabled_agents"]
        .as_array()
        .expect("disabled_agents should be an array");
    let slugs: Vec<&str> = disabled.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        slugs.contains(&"blind-hunter"),
        "disabled_agents should contain blind-hunter, got: {:?}",
        slugs
    );
    assert!(
        slugs.contains(&"end-user-simulator"),
        "disabled_agents should contain end-user-simulator, got: {:?}",
        slugs
    );
    assert_eq!(
        slugs.len(),
        2,
        "disabled_agents should have exactly 2 entries, got: {:?}",
        slugs
    );
}

// ---------------------------------------------------------------------------
// AC-04 (Journey 14): --tpm-disable-agent implementer is stripped + badge
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_disable_implementer_stripped_and_badge() {
    require_tmux!();

    let (mut h, plugin_dir) = setup_tpm_harness("tui_cfg_impl_prot");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-disable-agent",
            "implementer",
            "--tpm-disable-agent",
            "blind-hunter",
            "-t",
            "Impl Protected",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm --tpm-disable-agent implementer --tpm-disable-agent blind-hunter failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify config.json: implementer must NOT be in disabled_agents
    let config = read_tpm_config(&h);
    let disabled = config["disabled_agents"]
        .as_array()
        .expect("disabled_agents should be an array");
    let slugs: Vec<&str> = disabled.iter().filter_map(|v| v.as_str()).collect();

    assert_eq!(
        slugs,
        vec!["blind-hunter"],
        "disabled_agents should contain only blind-hunter (implementer stripped), got: {:?}",
        slugs
    );
    assert!(
        !slugs.contains(&"implementer"),
        "disabled_agents must NOT contain implementer, got: {:?}",
        slugs
    );

    // Spawn TUI and verify badge
    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("Impl Protected");

    let screen = h.capture_screen();
    let session_line = screen
        .lines()
        .find(|line| line.contains("Impl Protected"))
        .unwrap_or_else(|| panic!("'Impl Protected' not found in screen:\n{}", screen));
    assert!(
        session_line.contains("TPM"),
        "The line with 'Impl Protected' should contain TPM badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        session_line,
        screen
    );
}

// ---------------------------------------------------------------------------
// AC-05 (Journey 15): Default --tpm writes standard tier, no max_review_cycles
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_default_flags_writes_standard_tier() {
    let (h, plugin_dir) = setup_tpm_harness("tui_cfg_defaults");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &["add", project.to_str().unwrap(), "--tpm", "-t", "Defaults"],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = read_tpm_config(&h);

    assert_eq!(
        config["tier"].as_str(),
        Some("standard"),
        "default tier should be 'standard', got: {}",
        config
    );

    assert!(
        config.get("max_review_cycles").is_none(),
        "default config should omit max_review_cycles, got: {}",
        config
    );
}
