//! E2E tests for the STATE.md side panel toggle (Journeys 8-10).
//!
//! Each test creates an isolated session pointing at a project directory with a
//! pre-seeded `.tpm/STATE.md`, spawns the TUI in tmux, selects the session, and
//! toggles the state panel with `S`. Content is verified via `wait_for` (present)
//! and `wait_for_absent` (hidden after second toggle).

use serial_test::serial;
use std::path::Path;
use std::time::Duration;

use crate::harness::{require_tmux, TuiTestHarness};

// ---------------------------------------------------------------------------
// Helpers (duplicated per D-02 convention to avoid cross-file merge conflicts)
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
fn seed_session(h: &TuiTestHarness, title: &str, dir: &Path) {
    let profile_dir = config_dir(h).join("profiles/default");
    std::fs::create_dir_all(&profile_dir).expect("create default profile dir");

    let session = format!(
        r#"[{{"id":"test_state_panel","title":"{title}","project_path":"{}","group_path":"","command":"","tool":"claude","yolo_mode":false,"status":"idle","created_at":"2026-01-01T00:00:00Z","tpm_managed":true}}]"#,
        dir.display(),
    );
    std::fs::write(profile_dir.join("sessions.json"), session).expect("write sessions.json");
}

// ===========================================================================
// AC-01 (Journey 8): Toggle STATE.md panel on/off with Wave 2 content
// ===========================================================================

#[test]
#[serial]
fn state_panel_toggle_wave2_appears_and_disappears() {
    require_tmux!();

    let mut h = TuiTestHarness::new("state_panel_wave2");

    let project = h.project_path();
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).expect("create .tpm dir");
    std::fs::write(
        tpm_dir.join("STATE.md"),
        "## Wave 2 (implementing)\n\n| Task | Status |\n|---|---|\n| task-01 | done |\n",
    )
    .expect("write STATE.md");

    seed_session(&h, "Wave2 Toggle", &project);

    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("Wave2 Toggle");

    // Toggle state panel on
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(800));

    // AC-01a: "Wave 2" should be visible
    h.wait_for_timeout("Wave 2", Duration::from_secs(5));

    // Toggle state panel off
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(500));

    // AC-01b: "Wave 2" should disappear
    h.wait_for_absent("Wave 2", Duration::from_secs(5));
}

// ===========================================================================
// AC-02 (Journey 9): Complex multi-row table content
// ===========================================================================

#[test]
#[serial]
fn state_panel_complex_table_shows_all_rows() {
    require_tmux!();

    let mut h = TuiTestHarness::new("state_panel_table");

    let project = h.project_path();
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).expect("create .tpm dir");
    std::fs::write(
        tpm_dir.join("STATE.md"),
        "# TPM State\n\n## Tasks\n\n| Task | Status |\n|---|---|\n| task-01 | done |\n| task-02 | in-progress |\n| task-03 | blocked |\n",
    )
    .expect("write STATE.md");

    seed_session(&h, "Table Test", &project);

    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("Table Test");

    // Toggle state panel on
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(800));

    // AC-02: all task names and status values visible
    h.wait_for_timeout("task-01", Duration::from_secs(5));
    h.assert_screen_contains("task-02");
    h.assert_screen_contains("task-03");
    h.assert_screen_contains("done");
    h.assert_screen_contains("in-progress");
    h.assert_screen_contains("blocked");
}

// ===========================================================================
// AC-03 (Journey 10): CJK/unicode content
// ===========================================================================

#[test]
#[serial]
fn state_panel_cjk_unicode_content_renders() {
    require_tmux!();

    let mut h = TuiTestHarness::new("state_panel_cjk");

    let project = h.project_path();
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).expect("create .tpm dir");
    std::fs::write(
        tpm_dir.join("STATE.md"),
        "\u{72B6}\u{614B}: \u{5B8C}\u{4E86}\n\n\u{65E5}\u{672C}\u{8A9E}\u{30C6}\u{30B9}\u{30C8}\n",
    )
    .expect("write STATE.md");

    seed_session(&h, "CJK Test", &project);

    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("CJK Test");

    // Toggle state panel on
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(800));

    // AC-03: CJK characters should be visible in the pane capture
    h.wait_for_timeout("\u{72B6}\u{614B}", Duration::from_secs(5));
    h.assert_screen_contains("\u{65E5}\u{672C}\u{8A9E}\u{30C6}\u{30B9}\u{30C8}");
}

// ===========================================================================
// AC-07: Fullscreen toggle and preview hiding
// ===========================================================================

#[test]
#[serial]
fn state_panel_fullscreen_toggle_hides_preview() {
    require_tmux!();

    let mut h = TuiTestHarness::new("state_panel_fs");

    let project = h.project_path();
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).expect("create .tpm dir");
    std::fs::write(
        tpm_dir.join("STATE.md"),
        "## Tasks\n\n| Task | Status |\n|---|---|\n| task-01 | done |\n| task-02 | implementing |\n",
    )
    .expect("write STATE.md");

    seed_session(&h, "FS Toggle", &project);

    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("FS Toggle");

    // Open state panel in split mode
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(800));
    h.wait_for_timeout("task-01", Duration::from_secs(5));
    // Preview title should be visible in split mode
    h.assert_screen_contains("Preview");

    // Press Shift+F to go fullscreen
    h.send_keys("F");
    std::thread::sleep(Duration::from_millis(500));

    // (a) State panel content still visible
    h.assert_screen_contains("task-01");
    // (b) Preview title should be gone (fullscreen hides preview)
    h.assert_screen_not_contains("Preview");

    // Press Shift+F again to restore split mode
    h.send_keys("F");
    std::thread::sleep(Duration::from_millis(500));

    // Preview reappears alongside state panel content
    h.assert_screen_contains("Preview");
    h.assert_screen_contains("task-01");
}

// ===========================================================================
// AC-08: Fullscreen reset on close
// ===========================================================================

#[test]
#[serial]
fn state_panel_fullscreen_resets_on_close() {
    require_tmux!();

    let mut h = TuiTestHarness::new("state_panel_fs_reset");

    let project = h.project_path();
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).expect("create .tpm dir");
    std::fs::write(
        tpm_dir.join("STATE.md"),
        "## Wave 1\n\n| Task | Status |\n|---|---|\n| task-01 | done |\n",
    )
    .expect("write STATE.md");

    seed_session(&h, "FS Reset", &project);

    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("FS Reset");

    // Open panel, go fullscreen
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(800));
    h.wait_for_timeout("task-01", Duration::from_secs(5));
    h.send_keys("F");
    std::thread::sleep(Duration::from_millis(500));

    // Verify fullscreen (no Preview)
    h.assert_screen_not_contains("Preview");

    // Close with S
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(500));

    // Panel content should be hidden
    h.wait_for_absent("TPM State", Duration::from_secs(5));

    // Re-open with S: should be in split mode (not fullscreen)
    h.send_keys("S");
    std::thread::sleep(Duration::from_millis(800));
    h.wait_for_timeout("task-01", Duration::from_secs(5));

    // Preview should be visible (split mode, not fullscreen)
    h.assert_screen_contains("Preview");
}
