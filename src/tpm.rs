//! TPM (Technical Project Manager) workflow integration helpers.
//!
//! The TPM workflow is implemented as an orchestrator skill living in the
//! companion plugin (`Loulen/tpm-workflow`). When a user opts into TPM mode
//! when creating a session, AoE needs to:
//!
//! 1. Locate the orchestrator system prompt on disk (the plugin can be
//!    installed in several places, bundled in this fork's `contrib/`,
//!    installed via `/plugin marketplace`, or pointed at by `TPM_WORKFLOW_PATH`).
//! 2. Inject it into the spawned `claude` command so the session boots up as
//!    the orchestrator from the first message.
//!
//! This module owns the path-resolution policy and the shell snippet that
//! gets appended to a session's `extra_args`. Keeping it separate from
//! `cli/add.rs` and `session/builder.rs` means both the CLI flag (`--tpm`)
//! and the TUI checkbox can share the same logic and tests.
//!
//! It also owns the native plugin installer invoked from the add-session
//! dialog when the user opts into TPM mode without having the plugin on disk.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::session::Instance;

/// Tier of TPM orchestration. Controls how the orchestrator dispatches work.
/// Currently threaded through the call chain without behavioral effect; future
/// waves will use it to select different orchestration strategies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TpmTier {
    Fast,
    #[default]
    Standard,
    Prod,
}

/// Known TPM agent slugs. The orchestrator dispatches these sub-sessions;
/// the implementer is always enabled and cannot be disabled.
pub const KNOWN_AGENTS: &[&str] = &[
    "planner",
    "implementer",
    "adversarial-reviewer",
    "blind-hunter",
    "edge-case-hunter",
    "acceptance-auditor",
    "merge-resolver",
    "qa-validator",
    "principal-engineer",
    "end-user-simulator",
];

/// TPM configuration that controls orchestrator behavior. Persisted to
/// `.tpm/config.json` at session creation so the orchestrator can read it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TpmConfig {
    #[serde(default)]
    pub tier: TpmTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_review_cycles: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_agents: Vec<String>,
}

/// Write `TpmConfig` as JSON to `.tpm/config.json` in the given project root.
/// Creates the `.tpm/` directory if it does not exist.
pub fn write_tpm_config(project_root: &Path, config: &TpmConfig) -> Result<()> {
    let tpm_dir = project_root.join(".tpm");
    std::fs::create_dir_all(&tpm_dir)
        .with_context(|| format!("failed to create {}", tpm_dir.display()))?;
    let config_path = tpm_dir.join("config.json");
    let serialized = serde_json::to_string_pretty(config)?;
    std::fs::write(&config_path, serialized)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    tracing::info!("wrote TPM config to {}", config_path.display());
    Ok(())
}

impl std::fmt::Display for TpmTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fast => write!(f, "fast"),
            Self::Standard => write!(f, "standard"),
            Self::Prod => write!(f, "prod"),
        }
    }
}

impl FromStr for TpmTier {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fast" => Ok(Self::Fast),
            "standard" => Ok(Self::Standard),
            "prod" => Ok(Self::Prod),
            other => Err(anyhow!(
                "unknown TPM tier: `{}`; expected fast, standard, or prod",
                other
            )),
        }
    }
}

/// Relative path of the orchestrator system prompt within the plugin.
/// Used by every entry in [`candidate_paths`].
const ORCHESTRATOR_RELATIVE: &str = "agents/orchestrator.md";

/// Environment variable that, when set, takes priority over every other
/// resolution strategy. Useful for development checkouts that live outside
/// `~/.claude/plugins`.
const ENV_OVERRIDE: &str = "TPM_WORKFLOW_PATH";

/// Canonical marketplace name as it appears in Claude Code's registry files
/// and on the filesystem (both the clone target and the cache subdir).
pub const MARKETPLACE_NAME: &str = "tpm-workflow";

/// Upstream clone URL. Used by `git clone` in [`install`].
pub const MARKETPLACE_REPO_URL: &str = "https://github.com/Loulen/tpm-workflow.git";

/// Short slug recorded in `known_marketplaces.json` so Claude Code can
/// reconstruct the remote if it later wants to refresh.
pub const MARKETPLACE_REPO_SLUG: &str = "Loulen/tpm-workflow";

/// Schema version we expect for `installed_plugins.json`. We refuse to write
/// on mismatch rather than silently upgrading (see D-03 in the plan).
pub const INSTALLED_PLUGINS_SCHEMA_VERSION: u64 = 2;

/// Relative path (from `$HOME`) of the user-scoped Claude Code settings file.
/// Owns the `enabledPlugins` map that gates plugin activation; separate from
/// `installed_plugins.json`, which only tracks presence.
const SETTINGS_REL_PATH: &str = ".claude/settings.json";

/// Plugin instance key used both in `installed_plugins.json` and in
/// `enabledPlugins`. Shape is `<plugin-name>@<marketplace-name>`.
const PLUGIN_INSTANCE_KEY: &str = "tpm-workflow@tpm-workflow";

/// Build the ordered list of candidate orchestrator paths. Earlier entries win.
///
/// The resolution chain:
///   1. `$TPM_WORKFLOW_PATH/agents/orchestrator.md` (env override)
///   2. `<repo_root>/contrib/tpm-workflow/agents/orchestrator.md`
///      (when running from a tpm-aoe checkout, the plugin is a git submodule)
///   3. `~/.claude/plugins/cache/tpm-workflow/tpm-workflow/<version>/agents/orchestrator.md`
///      (Claude Code marketplace install layout, newest version first)
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
        out.extend(cache_candidates(&home));
    }

    out
}

/// Walk the versioned plugin cache layout and return one orchestrator path
/// per version subdir, sorted so higher versions come first.
///
/// Real layout: `<home>/.claude/plugins/cache/tpm-workflow/tpm-workflow/<version>/agents/orchestrator.md`.
/// Descending lexicographic sort puts `"0.2.0"` before `"0.1.0"`; the
/// `"unknown"` sentinel (written by Claude's registry when no semver is
/// available) sorts above digits, which is an accepted quirk.
///
/// Missing directory or unreadable entries return an empty `Vec` silently.
fn cache_candidates(home: &Path) -> Vec<PathBuf> {
    let base = home
        .join(".claude")
        .join("plugins")
        .join("cache")
        .join(MARKETPLACE_NAME)
        .join(MARKETPLACE_NAME);
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut subdirs: Vec<std::ffi::OsString> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name())
        .collect();
    subdirs.sort();
    subdirs.reverse();
    subdirs
        .into_iter()
        .map(|name| base.join(name).join(ORCHESTRATOR_RELATIVE))
        .collect()
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

/// Preamble prepended to the orchestrator system prompt so Claude treats it
/// as authoritative over its default behavior.
///
/// Without this, `--append-system-prompt` loses to Claude Code's own "be a
/// helpful coding assistant" defaults — claude reads the orchestrator spec
/// but still starts implementing when the user sends a task. The preamble
/// exists to explicitly tell Claude that on first user turn it must
/// execute the spec's On Activation steps, not treat the message as a
/// normal coding request.
///
/// Kept ASCII + apostrophe-free so the whole string nests cleanly inside
/// the `bash -lc '... "..." ...'` wrapping that tmux uses to launch the
/// session — no extra shell escaping required on the call site.
const TPM_PREAMBLE: &str = "SYSTEM PROMPT OVERRIDE — TPM ORCHESTRATOR MODE.

The instructions that follow override any conflicting guidance in your default system prompt. When the user sends their first message in this session, execute the On Activation steps in the spec below. Do not treat that message as a direct request to write code yourself. You are the orchestrator: your job is to dispatch other sessions via the aoe CLI and coordinate them. You never edit project source code from this session — all code changes happen in sub-sessions you spawn.

---

";

/// Cheap "is the plugin registered in Claude Code's installed_plugins.json"
/// check. Any I/O or parse error is treated as "not installed" so callers
/// can assume `false` means "surface the install popup".
pub fn is_installed() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let path = home
        .join(".claude")
        .join("plugins")
        .join("installed_plugins.json");
    is_installed_at(&path)
}

fn is_installed_at(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return false;
    };
    matches!(
        v.pointer("/plugins/tpm-workflow@tpm-workflow"),
        Some(serde_json::Value::Array(arr)) if !arr.is_empty()
    )
}

/// Native install: clone (or refresh) the marketplace repo into
/// `~/.claude/plugins/marketplaces/tpm-workflow/`, copy its contents (minus
/// `.git`) into the versioned cache dir, and splice the two registry JSON
/// files Claude Code reads on startup.
///
/// Blocks on `git clone` (1-3s). Callers should surface any error to the UI.
pub fn install() -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
    let plugins_dir = home.join(".claude").join("plugins");
    let marketplace_dir = plugins_dir.join("marketplaces").join(MARKETPLACE_NAME);

    clone_or_refresh_marketplace(&marketplace_dir)?;
    let (plugin_name, version) = read_marketplace_metadata(&marketplace_dir)?;

    let cache_dest = plugins_dir
        .join("cache")
        .join(MARKETPLACE_NAME)
        .join(&plugin_name)
        .join(&version);
    copy_into_cache(&marketplace_dir, &cache_dest)?;

    let known_path = plugins_dir.join("known_marketplaces.json");
    update_known_marketplaces(&known_path, &marketplace_dir)?;

    let sha = git_head_sha(&marketplace_dir);
    let installed_path = plugins_dir.join("installed_plugins.json");
    update_installed_plugins(&installed_path, &cache_dest, &version, sha)?;

    let settings_path = home.join(SETTINGS_REL_PATH);
    update_user_settings(&settings_path, PLUGIN_INSTANCE_KEY)?;

    Ok(())
}

/// Flip the `enabledPlugins[<key>]` entry to `true` in the user-scoped
/// settings file, preserving all other fields.
///
/// Claude Code tracks plugin presence in `installed_plugins.json` but tracks
/// activation in `settings.json`, so a fresh install shows up as "installed
/// but disabled" unless we also write here. Accepting the install popup is
/// meant as "install AND enable", so we always overwrite to `true`, even if
/// the user had previously disabled the plugin explicitly.
fn update_user_settings(path: &Path, plugin_key: &str) -> Result<()> {
    let mut v = load_json_or(path, || serde_json::json!({}))?;
    if !v.is_object() {
        return Err(anyhow!(
            "{} root is not a JSON object; refusing to overwrite",
            path.display()
        ));
    }
    let root = v.as_object_mut().expect("object");
    let enabled = root
        .entry("enabledPlugins".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !enabled.is_object() {
        return Err(anyhow!(
            "{}: enabledPlugins is not a JSON object; refusing to overwrite",
            path.display()
        ));
    }
    let enabled_obj = enabled.as_object_mut().expect("object");
    enabled_obj.insert(plugin_key.to_string(), serde_json::Value::Bool(true));
    atomic_write_json(path, &v)
}

fn clone_or_refresh_marketplace(target: &Path) -> Result<()> {
    if target.exists() {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(target)
            .args(["pull", "--ff-only"])
            .status();
        match status {
            Ok(s) if !s.success() => {
                tracing::warn!(
                    "git pull --ff-only in {} exited with {}; continuing with cached copy",
                    target.display(),
                    s
                );
            }
            Err(e) => {
                tracing::warn!(
                    "failed to execute git pull in {}: {}; continuing with cached copy",
                    target.display(),
                    e
                );
            }
            _ => {}
        }
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let status = std::process::Command::new("git")
        .arg("clone")
        .arg(MARKETPLACE_REPO_URL)
        .arg(target)
        .status()
        .context("Failed to execute git clone")?;
    if !status.success() {
        return Err(anyhow!(
            "git clone {} {} exited with status {}",
            MARKETPLACE_REPO_URL,
            target.display(),
            status
        ));
    }
    Ok(())
}

fn read_marketplace_metadata(target: &Path) -> Result<(String, String)> {
    let path = target.join(".claude-plugin").join("marketplace.json");
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    let name = v
        .pointer("/plugins/0/name")
        .and_then(|x| x.as_str())
        .ok_or_else(|| {
            anyhow!(
                "Missing or non-string /plugins/0/name in {}",
                path.display()
            )
        })?
        .to_string();
    let version = v
        .pointer("/plugins/0/version")
        .and_then(|x| x.as_str())
        .ok_or_else(|| {
            anyhow!(
                "Missing or non-string /plugins/0/version in {}",
                path.display()
            )
        })?
        .to_string();
    Ok((name, version))
}

fn copy_into_cache(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)
        .with_context(|| format!("Failed to create {}", dest.display()))?;
    for entry in
        std::fs::read_dir(source).with_context(|| format!("Failed to read {}", source.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if name == std::ffi::OsStr::new(".git") {
            continue;
        }
        let src_path = entry.path();
        let dest_path = dest.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_into_cache(&src_path, &dest_path)?;
        } else if ft.is_file() {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src_path, &dest_path).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    src_path.display(),
                    dest_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn update_known_marketplaces(path: &Path, install_location: &Path) -> Result<()> {
    let mut v = load_json_or(path, || serde_json::json!({}))?;
    if !v.is_object() {
        v = serde_json::json!({});
    }
    let entry = serde_json::json!({
        "source": {"source": "github", "repo": MARKETPLACE_REPO_SLUG},
        "installLocation": install_location.to_string_lossy(),
        "lastUpdated": now_rfc3339(),
    });
    v.as_object_mut()
        .expect("ensured object above")
        .insert(MARKETPLACE_NAME.to_string(), entry);
    atomic_write_json(path, &v)
}

fn update_installed_plugins(
    path: &Path,
    install_path: &Path,
    version: &str,
    sha: Option<String>,
) -> Result<()> {
    let mut v = load_json_or(path, || {
        serde_json::json!({
            "version": INSTALLED_PLUGINS_SCHEMA_VERSION,
            "plugins": {},
        })
    })?;

    let actual = v.get("version").and_then(|x| x.as_u64()).unwrap_or(0);
    if actual != INSTALLED_PLUGINS_SCHEMA_VERSION {
        return Err(anyhow!(
            "installed_plugins.json schema version is {}, expected 2. Refusing to write. Run /plugin install tpm-workflow from a Claude session instead.",
            actual
        ));
    }

    if !v.is_object() {
        return Err(anyhow!(
            "installed_plugins.json root is not a JSON object: {}",
            path.display()
        ));
    }
    let root = v.as_object_mut().expect("object");
    let plugins = root
        .entry("plugins".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !plugins.is_object() {
        *plugins = serde_json::json!({});
    }
    let plugins = plugins.as_object_mut().expect("object");

    let now = now_rfc3339();
    let mut entry = serde_json::Map::new();
    entry.insert(
        "scope".to_string(),
        serde_json::Value::String("user".to_string()),
    );
    entry.insert(
        "installPath".to_string(),
        serde_json::Value::String(install_path.to_string_lossy().to_string()),
    );
    entry.insert(
        "version".to_string(),
        serde_json::Value::String(version.to_string()),
    );
    entry.insert(
        "installedAt".to_string(),
        serde_json::Value::String(now.clone()),
    );
    entry.insert("lastUpdated".to_string(), serde_json::Value::String(now));
    if let Some(s) = sha {
        entry.insert("gitCommitSha".to_string(), serde_json::Value::String(s));
    }

    plugins.insert(
        format!("{}@{}", MARKETPLACE_NAME, MARKETPLACE_NAME),
        serde_json::Value::Array(vec![serde_json::Value::Object(entry)]),
    );

    atomic_write_json(path, &v)
}

fn git_head_sha(target: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(target)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if sha.is_empty() {
        None
    } else {
        Some(sha)
    }
}

fn load_json_or(
    path: &Path,
    default: impl FnOnce() -> serde_json::Value,
) -> Result<serde_json::Value> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str::<serde_json::Value>(&s)
            .with_context(|| format!("Failed to parse {}", path.display())),
        Err(_) => Ok(default()),
    }
}

fn atomic_write_json(path: &Path, v: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("Failed to create tempfile in {}", parent.display()))?;
    let serialized = serde_json::to_string_pretty(v)?;
    std::fs::write(tmp.path(), serialized)
        .with_context(|| format!("Failed to write tempfile {}", tmp.path().display()))?;
    tmp.persist(path)
        .map_err(|e| anyhow!("atomic rename to {} failed: {}", path.display(), e))?;
    Ok(())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Build the shell snippet that should be appended to `extra_args` when
/// launching `claude` in TPM mode. We use `--append-system-prompt` (rather
/// than `--system-prompt`) so the orchestrator instructions augment Claude's
/// default system prompt instead of replacing it, then prepend
/// [`TPM_PREAMBLE`] to give those instructions precedence when they conflict
/// with the defaults.
///
/// The path is single-quoted to defend against spaces; the orchestrator
/// content itself is read at session-start time via `cat`, which keeps the
/// shell snippet short enough for tmux's command line.
pub fn extra_args_snippet(orchestrator_path: &Path) -> String {
    // The preamble lives inside the outer double-quoted argument alongside
    // `$(cat ...)`. Both halves get concatenated by the shell into one
    // --append-system-prompt value.
    format!(
        "--append-system-prompt \"{}$(cat {})\"",
        TPM_PREAMBLE,
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
    _tier: TpmTier,
) -> Result<String> {
    validate_tool(tool)?;
    let path = resolve_orchestrator(repo_root)
        .context("Failed to resolve TPM orchestrator system prompt")?;
    Ok(merge_extra_args(
        existing_extra_args,
        &extra_args_snippet(&path),
    ))
}

/// TPM artifact filenames eligible for archival.
const ARCHIVABLE_FILES: &[&str] = &["STATE.md", "SUMMARY.md", "PLAN.md", "config.json"];

/// Resolve the `.tpm/` directory for a session instance.
///
/// Checks worktree main_repo_path first (the primary repo root for worktree
/// sessions), then falls back to project_path (for non-worktree sessions or
/// when the main repo doesn't have `.tpm/`). For workspace sessions, checks
/// the first repo's worktree_path as a last resort.
///
/// Returns `None` if no `.tpm/` directory with a `STATE.md` exists.
pub fn resolve_tpm_dir(instance: &Instance) -> Option<PathBuf> {
    // Worktree session: check main_repo_path first
    if let Some(wt) = &instance.worktree_info {
        let candidate = PathBuf::from(&wt.main_repo_path).join(".tpm");
        if candidate.join("STATE.md").is_file() {
            return Some(candidate);
        }
    }

    // project_path (always present)
    let candidate = PathBuf::from(&instance.project_path).join(".tpm");
    if candidate.join("STATE.md").is_file() {
        return Some(candidate);
    }

    // Workspace session: check first repo's worktree_path
    if let Some(ws) = &instance.workspace_info {
        if let Some(repo) = ws.repos.first() {
            let candidate = PathBuf::from(&repo.worktree_path).join(".tpm");
            if candidate.join("STATE.md").is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Maximum length of the sanitized title component in archive dir names.
/// Keeps the full path well under the 255-byte filesystem limit even after
/// the timestamp and session ID prefix are prepended.
const MAX_TITLE_LEN: usize = 100;

/// Replace characters unsafe for filenames with hyphens and truncate to
/// [`MAX_TITLE_LEN`]. Returns an empty string when the input is empty or
/// contains only special characters; callers should fall back to the
/// session ID in that case.
fn sanitize_title(title: &str) -> String {
    let sanitized: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Trim leading/trailing hyphens so pure-special-char titles collapse to ""
    let trimmed = sanitized.trim_matches('-');
    if trimmed.len() > MAX_TITLE_LEN {
        trimmed[..MAX_TITLE_LEN].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Build the archive directory name: `<timestamp>_<id_prefix>_<title>`.
/// Falls back to using only the session ID when the sanitized title is empty.
fn archive_dir_name(now: &chrono::DateTime<chrono::Utc>, instance: &Instance) -> String {
    let timestamp = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let id_prefix = &instance.id[..instance.id.len().min(8)];
    let safe_title = sanitize_title(&instance.title);
    if safe_title.is_empty() {
        format!("{}_{}", timestamp, id_prefix)
    } else {
        format!("{}_{}_{}", timestamp, id_prefix, safe_title)
    }
}

/// Archive `.tpm/` artifacts to the AoE history directory before worktree
/// cleanup. Returns `Ok(true)` if archival happened, `Ok(false)` if no
/// `.tpm/STATE.md` was found on disk. Errors are propagated but callers
/// should treat them as non-fatal warnings.
pub fn archive_tpm_artifacts(instance: &Instance) -> Result<bool> {
    let tpm_dir = match resolve_tpm_dir(instance) {
        Some(dir) => dir,
        None => return Ok(false),
    };

    let now = chrono::Utc::now();
    let app_dir = crate::session::get_app_dir().context("failed to locate AoE config dir")?;
    let archive_dir = app_dir
        .join("history")
        .join(archive_dir_name(&now, instance));

    std::fs::create_dir_all(&archive_dir)
        .with_context(|| format!("failed to create {}", archive_dir.display()))?;

    for filename in ARCHIVABLE_FILES {
        let src = tpm_dir.join(filename);
        if src.is_file() {
            let dest = archive_dir.join(filename);
            std::fs::copy(&src, &dest).with_context(|| {
                format!("failed to copy {} to {}", src.display(), dest.display())
            })?;
        }
    }

    // Write metadata.json with session context
    let metadata = serde_json::json!({
        "session_id": instance.id,
        "title": instance.title,
        "archived_at": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "project_path": instance.project_path,
    });
    let metadata_path = archive_dir.join("metadata.json");
    let serialized = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&metadata_path, serialized)
        .with_context(|| format!("failed to write {}", metadata_path.display()))?;

    tracing::info!(
        "archived TPM artifacts for session {} to {}",
        instance.id,
        archive_dir.display()
    );

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn tpm_tier_default_is_standard() {
        assert_eq!(TpmTier::default(), TpmTier::Standard);
    }

    #[test]
    fn tpm_tier_display_lowercase() {
        assert_eq!(TpmTier::Fast.to_string(), "fast");
        assert_eq!(TpmTier::Standard.to_string(), "standard");
        assert_eq!(TpmTier::Prod.to_string(), "prod");
    }

    #[test]
    fn tpm_tier_from_str_lowercase() {
        assert_eq!("fast".parse::<TpmTier>().unwrap(), TpmTier::Fast);
        assert_eq!("standard".parse::<TpmTier>().unwrap(), TpmTier::Standard);
        assert_eq!("prod".parse::<TpmTier>().unwrap(), TpmTier::Prod);
    }

    #[test]
    fn tpm_tier_from_str_case_insensitive() {
        assert_eq!("FAST".parse::<TpmTier>().unwrap(), TpmTier::Fast);
        assert_eq!("Standard".parse::<TpmTier>().unwrap(), TpmTier::Standard);
        assert_eq!("PROD".parse::<TpmTier>().unwrap(), TpmTier::Prod);
        assert_eq!("pRoD".parse::<TpmTier>().unwrap(), TpmTier::Prod);
    }

    #[test]
    fn tpm_tier_from_str_unknown_returns_err() {
        let err = "unknown".parse::<TpmTier>().unwrap_err().to_string();
        assert!(err.contains("unknown TPM tier"));
    }

    #[test]
    fn tpm_tier_display_from_str_roundtrip() {
        for tier in [TpmTier::Fast, TpmTier::Standard, TpmTier::Prod] {
            let s = tier.to_string();
            let parsed: TpmTier = s.parse().unwrap();
            assert_eq!(parsed, tier);
        }
    }

    /// RAII guard that overrides `$HOME` for the lifetime of a test and
    /// restores it on drop. Combined with `#[serial]` this keeps our tests
    /// from leaking env state into their neighbours.
    struct HomeGuard {
        prev: Option<std::ffi::OsString>,
    }

    impl HomeGuard {
        fn set(path: &Path) -> Self {
            let prev = std::env::var_os("HOME");
            std::env::set_var("HOME", path);
            Self { prev }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(p) => std::env::set_var("HOME", p),
                None => std::env::remove_var("HOME"),
            }
        }
    }

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
        // Point HOME at an empty tempdir so the versioned cache walk returns
        // nothing, even on dev machines where the real cache is populated.
        let home_temp = TempDir::new().unwrap();
        let _home = HomeGuard::set(home_temp.path());

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
    fn extra_args_snippet_carries_preamble_before_cat() {
        // Regression guard: the preamble must live inside the quoted value and
        // come BEFORE the $(cat …) expansion so Claude reads the override
        // instructions first, then the orchestrator spec.
        let snippet = extra_args_snippet(Path::new("/tmp/orch.md"));
        let preamble_pos = snippet
            .find("SYSTEM PROMPT OVERRIDE")
            .expect("preamble should be present");
        let cat_pos = snippet
            .find("$(cat")
            .expect("cat expansion should be present");
        assert!(
            preamble_pos < cat_pos,
            "preamble should precede the cat expansion; got snippet: {}",
            snippet
        );
    }

    #[test]
    fn preamble_stays_shell_safe() {
        // The preamble lives inside double-quoted bash context already nested
        // inside single-quoted `bash -lc '...'`. Any of these characters would
        // break that nesting or trigger unintended shell expansion.
        assert!(
            !TPM_PREAMBLE.contains('"'),
            "double quote would close the outer arg"
        );
        assert!(
            !TPM_PREAMBLE.contains('\''),
            "apostrophe would break the outer bash -lc wrapper"
        );
        assert!(
            !TPM_PREAMBLE.contains('`'),
            "backtick would trigger command substitution"
        );
        assert!(
            !TPM_PREAMBLE.contains('$'),
            "dollar sign would trigger variable or $(..) expansion"
        );
        assert!(
            !TPM_PREAMBLE.contains('\\'),
            "backslash would start an escape sequence"
        );
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

    /// AC-02: versioned cache subdirs are returned newest-first.
    #[test]
    #[serial]
    fn candidate_paths_picks_highest_version_first() {
        let home = TempDir::new().unwrap();
        let base = home
            .path()
            .join(".claude")
            .join("plugins")
            .join("cache")
            .join("tpm-workflow")
            .join("tpm-workflow");
        for v in ["0.1.0", "0.2.0"] {
            let agents = base.join(v).join("agents");
            std::fs::create_dir_all(&agents).unwrap();
            std::fs::write(agents.join("orchestrator.md"), "# Orchestrator\n").unwrap();
        }

        let paths = cache_candidates(home.path());
        assert_eq!(paths.len(), 2, "expected 2 candidates, got {:?}", paths);
        let idx_020 = paths
            .iter()
            .position(|p| p.to_string_lossy().contains("0.2.0"))
            .expect("0.2.0 path present");
        let idx_010 = paths
            .iter()
            .position(|p| p.to_string_lossy().contains("0.1.0"))
            .expect("0.1.0 path present");
        assert!(
            idx_020 < idx_010,
            "0.2.0 must appear before 0.1.0; got {:?}",
            paths
        );
    }

    /// AC-02 companion: missing cache dir returns empty, no panic.
    #[test]
    #[serial]
    fn candidate_paths_empty_when_cache_absent() {
        let home = TempDir::new().unwrap();
        assert!(cache_candidates(home.path()).is_empty());
    }

    /// AC-03: is_installed distinguishes populated array vs empty vs missing.
    #[test]
    #[serial]
    fn is_installed_detects_present_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("installed_plugins.json");

        assert!(
            !is_installed_at(&path),
            "missing file must report not-installed"
        );

        std::fs::write(
            &path,
            r#"{"version":2,"plugins":{"tpm-workflow@tpm-workflow":[]}}"#,
        )
        .unwrap();
        assert!(
            !is_installed_at(&path),
            "empty array must report not-installed"
        );

        std::fs::write(
            &path,
            r#"{"version":2,"plugins":{"tpm-workflow@tpm-workflow":[{"scope":"user","version":"0.1.0"}]}}"#,
        )
        .unwrap();
        assert!(
            is_installed_at(&path),
            "non-empty array must report installed"
        );

        // Garbage JSON must not panic, must report false.
        std::fs::write(&path, r#"{not json"#).unwrap();
        assert!(!is_installed_at(&path));
    }

    fn write_stub_marketplace(home: &Path, version: &str) -> PathBuf {
        let marketplace = home
            .join(".claude")
            .join("plugins")
            .join("marketplaces")
            .join(MARKETPLACE_NAME);
        std::fs::create_dir_all(marketplace.join(".claude-plugin")).unwrap();
        std::fs::write(
            marketplace.join(".claude-plugin/marketplace.json"),
            format!(
                r#"{{"plugins":[{{"name":"tpm-workflow","version":"{}"}}]}}"#,
                version
            ),
        )
        .unwrap();
        std::fs::create_dir_all(marketplace.join("agents")).unwrap();
        std::fs::write(
            marketplace.join("agents").join("orchestrator.md"),
            "# Orchestrator\n",
        )
        .unwrap();
        marketplace
    }

    /// AC-04: full install against a pre-seeded local marketplace. No
    /// network access required — the clone step sees `target.exists()` and
    /// the subsequent `git pull` failure is tolerated.
    #[test]
    #[serial]
    fn install_end_to_end() {
        let home_temp = TempDir::new().unwrap();
        let _home = HomeGuard::set(home_temp.path());
        let home = home_temp.path();

        let _marketplace = write_stub_marketplace(home, "0.1.0");
        let plugins_dir = home.join(".claude").join("plugins");

        // Pre-existing sentinel in known_marketplaces.json.
        std::fs::write(
            plugins_dir.join("known_marketplaces.json"),
            r#"{"other-mkt":{"source":{"source":"local","path":"/tmp/x"}}}"#,
        )
        .unwrap();

        // Pre-existing sibling plugin in installed_plugins.json at schema v2.
        std::fs::write(
            plugins_dir.join("installed_plugins.json"),
            r#"{"version":2,"plugins":{"some-other@mkt":[{"scope":"user","version":"9.9.9"}]}}"#,
        )
        .unwrap();

        // Pre-existing settings.json with an unrelated root key and a sibling
        // enabledPlugins entry — both must survive the install.
        let settings_path = home.join(".claude").join("settings.json");
        std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        std::fs::write(
            &settings_path,
            r#"{"voiceEnabled":true,"enabledPlugins":{"something-else@mkt":false}}"#,
        )
        .unwrap();

        install().expect("install should succeed against stub marketplace");

        let cache_file = home
            .join(".claude/plugins/cache/tpm-workflow/tpm-workflow/0.1.0/agents/orchestrator.md");
        assert!(
            cache_file.exists(),
            "cache path missing: {}",
            cache_file.display()
        );

        let km: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(plugins_dir.join("known_marketplaces.json")).unwrap(),
        )
        .unwrap();
        assert!(
            km.get("tpm-workflow").is_some(),
            "known_marketplaces: tpm-workflow entry missing"
        );
        assert!(
            km.get("other-mkt").is_some(),
            "known_marketplaces: pre-existing other-mkt lost"
        );

        let ip: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(plugins_dir.join("installed_plugins.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            ip.pointer("/plugins/tpm-workflow@tpm-workflow/0/version")
                .and_then(|x| x.as_str()),
            Some("0.1.0")
        );
        assert!(
            ip.pointer("/plugins/some-other@mkt").is_some(),
            "installed_plugins: pre-existing some-other@mkt lost"
        );
        assert_eq!(
            ip.get("version").and_then(|x| x.as_u64()),
            Some(INSTALLED_PLUGINS_SCHEMA_VERSION)
        );

        // is_installed() should now also be true.
        assert!(is_installed());

        // settings.json: the enable flag is flipped to true, and pre-existing
        // keys at both the root and inside enabledPlugins survive the write.
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(
            settings.pointer("/enabledPlugins/tpm-workflow@tpm-workflow"),
            Some(&serde_json::Value::Bool(true)),
            "settings: enable flag not set for tpm-workflow@tpm-workflow: {}",
            settings
        );
        assert_eq!(
            settings.pointer("/voiceEnabled"),
            Some(&serde_json::Value::Bool(true)),
            "settings: root sentinel voiceEnabled lost: {}",
            settings
        );
        assert_eq!(
            settings.pointer("/enabledPlugins/something-else@mkt"),
            Some(&serde_json::Value::Bool(false)),
            "settings: pre-existing enabledPlugins entry lost: {}",
            settings
        );
    }

    /// AC-06: focused regression — with no pre-existing settings.json, install
    /// creates the file with exactly the expected shape.
    #[test]
    #[serial]
    fn install_sets_settings_enabled_plugins() {
        let home_temp = TempDir::new().unwrap();
        let _home = HomeGuard::set(home_temp.path());
        let home = home_temp.path();

        let _marketplace = write_stub_marketplace(home, "0.1.0");

        let settings_path = home.join(".claude").join("settings.json");
        assert!(
            !settings_path.exists(),
            "precondition: settings.json must not exist"
        );

        install().expect("install should succeed against stub marketplace");

        assert!(
            settings_path.exists(),
            "settings.json not created at {}",
            settings_path.display()
        );
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();

        let expected = serde_json::json!({
            "enabledPlugins": {
                "tpm-workflow@tpm-workflow": true,
            }
        });
        assert_eq!(
            settings, expected,
            "settings.json shape mismatch: got {}",
            settings
        );
    }

    /// AC-05: schema version 3 is rejected with a clear message; neither
    /// known_marketplaces.json nor the cache is corrupted before the bail
    /// (the version check runs inside update_installed_plugins which is the
    /// last write step, so the cache copy already happened — we only assert
    /// on the error message here, per the plan).
    #[test]
    #[serial]
    fn install_rejects_schema_version_3() {
        let home_temp = TempDir::new().unwrap();
        let _home = HomeGuard::set(home_temp.path());
        let home = home_temp.path();

        let _marketplace = write_stub_marketplace(home, "0.1.0");
        let plugins_dir = home.join(".claude").join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        std::fs::write(
            plugins_dir.join("installed_plugins.json"),
            r#"{"version":3,"plugins":{}}"#,
        )
        .unwrap();

        let err = install().unwrap_err().to_string();
        assert!(
            err.contains("schema version is 3"),
            "error missing substring: {}",
            err
        );
        assert!(
            err.contains("expected 2"),
            "error missing substring: {}",
            err
        );
    }

    // --- archive / resolve_tpm_dir tests ---

    fn create_test_instance(title: &str, project_path: &str) -> Instance {
        Instance::new(title, project_path)
    }

    /// RAII guard for `$XDG_CONFIG_HOME`. On Linux `dirs::config_dir()` checks
    /// this first, so we must clear it when redirecting via `HomeGuard` alone
    /// would be insufficient.
    struct XdgGuard {
        prev: Option<std::ffi::OsString>,
    }

    impl XdgGuard {
        fn unset() -> Self {
            let prev = std::env::var_os("XDG_CONFIG_HOME");
            std::env::remove_var("XDG_CONFIG_HOME");
            Self { prev }
        }
    }

    impl Drop for XdgGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(p) => std::env::set_var("XDG_CONFIG_HOME", p),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }

    #[test]
    fn sanitize_title_replaces_unsafe_chars() {
        assert_eq!(sanitize_title("my-task"), "my-task");
        assert_eq!(sanitize_title("feat/new thing"), "feat-new-thing");
        assert_eq!(sanitize_title("hello world: test!"), "hello-world--test");
        assert_eq!(
            sanitize_title("dots.and_underscores"),
            "dots.and_underscores"
        );
    }

    #[test]
    fn sanitize_title_empty_string() {
        assert_eq!(sanitize_title(""), "");
    }

    #[test]
    fn sanitize_title_all_special_chars() {
        assert_eq!(sanitize_title("///"), "");
        assert_eq!(sanitize_title("  "), "");
    }

    #[test]
    fn sanitize_title_truncates_long_input() {
        let long = "a".repeat(200);
        let result = sanitize_title(&long);
        assert_eq!(result.len(), MAX_TITLE_LEN);
    }

    #[test]
    fn archive_dir_name_falls_back_to_id_on_empty_title() {
        let instance = create_test_instance("", "/tmp/test");
        let now = chrono::Utc::now();
        let name = archive_dir_name(&now, &instance);
        // Format: <timestamp>_<id_prefix> (no trailing underscore)
        let id_prefix = &instance.id[..8];
        assert!(
            name.ends_with(id_prefix),
            "expected dir name to end with id prefix {}, got: {}",
            id_prefix,
            name
        );
        // No double underscore from missing title
        assert!(
            !name.contains("__"),
            "should not have double underscore: {}",
            name
        );
    }

    #[test]
    fn archive_dir_name_includes_id_and_title() {
        let instance = create_test_instance("my-task", "/tmp/test");
        let now = chrono::Utc::now();
        let name = archive_dir_name(&now, &instance);
        let id_prefix = &instance.id[..8];
        assert!(name.contains(id_prefix), "missing id prefix in: {}", name);
        assert!(
            name.ends_with("_my-task"),
            "should end with _my-task, got: {}",
            name
        );
    }

    #[test]
    fn resolve_tpm_dir_returns_none_when_no_tpm_dir() {
        let dir = TempDir::new().unwrap();
        let instance = create_test_instance("test", dir.path().to_str().unwrap());
        assert!(resolve_tpm_dir(&instance).is_none());
    }

    #[test]
    fn resolve_tpm_dir_finds_tpm_in_project_path() {
        let dir = TempDir::new().unwrap();
        let tpm = dir.path().join(".tpm");
        std::fs::create_dir_all(&tpm).unwrap();
        std::fs::write(tpm.join("STATE.md"), "# State\n").unwrap();

        let instance = create_test_instance("test", dir.path().to_str().unwrap());
        let resolved = resolve_tpm_dir(&instance);
        assert_eq!(resolved, Some(tpm));
    }

    #[test]
    fn resolve_tpm_dir_requires_state_md() {
        let dir = TempDir::new().unwrap();
        let tpm = dir.path().join(".tpm");
        std::fs::create_dir_all(&tpm).unwrap();
        // .tpm/ exists but no STATE.md inside
        std::fs::write(tpm.join("PLAN.md"), "# Plan\n").unwrap();

        let instance = create_test_instance("test", dir.path().to_str().unwrap());
        assert!(resolve_tpm_dir(&instance).is_none());
    }

    #[test]
    fn resolve_tpm_dir_prefers_main_repo_path() {
        let main_repo = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        // Put STATE.md in both locations
        let main_tpm = main_repo.path().join(".tpm");
        std::fs::create_dir_all(&main_tpm).unwrap();
        std::fs::write(main_tpm.join("STATE.md"), "# Main\n").unwrap();

        let wt_tpm = worktree.path().join(".tpm");
        std::fs::create_dir_all(&wt_tpm).unwrap();
        std::fs::write(wt_tpm.join("STATE.md"), "# WT\n").unwrap();

        let mut instance = create_test_instance("test", worktree.path().to_str().unwrap());
        instance.worktree_info = Some(crate::session::WorktreeInfo {
            branch: "feature/test".to_string(),
            main_repo_path: main_repo.path().to_string_lossy().to_string(),
            managed_by_aoe: true,
            created_at: chrono::Utc::now(),
        });

        let resolved = resolve_tpm_dir(&instance);
        assert_eq!(resolved, Some(main_tpm));
    }

    #[test]
    fn archive_tpm_artifacts_returns_false_when_no_tpm() {
        let dir = TempDir::new().unwrap();
        let instance = create_test_instance("test", dir.path().to_str().unwrap());
        assert_eq!(archive_tpm_artifacts(&instance).unwrap(), false);
    }

    // --- TpmConfig serde tests ---

    #[test]
    fn tpm_config_default_values() {
        let config = TpmConfig::default();
        assert_eq!(config.tier, TpmTier::Standard);
        assert!(config.max_review_cycles.is_none());
        assert!(config.disabled_agents.is_empty());
    }

    #[test]
    fn tpm_config_json_roundtrip_default() {
        let config = TpmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: TpmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tier, TpmTier::Standard);
        assert!(parsed.max_review_cycles.is_none());
        assert!(parsed.disabled_agents.is_empty());
    }

    #[test]
    fn tpm_config_json_roundtrip_with_overrides() {
        let config = TpmConfig {
            tier: TpmTier::Prod,
            max_review_cycles: Some(5),
            disabled_agents: vec![
                "principal-engineer".to_string(),
                "end-user-simulator".to_string(),
            ],
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: TpmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tier, TpmTier::Prod);
        assert_eq!(parsed.max_review_cycles, Some(5));
        assert_eq!(parsed.disabled_agents.len(), 2);
        assert_eq!(parsed.disabled_agents[0], "principal-engineer");
        assert_eq!(parsed.disabled_agents[1], "end-user-simulator");
    }

    #[test]
    fn tpm_config_json_roundtrip_with_disabled_agents_only() {
        let config = TpmConfig {
            tier: TpmTier::Fast,
            max_review_cycles: None,
            disabled_agents: vec!["blind-hunter".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();

        // max_review_cycles should be absent from JSON (skip_serializing_if)
        let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            raw.get("max_review_cycles").is_none(),
            "None max_review_cycles should be omitted from JSON"
        );

        let parsed: TpmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tier, TpmTier::Fast);
        assert!(parsed.max_review_cycles.is_none());
        assert_eq!(parsed.disabled_agents, vec!["blind-hunter"]);
    }

    #[test]
    fn tpm_config_json_skips_empty_disabled_agents() {
        let config = TpmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            raw.get("disabled_agents").is_none(),
            "empty disabled_agents should be omitted from JSON"
        );
    }

    #[test]
    fn tpm_tier_json_serde_roundtrip() {
        for tier in [TpmTier::Fast, TpmTier::Standard, TpmTier::Prod] {
            let json = serde_json::to_string(&tier).unwrap();
            let parsed: TpmTier = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, tier, "JSON roundtrip failed for {:?}", tier);
        }
    }

    #[test]
    fn tpm_tier_json_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&TpmTier::Fast).unwrap(), "\"fast\"");
        assert_eq!(
            serde_json::to_string(&TpmTier::Standard).unwrap(),
            "\"standard\""
        );
        assert_eq!(serde_json::to_string(&TpmTier::Prod).unwrap(), "\"prod\"");
    }

    #[test]
    fn write_tpm_config_creates_correct_json() {
        let dir = TempDir::new().unwrap();
        let config = TpmConfig {
            tier: TpmTier::Prod,
            max_review_cycles: Some(3),
            disabled_agents: vec!["end-user-simulator".to_string()],
        };

        write_tpm_config(dir.path(), &config).unwrap();

        let config_path = dir.path().join(".tpm/config.json");
        assert!(config_path.exists(), "config.json should exist");

        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: TpmConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.tier, TpmTier::Prod);
        assert_eq!(parsed.max_review_cycles, Some(3));
        assert_eq!(parsed.disabled_agents, vec!["end-user-simulator"]);
    }

    #[test]
    fn write_tpm_config_creates_tpm_directory() {
        let dir = TempDir::new().unwrap();
        let tpm_dir = dir.path().join(".tpm");
        assert!(!tpm_dir.exists(), "precondition: .tpm/ should not exist");

        write_tpm_config(dir.path(), &TpmConfig::default()).unwrap();
        assert!(tpm_dir.is_dir(), ".tpm/ directory should be created");
    }

    #[test]
    fn write_tpm_config_default_produces_minimal_json() {
        let dir = TempDir::new().unwrap();
        write_tpm_config(dir.path(), &TpmConfig::default()).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".tpm/config.json")).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Default config should have tier but omit optional/empty fields
        assert_eq!(raw["tier"].as_str(), Some("standard"));
        assert!(
            raw.get("max_review_cycles").is_none(),
            "default should omit max_review_cycles"
        );
        assert!(
            raw.get("disabled_agents").is_none(),
            "default should omit empty disabled_agents"
        );
    }

    #[test]
    #[serial]
    fn archive_tpm_artifacts_copies_files_and_writes_metadata() {
        let home_temp = TempDir::new().unwrap();
        let _home = HomeGuard::set(home_temp.path());
        let _xdg = XdgGuard::unset();

        let project = TempDir::new().unwrap();
        let tpm = project.path().join(".tpm");
        std::fs::create_dir_all(&tpm).unwrap();
        std::fs::write(tpm.join("STATE.md"), "# State content\n").unwrap();
        std::fs::write(tpm.join("SUMMARY.md"), "# Summary content\n").unwrap();
        // PLAN.md absent, should be skipped gracefully

        let instance = create_test_instance("my-task", project.path().to_str().unwrap());
        let result = archive_tpm_artifacts(&instance).unwrap();
        assert!(result, "archival should have happened");

        // Find the archive directory
        let app_dir = crate::session::get_app_dir().unwrap();
        let history_dir = app_dir.join("history");
        assert!(history_dir.exists(), "history dir should exist");

        let entries: Vec<_> = std::fs::read_dir(&history_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "expected exactly one archive entry");

        let archive_dir = entries[0].path();
        let dir_name = archive_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            dir_name.ends_with("_my-task"),
            "dir name should end with _my-task, got: {}",
            dir_name
        );
        // Verify session ID prefix is embedded
        let id_prefix = &instance.id[..8];
        assert!(
            dir_name.contains(id_prefix),
            "dir name should contain id prefix {}, got: {}",
            id_prefix,
            dir_name
        );

        // Check copied files
        assert!(archive_dir.join("STATE.md").is_file());
        assert!(archive_dir.join("SUMMARY.md").is_file());
        assert!(!archive_dir.join("PLAN.md").exists());

        // Check content preserved
        let state = std::fs::read_to_string(archive_dir.join("STATE.md")).unwrap();
        assert_eq!(state, "# State content\n");

        // Check metadata.json
        let meta_str = std::fs::read_to_string(archive_dir.join("metadata.json")).unwrap();
        let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap();
        assert_eq!(meta["session_id"].as_str().unwrap(), instance.id);
        assert_eq!(meta["title"].as_str().unwrap(), "my-task");
        assert_eq!(
            meta["project_path"].as_str().unwrap(),
            project.path().to_str().unwrap()
        );
        assert!(meta["archived_at"].as_str().is_some());
    }
}
