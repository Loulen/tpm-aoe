//! E2E tests for TPM session deletion and archival (Journeys 4-6).
//!
//! Validates:
//! - Journey 4: TUI deletion of a TPM session triggers `.tpm/` artifact archival
//! - Journey 5: CLI deletion (`aoe remove`) with metadata.json content checks
//! - Journey 6: Non-TPM session deletion does NOT create history entries
//!
//! These complement `tpm_artifacts.rs` (task-07) by exercising the TUI deletion
//! path (via `deletion_poller.rs`) in addition to the CLI path, and by verifying
//! archive content rather than just existence.

use serial_test::serial;
use std::time::Duration;

use crate::harness::{require_tmux, TuiTestHarness};
use crate::helpers::{
    find_session, history_dir, list_dir_entries, read_sessions, seed_tpm_artifacts,
    setup_tpm_harness,
};

// ---------------------------------------------------------------------------
// AC-01 (Journey 4): TUI deletion of TPM session archives artifacts
// ---------------------------------------------------------------------------

/// User creates TPM session via CLI, seeds `.tpm/` with STATE.md and SUMMARY.md,
/// spawns TUI, selects session, presses `d` then `y` to confirm deletion.
/// Session disappears from screen. `history/` has a new subdirectory containing
/// STATE.md (content matches "all done"), SUMMARY.md, and metadata.json with
/// `"session_id"` and `"title"` fields.
#[test]
#[serial]
fn tui_delete_tpm_session_archives_artifacts() {
    require_tmux!();

    let (mut h, plugin_dir, _) = setup_tpm_harness("tpm_tui_del");
    let project = h.project_path();

    // Create TPM session via CLI
    let add_output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "-t",
            "TUI Del TPM",
            "--tpm",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        add_output.status.success(),
        "aoe add --tpm failed: {}",
        String::from_utf8_lossy(&add_output.stderr)
    );

    // Seed .tpm/ with the exact content from the AC spec
    seed_tpm_artifacts(&project, "## Status\nall done", "## Summary\ntests passed");

    // Verify no history before deletion
    let history = history_dir(&h);
    assert!(
        list_dir_entries(&history).is_empty(),
        "history should be empty before deletion"
    );

    // Spawn TUI and wait for it to load
    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("TUI Del TPM");

    // Press 'd' to open the delete dialog
    h.send_keys("d");
    h.wait_for("Delete Session");

    // For sessions without worktree/sandbox, focus starts on NoButton.
    // Press 'y' to confirm directly (shortcut key).
    h.send_keys("y");

    // Wait for the session to disappear from the TUI
    h.wait_for_absent("TUI Del TPM", Duration::from_secs(10));

    // --- Verify archive contents ---
    let entries = list_dir_entries(&history);
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one archive entry in history, got {}",
        entries.len()
    );

    let archive_path = entries[0].path();

    // STATE.md: content matches "all done"
    let state_content =
        std::fs::read_to_string(archive_path.join("STATE.md")).expect("STATE.md in archive");
    assert!(
        state_content.contains("all done"),
        "STATE.md should contain 'all done', got: {}",
        state_content
    );

    // SUMMARY.md: exists and content matches
    let summary_content =
        std::fs::read_to_string(archive_path.join("SUMMARY.md")).expect("SUMMARY.md in archive");
    assert!(
        summary_content.contains("tests passed"),
        "SUMMARY.md should contain 'tests passed', got: {}",
        summary_content
    );

    // metadata.json: has session_id and title
    let meta_str = std::fs::read_to_string(archive_path.join("metadata.json"))
        .expect("metadata.json in archive");
    let meta: serde_json::Value = serde_json::from_str(&meta_str).expect("valid metadata JSON");
    assert!(
        meta["session_id"].as_str().is_some() && !meta["session_id"].as_str().unwrap().is_empty(),
        "metadata should have non-empty session_id"
    );
    assert_eq!(
        meta["title"].as_str().unwrap(),
        "TUI Del TPM",
        "metadata title mismatch"
    );
}

// ---------------------------------------------------------------------------
// AC-02 (Journey 5): CLI deletion with archive content verification
// ---------------------------------------------------------------------------

/// Same setup as Journey 4 but deletion via `aoe remove <title>` CLI.
/// Verifies history/ archive has metadata.json with valid `"archived_at"` ISO
/// timestamp, correct `"session_id"`, `"title"`, and `"project_path"` fields.
#[test]
#[serial]
fn cli_remove_tpm_session_archives_with_valid_metadata() {
    let (h, plugin_dir, _) = setup_tpm_harness("tpm_cli_del");
    let project = h.project_path();

    // Create TPM session via CLI
    let add_output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "-t",
            "CLI Del TPM",
            "--tpm",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        add_output.status.success(),
        "aoe add --tpm failed: {}",
        String::from_utf8_lossy(&add_output.stderr)
    );

    // Get session ID for later assertion
    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "CLI Del TPM");
    let session_id = session["id"]
        .as_str()
        .expect("session has no id")
        .to_string();

    // Seed .tpm/ artifacts with the AC spec content
    seed_tpm_artifacts(&project, "## Status\nall done", "## Summary\ntests passed");

    // Verify no history before removal
    let history = history_dir(&h);
    assert!(
        list_dir_entries(&history).is_empty(),
        "history should be empty before remove"
    );

    // Remove via CLI using the title
    let remove_output = h.run_cli(&["remove", "CLI Del TPM"]);
    assert!(
        remove_output.status.success(),
        "aoe remove failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&remove_output.stdout),
        String::from_utf8_lossy(&remove_output.stderr)
    );

    // --- Verify archive contents ---
    let entries = list_dir_entries(&history);
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one archive entry in history, got {}",
        entries.len()
    );

    let archive_path = entries[0].path();

    // STATE.md preserved
    let state_content =
        std::fs::read_to_string(archive_path.join("STATE.md")).expect("STATE.md in archive");
    assert!(
        state_content.contains("all done"),
        "STATE.md content not preserved: {}",
        state_content
    );

    // SUMMARY.md preserved
    let summary_content =
        std::fs::read_to_string(archive_path.join("SUMMARY.md")).expect("SUMMARY.md in archive");
    assert!(
        summary_content.contains("tests passed"),
        "SUMMARY.md content not preserved: {}",
        summary_content
    );

    // metadata.json: all fields present and correct
    let meta_str = std::fs::read_to_string(archive_path.join("metadata.json"))
        .expect("metadata.json in archive");
    let meta: serde_json::Value = serde_json::from_str(&meta_str).expect("valid metadata JSON");

    assert_eq!(
        meta["session_id"].as_str().unwrap(),
        session_id,
        "metadata session_id should match the original session"
    );
    assert_eq!(
        meta["title"].as_str().unwrap(),
        "CLI Del TPM",
        "metadata title mismatch"
    );
    assert_eq!(
        meta["project_path"].as_str().unwrap(),
        project.to_str().unwrap(),
        "metadata project_path mismatch"
    );

    // archived_at: must be valid ISO 8601 / RFC 3339
    let archived_at = meta["archived_at"]
        .as_str()
        .expect("archived_at missing from metadata");
    assert!(
        chrono::DateTime::parse_from_rfc3339(archived_at).is_ok(),
        "archived_at is not a valid ISO 8601 timestamp: {}",
        archived_at
    );
}

// ---------------------------------------------------------------------------
// AC-03 (Journey 6): non-TPM deletion does NOT create history entries
// ---------------------------------------------------------------------------

/// User creates a non-TPM session via CLI. Records entry count in `history/`.
/// Deletes session via `aoe remove`. `history/` has the same entry count
/// afterwards (no new archive created).
#[test]
#[serial]
fn cli_remove_non_tpm_session_no_archive() {
    let h = TuiTestHarness::new("tpm_del_no_archive");
    let project = h.project_path();

    // Create a plain session (no --tpm)
    let add_output = h.run_cli(&["add", project.to_str().unwrap(), "-t", "Plain Delete"]);
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

    // Record history entry count before removal
    let history = history_dir(&h);
    let count_before = list_dir_entries(&history).len();

    // Remove the session via CLI
    let remove_output = h.run_cli(&["remove", "Plain Delete"]);
    assert!(
        remove_output.status.success(),
        "aoe remove failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&remove_output.stdout),
        String::from_utf8_lossy(&remove_output.stderr)
    );

    // History should have the same entry count (no new archive)
    let count_after = list_dir_entries(&history).len();
    assert_eq!(
        count_before, count_after,
        "history entry count should not change for non-TPM session removal (before={}, after={})",
        count_before, count_after
    );
}
