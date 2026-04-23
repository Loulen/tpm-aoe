//! E2E tests for TPM session creation through the TUI new-session dialog and
//! badge rendering (user journeys 1-3).
//!
//! Journey 1: create a TPM session via the TUI dialog (TPM checkbox + tier
//!   overlay), then verify the badge and `.tpm/config.json`.
//! Journey 2: create a TPM session via `aoe add --tpm fast`, spawn TUI, verify
//!   the badge.
//! Journey 3: badge discrimination; 2 TPM + 1 regular session, verify only the
//!   TPM sessions show " TPM".

use serial_test::serial;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

use crate::harness::{require_tmux, TuiTestHarness};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Drop a fake `agents/orchestrator.md` under `root`.
fn write_fake_orchestrator(root: &Path) {
    let agents = root.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(agents.join("orchestrator.md"), "# Fake Orchestrator\n")
        .expect("write orchestrator.md");
}

/// Seed `installed_plugins.json` + orchestrator cache in the harness temp HOME
/// so `is_installed()` and `resolve_orchestrator()` succeed inside the TUI.
fn seed_tpm_plugin(h: &TuiTestHarness) {
    let home = h.home_path();

    // installed_plugins.json tells is_installed() the plugin is present
    let plugins_dir = home.join(".claude").join("plugins");
    std::fs::create_dir_all(&plugins_dir).expect("create plugins dir");

    let cache_dir = plugins_dir
        .join("cache")
        .join("tpm-workflow")
        .join("tpm-workflow")
        .join("0.1.0");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");
    write_fake_orchestrator(&cache_dir);

    let installed = serde_json::json!({
        "schema_version": 2,
        "plugins": {
            "tpm-workflow@tpm-workflow": [{
                "version": "0.1.0",
                "path": cache_dir.to_string_lossy()
            }]
        }
    });
    std::fs::write(
        plugins_dir.join("installed_plugins.json"),
        serde_json::to_string_pretty(&installed).unwrap(),
    )
    .expect("write installed_plugins.json");
}

/// Create a harness with a git-initialized project and seeded TPM plugin.
fn setup_tpm_tui_harness(name: &str) -> (TuiTestHarness, TempDir) {
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

    seed_tpm_plugin(&h);

    // Also create a fake plugin dir for CLI --tpm usage
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

/// Determine how many Tabs from the Path field to reach the TPM Mode field.
/// Checks the screen for interactive tool selection (●/○ radio on Tool line).
fn tabs_from_path_to_tpm(screen: &str) -> usize {
    // If the Tool line has radio buttons (● and ○), the Tool field is interactive
    let has_interactive_tool = screen.lines().any(|line| {
        line.contains("Tool:")
            && (line.contains('●') || line.contains('○'))
            && line.chars().filter(|c| *c == '●' || *c == '○').count() > 1
    });
    // Fields between Path and TPM: [Tool?] + YOLO + TPM
    if has_interactive_tool {
        3
    } else {
        2
    }
}

// ---------------------------------------------------------------------------
// AC-01 (Journey 1): TUI dialog creation with prod tier
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tui_create_tpm_session_prod_tier() {
    require_tmux!();

    let (mut h, _plugin_dir) = setup_tpm_tui_harness("tpm_tui_create_prod");
    let project = h.project_path();
    let project_str = project.to_str().unwrap().to_string();

    h.spawn_tui();
    h.wait_for("Agent of Empires");

    // Open new session dialog
    h.send_keys("n");
    h.wait_for("Title");

    // Type the title
    h.type_text("Prod Task");
    std::thread::sleep(Duration::from_millis(100));

    // Tab to Path field
    h.send_keys("Tab");
    std::thread::sleep(Duration::from_millis(100));

    // Clear the Path field (Ctrl+U = DeleteLine) and type the project path
    h.send_keys("C-u");
    std::thread::sleep(Duration::from_millis(100));
    h.type_text(&project_str);
    std::thread::sleep(Duration::from_millis(100));

    // Determine how many tabs from Path to TPM
    let screen = h.capture_screen();
    let tabs_needed = tabs_from_path_to_tpm(&screen);

    // Tab to TPM Mode field
    for _ in 0..tabs_needed {
        h.send_keys("Tab");
        std::thread::sleep(Duration::from_millis(100));
    }

    // Toggle TPM on with Space
    h.send_keys("Space");
    std::thread::sleep(Duration::from_millis(200));

    // Verify TPM toggled on (shows [x] and "standard" default tier)
    let screen = h.capture_screen();
    let tpm_enabled = screen
        .lines()
        .any(|line| line.contains("TPM Mode:") && line.contains("[x]"));
    assert!(
        tpm_enabled,
        "TPM Mode should be toggled on with [x].\n--- Screen ---\n{}",
        screen
    );

    // Open TPM config overlay with Ctrl+P
    h.send_keys("C-p");
    std::thread::sleep(Duration::from_millis(300));

    // Verify the TPM Configuration overlay is visible
    h.wait_for("TPM Configuration");

    // Tier field is focused by default. Default tier is standard.
    // Press Right once to go from standard → prod
    h.send_keys("Right");
    std::thread::sleep(Duration::from_millis(100));

    // Verify prod is selected (● prod)
    let screen = h.capture_screen();
    assert!(
        screen.contains("● prod"),
        "prod should be selected.\n--- Screen ---\n{}",
        screen
    );

    // Close the overlay with Esc
    h.send_keys("Escape");
    std::thread::sleep(Duration::from_millis(200));

    // Verify we're back on the main dialog with prod tier shown
    let screen = h.capture_screen();
    assert!(
        screen.contains("Orchestrator (prod)"),
        "TPM should show prod tier.\n--- Screen ---\n{}",
        screen
    );

    // Submit the dialog with Enter
    h.send_keys("Enter");
    std::thread::sleep(Duration::from_millis(500));

    // Handle "Path does not exist. Create?" if it appears
    let screen = h.capture_screen();
    if screen.contains("Create?") || screen.contains("create") {
        h.send_keys("y");
        std::thread::sleep(Duration::from_millis(500));
    }

    // Wait for the session to appear in the list
    h.wait_for_timeout("Prod Task", Duration::from_secs(10));

    // Verify " TPM" badge appears on the same line as "Prod Task"
    let screen = h.capture_screen();
    let prod_line = screen
        .lines()
        .find(|line| line.contains("Prod Task"))
        .unwrap_or_else(|| panic!("'Prod Task' not found in screen:\n{}", screen));
    assert!(
        prod_line.contains(" TPM"),
        "The line with 'Prod Task' should contain ' TPM' badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        prod_line,
        screen
    );

    // Verify .tpm/config.json was created with tier: prod
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("prod"),
        "config.json tier should be 'prod', got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-02 (Journey 2): CLI create → TUI verify badge
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn cli_create_tpm_fast_then_tui_shows_badge() {
    require_tmux!();

    let (mut h, plugin_dir) = setup_tpm_tui_harness("tpm_tui_cli_badge");
    let project = h.project_path();

    // Create TPM session via CLI
    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "fast",
            "-t",
            "Fast Runner",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm fast failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Spawn TUI and verify badge
    h.spawn_tui();
    h.wait_for("Agent of Empires");
    h.wait_for("Fast Runner");

    let screen = h.capture_screen();
    let runner_line = screen
        .lines()
        .find(|line| line.contains("Fast Runner"))
        .unwrap_or_else(|| panic!("'Fast Runner' not found in screen:\n{}", screen));
    assert!(
        runner_line.contains(" TPM"),
        "The line with 'Fast Runner' should contain ' TPM' badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        runner_line,
        screen
    );
}

// ---------------------------------------------------------------------------
// AC-03 (Journey 3): badge discrimination, 2 TPM + 1 regular
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn badge_discrimination_two_tpm_one_regular() {
    require_tmux!();

    let (mut h, plugin_dir) = setup_tpm_tui_harness("tpm_tui_badge_discrim");
    let project = h.project_path();
    let project_str = project.to_str().unwrap();

    // Create "TPM Alpha" with --tpm fast
    let output = h.run_cli_with_env(
        &["add", project_str, "--tpm", "fast", "-t", "TPM Alpha"],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add TPM Alpha failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Create "TPM Beta" with --tpm prod
    let output = h.run_cli_with_env(
        &["add", project_str, "--tpm", "prod", "-t", "TPM Beta"],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add TPM Beta failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Create "Regular Charlie" without --tpm
    let output = h.run_cli_with_env(
        &["add", project_str, "-t", "Regular Charlie"],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add Regular Charlie failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Spawn TUI
    h.spawn_tui();
    h.wait_for("Agent of Empires");

    // Wait for all sessions to render
    h.wait_for("TPM Alpha");
    h.wait_for("TPM Beta");
    h.wait_for("Regular Charlie");

    let screen = h.capture_screen();

    // "TPM Alpha" line should contain " TPM"
    let alpha_line = screen
        .lines()
        .find(|line| line.contains("TPM Alpha"))
        .unwrap_or_else(|| panic!("'TPM Alpha' not found in screen:\n{}", screen));
    assert!(
        alpha_line.contains(" TPM"),
        "'TPM Alpha' line should contain ' TPM' badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        alpha_line,
        screen
    );

    // "TPM Beta" line should contain " TPM"
    let beta_line = screen
        .lines()
        .find(|line| line.contains("TPM Beta"))
        .unwrap_or_else(|| panic!("'TPM Beta' not found in screen:\n{}", screen));
    assert!(
        beta_line.contains(" TPM"),
        "'TPM Beta' line should contain ' TPM' badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        beta_line,
        screen
    );

    // "Regular Charlie" line should NOT contain " TPM"
    let charlie_line = screen
        .lines()
        .find(|line| line.contains("Regular Charlie"))
        .unwrap_or_else(|| panic!("'Regular Charlie' not found in screen:\n{}", screen));
    assert!(
        !charlie_line.contains(" TPM"),
        "'Regular Charlie' line should NOT contain ' TPM' badge.\nLine: {:?}\n\n--- Full screen ---\n{}",
        charlie_line,
        screen
    );

    // Count total lines with " TPM" badge - should be exactly 2
    let tpm_badge_count = screen.lines().filter(|line| line.contains(" TPM")).count();
    assert_eq!(
        tpm_badge_count, 2,
        "Exactly 2 lines should contain ' TPM', got {}.\n--- Full screen ---\n{}",
        tpm_badge_count, screen
    );
}
