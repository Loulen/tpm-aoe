//! Migration v005: Backfill `tpm_managed` field on existing sessions
//!
//! Sessions created with TPM orchestrator mode have `extra_args` containing
//! `--append-system-prompt` and `TPM ORCHESTRATOR MODE`. This migration scans
//! all profile session files and sets `tpm_managed: true` on matching instances
//! so the TUI can display the TPM badge without parsing extra_args at render time.

use anyhow::Result;
use std::fs;
use tracing::{debug, info};

pub fn run() -> Result<()> {
    let app_dir = crate::session::get_app_dir()?;
    let profiles_dir = app_dir.join("profiles");

    if !profiles_dir.exists() {
        debug!("No profiles directory, skipping tpm_managed migration");
        return Ok(());
    }

    for entry in fs::read_dir(&profiles_dir)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let sessions_path = entry.path().join("sessions.json");
        migrate_sessions_file(&sessions_path)?;
    }

    Ok(())
}

fn migrate_sessions_file(path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(());
    }

    let mut sessions: Vec<serde_json::Value> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            debug!("Failed to parse {}: {}, skipping", path.display(), e);
            return Ok(());
        }
    };

    let mut changed = false;
    for session in &mut sessions {
        let obj = match session.as_object_mut() {
            Some(o) => o,
            None => continue,
        };

        // Skip if already set to true
        if obj
            .get("tpm_managed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let extra_args = obj.get("extra_args").and_then(|v| v.as_str()).unwrap_or("");

        if extra_args.contains("--append-system-prompt")
            && extra_args.contains("TPM ORCHESTRATOR MODE")
        {
            obj.insert("tpm_managed".to_string(), serde_json::Value::Bool(true));
            let title = obj
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            info!("Marked session '{}' as tpm_managed", title);
            changed = true;
        }
    }

    if changed {
        let new_content = serde_json::to_string_pretty(&sessions)?;
        fs::write(path, new_content)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_sets_tpm_managed() {
        let dir = tempfile::TempDir::new().unwrap();
        let sessions_path = dir.path().join("sessions.json");

        let content = serde_json::json!([
            {
                "id": "abc123",
                "title": "orchestrator",
                "project_path": "/tmp/test",
                "extra_args": "--append-system-prompt 'TPM ORCHESTRATOR MODE tier=fast'",
                "tool": "claude",
                "status": "idle",
                "created_at": "2025-01-01T00:00:00Z"
            },
            {
                "id": "def456",
                "title": "regular",
                "project_path": "/tmp/test2",
                "extra_args": "",
                "tool": "claude",
                "status": "idle",
                "created_at": "2025-01-01T00:00:00Z"
            }
        ]);
        fs::write(
            &sessions_path,
            serde_json::to_string_pretty(&content).unwrap(),
        )
        .unwrap();

        migrate_sessions_file(&sessions_path).unwrap();

        let result: Vec<serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(&sessions_path).unwrap()).unwrap();

        assert_eq!(
            result[0]["tpm_managed"].as_bool(),
            Some(true),
            "TPM session should be marked as tpm_managed"
        );
        assert!(
            result[1].get("tpm_managed").is_none()
                || result[1]["tpm_managed"].as_bool() == Some(false),
            "Regular session should not be marked as tpm_managed"
        );
    }

    #[test]
    fn test_migrate_idempotent() {
        let dir = tempfile::TempDir::new().unwrap();
        let sessions_path = dir.path().join("sessions.json");

        let content = serde_json::json!([{
            "id": "abc123",
            "title": "orchestrator",
            "project_path": "/tmp/test",
            "extra_args": "--append-system-prompt 'TPM ORCHESTRATOR MODE tier=fast'",
            "tpm_managed": true,
            "tool": "claude",
            "status": "idle",
            "created_at": "2025-01-01T00:00:00Z"
        }]);
        fs::write(
            &sessions_path,
            serde_json::to_string_pretty(&content).unwrap(),
        )
        .unwrap();

        migrate_sessions_file(&sessions_path).unwrap();

        let result: Vec<serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(&sessions_path).unwrap()).unwrap();

        assert_eq!(result[0]["tpm_managed"].as_bool(), Some(true));
    }

    #[test]
    fn test_migrate_nonexistent_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let sessions_path = dir.path().join("nonexistent.json");
        migrate_sessions_file(&sessions_path).unwrap();
    }

    #[test]
    fn test_migrate_empty_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let sessions_path = dir.path().join("sessions.json");
        fs::write(&sessions_path, "").unwrap();
        migrate_sessions_file(&sessions_path).unwrap();
    }

    #[test]
    fn test_migrate_no_matching_sessions() {
        let dir = tempfile::TempDir::new().unwrap();
        let sessions_path = dir.path().join("sessions.json");

        let content = serde_json::json!([{
            "id": "abc123",
            "title": "regular",
            "project_path": "/tmp/test",
            "extra_args": "--model opus",
            "tool": "claude",
            "status": "idle",
            "created_at": "2025-01-01T00:00:00Z"
        }]);
        fs::write(
            &sessions_path,
            serde_json::to_string_pretty(&content).unwrap(),
        )
        .unwrap();

        let before = fs::read_to_string(&sessions_path).unwrap();
        migrate_sessions_file(&sessions_path).unwrap();
        let after = fs::read_to_string(&sessions_path).unwrap();

        assert_eq!(
            before, after,
            "File should not be modified when no sessions match"
        );
    }
}
