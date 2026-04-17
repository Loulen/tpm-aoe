//! Background sweeper that detects session status transitions and emits events.
//!
//! This is the bridge between AoE's existing tmux-based status detection and
//! the typed event bus. Instead of modifying the hot-path `update_status`
//! method (which would require threading profile through the entire instance
//! chain), the sweeper runs separately, polls all sessions, and emits events
//! when statuses change.
//!
//! For TPM workflow integration, when a session ends and a `.tpm/SUMMARY.md`
//! exists in its worktree, the emitted `session.completed` event includes the
//! summary path so the orchestrator can pick it up immediately.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;

use super::types::Event;
use super::write_event;
use crate::session::{Instance, Status, Storage};

/// Tracked state for a session between polls.
#[derive(Debug, Clone)]
pub struct TrackedState {
    pub status: Status,
    /// Retained for future use: surface stale-instance metadata in logs/events
    /// when an instance disappears between sweeps.
    #[allow(dead_code)]
    pub title: String,
    #[allow(dead_code)]
    pub group_path: String,
}

/// Detect transitions between two statuses and produce an event if one occurred.
///
/// Returns None if no event should be emitted (e.g. status unchanged, or
/// transitioned to a non-emitting state).
fn detect_transition(prev: &Status, current: &Status, instance: &Instance) -> Option<Event> {
    if prev == current {
        return None;
    }

    let now = Utc::now();
    let session_id = instance.id.clone();
    let title = instance.title.clone();
    let group = if instance.group_path.is_empty() {
        None
    } else {
        Some(instance.group_path.clone())
    };
    let worktree = instance
        .workspace_info
        .as_ref()
        .and_then(|ws| ws.repos.first())
        .map(|r| r.worktree_path.clone())
        .or_else(|| {
            instance
                .worktree_info
                .as_ref()
                .map(|_| instance.project_path.clone())
        });

    match (prev, current) {
        // Active → Stopped means clean completion (the agent's Stop hook fired)
        (Status::Running | Status::Idle | Status::Waiting | Status::Unknown, Status::Stopped) => {
            let summary_path = worktree
                .as_ref()
                .map(PathBuf::from)
                .map(|p| p.join(".tpm").join("SUMMARY.md"))
                .filter(|p| p.exists())
                .map(|p| p.to_string_lossy().into_owned());

            Some(Event::SessionCompleted {
                ts: now,
                session_id,
                title,
                group,
                worktree,
                summary_path,
                exit_code: Some(0),
            })
        }

        // Active → Error means the session died unexpectedly
        (Status::Running | Status::Idle | Status::Waiting | Status::Unknown, Status::Error) => {
            Some(Event::SessionFailed {
                ts: now,
                session_id,
                title,
                group,
                error: instance
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "session entered Error state".to_string()),
            })
        }

        // Active → Waiting (the agent is asking for human input)
        (Status::Running | Status::Idle, Status::Waiting) => Some(Event::SessionWaiting {
            ts: now,
            session_id,
            title,
            group,
        }),

        // Running → Idle (the agent finished its current turn)
        (Status::Running, Status::Idle) => Some(Event::SessionIdle {
            ts: now,
            session_id,
            title,
            group,
            reason: "turn_completed".to_string(),
        }),

        // Other transitions (Starting → Running, Creating → anything, etc.)
        // are noise for orchestration purposes
        _ => None,
    }
}

/// Run the sweeper loop. Polls the profile's sessions periodically, emits
/// events on status transitions, and logs warnings on errors.
///
/// This blocks until cancelled. Spawn it as a tokio task or run it in a
/// dedicated thread.
pub async fn run_sweeper(profile: String, poll_interval: Duration) -> Result<()> {
    let mut tracked: HashMap<String, TrackedState> = HashMap::new();
    tracing::info!(
        "starting event sweeper for profile '{}' (poll interval {:?})",
        profile,
        poll_interval
    );

    loop {
        match sweep_once(&profile, &mut tracked) {
            Ok(emitted) => {
                if emitted > 0 {
                    tracing::debug!("sweeper emitted {} events", emitted);
                }
            }
            Err(e) => {
                tracing::warn!("sweeper iteration failed: {}", e);
            }
        }
        tokio::time::sleep(poll_interval).await;
    }
}

/// Single sweep iteration. Public for testing and one-shot use.
pub fn sweep_once(profile: &str, tracked: &mut HashMap<String, TrackedState>) -> Result<usize> {
    let storage = Storage::new(profile)?;
    let (mut instances, _groups) = storage.load_with_groups()?;

    crate::tmux::refresh_session_cache();

    let mut emitted = 0;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for inst in &mut instances {
        seen.insert(inst.id.clone());
        inst.update_status();

        let prev_state = tracked.get(&inst.id).cloned();

        if let Some(prev) = prev_state {
            if let Some(event) = detect_transition(&prev.status, &inst.status, inst) {
                if let Err(e) = write_event(profile, &event) {
                    tracing::warn!("failed to write event: {}", e);
                } else {
                    emitted += 1;
                }
            }
        }

        tracked.insert(
            inst.id.clone(),
            TrackedState {
                status: inst.status,
                title: inst.title.clone(),
                group_path: inst.group_path.clone(),
            },
        );
    }

    // Drop tracking for instances that no longer exist (deleted)
    tracked.retain(|id, _| seen.contains(id));

    Ok(emitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_instance(id: &str, status: Status) -> Instance {
        let mut inst = Instance::new("test-title", "/tmp");
        inst.id = id.to_string();
        inst.status = status;
        inst
    }

    #[test]
    fn test_active_to_stopped_emits_completed() {
        let inst = make_instance("abc", Status::Stopped);
        let event = detect_transition(&Status::Running, &Status::Stopped, &inst);
        assert!(event.is_some());
        match event.unwrap() {
            Event::SessionCompleted { session_id, .. } => assert_eq!(session_id, "abc"),
            other => panic!("expected SessionCompleted, got {:?}", other.type_name()),
        }
    }

    #[test]
    fn test_active_to_error_emits_failed() {
        let mut inst = make_instance("abc", Status::Error);
        inst.last_error = Some("boom".to_string());
        let event = detect_transition(&Status::Running, &Status::Error, &inst);
        match event.unwrap() {
            Event::SessionFailed { error, .. } => assert_eq!(error, "boom"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_running_to_waiting_emits_waiting() {
        let inst = make_instance("abc", Status::Waiting);
        let event = detect_transition(&Status::Running, &Status::Waiting, &inst);
        assert!(matches!(event.unwrap(), Event::SessionWaiting { .. }));
    }

    #[test]
    fn test_running_to_idle_emits_idle() {
        let inst = make_instance("abc", Status::Idle);
        let event = detect_transition(&Status::Running, &Status::Idle, &inst);
        match event.unwrap() {
            Event::SessionIdle { reason, .. } => assert_eq!(reason, "turn_completed"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_no_transition_returns_none() {
        let inst = make_instance("abc", Status::Running);
        assert!(detect_transition(&Status::Running, &Status::Running, &inst).is_none());
    }

    #[test]
    fn test_starting_to_running_returns_none() {
        // Starting → Running is noise, not an orchestration event
        let inst = make_instance("abc", Status::Running);
        assert!(detect_transition(&Status::Starting, &Status::Running, &inst).is_none());
    }

    #[test]
    fn test_creating_transitions_return_none() {
        let inst = make_instance("abc", Status::Idle);
        assert!(detect_transition(&Status::Creating, &Status::Idle, &inst).is_none());
    }

    #[test]
    fn test_completion_includes_summary_when_present() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let worktree = tmp.path();
        fs::create_dir_all(worktree.join(".tpm")).unwrap();
        fs::write(worktree.join(".tpm").join("SUMMARY.md"), "test summary").unwrap();

        let mut inst = make_instance("abc", Status::Stopped);
        inst.workspace_info = Some(crate::session::WorkspaceInfo {
            branch: "main".into(),
            workspace_dir: worktree.to_string_lossy().into_owned(),
            repos: vec![crate::session::WorkspaceRepo {
                name: "repo".into(),
                source_path: worktree.to_string_lossy().into_owned(),
                branch: "main".into(),
                worktree_path: worktree.to_string_lossy().into_owned(),
                main_repo_path: worktree.to_string_lossy().into_owned(),
                managed_by_aoe: true,
            }],
            created_at: Utc::now(),
            cleanup_on_delete: true,
        });

        let event = detect_transition(&Status::Running, &Status::Stopped, &inst);
        match event.unwrap() {
            Event::SessionCompleted { summary_path, .. } => {
                assert!(summary_path.is_some(), "summary_path should be set");
                assert!(summary_path.unwrap().ends_with("SUMMARY.md"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_completion_without_summary() {
        let inst = make_instance("abc", Status::Stopped);
        let event = detect_transition(&Status::Running, &Status::Stopped, &inst);
        match event.unwrap() {
            Event::SessionCompleted { summary_path, .. } => {
                assert!(
                    summary_path.is_none(),
                    "summary_path should be None when file doesn't exist"
                );
            }
            _ => panic!(),
        }
    }
}
