//! `aoe events` subcommand for emitting and consuming events on the event bus.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{bail, Result};
use chrono::Utc;
use clap::{Args, Subcommand};

use crate::events::{read_history, tail_events, write_event, Event};

#[derive(Subcommand)]
pub enum EventsCommands {
    /// Watch new events as they're emitted (live tail)
    Watch(WatchArgs),

    /// Read past events from the log
    History(HistoryArgs),

    /// Emit an event to the bus
    Emit(EmitArgs),
}

#[derive(Args)]
pub struct WatchArgs {
    /// Comma-separated list of event types to include (e.g. "session.completed,session.failed")
    #[arg(long, value_delimiter = ',')]
    pub filter: Vec<String>,

    /// Filter to events with this group
    #[arg(long)]
    pub group: Option<String>,
}

#[derive(Args)]
pub struct HistoryArgs {
    /// Show events since this duration ago (e.g. "1h", "30m", "2d")
    #[arg(long)]
    pub since: Option<String>,

    /// Comma-separated list of event types to include
    #[arg(long, value_delimiter = ',')]
    pub filter: Vec<String>,

    /// Filter to events with this group
    #[arg(long)]
    pub group: Option<String>,
}

#[derive(Args)]
pub struct EmitArgs {
    /// Event type (e.g. "session.completed", "custom")
    #[arg(value_name = "TYPE")]
    pub event_type: String,

    /// Session ID (for session.* events)
    #[arg(long)]
    pub session_id: Option<String>,

    /// Session title (for session.* events)
    #[arg(long)]
    pub title: Option<String>,

    /// Group name (for session.* events)
    #[arg(long)]
    pub group: Option<String>,

    /// Worktree path (for session.* events that have one)
    #[arg(long)]
    pub worktree: Option<String>,

    /// Path to SUMMARY.md (for session.completed)
    #[arg(long)]
    pub summary_path: Option<String>,

    /// Tool name (for session.started)
    #[arg(long)]
    pub tool: Option<String>,

    /// Exit code (for session.completed)
    #[arg(long)]
    pub exit_code: Option<i32>,

    /// Error message (for session.failed)
    #[arg(long)]
    pub error: Option<String>,

    /// Reason (for session.idle)
    #[arg(long)]
    pub reason: Option<String>,

    /// For custom events, the event name
    #[arg(long)]
    pub name: Option<String>,

    /// For custom events, additional key=value attributes (repeatable)
    #[arg(long = "attr", value_parser = parse_attr)]
    pub attrs: Vec<(String, serde_json::Value)>,
}

fn parse_attr(s: &str) -> Result<(String, serde_json::Value), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("attr must be key=value, got: {}", s))?;
    Ok((k.to_string(), serde_json::Value::String(v.to_string())))
}

pub async fn run(profile: &str, command: EventsCommands) -> Result<()> {
    match command {
        EventsCommands::Watch(args) => run_watch(profile, args),
        EventsCommands::History(args) => run_history(profile, args),
        EventsCommands::Emit(args) => run_emit(profile, args),
    }
}

fn run_watch(profile: &str, args: WatchArgs) -> Result<()> {
    let filter_set: std::collections::HashSet<String> = args.filter.into_iter().collect();
    let group_filter = args.group;

    tail_events(profile, Duration::from_millis(250), move |event| {
        if !filter_set.is_empty() && !filter_set.contains(event.type_name()) {
            return true; // continue tailing
        }
        if let Some(ref g) = group_filter {
            if event.group() != Some(g.as_str()) {
                return true;
            }
        }
        match serde_json::to_string(event) {
            Ok(line) => println!("{}", line),
            Err(e) => eprintln!("serialization error: {}", e),
        }
        // flush stdout so consumers see output immediately
        use std::io::Write;
        let _ = std::io::stdout().flush();
        true // keep going
    })
}

fn run_history(profile: &str, args: HistoryArgs) -> Result<()> {
    let since = args.since.as_deref().map(parse_since).transpose()?;

    let events = read_history(profile, since)?;
    let filter_set: std::collections::HashSet<String> = args.filter.into_iter().collect();

    for event in events {
        if !filter_set.is_empty() && !filter_set.contains(event.type_name()) {
            continue;
        }
        if let Some(ref g) = args.group {
            if event.group() != Some(g.as_str()) {
                continue;
            }
        }
        let line = serde_json::to_string(&event)?;
        println!("{}", line);
    }
    Ok(())
}

fn parse_since(s: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    // Parse simple suffixes: 30s, 5m, 1h, 2d
    let s = s.trim();
    if s.is_empty() {
        bail!("--since value cannot be empty");
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid number in --since '{}'", s))?;
    let duration = match unit {
        "s" => chrono::Duration::seconds(num),
        "m" => chrono::Duration::minutes(num),
        "h" => chrono::Duration::hours(num),
        "d" => chrono::Duration::days(num),
        _ => bail!("unknown duration unit '{}', use s/m/h/d", unit),
    };
    Ok(Utc::now() - duration)
}

fn run_emit(profile: &str, args: EmitArgs) -> Result<()> {
    let now = Utc::now();
    let event = match args.event_type.as_str() {
        "session.started" => Event::SessionStarted {
            ts: now,
            session_id: required(&args.session_id, "--session-id")?,
            title: required(&args.title, "--title")?,
            group: args.group,
            worktree: args.worktree,
            tool: args.tool.unwrap_or_else(|| "unknown".to_string()),
        },
        "session.completed" => Event::SessionCompleted {
            ts: now,
            session_id: required(&args.session_id, "--session-id")?,
            title: required(&args.title, "--title")?,
            group: args.group,
            worktree: args.worktree,
            summary_path: args.summary_path,
            exit_code: args.exit_code,
        },
        "session.failed" => Event::SessionFailed {
            ts: now,
            session_id: required(&args.session_id, "--session-id")?,
            title: required(&args.title, "--title")?,
            group: args.group,
            error: required(&args.error, "--error")?,
        },
        "session.idle" => Event::SessionIdle {
            ts: now,
            session_id: required(&args.session_id, "--session-id")?,
            title: required(&args.title, "--title")?,
            group: args.group,
            reason: required(&args.reason, "--reason")?,
        },
        "session.waiting" => Event::SessionWaiting {
            ts: now,
            session_id: required(&args.session_id, "--session-id")?,
            title: required(&args.title, "--title")?,
            group: args.group,
        },
        "custom" => {
            let name = required(&args.name, "--name")?;
            let attrs: BTreeMap<String, serde_json::Value> = args.attrs.into_iter().collect();
            Event::Custom {
                ts: now,
                name,
                attrs,
            }
        }
        other => bail!(
            "unsupported event type '{}'. Supported: session.started, session.completed, session.failed, session.idle, session.waiting, custom",
            other
        ),
    };

    write_event(profile, &event)?;
    Ok(())
}

fn required<T: Clone>(opt: &Option<T>, name: &str) -> Result<T> {
    opt.clone()
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {}", name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_since() {
        assert!(parse_since("1h").is_ok());
        assert!(parse_since("30m").is_ok());
        assert!(parse_since("60s").is_ok());
        assert!(parse_since("2d").is_ok());
        assert!(parse_since("").is_err());
        assert!(parse_since("invalid").is_err());
        assert!(parse_since("5x").is_err());
    }

    #[test]
    fn test_parse_attr() {
        let (k, v) = parse_attr("foo=bar").unwrap();
        assert_eq!(k, "foo");
        assert_eq!(v, serde_json::Value::String("bar".to_string()));

        // No equals
        assert!(parse_attr("nokey").is_err());
    }
}
