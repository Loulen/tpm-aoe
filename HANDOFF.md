# Autonomous Run Handoff — tpm-mode branch

**Run started from:** `.tpm-bootstrap-prompt.md`
**Branch:** `tpm-mode`
**Commits added:** 5 (4 features + this handoff)
**Tests:** all passing (1083 lib + 37 e2e)

---

## What now works end-to-end

A user wanting to spin up the TPM (Technical Project Manager) workflow can do
the following without any manual `aoe events daemon` or hand-edited TOML:

1. **Install the plugin** in Claude Code:
   ```
   /plugin marketplace add Loulen/tpm-workflow
   /plugin install tpm-workflow
   ```
2. **Create a TPM-friendly profile** (worktrees on, YOLO on, sounds off):
   ```
   aoe profile create tpm --template tpm
   ```
3. **Open the TUI on the new profile**:
   ```
   aoe -p tpm
   ```
4. **Press the add-session keybinding**, fill in the title/path, and toggle
   "TPM Mode" on. The dialog only shows the toggle when the plugin is
   resolvable and the selected tool is `claude`.
5. The orchestrator boots inside the spawned session with
   `--append-system-prompt "$(cat <orchestrator.md>)"`. The lifecycle sweeper
   (auto-started alongside the TUI) emits `session.completed`/`failed`/etc.
   onto the events bus, which the orchestrator tails via `aoe events watch`.

CLI users can do the equivalent via `aoe add <path> --tpm`.

## Items completed (in commit order)

| # | Commit       | Item                                                   |
|---|--------------|--------------------------------------------------------|
| 1 | `a63428c`    | `feat(profile): add tpm template for orchestrated workflows` |
| 2 | `8787006`    | `feat(events): auto-run sweeper alongside TUI`         |
| 3 | `9dd6aba`    | `feat(tui): add TPM workflow toggle to add-session dialog` |
| 4 | `cfaa84d`    | `test(e2e): smoke test for tpm-mode session creation`  |

## New surfaces

- **CLI:** `aoe profile create <name> --template tpm`
- **CLI:** `aoe add <path> --tpm` (claude-only, requires plugin)
- **TUI:** "TPM Mode" checkbox in the new-session dialog (gated on plugin
  availability and tool=claude)
- **Config:** `[events] sweeper_enabled = true` (default; opt-out only)
- **Module:** `crate::tpm` — orchestrator path resolution + shell snippet
  used by both CLI and TUI
- **Module:** `crate::session::profile_templates` — extension point for
  future templates

## Resolution chain for the orchestrator prompt

When `--tpm` / TPM toggle is set, `crate::tpm::resolve_orchestrator` walks:

1. `$TPM_WORKFLOW_PATH/agents/orchestrator.md` (env override, dev checkouts)
2. `<repo_root>/contrib/tpm-workflow/agents/orchestrator.md` (this fork's
   submodule, walking up 8 ancestors so subdirs work)
3. `~/.claude/plugins/cache/tpm-workflow/tpm-workflow/agents/orchestrator.md`
   (Claude Code marketplace install)

If none resolve the command errors with installation hints (verified by
`tests/e2e/tpm.rs::aoe_add_tpm_without_plugin_errors_out`).

## Things the user should still do manually

1. **Smoke test the full orchestrator end-to-end.** The autonomous run
   verified wiring (the CLI/TUI inject the right `extra_args`, the sweeper
   spawns, events flow through the bus) but never actually booted the
   orchestrator against a real task. The first morning task in
   `contrib/tpm-workflow/docs/NEXT_STEPS.md` is exactly this:
   create a session with TPM toggled, give it a tiny feature request, and
   watch the planner/implementer wave run.

2. **Decide on the orchestrator system-prompt flag.** The run uses
   `--append-system-prompt` (Claude default + orchestrator instructions)
   rather than the `--system-prompt` mentioned in the bootstrap prompt
   (which would replace Claude's defaults entirely). The append variant felt
   safer because it keeps Claude's tool-use bootstrapping intact, but flip
   `crate::tpm::extra_args_snippet` if you'd rather replace.

3. **Web dashboard parity.** `src/server/api.rs` accepts session-creation
   requests but does not yet expose `tpm_mode` (it hardcodes false and
   leaves a comment). If you want TPM sessions creatable from the phone, add
   the field to `CreateSessionBody` and surface it in `web/`.

4. **Update `contrib/tpm-workflow/docs/FORK_PLAN.md`** — the table at the
   top still lists §2 (TUI checkbox) and §3 (profile defaults) as "Not
   implemented", and §4 (e2e test) isn't tracked there at all. Marking
   them done or removing them keeps the plugin's docs honest.

## Pre-existing issues observed (not touched)

- `cargo clippy --tests -- -D warnings` fails on three pre-existing items
  in upstream code (`useless vec!` in `tests/config_wiring.rs`, `unused mut`
  in `src/tui/home/tests.rs:2538`, `modulo 1` in
  `src/tui/status_poller.rs:244`). The husky pre-commit hook only runs
  `cargo clippy -- -D warnings` (no `--tests`), which passes, so commits
  succeed. Heads-up if you ever tighten CI.

## Verification commands

```bash
cargo test --lib                  # 1083 passed
cargo test --test e2e             # 37 passed, 1 ignored (Docker)
cargo test --test e2e tpm::       # the new smoke tests
cargo clippy -- -D warnings       # clean (matches husky pre-commit)
```

`TPM_AUTONOMOUS_RUN_COMPLETE`
