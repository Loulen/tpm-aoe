//! Event types emitted to the event bus.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A typed event emitted to the AoE event bus.
///
/// Events are serialized as JSON lines with a `type` discriminator. New event
/// variants can be added without breaking existing consumers as long as
/// consumers tolerate unknown types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    #[serde(rename = "session.started")]
    SessionStarted {
        ts: DateTime<Utc>,
        session_id: String,
        title: String,
        group: Option<String>,
        worktree: Option<String>,
        tool: String,
    },

    #[serde(rename = "session.completed")]
    SessionCompleted {
        ts: DateTime<Utc>,
        session_id: String,
        title: String,
        group: Option<String>,
        worktree: Option<String>,
        /// Path to SUMMARY.md in the worktree's `.tpm/` directory if present.
        summary_path: Option<String>,
        exit_code: Option<i32>,
    },

    #[serde(rename = "session.failed")]
    SessionFailed {
        ts: DateTime<Utc>,
        session_id: String,
        title: String,
        group: Option<String>,
        error: String,
    },

    #[serde(rename = "session.idle")]
    SessionIdle {
        ts: DateTime<Utc>,
        session_id: String,
        title: String,
        group: Option<String>,
        reason: String,
    },

    #[serde(rename = "session.waiting")]
    SessionWaiting {
        ts: DateTime<Utc>,
        session_id: String,
        title: String,
        group: Option<String>,
    },

    #[serde(rename = "worktree.created")]
    WorktreeCreated {
        ts: DateTime<Utc>,
        name: String,
        path: String,
        branch: Option<String>,
    },

    #[serde(rename = "worktree.removed")]
    WorktreeRemoved {
        ts: DateTime<Utc>,
        name: String,
        path: String,
    },

    /// Generic custom event for plugins or scripts that need to emit
    /// project-specific events without modifying AoE.
    #[serde(rename = "custom")]
    Custom {
        ts: DateTime<Utc>,
        name: String,
        #[serde(default)]
        attrs: BTreeMap<String, serde_json::Value>,
    },
}

impl Event {
    /// Return the event type discriminator as a string for filtering.
    pub fn type_name(&self) -> &'static str {
        match self {
            Event::SessionStarted { .. } => "session.started",
            Event::SessionCompleted { .. } => "session.completed",
            Event::SessionFailed { .. } => "session.failed",
            Event::SessionIdle { .. } => "session.idle",
            Event::SessionWaiting { .. } => "session.waiting",
            Event::WorktreeCreated { .. } => "worktree.created",
            Event::WorktreeRemoved { .. } => "worktree.removed",
            Event::Custom { .. } => "custom",
        }
    }

    /// Return the group associated with this event, if any. Used for filtering
    /// by `--group` in `aoe events watch`.
    pub fn group(&self) -> Option<&str> {
        match self {
            Event::SessionStarted { group, .. }
            | Event::SessionCompleted { group, .. }
            | Event::SessionFailed { group, .. }
            | Event::SessionIdle { group, .. }
            | Event::SessionWaiting { group, .. } => group.as_deref(),
            Event::WorktreeCreated { .. }
            | Event::WorktreeRemoved { .. }
            | Event::Custom { .. } => None,
        }
    }

    /// Return the session ID associated with this event, if any. Used for
    /// filtering by `--session` in `aoe events watch` — the TPM orchestrator
    /// uses this to pin one Monitor per dispatched session so it can react
    /// to each session's completion independently without parsing the JSON
    /// to demultiplex a group-wide stream.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Event::SessionStarted { session_id, .. }
            | Event::SessionCompleted { session_id, .. }
            | Event::SessionFailed { session_id, .. }
            | Event::SessionIdle { session_id, .. }
            | Event::SessionWaiting { session_id, .. } => Some(session_id.as_str()),
            Event::WorktreeCreated { .. }
            | Event::WorktreeRemoved { .. }
            | Event::Custom { .. } => None,
        }
    }

    /// Return the timestamp.
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Event::SessionStarted { ts, .. }
            | Event::SessionCompleted { ts, .. }
            | Event::SessionFailed { ts, .. }
            | Event::SessionIdle { ts, .. }
            | Event::SessionWaiting { ts, .. }
            | Event::WorktreeCreated { ts, .. }
            | Event::WorktreeRemoved { ts, .. }
            | Event::Custom { ts, .. } => *ts,
        }
    }
}

#[cfg(test)]
mod session_id_tests {
    use super::*;

    #[test]
    fn session_id_is_returned_for_session_events() {
        let ts = Utc::now();
        let ev = Event::SessionCompleted {
            ts,
            session_id: "abc123".into(),
            title: "t".into(),
            group: None,
            worktree: None,
            summary_path: None,
            exit_code: Some(0),
        };
        assert_eq!(ev.session_id(), Some("abc123"));
    }

    #[test]
    fn session_id_is_none_for_non_session_events() {
        use std::collections::BTreeMap;
        let ev = Event::Custom {
            ts: Utc::now(),
            name: "custom".into(),
            attrs: BTreeMap::new(),
        };
        assert!(ev.session_id().is_none());

        let ev = Event::WorktreeCreated {
            ts: Utc::now(),
            name: "wt".into(),
            path: "/tmp/wt".into(),
            branch: Some("b".into()),
        };
        assert!(ev.session_id().is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_started_serialization() {
        let event = Event::SessionStarted {
            ts: Utc::now(),
            session_id: "abc-123".to_string(),
            title: "test-session".to_string(),
            group: Some("task-auth".to_string()),
            worktree: Some("/tmp/wt".to_string()),
            tool: "claude".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"session.started""#));
        assert!(json.contains(r#""session_id":"abc-123""#));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::SessionStarted { session_id, .. } => assert_eq!(session_id, "abc-123"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_type_name() {
        let e = Event::SessionCompleted {
            ts: Utc::now(),
            session_id: "x".into(),
            title: "x".into(),
            group: None,
            worktree: None,
            summary_path: None,
            exit_code: Some(0),
        };
        assert_eq!(e.type_name(), "session.completed");
    }

    #[test]
    fn test_custom_event() {
        let mut attrs = BTreeMap::new();
        attrs.insert("foo".to_string(), serde_json::json!("bar"));
        let e = Event::Custom {
            ts: Utc::now(),
            name: "my.event".into(),
            attrs,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains(r#""type":"custom""#));
        assert!(json.contains(r#""name":"my.event""#));
    }

    #[test]
    fn test_group_filter() {
        let e = Event::SessionStarted {
            ts: Utc::now(),
            session_id: "x".into(),
            title: "x".into(),
            group: Some("task-auth".into()),
            worktree: None,
            tool: "claude".into(),
        };
        assert_eq!(e.group(), Some("task-auth"));
    }
}
