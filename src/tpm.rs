//! TPM (Technical Project Manager) workflow integration helpers.
//!
//! The TPM workflow is implemented as an orchestrator skill living in the
//! companion plugin (`Loulen/tpm-workflow`). When a user opts into TPM mode
//! when creating a session, AoE needs to:
//!
//! 1. Locate the orchestrator system prompt on disk (the plugin can be
//!    installed in several places — bundled in this fork's `contrib/`,
//!    installed via `/plugin marketplace`, or pointed at by `TPM_WORKFLOW_PATH`).
//! 2. Inject it into the spawned `claude` command so the session boots up as
//!    the orchestrator from the first message.
//!
//! This module owns the path-resolution policy and the shell snippet that
//! gets appended to a session's `extra_args`. Keeping it separate from
//! `cli/add.rs` and `session/builder.rs` means both the CLI flag (`--tpm`)
//! and the TUI checkbox can share the same logic and tests.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// Relative path of the orchestrator system prompt within the plugin.
/// Used by every entry in [`candidate_paths`].
const ORCHESTRATOR_RELATIVE: &str = "agents/orchestrator.md";

/// Environment variable that, when set, takes priority over every other
/// resolution strategy. Useful for development checkouts that live outside
/// `~/.claude/plugins`.
const ENV_OVERRIDE: &str = "TPM_WORKFLOW_PATH";

/// Build the ordered list of candidate orchestrator paths. Earlier entries win.
///
/// The resolution chain:
///   1. `$TPM_WORKFLOW_PATH/agents/orchestrator.md` (env override)
///   2. `<repo_root>/contrib/tpm-workflow/agents/orchestrator.md`
///      (when running from a tpm-aoe checkout — the plugin is a git submodule)
///   3. `~/.claude/plugins/cache/tpm-workflow/tpm-workflow/agents/orchestrator.md`
///      (Claude Code marketplace install layout)
///
/// `repo_root` is typically the session's working directory; we walk upwards
/// looking for a `contrib/tpm-workflow` so `aoe add ./subdir --tpm` works.
pub fn candidate_paths(repo_root: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Ok(env_path) = std::env::var(ENV_OVERRIDE) {
        if !env_path.trim().is_empty() {
            out.push(PathBuf::from(env_path).join(ORCHESTRATOR_RELATIVE));
        }
    }

    if let Some(root) = repo_root {
        for ancestor in root.ancestors().take(8) {
            out.push(
                ancestor
                    .join("contrib")
                    .join("tpm-workflow")
                    .join(ORCHESTRATOR_RELATIVE),
            );
        }
    }

    if let Some(home) = dirs::home_dir() {
        out.push(
            home.join(".claude")
                .join("plugins")
                .join("cache")
                .join("tpm-workflow")
                .join("tpm-workflow")
                .join(ORCHESTRATOR_RELATIVE),
        );
    }

    out
}

/// Resolve the orchestrator system prompt path, returning an error with
/// installation hints when nothing is found.
pub fn resolve_orchestrator(repo_root: Option<&Path>) -> Result<PathBuf> {
    let candidates = candidate_paths(repo_root);
    for path in &candidates {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    Err(anyhow!(
        "Could not locate the TPM orchestrator prompt. Tried:\n  {}\n\nInstall the plugin with:\n  /plugin marketplace add Loulen/tpm-workflow\n  /plugin install tpm-workflow\n\nOr set TPM_WORKFLOW_PATH to a local checkout that contains agents/{}.",
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  "),
        "orchestrator.md"
    ))
}

/// Build the shell snippet that should be appended to `extra_args` when
/// launching `claude` in TPM mode. We use `--append-system-prompt` (rather
/// than `--system-prompt`) so the orchestrator instructions augment Claude's
/// default system prompt instead of replacing it.
///
/// The path is single-quoted to defend against spaces; the orchestrator
/// content itself is read at session-start time via `cat`, which keeps the
/// shell snippet short enough for tmux's command line.
pub fn extra_args_snippet(orchestrator_path: &Path) -> String {
    format!(
        "--append-system-prompt \"$(cat {})\"",
        shell_single_quote(orchestrator_path.to_string_lossy().as_ref())
    )
}

/// Concatenate a TPM snippet with whatever the user already configured in
/// `extra_args`. The TPM snippet goes first so user-provided flags can
/// override anything we set.
pub fn merge_extra_args(existing: &str, tpm_snippet: &str) -> String {
    if existing.trim().is_empty() {
        tpm_snippet.to_string()
    } else {
        format!("{} {}", tpm_snippet, existing.trim())
    }
}

/// POSIX-shell single-quote a string. Embedded single quotes are escaped via
/// the standard `'\''` trick.
fn shell_single_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

/// Validate that the user's selected tool can host the TPM orchestrator.
/// The orchestrator skill targets Claude's `--append-system-prompt` flag, so
/// other agents would silently ignore it.
pub fn validate_tool(tool: &str) -> Result<()> {
    if tool == "claude" {
        Ok(())
    } else {
        Err(anyhow!(
            "TPM mode currently requires the `claude` tool (got `{}`). Switch the tool selector to claude or disable TPM mode.",
            tool
        ))
    }
}

/// One-shot helper used by both the CLI and the TUI: validate the tool,
/// resolve the orchestrator path, and produce an `extra_args` value with the
/// snippet merged in. Errors propagate so the caller can surface them.
pub fn build_tpm_extra_args(
    tool: &str,
    repo_root: Option<&Path>,
    existing_extra_args: &str,
) -> Result<String> {
    validate_tool(tool)?;
    let path = resolve_orchestrator(repo_root)
        .context("Failed to resolve TPM orchestrator system prompt")?;
    Ok(merge_extra_args(
        existing_extra_args,
        &extra_args_snippet(&path),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Write a fake orchestrator under a `contrib/tpm-workflow` subtree.
    /// Returns the file path so callers can assert on the resolved value.
    fn write_orchestrator(root: &Path) -> PathBuf {
        let p = root.join("contrib/tpm-workflow/agents");
        std::fs::create_dir_all(&p).unwrap();
        let file = p.join("orchestrator.md");
        std::fs::write(&file, "# Orchestrator\n").unwrap();
        file
    }

    /// Write a fake orchestrator at the layout `TPM_WORKFLOW_PATH` expects:
    /// the env var points at the *plugin* root, so the orchestrator lives at
    /// `<root>/agents/orchestrator.md`.
    fn write_orchestrator_at_plugin_root(root: &Path) -> PathBuf {
        let p = root.join("agents");
        std::fs::create_dir_all(&p).unwrap();
        let file = p.join("orchestrator.md");
        std::fs::write(&file, "# Orchestrator\n").unwrap();
        file
    }

    #[test]
    #[serial]
    fn env_override_wins_over_contrib() {
        let env_dir = TempDir::new().unwrap();
        let contrib_dir = TempDir::new().unwrap();
        let env_orch = write_orchestrator_at_plugin_root(env_dir.path());
        let _contrib_orch = write_orchestrator(contrib_dir.path());

        std::env::set_var(ENV_OVERRIDE, env_dir.path());
        let resolved = resolve_orchestrator(Some(contrib_dir.path())).unwrap();
        std::env::remove_var(ENV_OVERRIDE);

        assert_eq!(resolved, env_orch);
    }

    #[test]
    #[serial]
    fn contrib_dir_resolves_when_env_unset() {
        std::env::remove_var(ENV_OVERRIDE);
        let dir = TempDir::new().unwrap();
        let expected = write_orchestrator(dir.path());
        let resolved = resolve_orchestrator(Some(dir.path())).unwrap();
        assert_eq!(resolved, expected);
    }

    #[test]
    #[serial]
    fn contrib_dir_walks_up_ancestors() {
        std::env::remove_var(ENV_OVERRIDE);
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        let expected = write_orchestrator(dir.path());

        let resolved = resolve_orchestrator(Some(&nested)).unwrap();
        assert_eq!(resolved, expected);
    }

    #[test]
    #[serial]
    fn missing_plugin_returns_actionable_error() {
        std::env::remove_var(ENV_OVERRIDE);
        let dir = TempDir::new().unwrap();
        let err = resolve_orchestrator(Some(dir.path()))
            .unwrap_err()
            .to_string();
        assert!(err.contains("Could not locate the TPM orchestrator prompt"));
        assert!(err.contains("/plugin marketplace add"));
        assert!(err.contains("TPM_WORKFLOW_PATH"));
    }

    #[test]
    fn shell_quote_handles_embedded_quotes() {
        assert_eq!(shell_single_quote("plain"), "'plain'");
        assert_eq!(shell_single_quote("with space"), "'with space'");
        assert_eq!(shell_single_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn extra_args_snippet_uses_append_flag() {
        let snippet = extra_args_snippet(Path::new("/tmp/orch.md"));
        assert!(snippet.starts_with("--append-system-prompt"));
        assert!(snippet.contains("/tmp/orch.md"));
        assert!(snippet.contains("$(cat"));
    }

    #[test]
    fn merge_extra_args_prepends_snippet() {
        let merged = merge_extra_args("--model opus", "--append-system-prompt FOO");
        assert_eq!(merged, "--append-system-prompt FOO --model opus");
    }

    #[test]
    fn merge_extra_args_returns_snippet_when_empty() {
        let merged = merge_extra_args("   ", "--snippet");
        assert_eq!(merged, "--snippet");
    }

    #[test]
    fn validate_tool_accepts_claude_only() {
        assert!(validate_tool("claude").is_ok());
        let err = validate_tool("opencode").unwrap_err().to_string();
        assert!(err.contains("requires the `claude` tool"));
    }
}
