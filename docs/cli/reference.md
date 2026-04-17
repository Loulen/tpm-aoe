# Command-Line Help for `aoe`

This document contains the help content for the `aoe` command-line program.

**Command Overview:**

* [`aoe`↴](#aoe)
* [`aoe add`↴](#aoe-add)
* [`aoe init`↴](#aoe-init)
* [`aoe list`↴](#aoe-list)
* [`aoe remove`↴](#aoe-remove)
* [`aoe send`↴](#aoe-send)
* [`aoe status`↴](#aoe-status)
* [`aoe session`↴](#aoe-session)
* [`aoe session start`↴](#aoe-session-start)
* [`aoe session stop`↴](#aoe-session-stop)
* [`aoe session restart`↴](#aoe-session-restart)
* [`aoe session attach`↴](#aoe-session-attach)
* [`aoe session show`↴](#aoe-session-show)
* [`aoe session rename`↴](#aoe-session-rename)
* [`aoe session capture`↴](#aoe-session-capture)
* [`aoe session current`↴](#aoe-session-current)
* [`aoe group`↴](#aoe-group)
* [`aoe group list`↴](#aoe-group-list)
* [`aoe group create`↴](#aoe-group-create)
* [`aoe group delete`↴](#aoe-group-delete)
* [`aoe group move`↴](#aoe-group-move)
* [`aoe events`↴](#aoe-events)
* [`aoe events watch`↴](#aoe-events-watch)
* [`aoe events history`↴](#aoe-events-history)
* [`aoe events emit`↴](#aoe-events-emit)
* [`aoe events daemon`↴](#aoe-events-daemon)
* [`aoe profile`↴](#aoe-profile)
* [`aoe profile list`↴](#aoe-profile-list)
* [`aoe profile create`↴](#aoe-profile-create)
* [`aoe profile delete`↴](#aoe-profile-delete)
* [`aoe profile rename`↴](#aoe-profile-rename)
* [`aoe profile default`↴](#aoe-profile-default)
* [`aoe worktree`↴](#aoe-worktree)
* [`aoe worktree list`↴](#aoe-worktree-list)
* [`aoe worktree info`↴](#aoe-worktree-info)
* [`aoe worktree cleanup`↴](#aoe-worktree-cleanup)
* [`aoe tmux`↴](#aoe-tmux)
* [`aoe tmux status`↴](#aoe-tmux-status)
* [`aoe sounds`↴](#aoe-sounds)
* [`aoe sounds install`↴](#aoe-sounds-install)
* [`aoe sounds list`↴](#aoe-sounds-list)
* [`aoe sounds test`↴](#aoe-sounds-test)
* [`aoe theme`↴](#aoe-theme)
* [`aoe theme list`↴](#aoe-theme-list)
* [`aoe theme export`↴](#aoe-theme-export)
* [`aoe theme dir`↴](#aoe-theme-dir)
* [`aoe serve`↴](#aoe-serve)
* [`aoe uninstall`↴](#aoe-uninstall)
* [`aoe completion`↴](#aoe-completion)

## `aoe`

Agent of Empires (aoe) is a terminal session manager that uses tmux to help you manage and monitor AI coding agents like Claude Code and OpenCode.

Run without arguments to launch the TUI dashboard.

**Usage:** `aoe [OPTIONS] [COMMAND]`

###### **Subcommands:**

* `add` — Add a new session
* `init` — Initialize .agent-of-empires/config.toml in a repository
* `list` — List all sessions
* `remove` — Remove a session
* `send` — Send a message to a running agent session
* `status` — Show session status summary
* `session` — Manage session lifecycle (start, stop, attach, etc.)
* `group` — Manage groups for organizing sessions
* `events` — Stream and emit events on the event bus (for orchestration)
* `profile` — Manage profiles (separate workspaces)
* `worktree` — Manage git worktrees for parallel development
* `tmux` — tmux integration utilities
* `sounds` — Manage sound effects for agent state transitions
* `theme` — Manage color themes (list, export, customize)
* `serve` — Start a web dashboard for remote session access [experimental]
* `uninstall` — Uninstall Agent of Empires
* `completion` — Generate shell completions

###### **Options:**

* `-p`, `--profile <PROFILE>` — Profile to use (separate workspace with its own sessions)



## `aoe add`

Add a new session

**Usage:** `aoe add [OPTIONS] [PATH]`

###### **Arguments:**

* `<PATH>` — Project directory (defaults to current directory)

  Default value: `.`

###### **Options:**

* `-t`, `--title <TITLE>` — Session title (defaults to folder name)
* `-g`, `--group <GROUP>` — Group path (defaults to parent folder)
* `-c`, `--cmd <COMMAND>` — Command to run (e.g., 'claude' or any other supported agent)
* `-P`, `--parent <PARENT>` — Parent session (creates sub-session, inherits group)
* `-l`, `--launch` — Launch the session immediately after creating
* `-w`, `--worktree <WORKTREE_BRANCH>` — Create session in a git worktree for the specified branch
* `-b`, `--new-branch` — Create a new branch (use with --worktree)
* `-r`, `--repo <EXTRA_REPOS>` — Additional repositories for multi-repo workspace (use with --worktree)
* `-s`, `--sandbox` — Run session in Docker sandbox
* `--sandbox-image <SANDBOX_IMAGE>` — Custom Docker image for sandbox (implies --sandbox)
* `-y`, `--yolo` — Enable YOLO mode (skip permission prompts)
* `--trust-hooks` — Automatically trust repository hooks without prompting
* `--extra-args <EXTRA_ARGS>` — Extra arguments to append after the agent binary
* `--cmd-override <CMD_OVERRIDE>` — Override the agent binary command
* `--tpm` — Boot the session as the TPM orchestrator. Requires the `tpm-workflow` plugin (or a `TPM_WORKFLOW_PATH` checkout). Currently only compatible with the `claude` tool



## `aoe init`

Initialize .agent-of-empires/config.toml in a repository

**Usage:** `aoe init [PATH]`

###### **Arguments:**

* `<PATH>` — Directory to initialize (defaults to current directory)

  Default value: `.`



## `aoe list`

List all sessions

**Usage:** `aoe list [OPTIONS]`

###### **Options:**

* `--json` — Output as JSON
* `--all` — List sessions from all profiles



## `aoe remove`

Remove a session

**Usage:** `aoe remove [OPTIONS] <IDENTIFIER>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title to remove

###### **Options:**

* `--delete-worktree` — Delete worktree directory (default: keep worktree)
* `--delete-branch` — Delete git branch after worktree removal (default: per config)
* `--force` — Force worktree removal even with untracked/modified files
* `--keep-container` — Keep container instead of deleting it (default: delete per config)



## `aoe send`

Send a message to a running agent session

**Usage:** `aoe send <IDENTIFIER> <MESSAGE>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title
* `<MESSAGE>` — Message to send to the agent



## `aoe status`

Show session status summary

**Usage:** `aoe status [OPTIONS]`

###### **Options:**

* `-v`, `--verbose` — Show detailed session list
* `-q`, `--quiet` — Only output waiting count (for scripts)
* `--json` — Output as JSON



## `aoe session`

Manage session lifecycle (start, stop, attach, etc.)

**Usage:** `aoe session <COMMAND>`

###### **Subcommands:**

* `start` — Start a session's tmux process
* `stop` — Stop session process
* `restart` — Restart session
* `attach` — Attach to session interactively
* `show` — Show session details
* `rename` — Rename a session
* `capture` — Capture tmux pane output
* `current` — Auto-detect current session



## `aoe session start`

Start a session's tmux process

**Usage:** `aoe session start <IDENTIFIER>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title



## `aoe session stop`

Stop session process

**Usage:** `aoe session stop <IDENTIFIER>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title



## `aoe session restart`

Restart session

**Usage:** `aoe session restart <IDENTIFIER>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title



## `aoe session attach`

Attach to session interactively

**Usage:** `aoe session attach <IDENTIFIER>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title



## `aoe session show`

Show session details

**Usage:** `aoe session show [OPTIONS] [IDENTIFIER]`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title (optional, auto-detects in tmux)

###### **Options:**

* `--json` — Output as JSON



## `aoe session rename`

Rename a session

**Usage:** `aoe session rename [OPTIONS] [IDENTIFIER]`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title (optional, auto-detects in tmux)

###### **Options:**

* `-t`, `--title <TITLE>` — New title for the session
* `-g`, `--group <GROUP>` — New group for the session (empty string to ungroup)



## `aoe session capture`

Capture tmux pane output

**Usage:** `aoe session capture [OPTIONS] [IDENTIFIER]`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title (auto-detects in tmux if omitted)

###### **Options:**

* `-n`, `--lines <LINES>` — Number of lines to capture

  Default value: `50`
* `--strip-ansi` — Strip ANSI escape codes
* `--json` — Output as JSON



## `aoe session current`

Auto-detect current session

**Usage:** `aoe session current [OPTIONS]`

###### **Options:**

* `-q`, `--quiet` — Just session name (for scripting)
* `--json` — Output as JSON



## `aoe group`

Manage groups for organizing sessions

**Usage:** `aoe group <COMMAND>`

###### **Subcommands:**

* `list` — List all groups
* `create` — Create a new group
* `delete` — Delete a group
* `move` — Move session to group



## `aoe group list`

List all groups

**Usage:** `aoe group list [OPTIONS]`

###### **Options:**

* `--json` — Output as JSON



## `aoe group create`

Create a new group

**Usage:** `aoe group create [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` — Group name

###### **Options:**

* `--parent <PARENT>` — Parent group for creating subgroups



## `aoe group delete`

Delete a group

**Usage:** `aoe group delete [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` — Group name

###### **Options:**

* `--force` — Force delete by moving sessions to default group



## `aoe group move`

Move session to group

**Usage:** `aoe group move <IDENTIFIER> <GROUP>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title
* `<GROUP>` — Target group



## `aoe events`

Stream and emit events on the event bus (for orchestration)

**Usage:** `aoe events <COMMAND>`

###### **Subcommands:**

* `watch` — Watch new events as they're emitted (live tail)
* `history` — Read past events from the log
* `emit` — Emit an event to the bus
* `daemon` — Run the lifecycle sweeper that auto-emits session.* events on status transitions



## `aoe events watch`

Watch new events as they're emitted (live tail)

**Usage:** `aoe events watch [OPTIONS]`

###### **Options:**

* `--filter <FILTER>` — Comma-separated list of event types to include (e.g. "session.completed,session.failed")
* `--group <GROUP>` — Filter to events with this group



## `aoe events history`

Read past events from the log

**Usage:** `aoe events history [OPTIONS]`

###### **Options:**

* `--since <SINCE>` — Show events since this duration ago (e.g. "1h", "30m", "2d")
* `--filter <FILTER>` — Comma-separated list of event types to include
* `--group <GROUP>` — Filter to events with this group



## `aoe events emit`

Emit an event to the bus

**Usage:** `aoe events emit [OPTIONS] <TYPE>`

###### **Arguments:**

* `<TYPE>` — Event type (e.g. "session.completed", "custom")

###### **Options:**

* `--session-id <SESSION_ID>` — Session ID (for session.* events)
* `--title <TITLE>` — Session title (for session.* events)
* `--group <GROUP>` — Group name (for session.* events)
* `--worktree <WORKTREE>` — Worktree path (for session.* events that have one)
* `--summary-path <SUMMARY_PATH>` — Path to SUMMARY.md (for session.completed)
* `--tool <TOOL>` — Tool name (for session.started)
* `--exit-code <EXIT_CODE>` — Exit code (for session.completed)
* `--error <ERROR>` — Error message (for session.failed)
* `--reason <REASON>` — Reason (for session.idle)
* `--name <NAME>` — For custom events, the event name
* `--attr <ATTRS>` — For custom events, additional key=value attributes (repeatable)



## `aoe events daemon`

Run the lifecycle sweeper that auto-emits session.* events on status transitions

**Usage:** `aoe events daemon [OPTIONS]`

###### **Options:**

* `--interval <INTERVAL>` — Poll interval in seconds (default: 2)

  Default value: `2`



## `aoe profile`

Manage profiles (separate workspaces)

**Usage:** `aoe profile [COMMAND]`

###### **Subcommands:**

* `list` — List all profiles
* `create` — Create a new profile
* `delete` — Delete a profile
* `rename` — Rename a profile
* `default` — Show or set default profile



## `aoe profile list`

List all profiles

**Usage:** `aoe profile list`



## `aoe profile create`

Create a new profile

**Usage:** `aoe profile create [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` — Profile name

###### **Options:**

* `--template <TEMPLATE>` — Pre-configure the profile from a built-in template (e.g., `tpm`). The template's overrides are written to the profile's config.toml

  Possible values: `tpm`




## `aoe profile delete`

Delete a profile

**Usage:** `aoe profile delete <NAME>`

###### **Arguments:**

* `<NAME>` — Profile name



## `aoe profile rename`

Rename a profile

**Usage:** `aoe profile rename <OLD_NAME> <NEW_NAME>`

###### **Arguments:**

* `<OLD_NAME>` — Current profile name
* `<NEW_NAME>` — New profile name



## `aoe profile default`

Show or set default profile

**Usage:** `aoe profile default [NAME]`

###### **Arguments:**

* `<NAME>` — Profile name (optional, shows current if not provided)



## `aoe worktree`

Manage git worktrees for parallel development

**Usage:** `aoe worktree <COMMAND>`

###### **Subcommands:**

* `list` — List all worktrees in current repository
* `info` — Show worktree information for a session
* `cleanup` — Cleanup orphaned worktrees



## `aoe worktree list`

List all worktrees in current repository

**Usage:** `aoe worktree list`



## `aoe worktree info`

Show worktree information for a session

**Usage:** `aoe worktree info <IDENTIFIER>`

###### **Arguments:**

* `<IDENTIFIER>` — Session ID or title



## `aoe worktree cleanup`

Cleanup orphaned worktrees

**Usage:** `aoe worktree cleanup [OPTIONS]`

###### **Options:**

* `-f`, `--force` — Actually remove worktrees (default is dry-run)



## `aoe tmux`

tmux integration utilities

**Usage:** `aoe tmux <COMMAND>`

###### **Subcommands:**

* `status` — Output session info for use in custom tmux status bar



## `aoe tmux status`

Output session info for use in custom tmux status bar

Add this to your ~/.tmux.conf: set -g status-right "#(aoe tmux status)"

**Usage:** `aoe tmux status [OPTIONS]`

###### **Options:**

* `-f`, `--format <FORMAT>` — Output format (text or json)

  Default value: `text`



## `aoe sounds`

Manage sound effects for agent state transitions

**Usage:** `aoe sounds <COMMAND>`

###### **Subcommands:**

* `install` — Install bundled sound effects
* `list` — List currently installed sounds
* `test` — Test a sound by playing it



## `aoe sounds install`

Install bundled sound effects

**Usage:** `aoe sounds install`



## `aoe sounds list`

List currently installed sounds

**Usage:** `aoe sounds list`



## `aoe sounds test`

Test a sound by playing it

**Usage:** `aoe sounds test <NAME>`

###### **Arguments:**

* `<NAME>` — Sound file name (without extension)



## `aoe theme`

Manage color themes (list, export, customize)

**Usage:** `aoe theme <COMMAND>`

###### **Subcommands:**

* `list` — List all available themes (built-in and custom)
* `export` — Export a built-in theme as a TOML file for customization
* `dir` — Show the custom themes directory path



## `aoe theme list`

List all available themes (built-in and custom)

**Usage:** `aoe theme list`



## `aoe theme export`

Export a built-in theme as a TOML file for customization

**Usage:** `aoe theme export [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` — Theme name to export

###### **Options:**

* `-o`, `--output <OUTPUT>` — Output file path (defaults to <name>.toml in the themes directory)



## `aoe theme dir`

Show the custom themes directory path

**Usage:** `aoe theme dir`



## `aoe serve`

Start a web dashboard for remote session access [experimental]

**Usage:** `aoe serve [OPTIONS]`

###### **Options:**

* `--port <PORT>` — Port to listen on

  Default value: `8080`
* `--host <HOST>` — Host/IP to bind to (use 0.0.0.0 for LAN/VPN access)

  Default value: `127.0.0.1`
* `--no-auth` — Disable authentication (only allowed with localhost binding)
* `--read-only` — Read-only mode: view terminals but cannot send keystrokes
* `--remote` — Expose via Cloudflare Tunnel for secure remote access
* `--tunnel-name <TUNNEL_NAME>` — Use a named Cloudflare Tunnel (requires prior `cloudflared tunnel create`)
* `--tunnel-url <TUNNEL_URL>` — Hostname for a named tunnel (e.g., aoe.example.com)
* `--daemon` — Run as a background daemon (detach from terminal)
* `--stop` — Stop a running daemon
* `--passphrase <PASSPHRASE>` — Require a passphrase for login (second-factor auth). Can also be set via AOE_SERVE_PASSPHRASE environment variable



## `aoe uninstall`

Uninstall Agent of Empires

**Usage:** `aoe uninstall [OPTIONS]`

###### **Options:**

* `--keep-data` — Keep data directory (sessions, config, logs)
* `--keep-tmux-config` — Keep tmux configuration
* `--dry-run` — Show what would be removed without removing
* `-y` — Skip confirmation prompts



## `aoe completion`

Generate shell completions

**Usage:** `aoe completion <SHELL>`

###### **Arguments:**

* `<SHELL>` — Shell to generate completions for

  Possible values: `bash`, `elvish`, `fish`, `powershell`, `zsh`




<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
