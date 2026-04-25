//! E2E tests for TPM tier system, plugin resolution, and system prompt preamble.
//!
//! Validates Feature 3 (tier), Feature 2 (plugin resolution), and Feature 11
//! (system prompt override preamble) through user scenarios: creating TPM
//! sessions with different tiers, verifying sessions.json + .tpm/config.json,
//! and testing error paths when the plugin is missing.
//!
//! Uses the same `TPM_WORKFLOW_PATH` env override pattern as `tpm.rs`, with a
//! fake `agents/orchestrator.md` in a tempdir.

use serial_test::serial;

use crate::harness::TuiTestHarness;
use crate::helpers::{find_session, read_sessions, read_tpm_config, setup_tpm_harness};

// ---------------------------------------------------------------------------
// AC-01: --tpm with no tier value defaults to standard
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn aoe_add_tpm_default_tier_sets_tpm_managed_and_injects_prompt() {
    let (h, plugin_dir, orch_path) = setup_tpm_harness("tpm_tier_default");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "-t",
            "Standard TPM",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Standard TPM");

    // tpm_managed must be true
    assert_eq!(
        session["tpm_managed"].as_bool(),
        Some(true),
        "tpm_managed should be true for --tpm session"
    );

    // extra_args must contain --append-system-prompt and the orchestrator path
    let extra_args = session["extra_args"].as_str().unwrap_or("");
    assert!(
        extra_args.contains("--append-system-prompt"),
        "extra_args should contain --append-system-prompt, got: {}",
        extra_args
    );
    assert!(
        extra_args.contains(orch_path.to_string_lossy().as_ref()),
        "extra_args should reference orchestrator path. extra_args={}, expected substring={}",
        extra_args,
        orch_path.display()
    );

    // .tpm/config.json: tier must be "standard" (the default when no tier is specified)
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("standard"),
        "config.json tier should be 'standard' by default, got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-02: --tpm fast creates config.json with tier "fast"
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn aoe_add_tpm_fast_tier_writes_config_json() {
    let (h, plugin_dir, _orch_path) = setup_tpm_harness("tpm_tier_fast");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "fast",
            "-t",
            "Fast TPM",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm fast failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // sessions.json: tpm_managed must be true
    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Fast TPM");
    assert_eq!(
        session["tpm_managed"].as_bool(),
        Some(true),
        "tpm_managed should be true"
    );

    // .tpm/config.json: tier must be "fast"
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("fast"),
        "config.json tier should be 'fast', got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-03: --tpm prod creates config.json with tier "prod"
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn aoe_add_tpm_prod_tier_writes_config_json() {
    let (h, plugin_dir, _orch_path) = setup_tpm_harness("tpm_tier_prod");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "prod",
            "-t",
            "Prod TPM",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm prod failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // .tpm/config.json: tier must be "prod"
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("prod"),
        "config.json tier should be 'prod', got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-04: preamble is injected in extra_args
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn aoe_add_tpm_injects_preamble_in_extra_args() {
    let (h, plugin_dir, _orch_path) = setup_tpm_harness("tpm_preamble");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "-t",
            "Preamble check",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Preamble check");
    let extra_args = session["extra_args"].as_str().unwrap_or("");

    assert!(
        extra_args.contains("SYSTEM PROMPT OVERRIDE"),
        "extra_args should contain the preamble opening 'SYSTEM PROMPT OVERRIDE'. got: {}",
        extra_args
    );
}

// ---------------------------------------------------------------------------
// AC-05: missing plugin produces non-zero exit + helpful error
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn aoe_add_tpm_without_plugin_produces_helpful_error() {
    let h = TuiTestHarness::new("tpm_tier_no_plugin");
    let project = h.project_path();

    // No TPM_WORKFLOW_PATH, no plugin in isolated HOME
    let output = h.run_cli_with_env(
        &["add", project.to_str().unwrap(), "-t", "No Plugin", "--tpm"],
        &[],
    );

    assert!(
        !output.status.success(),
        "aoe add --tpm should fail without plugin.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("orchestrator")
            || combined.contains("tpm-workflow")
            || combined.contains("TPM_WORKFLOW_PATH"),
        "error should mention orchestrator, tpm-workflow, or TPM_WORKFLOW_PATH. combined: {}",
        combined
    );
}

// ---------------------------------------------------------------------------
// AC-06: preamble constant is shell-safe (supplementary unit test)
// ---------------------------------------------------------------------------

/// The TPM_PREAMBLE constant is not publicly exported, but we can verify its
/// shell safety by inspecting what `extra_args_snippet` produces. The preamble
/// is embedded in the double-quoted argument; single quotes and backslashes
/// would break the bash wrapping tmux uses.
///
/// This is a supplementary unit test (justified: shell safety cannot be
/// observed from CLI output, only from inspecting the constant's content).
#[test]
fn preamble_constant_contains_no_single_quotes_or_backslashes() {
    // Build a snippet and extract the part between the opening " and $(cat
    // That region is the preamble.
    let snippet = agent_of_empires::tpm::extra_args_snippet(std::path::Path::new("/tmp/test.md"));

    // The snippet format is: --append-system-prompt "<preamble>$(cat ...)"
    // Extract the preamble by finding the content between first " and $(cat
    let after_quote = snippet
        .find('"')
        .map(|i| &snippet[i + 1..])
        .expect("snippet should contain opening quote");
    let preamble_end = after_quote
        .find("$(cat")
        .expect("snippet should contain $(cat");
    let preamble = &after_quote[..preamble_end];

    assert!(
        !preamble.contains('\''),
        "preamble must not contain single quotes for shell safety. got: {}",
        preamble
    );
    assert!(
        !preamble.contains('\\'),
        "preamble must not contain backslashes for shell safety. got: {}",
        preamble
    );
}

// ---------------------------------------------------------------------------
// Unknown agent slug validation (task-06 pre-existing finding)
// ---------------------------------------------------------------------------

/// AC-02: `--tpm-disable-agent nonexistent-agent` succeeds (exit 0) but stderr
/// contains a warning about the unknown slug.
#[test]
#[serial]
fn aoe_add_tpm_disable_unknown_agent_warns_on_stderr() {
    let (h, plugin_dir, _orch_path) = setup_tpm_harness("tpm_unknown_agent_warn");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-disable-agent",
            "nonexistent-agent",
            "-t",
            "Unknown Agent Test",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add should succeed even with unknown agent slug.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nonexistent-agent"),
        "stderr should mention the unknown slug 'nonexistent-agent'. stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("warning") || stderr.contains("unknown"),
        "stderr should contain 'warning' or 'unknown'. stderr: {}",
        stderr
    );
}

/// AC-03: `--tpm-disable-agent blind-hunter` (a known agent) produces no
/// warning on stderr.
#[test]
#[serial]
fn aoe_add_tpm_disable_known_agent_no_warning() {
    let (h, plugin_dir, _orch_path) = setup_tpm_harness("tpm_known_agent_ok");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-disable-agent",
            "blind-hunter",
            "-t",
            "Known Agent Test",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add should succeed with known agent slug.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("warning"),
        "stderr should NOT contain a warning for known agent 'blind-hunter'. stderr: {}",
        stderr
    );
}

/// AC-04: `--tpm-disable-agent implementer` is silently stripped (existing
/// behavior preserved). The implementer cannot be disabled.
#[test]
#[serial]
fn aoe_add_tpm_disable_implementer_silently_stripped() {
    let (h, plugin_dir, _orch_path) = setup_tpm_harness("tpm_implementer_strip");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--tpm-disable-agent",
            "implementer",
            "-t",
            "Implementer Strip Test",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add should succeed when disabling implementer.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("warning"),
        "stderr should NOT contain a warning for 'implementer' (silently stripped). stderr: {}",
        stderr
    );

    // Verify config.json has empty disabled_agents (implementer was stripped)
    let config = read_tpm_config(&h);
    let disabled = config["disabled_agents"].as_array();
    assert!(
        disabled.is_none() || disabled.unwrap().is_empty(),
        "disabled_agents should be empty after stripping implementer. config: {}",
        config
    );
}
