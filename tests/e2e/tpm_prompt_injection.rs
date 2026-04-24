//! E2E tests for the full TPM prompt injection chain.
//!
//! Existing tests (`tpm.rs`, `tpm_tier.rs`) verify that `aoe add --tpm`
//! writes the correct `extra_args` into `sessions.json`. This module goes
//! one step further: it evaluates the shell command that tmux would run,
//! including the `$(cat orchestrator.md)` expansion, and asserts on the
//! content the model binary actually receives via `--append-system-prompt`.
//!
//! Approach: a fake `claude` shell script captures the `--append-system-prompt`
//! argument value to a file. The test reconstructs the launch command from
//! `sessions.json` and runs it through `bash -c`, which triggers the same
//! shell expansion that tmux does at session start time.
//!
//! # Scenarios
//!
//! 1. CLI fast tier: prompt contains preamble + orchestrator, config.json tier=fast
//! 2. CLI prod tier: same, tier=prod
//! 3. CLI standard tier (default): same, tier=standard
//! 4. TUI creation with fast tier: same chain via TUI dialog
//! 5. Prompt content: key preamble phrases + orchestrator content ordering

use serial_test::serial;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

use crate::harness::{require_tmux, TuiTestHarness};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Orchestrator content written to the fake plugin. Contains recognizable
/// phrases that the tests assert on after shell expansion.
const FAKE_ORCHESTRATOR_CONTENT: &str = "\
# TPM Orchestrator

You are the Technical Project Manager for this codebase.
Follow the PLAN then APPROVE then DISPATCH cycle.
Coordinate sub-sessions via the aoe CLI.
Never edit project source code directly.
";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write `agents/orchestrator.md` with known content under `root`.
fn write_orchestrator(root: &Path) {
    let agents = root.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(agents.join("orchestrator.md"), FAKE_ORCHESTRATOR_CONTENT)
        .expect("write orchestrator.md");
}

/// Write a shell script that captures `--append-system-prompt` to `capture_path`.
/// Returns the script's absolute path.
fn write_capture_claude(dir: &Path, capture_path: &Path) -> std::path::PathBuf {
    let script_path = dir.join("capture-claude");
    // The script iterates argv, looks for --append-system-prompt, and writes
    // the NEXT argument (the expanded prompt value) to the capture file.
    let script = format!(
        r#"#!/bin/bash
prev=""
for arg in "$@"; do
  if [ "$prev" = "--append-system-prompt" ]; then
    printf '%s' "$arg" > '{capture}'
  fi
  prev="$arg"
done
"#,
        capture = capture_path.display()
    );
    std::fs::write(&script_path, &script).expect("write capture-claude");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod capture-claude");
    }
    script_path
}

/// Bootstrap a harness with a git-initialized project, a fake plugin dir
/// with known orchestrator content, and a capture-claude script.
///
/// Returns `(harness, plugin_dir, capture_claude_path, capture_file_path)`.
fn setup_injection_harness(
    name: &str,
) -> (
    TuiTestHarness,
    TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
) {
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
    write_orchestrator(plugin_dir.path());

    let capture_file = h.home_path().join("captured-prompt.txt");
    let capture_claude = write_capture_claude(h.home_path(), &capture_file);

    (h, plugin_dir, capture_claude, capture_file)
}

/// Read the persisted `sessions.json` from the harness's isolated profile dir.
fn read_sessions(h: &TuiTestHarness) -> serde_json::Value {
    let path = if cfg!(target_os = "linux") {
        h.home_path()
            .join(".config/agent-of-empires/profiles/default/sessions.json")
    } else {
        h.home_path()
            .join(".agent-of-empires/profiles/default/sessions.json")
    };
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("invalid sessions JSON")
}

/// Find the session entry with the given title.
fn find_session<'a>(sessions: &'a serde_json::Value, title: &str) -> &'a serde_json::Value {
    sessions
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["title"] == title))
        .unwrap_or_else(|| panic!("session with title {:?} not found in sessions.json", title))
}

/// Read `.tpm/config.json` from the project directory inside the harness.
fn read_tpm_config(h: &TuiTestHarness) -> serde_json::Value {
    let path = h.project_path().join(".tpm/config.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("invalid .tpm/config.json")
}

/// Reconstruct the launch command from `sessions.json` fields and run it
/// through `bash -c` to trigger shell expansion (the same expansion tmux
/// performs at session start). Returns the captured prompt text.
///
/// Requires `session["command"]` to be set (via `--cmd-override`) to the
/// capture-claude script's absolute path.
fn run_and_capture_prompt(session: &serde_json::Value, capture_file: &Path) -> String {
    let command = session["command"]
        .as_str()
        .expect("session command should be set (via --cmd-override)");
    let extra_args = session["extra_args"]
        .as_str()
        .expect("session extra_args should be set (TPM injection)");

    assert!(
        !command.is_empty(),
        "command must not be empty for prompt capture"
    );
    assert!(
        !extra_args.is_empty(),
        "extra_args must not be empty for prompt capture"
    );

    let full_cmd = format!("{} {}", command, extra_args);

    let output = Command::new("bash")
        .arg("-c")
        .arg(&full_cmd)
        .output()
        .expect("failed to run bash -c with the reconstructed command");

    assert!(
        capture_file.exists(),
        "capture file was not created. bash exit={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::read_to_string(capture_file).unwrap_or_else(|e| {
        panic!(
            "failed to read capture file {}: {}",
            capture_file.display(),
            e
        )
    })
}

/// For TUI tests: run the capture-claude directly with the `extra_args`
/// extracted from sessions.json. Unlike `run_and_capture_prompt`, this
/// doesn't require `--cmd-override`; the capture-claude path is passed
/// explicitly.
fn run_capture_with_extra_args(
    capture_claude: &Path,
    extra_args: &str,
    capture_file: &Path,
) -> String {
    let full_cmd = format!("{} {}", capture_claude.display(), extra_args);

    let output = Command::new("bash")
        .arg("-c")
        .arg(&full_cmd)
        .output()
        .expect("failed to run bash -c with capture-claude");

    assert!(
        capture_file.exists(),
        "capture file was not created. bash exit={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::read_to_string(capture_file).unwrap_or_else(|e| {
        panic!(
            "failed to read capture file {}: {}",
            capture_file.display(),
            e
        )
    })
}

/// Seed `installed_plugins.json` + orchestrator cache with known content
/// in the harness temp HOME, so the TUI can resolve the plugin.
fn seed_tpm_plugin_with_content(h: &TuiTestHarness) {
    let home = h.home_path();

    let plugins_dir = home.join(".claude").join("plugins");
    std::fs::create_dir_all(&plugins_dir).expect("create plugins dir");

    let cache_dir = plugins_dir
        .join("cache")
        .join("tpm-workflow")
        .join("tpm-workflow")
        .join("0.1.0");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");
    write_orchestrator(&cache_dir);

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

/// Determine how many Tabs from the Path field to reach the TPM Mode field.
fn tabs_from_path_to_tpm(screen: &str) -> usize {
    let has_interactive_tool = screen.lines().any(|line| {
        line.contains("Tool:")
            && (line.contains('●') || line.contains('○'))
            && line.chars().filter(|c| *c == '●' || *c == '○').count() > 1
    });
    if has_interactive_tool {
        3
    } else {
        2
    }
}

// ---------------------------------------------------------------------------
// AC-01: Fast tier — captured prompt contains orchestrator text, tier:fast
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn cli_fast_tier_prompt_injection() {
    let (h, plugin_dir, fake_claude, capture_file) = setup_injection_harness("prompt_inj_fast");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "fast",
            "--cmd-override",
            fake_claude.to_str().unwrap(),
            "-t",
            "Fast Inject",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm fast failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // config.json must have tier:fast
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("fast"),
        "config.json tier should be 'fast', got: {}",
        config
    );

    // Run the shell command and capture the prompt
    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Fast Inject");
    let captured = run_and_capture_prompt(session, &capture_file);

    // Preamble present
    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "captured prompt must contain preamble 'SYSTEM PROMPT OVERRIDE'.\n--- captured (first 300 chars) ---\n{}",
        &captured[..captured.len().min(300)]
    );

    // Orchestrator content present (shell-expanded from $(cat ...))
    assert!(
        captured.contains("Technical Project Manager"),
        "captured prompt must contain orchestrator phrase 'Technical Project Manager'.\n--- captured (first 300 chars) ---\n{}",
        &captured[..captured.len().min(300)]
    );
}

// ---------------------------------------------------------------------------
// AC-02: Prod tier — captured prompt contains orchestrator text, tier:prod
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn cli_prod_tier_prompt_injection() {
    let (h, plugin_dir, fake_claude, capture_file) = setup_injection_harness("prompt_inj_prod");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "prod",
            "--cmd-override",
            fake_claude.to_str().unwrap(),
            "-t",
            "Prod Inject",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm prod failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("prod"),
        "config.json tier should be 'prod', got: {}",
        config
    );

    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Prod Inject");
    let captured = run_and_capture_prompt(session, &capture_file);

    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "captured prompt must contain preamble.\n--- captured ---\n{}",
        &captured[..captured.len().min(300)]
    );
    assert!(
        captured.contains("Technical Project Manager"),
        "captured prompt must contain orchestrator content.\n--- captured ---\n{}",
        &captured[..captured.len().min(300)]
    );
}

// ---------------------------------------------------------------------------
// AC-03: Standard tier (default) — tier:standard, prompt present
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn cli_standard_tier_default_prompt_injection() {
    let (h, plugin_dir, fake_claude, capture_file) = setup_injection_harness("prompt_inj_standard");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "--cmd-override",
            fake_claude.to_str().unwrap(),
            "-t",
            "Standard Inject",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add --tpm (default) failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("standard"),
        "config.json tier should be 'standard', got: {}",
        config
    );

    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Standard Inject");
    let captured = run_and_capture_prompt(session, &capture_file);

    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "captured prompt must contain preamble.\n--- captured ---\n{}",
        &captured[..captured.len().min(300)]
    );
    assert!(
        captured.contains("Technical Project Manager"),
        "captured prompt must contain orchestrator content.\n--- captured ---\n{}",
        &captured[..captured.len().min(300)]
    );
}

// ---------------------------------------------------------------------------
// AC-04: TUI creation with fast tier — full prompt injection via TUI dialog
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn tui_fast_tier_prompt_injection() {
    require_tmux!();

    let h = TuiTestHarness::new("prompt_inj_tui_fast");
    let project = h.project_path();
    let project_str = project.to_str().unwrap().to_string();

    let git_init = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(&project)
        .output()
        .expect("git init");
    assert!(git_init.status.success());

    // Seed the plugin with known orchestrator content so TUI can resolve it
    seed_tpm_plugin_with_content(&h);

    // Prepare capture infrastructure
    let capture_file = h.home_path().join("captured-prompt-tui.txt");
    let capture_claude = write_capture_claude(h.home_path(), &capture_file);

    // --- TUI flow: create session with TPM + fast tier ---
    let mut h = h; // rebind as mutable for spawn
    h.spawn_tui();
    h.wait_for("Agent of Empires");

    // Open new session dialog
    h.send_keys("n");
    h.wait_for("Title");

    // Type the title
    h.type_text("TUI Fast Task");
    std::thread::sleep(Duration::from_millis(100));

    // Tab to Path field
    h.send_keys("Tab");
    std::thread::sleep(Duration::from_millis(100));

    // Clear and type the project path
    h.send_keys("C-u");
    std::thread::sleep(Duration::from_millis(100));
    h.type_text(&project_str);
    std::thread::sleep(Duration::from_millis(100));

    // Tab from Path to TPM Mode field
    let screen = h.capture_screen();
    let tabs_needed = tabs_from_path_to_tpm(&screen);
    for _ in 0..tabs_needed {
        h.send_keys("Tab");
        std::thread::sleep(Duration::from_millis(100));
    }

    // Toggle TPM on
    h.send_keys("Space");
    std::thread::sleep(Duration::from_millis(200));

    // Open TPM config overlay with Ctrl+P
    h.send_keys("C-p");
    std::thread::sleep(Duration::from_millis(300));
    h.wait_for("TPM Configuration");

    // Select fast tier: default is standard, Left goes to fast
    h.send_keys("Left");
    std::thread::sleep(Duration::from_millis(100));

    let screen = h.capture_screen();
    assert!(
        screen.contains("● fast"),
        "fast tier should be selected.\n--- Screen ---\n{}",
        screen
    );

    // Close overlay
    h.send_keys("Escape");
    std::thread::sleep(Duration::from_millis(200));

    // Submit the dialog
    h.send_keys("Enter");
    std::thread::sleep(Duration::from_millis(500));

    // Handle "Path does not exist. Create?" prompt if it appears
    let screen = h.capture_screen();
    if screen.contains("Create?") || screen.contains("create") {
        h.send_keys("y");
        std::thread::sleep(Duration::from_millis(500));
    }

    // Wait for the session to appear in the list
    h.wait_for_timeout("TUI Fast Task", Duration::from_secs(10));

    // Verify config.json has tier:fast
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("fast"),
        "config.json tier should be 'fast' (TUI), got: {}",
        config
    );

    // Read sessions.json to get the extra_args produced by the TUI
    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "TUI Fast Task");
    let extra_args = session["extra_args"]
        .as_str()
        .expect("TUI session should have extra_args");

    assert!(
        extra_args.contains("--append-system-prompt"),
        "TUI extra_args must contain --append-system-prompt. got: {}",
        extra_args
    );

    // Run capture-claude with the TUI-generated extra_args
    let captured = run_capture_with_extra_args(&capture_claude, extra_args, &capture_file);

    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "TUI-created prompt must contain preamble.\n--- captured ---\n{}",
        &captured[..captured.len().min(300)]
    );
    assert!(
        captured.contains("Technical Project Manager"),
        "TUI-created prompt must contain orchestrator content.\n--- captured ---\n{}",
        &captured[..captured.len().min(300)]
    );
}

// ---------------------------------------------------------------------------
// AC-05: Prompt contains key orchestrator phrases + correct ordering
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn prompt_contains_key_orchestrator_phrases() {
    let (h, plugin_dir, fake_claude, capture_file) = setup_injection_harness("prompt_inj_phrases");
    let project = h.project_path();

    let output = h.run_cli_with_env(
        &[
            "add",
            project.to_str().unwrap(),
            "--tpm",
            "fast",
            "--cmd-override",
            fake_claude.to_str().unwrap(),
            "-t",
            "Phrase Check",
        ],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        output.status.success(),
        "aoe add failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let sessions = read_sessions(&h);
    let session = find_session(&sessions, "Phrase Check");
    let captured = run_and_capture_prompt(session, &capture_file);

    // --- Preamble phrases (from TPM_PREAMBLE constant) ---
    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "must contain 'SYSTEM PROMPT OVERRIDE'"
    );
    assert!(
        captured.contains("TPM ORCHESTRATOR MODE"),
        "must contain 'TPM ORCHESTRATOR MODE'"
    );
    assert!(
        captured.contains("You are the orchestrator"),
        "must contain 'You are the orchestrator' from preamble"
    );

    // --- Orchestrator content phrases (from FAKE_ORCHESTRATOR_CONTENT) ---
    assert!(
        captured.contains("Technical Project Manager"),
        "must contain orchestrator phrase 'Technical Project Manager'"
    );
    assert!(
        captured.contains("PLAN then APPROVE then DISPATCH"),
        "must contain orchestrator phrase 'PLAN then APPROVE then DISPATCH'"
    );
    assert!(
        captured.contains("Coordinate sub-sessions via the aoe CLI"),
        "must contain orchestrator phrase about aoe CLI coordination"
    );

    // --- Ordering: preamble comes before orchestrator content ---
    let preamble_pos = captured
        .find("SYSTEM PROMPT OVERRIDE")
        .expect("preamble position");
    let orchestrator_pos = captured
        .find("Technical Project Manager")
        .expect("orchestrator position");
    assert!(
        preamble_pos < orchestrator_pos,
        "preamble (pos {}) must appear before orchestrator content (pos {})",
        preamble_pos,
        orchestrator_pos
    );
}
