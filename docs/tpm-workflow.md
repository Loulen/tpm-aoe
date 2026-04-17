# TPM Workflow

The **TPM (Technical Project Manager) workflow** is an opinionated way to run
multi-agent feature development inside `aoe`. A long-lived "orchestrator"
session decomposes a task with a planner, dispatches implementer sessions in
parallel through `aoe add`, watches their completion via `aoe events watch`,
then routes summaries to a reviewer/merger and back to the user.

The orchestrator and its sub-agents live in the companion plugin
[`tpm-workflow`](https://github.com/Loulen/tpm-workflow); this fork ships the
AoE-side primitives the plugin needs (events bus, lifecycle sweeper, profile
template, and an upcoming TUI toggle).

## Quick start

1. **Install the plugin in Claude Code** so the orchestrator and sub-agents
   are available:

   ```text
   /plugin marketplace add Loulen/tpm-workflow
   /plugin install tpm-workflow
   ```

2. **Create a TPM-friendly AoE profile.** This pre-configures the profile
   with the defaults the orchestrator expects (worktrees on, YOLO on,
   notification sounds off):

   ```bash
   aoe profile create tpm --template tpm
   ```

   The command writes a `config.toml` to `<profile_dir>/profiles/tpm/` that
   you can hand-edit afterwards.

3. **Open the TUI on the new profile** and create a session, providing your
   feature request as the initial message. The orchestrator skill in the
   plugin takes over from there.

   ```bash
   aoe -p tpm
   ```

## What the `tpm` profile template sets

The `--template tpm` flag seeds the new profile with these overrides:

| Section     | Override                                            | Why                                                                  |
|-------------|-----------------------------------------------------|----------------------------------------------------------------------|
| `worktree`  | `enabled = true`                                    | Every implementer session runs in its own worktree for isolation.    |
| `worktree`  | `path_template = "../{repo-name}-tpm/{branch}"`     | Keeps TPM worktrees in a sibling dir, separate from ad-hoc ones.     |
| `worktree`  | `auto_cleanup = true`                               | Removes worktrees when the session is deleted.                       |
| `session`   | `yolo_mode_default = true`                          | Implementers run unattended; the user already trusts the orchestrator. |
| `sound`     | `enabled = false`                                   | Background sessions completing every few minutes shouldn't beep.     |

All other settings inherit from the global config — edit
`<profile_dir>/profiles/tpm/config.toml` to customize further.

## Naming conventions

The orchestrator tags each task with a short slug and uses it consistently
across AoE state, so events, groups, and worktrees stay correlated:

- Group: `tpm-{task-slug}` (visible in the TUI as a group header)
- Worktree branch: `tpm-{task-slug}` (the branch name; combined with the
  template above this lands at `../{repo-name}-tpm/tpm-{task-slug}/`)
- Session title: a sub-agent role, e.g. `planner`, `implementer-auth-login`

## Events

The orchestrator monitors child sessions through the events bus
(`src/events/`). The lifecycle sweeper polls all sessions in the active
profile and emits typed events on status transitions:

- `session.completed` — a child session ended cleanly. If the worktree
  contains `.tpm/SUMMARY.md`, the event includes its absolute path so the
  orchestrator can pick the summary up immediately.
- `session.failed` — a child session entered the Error state.
- `session.waiting` — a child is asking for human input.
- `session.idle` — a child finished a turn but is still alive.

Tail events live with:

```bash
aoe events watch              # live tail
aoe events history            # last N events
aoe events watch --filter session.completed
```

The sweeper is wired to start automatically alongside the TUI; you only need
to run `aoe events daemon` separately if you want to watch events from a
non-TUI session (e.g. from the orchestrator itself).

## How the TPM toggle wires up the orchestrator

Ticking "TPM Mode" in the new-session dialog (or passing `--tpm` to
`aoe add`) does three things at session-creation time:

1. Confirms the `tpm-workflow` plugin is installed (walks
   `$TPM_WORKFLOW_PATH`, then `contrib/tpm-workflow/`, then
   `~/.claude/plugins/cache/tpm-workflow/tpm-workflow/`).
2. Confirms the selected tool is `claude` — other tools can't host the
   orchestrator because the wiring relies on `--append-system-prompt`.
3. Appends the orchestrator system prompt to the spawned `claude` command.
   The appended value has two parts:
   - A short **override preamble** that instructs Claude to prioritize the
     orchestrator spec over its default system prompt and to treat the
     user's first message as a task description rather than a direct
     coding request. Without this, Claude's defaults win and the session
     just starts implementing.
   - The plugin's `agents/orchestrator.md`, read at launch via
     `$(cat <path>)` so any plugin update takes effect on the next session
     without rebuilding AoE.

Once the session is running, the orchestrator spawns its own child
sessions via `aoe add` (planner, implementers in parallel worktrees,
reviewers, merge resolvers). Those sub-sessions get their own personas
via `--system-prompt` per the plugin's `dispatch-session` skill. The
events sweeper relays their lifecycle to the orchestrator over the
events bus.

**Ticking the box opts into an autonomous multi-session workflow.** The
orchestrator halts for plan approval once (per the spec's Step 2) and
then runs waves unattended until the feature is done or a sub-session
fails.
