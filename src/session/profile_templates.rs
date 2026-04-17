//! Pre-configured profile templates for common workflows.
//!
//! A template is a set of profile-level overrides that gets written to
//! `<profile_dir>/config.toml` when the user runs `aoe profile create
//! --template <name>`. Templates are intentionally non-destructive: they only
//! set fields that differ from the global defaults, so the user can still
//! customize the resulting profile by editing the file.
//!
//! Currently only the `tpm` template is shipped; add new ones by extending
//! [`Template`] and matching on it in [`build`].
//!
//! Note: profile config schemas are versioned indirectly through the global
//! config — when adding fields with `serde(default)`, old template TOMLs keep
//! deserializing cleanly, so we don't need a per-template version field.

use anyhow::{bail, Result};
use std::str::FromStr;

use super::profile_config::{ProfileConfig, SessionConfigOverride, WorktreeConfigOverride};
use crate::sound::SoundConfigOverride;

/// Identifies a built-in profile template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// Defaults tuned for the TPM (Technical Project Manager) orchestration
    /// workflow: worktrees on by default, YOLO on (orchestrator runs unattended),
    /// sounds off (background sessions shouldn't beep).
    Tpm,
}

impl Template {
    /// All known template identifiers, used by clap to validate the flag value.
    pub const ALL: &'static [&'static str] = &["tpm"];

    pub fn as_str(self) -> &'static str {
        match self {
            Template::Tpm => "tpm",
        }
    }
}

impl FromStr for Template {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "tpm" => Ok(Template::Tpm),
            other => bail!(
                "Unknown profile template: '{}'. Known templates: {}",
                other,
                Template::ALL.join(", ")
            ),
        }
    }
}

/// Build the [`ProfileConfig`] for a template. The result is what would land in
/// `<profile_dir>/config.toml`; callers should serialize and write it.
pub fn build(template: Template) -> ProfileConfig {
    match template {
        Template::Tpm => tpm_profile(),
    }
}

fn tpm_profile() -> ProfileConfig {
    ProfileConfig {
        worktree: Some(WorktreeConfigOverride {
            enabled: Some(true),
            // Keep TPM worktrees in their own sibling directory so a single
            // repo can mix tpm-orchestrated and ad-hoc worktrees without
            // collisions. {repo-name} and {branch} are expanded by the
            // worktree builder; the orchestrator sets {branch} to a slug like
            // `tpm-{task}`.
            path_template: Some("../{repo-name}-tpm/{branch}".to_string()),
            auto_cleanup: Some(true),
            ..Default::default()
        }),
        session: Some(SessionConfigOverride {
            // Orchestrator-spawned sessions run unattended. The user is
            // already trusting the orchestrator; per-tool permission prompts
            // would block the workflow.
            yolo_mode_default: Some(true),
            ..Default::default()
        }),
        // Background TPM sessions completing every few minutes shouldn't
        // beep. Users can re-enable in settings.toml if they want audible
        // notifications.
        sound: Some(SoundConfigOverride {
            enabled: Some(false),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_from_str_accepts_known() {
        assert_eq!(Template::from_str("tpm").unwrap(), Template::Tpm);
        assert_eq!(Template::from_str("TPM").unwrap(), Template::Tpm);
    }

    #[test]
    fn template_from_str_rejects_unknown() {
        let err = Template::from_str("nope").unwrap_err().to_string();
        assert!(err.contains("Unknown profile template"));
        assert!(err.contains("tpm"));
    }

    #[test]
    fn tpm_template_enables_worktrees() {
        let cfg = build(Template::Tpm);
        let wt = cfg.worktree.expect("worktree override should be set");
        assert_eq!(wt.enabled, Some(true));
        assert_eq!(
            wt.path_template.as_deref(),
            Some("../{repo-name}-tpm/{branch}")
        );
    }

    #[test]
    fn tpm_template_enables_yolo_default() {
        let cfg = build(Template::Tpm);
        let session = cfg.session.expect("session override should be set");
        assert_eq!(session.yolo_mode_default, Some(true));
    }

    #[test]
    fn tpm_template_disables_sound() {
        let cfg = build(Template::Tpm);
        let sound = cfg.sound.expect("sound override should be set");
        assert_eq!(sound.enabled, Some(false));
    }

    #[test]
    fn tpm_template_serializes_to_non_empty_toml() {
        let cfg = build(Template::Tpm);
        let toml = toml::to_string_pretty(&cfg).unwrap();
        assert!(toml.contains("[worktree]"));
        assert!(toml.contains("[session]"));
        assert!(toml.contains("[sound]"));
        assert!(toml.contains("enabled = true"));
    }

    #[test]
    fn tpm_template_roundtrips_through_toml() {
        let cfg = build(Template::Tpm);
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let parsed: ProfileConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.worktree.unwrap().enabled, Some(true));
        assert_eq!(parsed.session.unwrap().yolo_mode_default, Some(true));
    }

    /// End-to-end smoke test: `create_profile_with_template` should write a
    /// non-empty TOML to the profile dir that round-trips back to the same
    /// overrides. Uses a tempdir + env override so it doesn't touch real user
    /// state. Marked `#[serial]` because it mutates `XDG_CONFIG_HOME`/`HOME`
    /// which other tests in the workspace also use.
    #[test]
    #[serial_test::serial]
    fn create_profile_with_template_writes_config() {
        use crate::session::{
            create_profile_with_template, get_profile_dir, profile_config::load_profile_config,
        };
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        std::env::set_var("HOME", temp.path());
        #[cfg(target_os = "linux")]
        std::env::set_var("XDG_CONFIG_HOME", temp.path().join(".config"));

        create_profile_with_template("smoke-tpm", Template::Tpm).unwrap();

        let dir = get_profile_dir("smoke-tpm").unwrap();
        let cfg_path = dir.join("config.toml");
        assert!(
            cfg_path.exists(),
            "expected config.toml to exist at {}",
            cfg_path.display()
        );
        let raw = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(
            !raw.trim().is_empty(),
            "template config should not be empty"
        );

        let loaded = load_profile_config("smoke-tpm").unwrap();
        assert_eq!(loaded.worktree.unwrap().enabled, Some(true));
        assert_eq!(loaded.session.unwrap().yolo_mode_default, Some(true));
        assert_eq!(loaded.sound.unwrap().enabled, Some(false));
    }
}
