//! Shared helper functions for TPM e2e tests.
//!
//! Extracted from multiple test files to eliminate duplication. These helpers
//! were previously copy-pasted across 8+ files per D-02 convention during
//! wave-1 to avoid merge conflicts; now consolidated in wave-2.

use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use crate::harness::TuiTestHarness;

/// Read the persisted `sessions.json` from the harness's isolated profile dir.
pub(crate) fn read_sessions(h: &TuiTestHarness) -> serde_json::Value {
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

/// Drop a fake `agents/orchestrator.md` under `root`. Returns the path to the
/// created file so callers can assert on the resolved value.
pub(crate) fn write_fake_orchestrator(root: &Path) -> std::path::PathBuf {
    let agents = root.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    let file = agents.join("orchestrator.md");
    std::fs::write(&file, "# Fake Orchestrator\n").expect("write orchestrator.md");
    file
}

/// Find the session entry with the given title in sessions.json.
pub(crate) fn find_session<'a>(
    sessions: &'a serde_json::Value,
    title: &str,
) -> &'a serde_json::Value {
    sessions
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["title"] == title))
        .unwrap_or_else(|| panic!("session with title {:?} not found in sessions.json", title))
}

/// Return the history dir path for the harness's isolated home.
pub(crate) fn history_dir(h: &TuiTestHarness) -> std::path::PathBuf {
    if cfg!(target_os = "linux") {
        h.home_path().join(".config/agent-of-empires/history")
    } else {
        h.home_path().join(".agent-of-empires/history")
    }
}

/// Read `.tpm/config.json` from the project directory inside the harness.
pub(crate) fn read_tpm_config(h: &TuiTestHarness) -> serde_json::Value {
    let path = h.project_path().join(".tpm/config.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("invalid .tpm/config.json")
}

/// Create a harness with a git-initialized project and a fake plugin dir.
/// Returns (harness, plugin_dir TempDir, orchestrator path).
pub(crate) fn setup_tpm_harness(name: &str) -> (TuiTestHarness, TempDir, std::path::PathBuf) {
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
    let orch_path = write_fake_orchestrator(plugin_dir.path());

    (h, plugin_dir, orch_path)
}

/// Return the path to events.jsonl inside the harness's isolated profile dir.
#[allow(dead_code)] // Used by events.rs once migrated
pub(crate) fn events_jsonl_path(h: &TuiTestHarness) -> std::path::PathBuf {
    if cfg!(target_os = "linux") {
        h.home_path()
            .join(".config/agent-of-empires/profiles/default/events.jsonl")
    } else {
        h.home_path()
            .join(".agent-of-empires/profiles/default/events.jsonl")
    }
}

/// Return the config dir under the harness's isolated home.
#[allow(dead_code)] // Used by tui_state_panel.rs and profile_template.rs once migrated
pub(crate) fn config_dir(h: &TuiTestHarness) -> std::path::PathBuf {
    if cfg!(target_os = "linux") {
        h.home_path().join(".config/agent-of-empires")
    } else {
        h.home_path().join(".agent-of-empires")
    }
}

/// List entries in a directory, returning an empty vec if the dir doesn't exist.
pub(crate) fn list_dir_entries(dir: &Path) -> Vec<std::fs::DirEntry> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Seed `installed_plugins.json` + orchestrator cache in the harness temp HOME
/// so `is_installed()` and `resolve_orchestrator()` succeed inside the TUI.
pub(crate) fn seed_tpm_plugin(h: &TuiTestHarness) {
    let home = h.home_path();

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

/// Seed `.tpm/` directory with STATE.md and SUMMARY.md for archival testing.
pub(crate) fn seed_tpm_artifacts(project: &Path, state_content: &str, summary_content: &str) {
    let tpm_dir = project.join(".tpm");
    std::fs::create_dir_all(&tpm_dir).expect("create .tpm dir");
    std::fs::write(tpm_dir.join("STATE.md"), state_content).expect("write STATE.md");
    std::fs::write(tpm_dir.join("SUMMARY.md"), summary_content).expect("write SUMMARY.md");
}
