//! E2E tests for the send_keys rewrite (Feature 10).
//!
//! Validates that `TmuxSession::send_keys` using `tmux load-buffer` + `paste-buffer`
//! correctly delivers text to a running tmux session. Each test creates a tmux
//! session running `cat` (echoes stdin to pane), sends text via `Session::send_keys`,
//! then captures the pane content to verify delivery.

use serial_test::serial;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::harness::require_tmux;

/// Create a detached tmux session running `cat` (echoes stdin to the pane).
fn create_cat_session(name: &str) {
    let output = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            name,
            "-x",
            "120",
            "-y",
            "40",
            "cat",
        ])
        .output()
        .expect("tmux new-session");
    assert!(
        output.status.success(),
        "failed to create cat session '{}': {}",
        name,
        String::from_utf8_lossy(&output.stderr)
    );
    // Wait for cat to start and be ready for input.
    thread::sleep(Duration::from_millis(300));
}

/// Kill a tmux session, ignoring errors if it already exited.
fn kill_session(name: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", name])
        .output();
}

/// Capture pane content as plain text (no ANSI escapes).
fn capture_pane(name: &str) -> String {
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-t", name])
        .output()
        .expect("tmux capture-pane");
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Assert that no temp file for the current process remains after send_keys.
/// The implementation creates `/tmp/aoe-send-<PID>.txt` and removes it after use.
fn assert_temp_file_cleaned_up() {
    let expected = std::env::temp_dir().join(format!("aoe-send-{}.txt", std::process::id()));
    assert!(
        !expected.exists(),
        "temp file {} should not exist after send_keys returns",
        expected.display()
    );
}

/// AC-01: User sends "hello world" to a tmux session running `cat`.
/// `tmux capture-pane` output must contain "hello world".
#[test]
#[serial]
fn test_send_keys_delivers_simple_text_to_cat_session() {
    require_tmux!();

    let session_name = format!("aoe_test_send_{}", std::process::id());
    create_cat_session(&session_name);

    let session = agent_of_empires::tmux::Session::from_name(&session_name);
    session
        .send_keys("hello world")
        .expect("send_keys should succeed");

    // Allow time for cat to echo the text back.
    thread::sleep(Duration::from_millis(500));

    let content = capture_pane(&session_name);
    assert!(
        content.contains("hello world"),
        "pane should contain 'hello world' after send_keys.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );

    // AC-04: verify temp file cleanup
    assert_temp_file_cleaned_up();

    kill_session(&session_name);
}

/// AC-02: User sends multi-line text "line1\nline2\nline3". Captured pane
/// contains all three lines (bracket-paste mode delivers newlines as literal text).
#[test]
#[serial]
fn test_send_keys_delivers_multiline_text_preserving_newlines() {
    require_tmux!();

    let session_name = format!("aoe_test_send_{}", std::process::id());
    create_cat_session(&session_name);

    let session = agent_of_empires::tmux::Session::from_name(&session_name);
    session
        .send_keys("line1\nline2\nline3")
        .expect("send_keys should succeed");

    thread::sleep(Duration::from_millis(500));

    let content = capture_pane(&session_name);
    assert!(
        content.contains("line1"),
        "pane should contain 'line1'.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );
    assert!(
        content.contains("line2"),
        "pane should contain 'line2'.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );
    assert!(
        content.contains("line3"),
        "pane should contain 'line3'.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );

    // AC-04: verify temp file cleanup
    assert_temp_file_cleaned_up();

    kill_session(&session_name);
}

/// AC-03: User sends text with shell-special characters:
/// `it's a "test" with $VAR and \backslash`. Captured pane contains the exact
/// string with no shell expansion.
#[test]
#[serial]
fn test_send_keys_preserves_shell_special_characters() {
    require_tmux!();

    let session_name = format!("aoe_test_send_{}", std::process::id());
    create_cat_session(&session_name);

    let special_text = r#"it's a "test" with $VAR and \backslash"#;

    let session = agent_of_empires::tmux::Session::from_name(&session_name);
    session
        .send_keys(special_text)
        .expect("send_keys should succeed");

    thread::sleep(Duration::from_millis(500));

    let content = capture_pane(&session_name);

    // Verify each category of special character is preserved:
    // Single quote
    assert!(
        content.contains("it's"),
        "single quote should be preserved in pane output.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );
    // Double quotes
    assert!(
        content.contains(r#""test""#),
        "double quotes should be preserved in pane output.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );
    // Dollar sign (no shell expansion)
    assert!(
        content.contains("$VAR"),
        "$VAR should appear literally, not expanded.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );
    // Backslash
    assert!(
        content.contains("\\backslash"),
        "backslash should be preserved in pane output.\n\
         --- captured pane ---\n{}\n--- end ---",
        content
    );

    // AC-04: verify temp file cleanup
    assert_temp_file_cleaned_up();

    kill_session(&session_name);
}

/// AC-04 (dedicated): After each send_keys call, no files matching
/// `/tmp/aoe-send-*.txt` remain on disk. Sends multiple messages to
/// stress-test cleanup across repeated calls.
#[test]
#[serial]
fn test_send_keys_cleans_up_temp_files_after_each_call() {
    require_tmux!();

    let session_name = format!("aoe_test_send_{}", std::process::id());
    create_cat_session(&session_name);

    let session = agent_of_empires::tmux::Session::from_name(&session_name);

    // Send multiple messages and verify cleanup after each one.
    for (i, msg) in ["first", "second", "third"].iter().enumerate() {
        session
            .send_keys(msg)
            .unwrap_or_else(|e| panic!("send_keys call {} failed: {}", i + 1, e));
        thread::sleep(Duration::from_millis(300));
        assert_temp_file_cleaned_up();
    }

    // Also scan /tmp for any stray aoe-send files from this process.
    let tmp_dir = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
        let stray: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("aoe-send-") && name.ends_with(".txt")
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            stray.is_empty(),
            "no aoe-send-*.txt files should remain in {}: {:?}",
            tmp_dir.display(),
            stray
        );
    }

    kill_session(&session_name);
}
