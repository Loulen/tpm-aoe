//! Event log file management.
//!
//! Events are appended as JSON lines to `<profile_dir>/events.jsonl`. The file
//! is the source of truth for both live tailing and historical queries.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use super::types::Event;
use crate::session::get_profile_dir;

/// Return the path to the events log file for the given profile.
pub fn events_log_path(profile: &str) -> Result<PathBuf> {
    Ok(get_profile_dir(profile)?.join("events.jsonl"))
}

/// Append an event to the profile's event log.
///
/// The write is line-buffered and uses an exclusive append, so concurrent
/// writers from multiple processes won't tear lines as long as each event is
/// less than the OS pipe buffer size (typically 4 KiB, plenty for our events).
pub fn write_event(profile: &str, event: &Event) -> Result<()> {
    let path = events_log_path(profile)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening events log {}", path.display()))?;

    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    file.write_all(line.as_bytes())?;
    file.flush()?;
    Ok(())
}

/// Read historical events from the log, optionally filtered by time window.
///
/// `since` filters to events at or after the given timestamp. `None` returns all events.
pub fn read_history(profile: &str, since: Option<DateTime<Utc>>) -> Result<Vec<Event>> {
    let path = events_log_path(profile)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(&path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("malformed event line, skipping: {}", e);
                continue;
            }
        };
        if let Some(cutoff) = since {
            if event.timestamp() < cutoff {
                continue;
            }
        }
        events.push(event);
    }

    Ok(events)
}

/// Tail the events log, calling the provided callback for each new event.
///
/// Polls the file at the given interval. Returns when the callback returns false
/// (signaling shutdown) or when an unrecoverable error occurs.
///
/// This is a simple polling tail rather than inotify because:
/// (1) cross-platform consistency (macOS lacks inotify)
/// (2) we don't need millisecond responsiveness for orchestration events
/// (3) it's much simpler to reason about
pub fn tail_events<F>(profile: &str, poll_interval: Duration, mut callback: F) -> Result<()>
where
    F: FnMut(&Event) -> bool,
{
    let path = events_log_path(profile)?;

    // Wait for the file to exist if it doesn't yet
    while !path.exists() {
        std::thread::sleep(poll_interval);
    }

    let mut file = std::fs::File::open(&path)?;
    // Start at end of file (don't replay history)
    file.seek(SeekFrom::End(0))?;
    let mut reader = BufReader::new(file);
    let mut buffer = String::new();

    loop {
        buffer.clear();
        let bytes_read = reader.read_line(&mut buffer)?;

        if bytes_read == 0 {
            // EOF, sleep and try again
            std::thread::sleep(poll_interval);
            continue;
        }

        if buffer.trim().is_empty() {
            continue;
        }

        let event: Event = match serde_json::from_str(buffer.trim()) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("malformed event line, skipping: {}", e);
                continue;
            }
        };

        if !callback(&event) {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use tempfile::TempDir;

    fn with_temp_profile<F>(f: F)
    where
        F: FnOnce(&str),
    {
        let tmp = TempDir::new().unwrap();
        let original = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());

        let profile = format!(
            "test-events-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        f(&profile);

        match original {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    #[serial_test::serial]
    fn test_write_and_read() {
        with_temp_profile(|profile| {
            let event = Event::SessionStarted {
                ts: Utc::now(),
                session_id: "abc".into(),
                title: "test".into(),
                group: None,
                worktree: None,
                tool: "claude".into(),
            };

            write_event(profile, &event).unwrap();

            let history = read_history(profile, None).unwrap();
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].type_name(), "session.started");
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_history_with_since_filter() {
        with_temp_profile(|profile| {
            let old_event = Event::SessionStarted {
                ts: Utc::now() - chrono::Duration::hours(2),
                session_id: "old".into(),
                title: "old".into(),
                group: None,
                worktree: None,
                tool: "claude".into(),
            };

            let new_event = Event::SessionStarted {
                ts: Utc::now(),
                session_id: "new".into(),
                title: "new".into(),
                group: None,
                worktree: None,
                tool: "claude".into(),
            };

            write_event(profile, &old_event).unwrap();
            write_event(profile, &new_event).unwrap();

            // Without filter: 2 events
            let all = read_history(profile, None).unwrap();
            assert_eq!(all.len(), 2);

            // With 1-hour cutoff: 1 event
            let cutoff = Utc::now() - chrono::Duration::hours(1);
            let recent = read_history(profile, Some(cutoff)).unwrap();
            assert_eq!(recent.len(), 1);
            match &recent[0] {
                Event::SessionStarted { session_id, .. } => assert_eq!(session_id, "new"),
                _ => panic!(),
            }
        });
    }

    #[test]
    #[serial_test::serial]
    fn test_tail_picks_up_new_event() {
        with_temp_profile(|profile| {
            // Create the file with one event so tail can attach
            let initial = Event::SessionStarted {
                ts: Utc::now(),
                session_id: "initial".into(),
                title: "initial".into(),
                group: None,
                worktree: None,
                tool: "claude".into(),
            };
            write_event(profile, &initial).unwrap();

            let received = Arc::new(Mutex::new(Vec::new()));
            let received_clone = received.clone();
            let profile_clone = profile.to_string();

            let handle = thread::spawn(move || {
                tail_events(&profile_clone, Duration::from_millis(50), |event| {
                    received_clone
                        .lock()
                        .unwrap()
                        .push(event.type_name().to_string());
                    received_clone.lock().unwrap().len() < 2 // stop after 2
                })
            });

            // Give the tailer a moment to attach
            thread::sleep(Duration::from_millis(200));

            // Write two events
            write_event(
                profile,
                &Event::SessionCompleted {
                    ts: Utc::now(),
                    session_id: "x".into(),
                    title: "x".into(),
                    group: None,
                    worktree: None,
                    summary_path: None,
                    exit_code: Some(0),
                },
            )
            .unwrap();

            write_event(
                profile,
                &Event::SessionFailed {
                    ts: Utc::now(),
                    session_id: "y".into(),
                    title: "y".into(),
                    group: None,
                    error: "boom".into(),
                },
            )
            .unwrap();

            handle.join().unwrap().unwrap();

            let received = received.lock().unwrap();
            assert_eq!(received.len(), 2);
            assert_eq!(received[0], "session.completed");
            assert_eq!(received[1], "session.failed");
        });
    }
}
