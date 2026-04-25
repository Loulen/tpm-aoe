//! E2E tests for TPM config panel wiring (Feature 8) and TPM badge indicator
//! (Feature 7).
//!
//! CLI tests exercise `aoe add` with `--tpm-review-passes` and
//! `--tpm-disable-agent` flags, then assert on `.tpm/config.json` contents.
//! The TUI test creates a TPM session and a non-TPM session, spawns the TUI
//! in tmux, and verifies the " TPM" badge appears only next to the TPM session.
//!
//! Uses the same `TPM_WORKFLOW_PATH` env override as `tpm.rs` and `tpm_tier.rs`.

use serial_test::serial;

use crate::harness::require_tmux;
use crate::helpers::{find_session, read_sessions, read_tpm_config, setup_tpm_harness};

// ---------------------------------------------------------------------------
// AC-01: --tpm --tpm-review-passes 5 writes max_review_cycles: 5
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_review_passes_five_writes_max_review_cycles() {
    let (h, plugin_dir, _) = setup_tpm_harness("tpm_cfg_review5");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-review-passes",
            "5",
            "-t",
            "Review limited",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm --tpm-review-passes 5 failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = read_tpm_config(&h);
    assert_eq!(
        config["max_review_cycles"].as_u64(),
        Some(5),
        "config.json should have max_review_cycles: 5, got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-02: --tpm-disable-agent blind-hunter --tpm-disable-agent end-user-simulator
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_disable_agents_writes_disabled_agents_list() {
    let (h, plugin_dir, _) = setup_tpm_harness("tpm_cfg_disable2");
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
            "Agents disabled",
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
// AC-03: --tpm-disable-agent implementer is silently stripped
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_disable_implementer_is_silently_stripped() {
    let (h, plugin_dir, _) = setup_tpm_harness("tpm_cfg_no_impl");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-disable-agent",
            "implementer",
            "-t",
            "Implementer kept",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm --tpm-disable-agent implementer failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = read_tpm_config(&h);
    // disabled_agents is Vec with skip_serializing_if = "Vec::is_empty", so when
    // the implementer is filtered out leaving an empty Vec, the field is absent
    // from the serialized JSON.
    let disabled = config.get("disabled_agents");
    match disabled {
        None => {} // field absent = empty list, correct
        Some(v) => {
            let arr = v.as_array().expect("disabled_agents should be an array");
            assert!(
                arr.is_empty(),
                "disabled_agents should be empty after stripping implementer, got: {:?}",
                arr
            );
        }
    }
}

// ---------------------------------------------------------------------------
// AC-04: --tpm-review-passes 0 filters out max_review_cycles
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_review_passes_zero_omits_max_review_cycles() {
    let (h, plugin_dir, _) = setup_tpm_harness("tpm_cfg_zero");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-review-passes",
            "0",
            "-t",
            "Zero passes",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm --tpm-review-passes 0 failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = read_tpm_config(&h);
    assert!(
        config.get("max_review_cycles").is_none(),
        "config.json should not have max_review_cycles when value is 0, got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-05: TPM session has tpm_managed: true, non-TPM has false
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_managed_flag_distinguishes_tpm_from_non_tpm_sessions() {
    let (h, plugin_dir, _) = setup_tpm_harness("tpm_cfg_managed");
    let project = h.project_path();

    // Create TPM session
    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "-t",
            "TPM Session",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Create non-TPM session (same project, no --tpm flag)
    let output = h.run_cli_with_env(
        &["add", project.to_str().unwrap(), "-t", "Regular Session"],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add (no --tpm) failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let sessions = read_sessions(&h);
    let tpm_session = find_session(&sessions, "TPM Session");
    let regular_session = find_session(&sessions, "Regular Session");

    assert_eq!(
        tpm_session["tpm_managed"].as_bool(),
        Some(true),
        "TPM session should have tpm_managed: true"
    );

    // Non-TPM session: tpm_managed is either false or absent (defaults to false via serde)
    let regular_managed = regular_session["tpm_managed"].as_bool().unwrap_or(false);
    assert!(
        !regular_managed,
        "Regular session should have tpm_managed: false (or absent), got: {}",
        regular_session["tpm_managed"]
    );
}

// ---------------------------------------------------------------------------
// AC-06: TUI shows " TPM" badge for TPM session, not for non-TPM session
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tui_shows_tpm_badge_for_tpm_session_only() {
    require_tmux!();

    let (mut h, plugin_dir, _) = setup_tpm_harness("tpm_cfg_badge");
    let project = h.project_path();

    // Create TPM session via CLI
    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "-t",
            "My TPM Task",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Create non-TPM session via CLI
    let output = h.run_cli_with_env(
        &["add", project.to_str().unwrap(), "-t", "Regular Session"],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add (non-TPM) failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Spawn the TUI (100x30 via harness defaults)
    h.spawn_tui();
    h.wait_for("Agent of Empires");

    // Wait for both session titles to appear
    h.wait_for("My TPM Task");
    h.wait_for("Regular Session");

    // Capture and analyze the screen
    let screen = h.capture_screen();

    // Find the line containing "My TPM Task" and assert " TPM" is on it
    let tpm_line = screen
        .lines()
        .find(|line| line.contains("My TPM Task"))
        .unwrap_or_else(|| panic!("'My TPM Task' not found in screen:\n{}", screen));
    assert!(
        tpm_line.contains(" TPM"),
        "The line with 'My TPM Task' should contain ' TPM' badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        tpm_line,
        screen
    );

    // Find the line containing "Regular Session" and assert " TPM" is NOT on it
    let regular_line = screen
        .lines()
        .find(|line| line.contains("Regular Session"))
        .unwrap_or_else(|| panic!("'Regular Session' not found in screen:\n{}", screen));
    assert!(
        !regular_line.contains(" TPM"),
        "The line with 'Regular Session' should NOT contain ' TPM' badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        regular_line,
        screen
    );
}

// ---------------------------------------------------------------------------
// AC-07: Default --tpm (no extra flags) writes tier: standard, minimal config
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tpm_default_config_writes_standard_tier_minimal() {
    let (h, plugin_dir, _) = setup_tpm_harness("tpm_cfg_default");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "-t",
            "Default config",
        ],
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

    // disabled_agents is empty, so skip_serializing_if omits it
    let disabled = config.get("disabled_agents");
    match disabled {
        None => {} // correct: empty Vec is omitted
        Some(v) => {
            let arr = v.as_array().expect("disabled_agents should be an array");
            assert!(
                arr.is_empty(),
                "default config disabled_agents should be empty, got: {:?}",
                arr
            );
        }
    }
}
