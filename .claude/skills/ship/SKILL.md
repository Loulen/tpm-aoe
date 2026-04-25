# Ship Skill

## Preflight

```bash
git status --porcelain
```

Abort if working tree is dirty. Verify current branch is the feature/integration branch (not `main`).

Check tools:
```bash
which gh && gh auth status
which cargo
```

## Submodule: tpm-workflow

The `contrib/tpm-workflow` submodule tracks `Loulen/tpm-workflow`. If the feature branch's submodule ref differs from `main`'s:

1. `cd contrib/tpm-workflow`
2. Ensure `main` is pushed to origin: `git push origin main`
3. Determine next version from conventional commits since last tag (or `0.1.0` if no tags). Use semver: `feat:` = minor, `fix:` = patch, breaking = major.
4. Tag: `git tag -a vX.Y.Z -m "vX.Y.Z"` and push: `git push origin main --tags`
5. Update the plugin version in `.claude-plugin/marketplace.json` if it differs from the new tag, commit, push.
6. Back in the main repo, `git add contrib/tpm-workflow` if the ref changed.

If the submodule ref is unchanged vs main, skip this section.

## PR

Push the feature branch and create a PR if none exists:

```bash
git push -u origin <branch>
gh pr create --title "<title>" --body "<body>" --base main
```

Title: conventional commit style, under 70 chars.
Body: summary of changes, test results, link to QA report.

If a PR already exists, skip creation but ensure the branch is pushed.

## CI Gates

Wait for CI to pass on the PR:

```bash
gh pr checks <pr-number> --watch --fail-fast
```

Timeout: 15 minutes. If CI fails, abort and report which check failed.

## Merge

Squash merge via gh:

```bash
gh pr merge <pr-number> --squash --delete-branch
```

## Version Detection

After merge, checkout `main` and determine the version bump from conventional commits since the last tag:

- `feat:` commits → minor bump
- `fix:`, `test:`, `refactor:` → patch bump
- `BREAKING CHANGE` or `!` → major bump
- If only `test:`, `docs:`, `chore:` → patch bump (still release for local rebuild)

Current version source: `Cargo.toml` `version` field. Tag format: `vX.Y.Z`.

## Manifests

Bump version in:
- `Cargo.toml` (root package `version` field)

Commit: `chore(release): vX.Y.Z`

## Tagging

```bash
git tag -a vX.Y.Z -m "vX.Y.Z"
```

## Push

```bash
git push origin main --tags
```

## Local Build

Rebuild and install locally so `aoe` reflects the new version:

```bash
cargo build --release
```

The binary is at `target/release/aoe`. If the user has a symlink or PATH entry, it picks up automatically.

Also update the tpm-workflow plugin cache so `claude` picks up the latest plugin version:

```bash
claude plugin update tpm-workflow
```

Or if manual: copy `contrib/tpm-workflow/` contents to `~/.claude/plugins/cache/tpm-workflow/tpm-workflow/<new-version>/`.

## Branch Cleanup

The feature branch is deleted by `gh pr merge --delete-branch`. Clean up local tracking:

```bash
git branch -d <feature-branch>
git remote prune origin
```
