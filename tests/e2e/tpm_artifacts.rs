//! E2E tests for TPM artifact archival during session removal (Feature 9).
//!
//! Validates that `aoe remove` archives `.tpm/` artifacts (STATE.md, SUMMARY.md,
//! config.json) to `<app_dir>/history/` with a metadata.json before deleting
//! the session. Also verifies that non-TPM sessions and partial `.tpm/`
//! directories are handled gracefully.
//!
//! The archival call is unconditional in `remove.rs` (line 117), so these tests
//! exercise it for both TPM and plain sessions.

use serial_test::serial;
use std::process::Command;
use tempfile::TempDir;

use crate::harness::TuiTestHarness;
use crate::helpers::{
    find_session, history_dir, list_dir_entries, read_sessions, write_fake_orchestrator,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// AC-01 + AC-02: User creates a TPM session, writes STATE.md and SUMMARY.md
/// to `<project>/.tpm/`, then runs `aoe remove`. The history directory gets a
/// new subdirectory containing STATE.md, SUMMARY.md, and metadata.json with
/// correct session_id, title, and a valid ISO 8601 archived_at timestamp.
#[test]
#[serial]
fn aoe_remove_archives_tpm_artifacts_with_correct_metadata() {
    let h = TuiTestHarness::new("tpm_artifacts_archive");
    let project = h.project_path();

    // git init required for --tpm resolution
    let git_init = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(&project)
        .output()
        .expect("git init");
    assert!(git_init.status.success());

    // Fake plugin so TPM_WORKFLOW_PATH resolves
    let plugin_dir = TempDir::new().expect("plugin tempdir");
    let _orch = write_fake_orchestrator(plugin_dir.path());

    // Create TPM session
    let add_output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "-t",
            "Archive Test",
            "--tpm",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        add_output.status.success(),
        "aoe add --tpm failed: {}",
        String::from_utf8_lossy(&add_output.stderr)
    );

    // Get session ID from sessions.json
    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Archive Test");
    let session_id = session["id"].as_str().expect("session has no id");

    // Write TPM artifacts (config.json already created by --tpm; add STATE.md
    // and SUMMARY.md manually to simulate a completed orchestration run)
    let tpm_dir = project.join(".tpm");
    assert!(tpm_dir.exists(), ".tpm/ should exist after aoe add --tpm");
    std::fs::write(
        tpm_dir.join("STATE.md"),
        "---\nstatus: completed\n---\n# State\nAll waves done.\n",
    )
    .unwrap();
    std::fs::write(
        tpm_dir.join("SUMMARY.md"),
        "# Summary\nAll tasks completed successfully.\n",
    )
    .unwrap();

    // Verify no history before removal
    let history = history_dir(&h);
    assert!(
        list_dir_entries(&history).is_empty(),
        "history should be empty before remove"
    );

    // Remove the session
    let remove_output = h.run_cli(&["remove", session_id]);
    let stdout = String::from_utf8_lossy(&remove_output.stdout);
    let stderr = String::from_utf8_lossy(&remove_output.stderr);
    assert!(
        remove_output.status.success(),
        "aoe remove failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("Removed session"),
        "expected 'Removed session' in stdout: {}",
        stdout
    );

    // AC-01: archive directory exists with expected files
    let entries = list_dir_entries(&history);
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one archive entry in history, got {}",
        entries.len()
    );

    let archive_path = entries[0].path();
    assert!(
        archive_path.join("STATE.md").is_file(),
        "STATE.md missing from archive"
    );
    assert!(
        archive_path.join("SUMMARY.md").is_file(),
        "SUMMARY.md missing from archive"
    );
    assert!(
        archive_path.join("metadata.json").is_file(),
        "metadata.json missing from archive"
    );

    // Verify archived content matches the originals
    let state_content = std::fs::read_to_string(archive_path.join("STATE.md")).unwrap();
    assert!(
        state_content.contains("status: completed"),
        "STATE.md content not preserved in archive"
    );
    let summary_content = std::fs::read_to_string(archive_path.join("SUMMARY.md")).unwrap();
    assert!(
        summary_content.contains("All tasks completed"),
        "SUMMARY.md content not preserved in archive"
    );

    // AC-02: metadata.json has correct session_id, title, and valid archived_at
    let meta_str = std::fs::read_to_string(archive_path.join("metadata.json")).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_str).expect("invalid metadata JSON");
    assert_eq!(
        meta["session_id"].as_str().unwrap(),
        session_id,
        "metadata session_id mismatch"
    );
    assert_eq!(
        meta["title"].as_str().unwrap(),
        "Archive Test",
        "metadata title mismatch"
    );
    let archived_at = meta["archived_at"]
        .as_str()
        .expect("archived_at missing from metadata");
    assert!(
        chrono::DateTime::parse_from_rfc3339(archived_at).is_ok(),
        "archived_at is not valid ISO 8601: {}",
        archived_at
    );
}

/// AC-03: User creates a non-TPM session (no `.tpm/` dir), removes it. No new
/// entry appears in the history directory.
#[test]
#[serial]
fn aoe_remove_non_tpm_session_does_not_create_history_entry() {
    let h = TuiTestHarness::new("tpm_artifacts_no_tpm");
    let project = h.project_path();

    // Create a plain session (no --tpm)
    let add_output = h.run_cli(&["add", project.to_str().unwrap(), "-t", "Plain Session"]);
    assert!(
        add_output.status.success(),
        "aoe add failed: {}",
        String::from_utf8_lossy(&add_output.stderr)
    );

    // Confirm .tpm/ does NOT exist in the project
    assert!(
        !project.join(".tpm").exists(),
        ".tpm/ should not exist for non-TPM session"
    );

    // Remove the session
    let remove_output = h.run_cli(&["remove", "Plain Session"]);
    assert!(
        remove_output.status.success(),
        "aoe remove failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&remove_output.stdout),
        String::from_utf8_lossy(&remove_output.stderr)
    );

    // No history entry should exist
    let history = history_dir(&h);
    assert!(
        list_dir_entries(&history).is_empty(),
        "history should be empty for non-TPM session removal"
    );
}

/// AC-04: TPM session with only config.json in `.tpm/` (no STATE.md). Archival
/// depends on `resolve_tpm_dir` which requires STATE.md, so it should skip
/// cleanly: no crash, no error output, no history entry.
#[test]
#[serial]
fn aoe_remove_partial_tpm_dir_skips_archival_cleanly() {
    let h = TuiTestHarness::new("tpm_artifacts_partial");
    let project = h.project_path();

    // Create a plain session
    let add_output = h.run_cli(&["add", project.to_str().unwrap(), "-t", "Partial TPM"]);
    assert!(
        add_output.status.success(),
        "aoe add failed: {}",
        String::from_utf8_lossy(&add_output.stderr)
    );

    // Manually create .tpm/ with only config.json (no STATE.md)
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).unwrap();
    std::fs::write(tpm_dir.join("config.json"), r#"{"tier":"standard"}"#).unwrap();

    // Remove the session
    let remove_output = h.run_cli(&["remove", "Partial TPM"]);
    let stdout = String::from_utf8_lossy(&remove_output.stdout);
    let stderr = String::from_utf8_lossy(&remove_output.stderr);
    assert!(
        remove_output.status.success(),
        "aoe remove should succeed with partial .tpm/.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // No archive created (resolve_tpm_dir requires STATE.md)
    let history = history_dir(&h);
    assert!(
        list_dir_entries(&history).is_empty(),
        "no history entry should exist when .tpm/ has no STATE.md"
    );

    // No archival error in stderr
    assert!(
        !stderr.contains("failed to archive"),
        "stderr should not contain archival errors: {}",
        stderr
    );
}
