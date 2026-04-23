//! End-to-end tests for the `--worktree-from <base>` flag (Feature 6).
//!
//! These tests exercise the full `aoe add -w <branch> -b --worktree-from <base>`
//! flow via CLI, verifying that the created worktree's HEAD matches the expected
//! base branch, that invalid base branches produce clear errors, and that
//! omitting `--worktree-from` still defaults to the current branch HEAD.

use serial_test::serial;
use std::path::PathBuf;

use crate::harness::TuiTestHarness;

// ---------------------------------------------------------------------------
// Git repo setup helpers
// ---------------------------------------------------------------------------

/// Create a git repo at `path` with an initial commit on the default branch
/// (main). Returns the OID of the initial commit.
fn git_init_with_commit(path: &std::path::Path) -> git2::Oid {
    let repo = git2::Repository::init(path).expect("git init");

    // Configure user for commits
    let mut config = repo.config().expect("repo config");
    config
        .set_str("user.name", "E2E Test")
        .expect("set user.name");
    config
        .set_str("user.email", "e2e@test.local")
        .expect("set user.email");

    let sig = git2::Signature::now("E2E Test", "e2e@test.local").expect("signature");

    // Create a file so the tree is non-empty
    std::fs::write(path.join("README.md"), "# Test repo\n").expect("write README");
    let mut index = repo.index().expect("index");
    index
        .add_path(std::path::Path::new("README.md"))
        .expect("add README");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write_tree");
    let tree = repo.find_tree(tree_id).expect("find_tree");

    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .expect("initial commit");

    // Rename default branch to main (some systems default to master)
    let head_ref = repo.head().expect("HEAD");
    if head_ref.shorthand() != Some("main") {
        let mut branch = repo
            .find_branch(head_ref.shorthand().unwrap(), git2::BranchType::Local)
            .expect("find default branch");
        branch.rename("main", true).expect("rename to main");
    }

    oid
}

/// Add a commit on a new branch forked from the current HEAD. Returns the
/// new commit's OID.
fn create_branch_with_commit(
    repo_path: &std::path::Path,
    branch_name: &str,
    file_name: &str,
    file_content: &str,
) -> git2::Oid {
    let repo = git2::Repository::open(repo_path).expect("open repo");
    let sig = git2::Signature::now("E2E Test", "e2e@test.local").expect("signature");

    // Create branch at HEAD
    let head_commit = repo.head().expect("HEAD").peel_to_commit().expect("commit");
    repo.branch(branch_name, &head_commit, false)
        .expect("create branch");

    // Checkout the new branch
    let refname = format!("refs/heads/{}", branch_name);
    repo.set_head(&refname).expect("set head");
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .expect("checkout");

    // Add a file and commit
    std::fs::write(repo_path.join(file_name), file_content).expect("write file");
    let mut index = repo.index().expect("index");
    index
        .add_path(std::path::Path::new(file_name))
        .expect("add file");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write_tree");
    let tree = repo.find_tree(tree_id).expect("find_tree");

    let parent = repo.head().expect("HEAD").peel_to_commit().expect("commit");
    let oid = repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("Add {} on {}", file_name, branch_name),
            &tree,
            &[&parent],
        )
        .expect("commit");

    // Switch back to main so the repo is in a clean state for aoe
    repo.set_head("refs/heads/main").expect("set head to main");
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .expect("checkout main");

    oid
}

/// Get the HEAD commit OID of a repo at the given path.
fn head_oid(repo_path: &std::path::Path) -> git2::Oid {
    let repo = git2::Repository::open(repo_path).expect("open repo");
    let head_ref = repo.head().expect("HEAD");
    let commit = head_ref.peel_to_commit().expect("commit");
    commit.id()
}

/// Read sessions.json from the harness's isolated home.
fn read_sessions(h: &TuiTestHarness) -> serde_json::Value {
    let path = if cfg!(target_os = "linux") {
        h.home_path()
            .join(".config/agent-of-empires/profiles/default/sessions.json")
    } else {
        h.home_path()
            .join(".agent-of-empires/profiles/default/sessions.json")
    };
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("invalid sessions JSON")
}

// ---------------------------------------------------------------------------
// AC-01: --worktree-from integration roots the new branch at integration HEAD
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn worktree_from_uses_specified_base_branch() {
    let h = TuiTestHarness::new("wt_from_base");
    let project = h.project_path();

    // Create repo with main branch
    git_init_with_commit(&project);

    // Create integration branch with one extra commit
    let integration_oid = create_branch_with_commit(
        &project,
        "integration",
        "integration.txt",
        "integration content\n",
    );

    // Sanity: main HEAD should be different from integration HEAD
    let main_oid = head_oid(&project);
    assert_ne!(
        main_oid, integration_oid,
        "integration should have a commit that main doesn't"
    );

    // Run: aoe add <project> -w task-branch -b --worktree-from integration -t 'Task from integration'
    let output = h.run_cli(&[
        "add",
        project.to_str().unwrap(),
        "-w",
        "task-branch",
        "-b",
        "--worktree-from",
        "integration",
        "-t",
        "Task from integration",
    ]);
    assert!(
        output.status.success(),
        "aoe add --worktree-from failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Find the worktree path from sessions.json
    let sessions = read_sessions(&h);
    let session = sessions
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["title"] == "Task from integration"))
        .expect("session not found in sessions.json");

    let wt_path = PathBuf::from(
        session["project_path"]
            .as_str()
            .expect("project_path missing"),
    );
    assert!(
        wt_path.exists(),
        "worktree dir should exist at {}",
        wt_path.display()
    );

    // Verify worktree HEAD matches integration HEAD, not main HEAD
    let wt_head = head_oid(&wt_path);
    assert_eq!(
        wt_head, integration_oid,
        "worktree HEAD should match integration branch HEAD.\n\
         worktree HEAD:    {}\n\
         integration HEAD: {}\n\
         main HEAD:        {}",
        wt_head, integration_oid, main_oid,
    );
}

// ---------------------------------------------------------------------------
// AC-02: --worktree-from nonexistent → non-zero exit, stderr mentions branch
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn worktree_from_nonexistent_branch_errors() {
    let h = TuiTestHarness::new("wt_from_bad");
    let project = h.project_path();

    git_init_with_commit(&project);

    let output = h.run_cli(&[
        "add",
        project.to_str().unwrap(),
        "-w",
        "task-branch",
        "-b",
        "--worktree-from",
        "nonexistent",
        "-t",
        "Bad base",
    ]);

    assert!(
        !output.status.success(),
        "aoe add --worktree-from nonexistent should fail.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        combined.contains("nonexistent"),
        "error output should mention the bad branch name 'nonexistent'. Got:\n{}",
        combined,
    );
}

// ---------------------------------------------------------------------------
// AC-03: aoe add --help mentions --worktree-from
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn add_help_mentions_worktree_from() {
    let h = TuiTestHarness::new("wt_from_help");

    let output = h.run_cli(&["add", "--help"]);
    assert!(
        output.status.success(),
        "aoe add --help failed.\nstderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--worktree-from"),
        "aoe add --help should mention --worktree-from. Got:\n{}",
        stdout,
    );
}

// ---------------------------------------------------------------------------
// AC-04: without --worktree-from, worktree HEAD matches current branch HEAD
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn worktree_without_from_defaults_to_current_head() {
    let h = TuiTestHarness::new("wt_no_from");
    let project = h.project_path();

    git_init_with_commit(&project);

    // Create integration branch (so main and integration diverge)
    create_branch_with_commit(
        &project,
        "integration",
        "integration.txt",
        "integration content\n",
    );

    // main HEAD (the current branch after create_branch_with_commit switches back)
    let main_oid = head_oid(&project);

    // Run: aoe add <project> -w default-branch -b -t 'Default base'
    // No --worktree-from, so the worktree should be rooted at main HEAD.
    let output = h.run_cli(&[
        "add",
        project.to_str().unwrap(),
        "-w",
        "default-branch",
        "-b",
        "-t",
        "Default base",
    ]);
    assert!(
        output.status.success(),
        "aoe add -w -b without --worktree-from failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Find worktree path
    let sessions = read_sessions(&h);
    let session = sessions
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s["title"] == "Default base"))
        .expect("session not found");

    let wt_path = PathBuf::from(
        session["project_path"]
            .as_str()
            .expect("project_path missing"),
    );

    let wt_head = head_oid(&wt_path);
    assert_eq!(
        wt_head, main_oid,
        "without --worktree-from, worktree HEAD should match main HEAD.\n\
         worktree HEAD: {}\n\
         main HEAD:     {}",
        wt_head, main_oid,
    );
}
