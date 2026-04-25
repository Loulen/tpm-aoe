//! End-to-end smoke tests for the TPM workflow integration.
//!
//! These tests don't actually boot the orchestrator — they only verify the
//! wiring: that `aoe add --tpm` resolves the orchestrator system prompt and
//! injects it into the spawned session's `extra_args`. The plugin is faked
//! out via `TPM_WORKFLOW_PATH` so the test stays hermetic.

use serial_test::serial;
use std::process::Command;
use tempfile::TempDir;

use crate::harness::TuiTestHarness;
use crate::helpers::{read_sessions, write_fake_orchestrator};

#[test]
#[serial]
fn aoe_add_tpm_injects_orchestrator_prompt() {
    let h = TuiTestHarness::new("tpm_add_smoke");
    let project = h.project_path();

    // Make the project a real git repo so future tests that combine --tpm
    // with --worktree have a canonical working setup. The smoke check below
    // doesn't strictly need git, but using a real repo keeps the fixture
    // close to how users will actually invoke the flag.
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

    // Fake plugin install via env override so the test doesn't depend on the
    // user's real ~/.claude/plugins state.
    let plugin_dir = TempDir::new().expect("plugin tempdir");
    let orch_path = write_fake_orchestrator(plugin_dir.path());

    let output = h.run_cli_with_env(
        &["add", project.to_str().unwrap(), "-t", "TPM Smoke", "--tpm"],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let sessions = read_sessions(&h);
    let session = sessions
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["title"] == "TPM Smoke"))
        .expect("TPM Smoke session not persisted");

    assert_eq!(
        session["tool"].as_str().unwrap_or(""),
        "claude",
        "TPM mode must run on claude"
    );

    let extra_args = session["extra_args"].as_str().unwrap_or("");
    assert!(
        extra_args.contains("--append-system-prompt"),
        "extra_args should carry the orchestrator system prompt flag, got: {}",
        extra_args
    );
    assert!(
        extra_args.contains(orch_path.to_string_lossy().as_ref()),
        "extra_args should reference the fake orchestrator path. extra_args={} expected substring={}",
        extra_args,
        orch_path.display()
    );
}

#[test]
#[serial]
fn aoe_add_tpm_without_plugin_errors_out() {
    // Sanity check: with no env override and the harness's isolated $HOME
    // (which has no ~/.claude/plugins/...), the resolver should fail and the
    // command should return a non-zero exit code. This guards against the
    // wiring silently swallowing a missing plugin.
    let h = TuiTestHarness::new("tpm_add_no_plugin");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "-t",
            "TPM No Plugin",
            "--tpm",
        ],
        &[],
    );

    assert!(
        !output.status.success(),
        "aoe add --tpm should fail when the plugin is not installed.\nstdout: {}\nstderr: {}",
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
            || combined.contains("TPM_WORKFLOW_PATH")
            || combined.contains("tpm-workflow"),
        "error should hint at the missing plugin. combined output: {}",
        combined
    );
}
