//! E2E tests for the full TPM prompt injection chain.
//!
//! Validates that the prompt injection pipeline works end-to-end for each TPM
//! tier: `aoe add --tpm <tier>` → `aoe session start` → tmux pane launches the
//! claude binary → claude receives `--append-system-prompt` with the preamble
//! ("SYSTEM PROMPT OVERRIDE") and the orchestrator file content.
//!
//! Uses a capturing claude stub that writes all args to a file so the test can
//! parse the `--append-system-prompt` value and verify its contents.
//!
//! ## Environment challenge
//!
//! `aoe session start` creates a tmux session on the default tmux server. The
//! command inside that session runs via `bash -lc '...'`, inheriting HOME and
//! PATH from the server's global environment (not from the `aoe` process). To
//! make the capturing stub findable, we temporarily set `tmux set-environment -g
//! HOME` to the fake home and create shell profile files (`.bash_profile`,
//! `.zshenv`) that prepend the stub directory to PATH. A Drop guard restores the
//! original HOME on cleanup.

use serial_test::serial;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

use crate::harness::{require_tmux, TuiTestHarness};

// ---------------------------------------------------------------------------
// tmux environment guard
// ---------------------------------------------------------------------------

/// Temporarily overrides the default tmux server's global HOME so that the
/// login shell inside sessions created by `aoe session start` sources our
/// test profile files (which set PATH to include the claude stub).
///
/// Restores the original HOME on drop.
struct TmuxEnvGuard {
    /// The original HOME value, or `None` if the server wasn't running (or
    /// HOME wasn't set).
    original_home: Option<String>,
    /// Whether a tmux server was already running before the guard was created.
    server_was_running: bool,
}

impl TmuxEnvGuard {
    /// # Warning: default tmux server side-effect
    ///
    /// This temporarily sets `HOME` in the default tmux server's global
    /// environment. During the guard's lifetime (~15s per test), any new
    /// tmux windows or panes on the default server will inherit the fake
    /// HOME. The original value is restored on drop.
    ///
    /// This is acceptable for CI (isolated environment) but can be
    /// surprising when running tests on a developer workstation with an
    /// active tmux session. The `#[serial]` attribute on the test
    /// functions prevents concurrent test runs from conflicting, but
    /// other tmux activity during the test window may be affected.
    fn set(fake_home: &Path) -> Self {
        eprintln!(
            "WARNING: TmuxEnvGuard is temporarily overriding the default tmux server's \
             global HOME to '{}'. New tmux windows may inherit this fake HOME until the \
             guard is dropped (~15s).",
            fake_home.display()
        );

        // Probe whether a tmux server is already running.
        let server_was_running = Command::new("tmux")
            .args(["list-sessions"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Save the original global HOME (if the server is running).
        let original_home = if server_was_running {
            Command::new("tmux")
                .args(["show-environment", "-g", "HOME"])
                .output()
                .ok()
                .and_then(|o| {
                    if !o.status.success() {
                        return None;
                    }
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    s.strip_prefix("HOME=").map(|v| v.to_string())
                })
        } else {
            None
        };

        // Set tmux global HOME to the fake home. If no server is running,
        // this fails silently; that's fine because `aoe session start` will
        // start a new server that inherits from the `aoe` process (which
        // already has HOME set to the fake home via `run_cli_with_env`).
        let _ = Command::new("tmux")
            .args(["set-environment", "-g", "HOME", fake_home.to_str().unwrap()])
            .output();

        Self {
            original_home,
            server_was_running,
        }
    }
}

impl Drop for TmuxEnvGuard {
    fn drop(&mut self) {
        if self.server_was_running {
            if let Some(ref orig) = self.original_home {
                let _ = Command::new("tmux")
                    .args(["set-environment", "-g", "HOME", orig])
                    .output();
            } else {
                // HOME wasn't explicitly in the global env; unset our override.
                let _ = Command::new("tmux")
                    .args(["set-environment", "-g", "-u", "HOME"])
                    .output();
            }
        }
        // If the server wasn't running before, aoe may have started one.
        // We don't kill it; the session cleanup in the test body handles
        // removing aoe sessions, and the server will idle-exit on its own.
    }
}

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

/// Create shell profile files in the fake home that prepend `stub_path` to
/// PATH. This ensures the capturing claude stub is found when the tmux pane
/// runs `bash -lc` (or `zsh -lc`).
fn write_shell_profiles(home: &Path, stub_path: &Path) {
    let export_line = format!("export PATH=\"{}:$PATH\"\n", stub_path.display());

    // bash -l sources .bash_profile (or .profile if no .bash_profile)
    std::fs::write(home.join(".bash_profile"), &export_line).expect("write .bash_profile");
    std::fs::write(home.join(".profile"), &export_line).expect("write .profile");

    // zsh -l sources .zshenv (always) and .zprofile (login)
    std::fs::write(home.join(".zshenv"), &export_line).expect("write .zshenv");
    std::fs::write(home.join(".zprofile"), &export_line).expect("write .zprofile");
}

/// Create a harness with a git-initialized project and a fake plugin dir.
/// The claude stub is replaced with a capturing version that writes the
/// `--append-system-prompt` value to `$HOME/.captured-prompt.txt`.
/// Shell profile files are created so the stub is found via PATH.
fn setup_capturing_harness(name: &str) -> (TuiTestHarness, TempDir) {
    let h = TuiTestHarness::new(name);
    let project = h.project_path();

    // git init so the project is a valid repo
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

    // Overwrite the default exit-0 claude stub with a capturing version.
    // This stub parses args to find --append-system-prompt and writes its
    // value to $HOME/.captured-prompt.txt, then drops into an interactive
    // shell so the tmux pane stays alive.
    let capturing_stub = r#"#!/bin/bash
while [ $# -gt 0 ]; do
    case "$1" in
        --append-system-prompt)
            echo "$2" > "$HOME/.captured-prompt.txt"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done
exec bash -i
"#;
    let claude_path = h.stub_path().join("claude");
    std::fs::write(&claude_path, capturing_stub).expect("write capturing claude stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&claude_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod capturing stub");
    }

    // Create shell profiles so the stub is discoverable via PATH inside the
    // tmux pane that `aoe session start` creates.
    write_shell_profiles(h.home_path(), h.stub_path());

    (h, plugin_dir)
}

/// Read `.tpm/config.json` from the project directory inside the harness.
fn read_tpm_config(h: &TuiTestHarness) -> serde_json::Value {
    let path = h.project_path().join(".tpm/config.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("invalid .tpm/config.json")
}

/// Wait for the captured prompt file to appear, then read it.
/// Polls with a 200ms interval, panics after `timeout`.
fn wait_for_captured_prompt(home: &Path, timeout: Duration) -> String {
    let path = home.join(".captured-prompt.txt");
    let start = std::time::Instant::now();
    loop {
        if path.exists() {
            // Brief extra sleep to ensure the file is fully written.
            std::thread::sleep(Duration::from_millis(200));
            return std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        }
        if start.elapsed() > timeout {
            // Dump diagnostic info: check if the tmux session exists and what
            // it shows.
            let sessions = Command::new("tmux")
                .args(["list-sessions", "-F", "#{session_name}"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|_| "(tmux not available)".to_string());
            panic!(
                "Timed out waiting for {} after {:?}.\ntmux sessions on default server:\n{}",
                path.display(),
                timeout,
                sessions
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Kill any tmux sessions whose name contains `aoe_{substring}` on the default
/// tmux server. The `aoe_` prefix ensures we only match AoE-created sessions
/// (naming format: `aoe_{sanitized_title}_{8char_id}`), avoiding false positives
/// against real user sessions.
fn cleanup_tmux_sessions_containing(substring: &str) {
    let prefixed = format!("aoe_{substring}");
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();
    if let Ok(out) = output {
        let names = String::from_utf8_lossy(&out.stdout);
        for name in names.lines() {
            if name.contains(&prefixed) {
                let _ = Command::new("tmux")
                    .args(["kill-session", "-t", name])
                    .output();
            }
        }
    }
}

/// Create a TPM session, start it, wait for the stub to capture the prompt,
/// and return the captured prompt content. Also cleans up the tmux session.
///
/// `tier_args`: the args for tier selection, e.g. `&["fast"]` or `&[]` for default.
/// `title`: the session title.
/// `session_substring`: a unique substring to identify the tmux session for cleanup.
fn create_and_capture_prompt(
    h: &TuiTestHarness,
    plugin_dir: &TempDir,
    tier_args: &[&str],
    title: &str,
    session_substring: &str,
) -> String {
    let project = h.project_path();

    // Build the add command args
    let mut add_args = vec!["add", project.to_str().unwrap(), "--tpm"];
    add_args.extend_from_slice(tier_args);
    add_args.extend_from_slice(&["-t", title]);

    let output = h.run_cli_with_env(&add_args, &[("TPM_WORKFLOW_PATH", plugin_dir.path())]);
    assert!(
        output.status.success(),
        "aoe add --tpm {} failed.\nstdout: {}\nstderr: {}",
        tier_args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Temporarily override the default tmux server's HOME so the login shell
    // inside the session sources our profile files (which set PATH to include
    // the stub). Drop guard restores the original.
    let _env_guard = TmuxEnvGuard::set(h.home_path());

    // Start the session (creates a tmux pane on the default server, which
    // runs the claude stub via PATH from our shell profiles).
    let start_output = h.run_cli_with_env(
        &["session", "start", title],
        &[("TPM_WORKFLOW_PATH", plugin_dir.path())],
    );
    assert!(
        start_output.status.success(),
        "aoe session start {:?} failed.\nstdout: {}\nstderr: {}",
        title,
        String::from_utf8_lossy(&start_output.stdout),
        String::from_utf8_lossy(&start_output.stderr)
    );

    // Wait for the stub to write the captured prompt file
    let captured = wait_for_captured_prompt(h.home_path(), Duration::from_secs(15));

    // Clean up the tmux session created by aoe (on the default tmux server).
    // Use a targeted substring to avoid killing unrelated sessions.
    cleanup_tmux_sessions_containing(session_substring);

    // Remove the captured file so the next test tier starts fresh
    let _ = std::fs::remove_file(h.home_path().join(".captured-prompt.txt"));

    captured
}

// ---------------------------------------------------------------------------
// AC-01, AC-02, AC-03: --tpm fast
// ---------------------------------------------------------------------------

/// Creates a TPM session with `--tpm fast`, starts it, and verifies:
/// - The captured prompt contains "SYSTEM PROMPT OVERRIDE" (preamble).
/// - The captured prompt contains "Fake Orchestrator" (orchestrator content).
/// - `.tpm/config.json` has `"tier": "fast"`.
#[test]
#[serial]
fn tpm_prompt_injection_fast_tier() {
    require_tmux!();

    let (h, plugin_dir) = setup_capturing_harness("tpm_inject_fast");

    let captured =
        create_and_capture_prompt(&h, &plugin_dir, &["fast"], "Inject Fast", "Inject_Fast");

    // AC-01: preamble present
    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "Captured prompt should contain preamble 'SYSTEM PROMPT OVERRIDE'.\nCaptured:\n{}",
        captured
    );

    // AC-02: orchestrator content injected
    assert!(
        captured.contains("Fake Orchestrator"),
        "Captured prompt should contain orchestrator content 'Fake Orchestrator'.\nCaptured:\n{}",
        captured
    );

    // AC-03: config.json has correct tier
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("fast"),
        "config.json tier should be 'fast', got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-04: --tpm (default = standard)
// ---------------------------------------------------------------------------

/// Creates a TPM session with `--tpm` (default tier), starts it, and verifies:
/// - The captured prompt contains preamble.
/// - `.tpm/config.json` has `"tier": "standard"`.
#[test]
#[serial]
fn tpm_prompt_injection_standard_tier() {
    require_tmux!();

    let (h, plugin_dir) = setup_capturing_harness("tpm_inject_std");

    let captured = create_and_capture_prompt(&h, &plugin_dir, &[], "Inject Std", "Inject_Std");

    // AC-04: preamble present
    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "Captured prompt should contain preamble 'SYSTEM PROMPT OVERRIDE'.\nCaptured:\n{}",
        captured
    );

    // AC-04: orchestrator content injected
    assert!(
        captured.contains("Fake Orchestrator"),
        "Captured prompt should contain orchestrator content 'Fake Orchestrator'.\nCaptured:\n{}",
        captured
    );

    // AC-04: config.json has correct tier
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("standard"),
        "config.json tier should be 'standard', got: {}",
        config
    );
}

// ---------------------------------------------------------------------------
// AC-05: --tpm prod
// ---------------------------------------------------------------------------

/// Creates a TPM session with `--tpm prod`, starts it, and verifies:
/// - The captured prompt contains preamble.
/// - `.tpm/config.json` has `"tier": "prod"`.
#[test]
#[serial]
fn tpm_prompt_injection_prod_tier() {
    require_tmux!();

    let (h, plugin_dir) = setup_capturing_harness("tpm_inject_prod");

    let captured =
        create_and_capture_prompt(&h, &plugin_dir, &["prod"], "Inject Prod", "Inject_Prod");

    // AC-05: preamble present
    assert!(
        captured.contains("SYSTEM PROMPT OVERRIDE"),
        "Captured prompt should contain preamble 'SYSTEM PROMPT OVERRIDE'.\nCaptured:\n{}",
        captured
    );

    // AC-05: orchestrator content injected
    assert!(
        captured.contains("Fake Orchestrator"),
        "Captured prompt should contain orchestrator content 'Fake Orchestrator'.\nCaptured:\n{}",
        captured
    );

    // AC-05: config.json has correct tier
    let config = read_tpm_config(&h);
    assert_eq!(
        config["tier"].as_str(),
        Some("prod"),
        "config.json tier should be 'prod', got: {}",
        config
    );
}
