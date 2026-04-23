//! E2E tests for `aoe events` subcommand.
//!
//! Validates event emission, history querying with filters, and error handling
//! through the full CLI binary with an isolated `$HOME`.

use serial_test::serial;
use std::io::Write;

use crate::harness::TuiTestHarness;

/// Return the path to events.jsonl inside the harness's isolated profile dir.
fn events_jsonl_path(h: &TuiTestHarness) -> std::path::PathBuf {
    if cfg!(target_os = "linux") {
        h.home_path()
            .join(".config/agent-of-empires/profiles/default/events.jsonl")
    } else {
        h.home_path()
            .join(".agent-of-empires/profiles/default/events.jsonl")
    }
}

// ---------------------------------------------------------------------------
// AC-01: Emit an event and verify it appears in history
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_emit_session_completed_then_history_shows_it() {
    let h = TuiTestHarness::new("events_ac01");

    // Emit a session.completed event
    let emit = h.run_cli(&[
        "events",
        "emit",
        "session.completed",
        "--session-id",
        "abc",
        "--title",
        "my task",
    ]);
    assert!(
        emit.status.success(),
        "emit failed: {}",
        String::from_utf8_lossy(&emit.stderr)
    );

    // Query history
    let history = h.run_cli(&["events", "history"]);
    assert!(
        history.status.success(),
        "history failed: {}",
        String::from_utf8_lossy(&history.stderr)
    );

    let stdout = String::from_utf8_lossy(&history.stdout);
    assert!(
        stdout.contains(r#""type":"session.completed""#),
        "history should contain session.completed event type.\nOutput:\n{}",
        stdout
    );
    assert!(
        stdout.contains(r#""session_id":"abc""#),
        "history should contain session_id abc.\nOutput:\n{}",
        stdout
    );

    // Cross-check: read the events.jsonl file directly
    let jsonl_path = events_jsonl_path(&h);
    let content = std::fs::read_to_string(&jsonl_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", jsonl_path.display(), e));
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1, "should have exactly 1 event line in JSONL");
    assert!(lines[0].contains(r#""type":"session.completed""#));
    assert!(lines[0].contains(r#""session_id":"abc""#));
}

// ---------------------------------------------------------------------------
// AC-02: Emit 3 events with different session IDs, filter by --session
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_history_session_filter_returns_only_matching_session() {
    let h = TuiTestHarness::new("events_ac02");

    // Emit 3 events with different session IDs
    for (sid, title) in &[("abc", "task-a"), ("def", "task-b"), ("ghi", "task-c")] {
        let emit = h.run_cli(&[
            "events",
            "emit",
            "session.completed",
            "--session-id",
            sid,
            "--title",
            title,
        ]);
        assert!(
            emit.status.success(),
            "emit for {} failed: {}",
            sid,
            String::from_utf8_lossy(&emit.stderr)
        );
    }

    // Query with --session filter
    let history = h.run_cli(&["events", "history", "--session", "def"]);
    assert!(
        history.status.success(),
        "history --session failed: {}",
        String::from_utf8_lossy(&history.stderr)
    );

    let stdout = String::from_utf8_lossy(&history.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "should have exactly 1 JSON line for session def, got {}.\nOutput:\n{}",
        lines.len(),
        stdout
    );
    assert!(
        lines[0].contains(r#""session_id":"def""#),
        "the single line should have session_id def.\nLine:\n{}",
        lines[0]
    );
}

// ---------------------------------------------------------------------------
// AC-03: Emit a custom event with --name and --attr, filter by --filter custom
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_emit_custom_event_with_attrs_and_filter() {
    let h = TuiTestHarness::new("events_ac03");

    let emit = h.run_cli(&[
        "events",
        "emit",
        "custom",
        "--name",
        "deploy.started",
        "--attr",
        "env=prod",
    ]);
    assert!(
        emit.status.success(),
        "emit custom failed: {}",
        String::from_utf8_lossy(&emit.stderr)
    );

    // Query with --filter custom
    let history = h.run_cli(&["events", "history", "--filter", "custom"]);
    assert!(
        history.status.success(),
        "history --filter custom failed: {}",
        String::from_utf8_lossy(&history.stderr)
    );

    let stdout = String::from_utf8_lossy(&history.stdout);
    assert!(
        stdout.contains(r#""name":"deploy.started""#),
        "history should contain custom event name.\nOutput:\n{}",
        stdout
    );
    assert!(
        stdout.contains(r#""env":"prod""#),
        "history should contain attr env=prod.\nOutput:\n{}",
        stdout
    );
}

// ---------------------------------------------------------------------------
// AC-04: Emit session.started without --session-id fails
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_emit_session_started_without_session_id_fails() {
    let h = TuiTestHarness::new("events_ac04");

    let emit = h.run_cli(&["events", "emit", "session.started"]);
    assert!(
        !emit.status.success(),
        "emit session.started without --session-id should fail"
    );

    let stderr = String::from_utf8_lossy(&emit.stderr);
    assert!(
        stderr.contains("--session-id"),
        "stderr should mention --session-id.\nstderr:\n{}",
        stderr
    );
}

// ---------------------------------------------------------------------------
// AC-05: --since filter excludes old events
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_history_since_filter_excludes_old_events() {
    let h = TuiTestHarness::new("events_ac05");

    // Write an old event directly to the JSONL file (timestamped 3 hours ago)
    let jsonl_path = events_jsonl_path(&h);
    if let Some(parent) = jsonl_path.parent() {
        std::fs::create_dir_all(parent).expect("create profile dir");
    }
    let old_ts = chrono::Utc::now() - chrono::Duration::hours(3);
    let old_event = serde_json::json!({
        "type": "session.completed",
        "ts": old_ts.to_rfc3339(),
        "session_id": "old-session",
        "title": "old task",
        "group": null,
        "worktree": null,
        "summary_path": null,
        "exit_code": 0
    });
    {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&jsonl_path)
            .expect("open events.jsonl for old event");
        writeln!(file, "{}", serde_json::to_string(&old_event).unwrap()).expect("write old event");
    }

    // Emit a fresh event via CLI
    let emit = h.run_cli(&[
        "events",
        "emit",
        "session.completed",
        "--session-id",
        "fresh-session",
        "--title",
        "fresh task",
    ]);
    assert!(
        emit.status.success(),
        "emit failed: {}",
        String::from_utf8_lossy(&emit.stderr)
    );

    // Verify both events exist without filter
    let all_history = h.run_cli(&["events", "history"]);
    let all_stdout = String::from_utf8_lossy(&all_history.stdout);
    let all_lines: Vec<&str> = all_stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(
        all_lines.len(),
        2,
        "without filter, should have 2 events.\nOutput:\n{}",
        all_stdout
    );

    // Query with --since 1h: only the fresh event should appear
    let history = h.run_cli(&["events", "history", "--since", "1h"]);
    assert!(
        history.status.success(),
        "history --since failed: {}",
        String::from_utf8_lossy(&history.stderr)
    );

    let stdout = String::from_utf8_lossy(&history.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "with --since 1h, should have only the fresh event.\nOutput:\n{}",
        stdout
    );
    assert!(
        stdout.contains(r#""session_id":"fresh-session""#),
        "the remaining event should be the fresh one.\nOutput:\n{}",
        stdout
    );
    assert!(
        !stdout.contains(r#""session_id":"old-session""#),
        "the old event should be filtered out.\nOutput:\n{}",
        stdout
    );
}
