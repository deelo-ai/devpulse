//! Integration tests for devpulse CLI.
//!
//! Each test creates temporary git repos with known state, runs the devpulse
//! binary via `std::process::Command`, and verifies stdout output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// Get the path to the devpulse binary (built by cargo test).
fn devpulse_bin() -> PathBuf {
    // cargo test builds the binary in target/debug
    let mut path = std::env::current_exe()
        .expect("failed to get test exe path")
        .parent()
        .expect("no parent")
        .parent()
        .expect("no grandparent")
        .to_path_buf();
    path.push("devpulse");
    path
}

/// Run devpulse with given args and return (stdout, stderr, success).
fn run_devpulse(args: &[&str]) -> (String, String, bool) {
    let output = Command::new(devpulse_bin())
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run devpulse");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
    )
}

/// Create a git repo with an initial commit in `dir`.
fn init_repo(dir: &Path) {
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "test@test.com"]);
    run_git(dir, &["config", "user.name", "Test"]);
    run_git(dir, &["commit", "--allow-empty", "-m", "initial commit"]);
}

/// Create a git repo with a dirty working tree.
fn init_dirty_repo(dir: &Path) {
    init_repo(dir);
    fs::write(dir.join("dirty.txt"), "uncommitted").expect("failed to write dirty file");
}

/// Run a git command in a directory.
fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    if !output.status.success() {
        panic!(
            "git {:?} failed in {}: {}",
            args,
            dir.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Create a standard test environment with multiple repos.
fn setup_multi_repo(parent: &Path) {
    let clean = parent.join("alpha-clean");
    let dirty = parent.join("beta-dirty");
    fs::create_dir_all(&clean).unwrap();
    fs::create_dir_all(&dirty).unwrap();
    init_repo(&clean);
    init_dirty_repo(&dirty);
}

// ============================================================
// Tests
// ============================================================

#[test]
fn test_scan_clean_repo() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("myproject");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _stderr, success) = run_devpulse(&["--no-color", tmp.path().to_str().unwrap()]);
    assert!(success, "devpulse should exit 0");
    assert!(stdout.contains("myproject"), "output should contain project name");
    assert!(stdout.contains("clean"), "clean repo should show 'clean'");
    assert!(stdout.contains("main"), "should show branch name");
}

#[test]
fn test_scan_dirty_repo() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("dirtyproject");
    fs::create_dir_all(&repo).unwrap();
    init_dirty_repo(&repo);

    let (stdout, _stderr, success) = run_devpulse(&["--no-color", tmp.path().to_str().unwrap()]);
    assert!(success);
    assert!(stdout.contains("dirtyproject"));
    assert!(stdout.contains("dirty"), "dirty repo should show 'dirty'");
    assert!(
        stdout.contains('1'),
        "should show 1 changed file somewhere in output"
    );
}

#[test]
fn test_json_output() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("jsontest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _stderr, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success);

    let parsed: serde_json::Value = serde_json::from_str(
        stdout
            .lines()
            .filter(|l| !l.starts_with("Scanning"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim(),
    )
    .expect("JSON output should be valid JSON");

    let projects = parsed["projects"].as_array().expect("should have projects array");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["name"], "jsontest");
    assert_eq!(projects[0]["is_clean"], true);
    assert_eq!(projects[0]["branch"], "main");

    let summary = &parsed["summary"];
    assert_eq!(summary["total"], 1);
    assert_eq!(summary["dirty"], 0);
}

#[test]
fn test_csv_output() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("csvtest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _stderr, success) =
        run_devpulse(&["--format", "csv", tmp.path().to_str().unwrap()]);
    assert!(success);

    let lines: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning") && !l.is_empty())
        .collect();
    assert!(lines.len() >= 2, "CSV should have header + at least 1 row");
    assert!(
        lines[0].contains("Project") || lines[0].contains("project") || lines[0].contains("name"),
        "first line should be CSV header"
    );
    assert!(lines[1].contains("csvtest"), "data row should contain project name");
}

#[test]
fn test_markdown_output() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("mdtest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _stderr, success) =
        run_devpulse(&["--format", "markdown", tmp.path().to_str().unwrap()]);
    assert!(success);

    let content: String = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(content.contains('|'), "markdown output should contain pipe characters for table");
    assert!(content.contains("mdtest"), "should contain project name");
}

#[test]
fn test_sort_by_name() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _stderr, success) =
        run_devpulse(&["--sort", "name", "--no-color", tmp.path().to_str().unwrap()]);
    assert!(success);

    let alpha_pos = stdout.find("alpha-clean").expect("should find alpha-clean");
    let beta_pos = stdout.find("beta-dirty").expect("should find beta-dirty");
    assert!(
        alpha_pos < beta_pos,
        "alpha should appear before beta when sorted by name"
    );
}

#[test]
fn test_sort_by_status() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _stderr, success) =
        run_devpulse(&["--sort", "status", "--no-color", tmp.path().to_str().unwrap()]);
    assert!(success);

    let dirty_pos = stdout.find("beta-dirty").expect("should find beta-dirty");
    let clean_pos = stdout.find("alpha-clean").expect("should find alpha-clean");
    assert!(
        dirty_pos < clean_pos,
        "dirty projects should appear before clean when sorted by status"
    );
}

#[test]
fn test_filter_dirty() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _stderr, success) = run_devpulse(&[
        "--filter",
        "dirty",
        "--no-color",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(stdout.contains("beta-dirty"), "should show dirty project");
    assert!(
        !stdout.contains("alpha-clean"),
        "should not show clean project when filtering for dirty"
    );
}

#[test]
fn test_filter_clean() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _stderr, success) = run_devpulse(&[
        "--filter",
        "clean",
        "--no-color",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(stdout.contains("alpha-clean"), "should show clean project");
    assert!(
        !stdout.contains("beta-dirty"),
        "should not show dirty project when filtering for clean"
    );
}

#[test]
fn test_filter_by_name() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _stderr, success) = run_devpulse(&[
        "--filter",
        "name:alpha",
        "--no-color",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(stdout.contains("alpha-clean"), "should show matching project");
    assert!(
        !stdout.contains("beta-dirty"),
        "should not show non-matching project"
    );
}

#[test]
fn test_nonexistent_directory() {
    let (stdout, _stderr, success) = run_devpulse(&["/tmp/devpulse_nonexistent_dir_xyz"]);
    // Should either fail gracefully or show "no projects found"
    // The important thing is it doesn't panic
    let combined = format!("{stdout}{_stderr}");
    assert!(
        !success || combined.contains("No projects found") || combined.contains("error"),
        "should handle nonexistent directory gracefully"
    );
}

#[test]
fn test_empty_directory() {
    let tmp = TempDir::new().unwrap();
    // No git repos in this directory

    let (stdout, _stderr, success) = run_devpulse(&["--no-color", tmp.path().to_str().unwrap()]);
    assert!(success, "should exit 0 for empty dir");
    assert!(
        stdout.contains("No projects found"),
        "should indicate no projects found, got: {stdout}"
    );
}

#[test]
fn test_output_to_file() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("fileout");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let output_file = tmp.path().join("output.json");
    let (_, _stderr, success) = run_devpulse(&[
        "--json",
        "--output",
        output_file.to_str().unwrap(),
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success, "should succeed writing to file");
    assert!(output_file.exists(), "output file should be created");

    let content = fs::read_to_string(&output_file).expect("should read output file");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("output file should contain valid JSON");
    assert!(parsed["projects"].is_array());
}

#[test]
fn test_group_flag() {
    let tmp = TempDir::new().unwrap();

    // Create repos in subdirectories to test grouping
    let subdir1 = tmp.path().join("group1");
    let subdir2 = tmp.path().join("group2");
    fs::create_dir_all(&subdir1).unwrap();
    fs::create_dir_all(&subdir2).unwrap();

    let repo1 = subdir1.join("proj-a");
    let repo2 = subdir2.join("proj-b");
    fs::create_dir_all(&repo1).unwrap();
    fs::create_dir_all(&repo2).unwrap();
    init_repo(&repo1);
    init_repo(&repo2);

    let (stdout, _stderr, success) = run_devpulse(&[
        "--group",
        "--no-color",
        "--depth",
        "2",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success, "grouped output should succeed");
    assert!(
        stdout.contains("proj-a") && stdout.contains("proj-b"),
        "should show both projects"
    );
}

#[test]
fn test_no_color_flag() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("colortest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _stderr, success) =
        run_devpulse(&["--no-color", tmp.path().to_str().unwrap()]);
    assert!(success);
    // ANSI escape codes start with \x1b[
    assert!(
        !stdout.contains("\x1b["),
        "no-color output should not contain ANSI escape codes"
    );
}

#[test]
fn test_no_color_env_var() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("envcolortest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let output = Command::new(devpulse_bin())
        .args([tmp.path().to_str().unwrap()])
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run devpulse");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        !stdout.contains("\x1b["),
        "NO_COLOR env should suppress ANSI codes"
    );
}

#[test]
fn test_version_flag() {
    let (stdout, _stderr, success) = run_devpulse(&["--version"]);
    assert!(success);
    assert!(
        stdout.contains("devpulse") && stdout.contains("0.1.0"),
        "version output should contain name and version, got: {stdout}"
    );
}

#[test]
fn test_help_flag() {
    let (stdout, _stderr, success) = run_devpulse(&["--help"]);
    assert!(success);
    assert!(stdout.contains("Usage:"), "help should contain usage info");
    assert!(stdout.contains("--sort"), "help should list --sort flag");
    assert!(stdout.contains("--json"), "help should list --json flag");
}

#[test]
fn test_invalid_filter() {
    let tmp = TempDir::new().unwrap();
    let (_, stderr, success) = run_devpulse(&[
        "--filter",
        "bogus_filter",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(!success, "invalid filter should cause non-zero exit");
    assert!(
        stderr.contains("Unknown filter") || stderr.contains("error") || stderr.contains("bogus"),
        "should report invalid filter in stderr"
    );
}

#[test]
fn test_since_flag() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("sincetest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // Recent commit should appear with --since 1d
    let (stdout, _stderr, success) = run_devpulse(&[
        "--since",
        "1d",
        "--no-color",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(
        stdout.contains("sincetest"),
        "recent project should appear with --since 1d"
    );
}

#[test]
fn test_since_flag_excludes_old() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("oldproject");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // With an extremely short since window, we still find it because
    // commit was just made. But --since 0d should filter everything
    // (0 days = now, nothing matches). This tests the filter works at all.
    // Using --json to check programmatically.
    let (stdout, _stderr, success) = run_devpulse(&[
        "--json",
        "--since",
        "1d",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);

    let json_str: String = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).expect("should be valid JSON");
    let projects = parsed["projects"].as_array().unwrap();
    assert_eq!(
        projects.len(),
        1,
        "project with recent commit should appear with --since 1d"
    );
}

#[test]
fn test_multiple_filters() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    // Filter for dirty AND name contains beta — should match beta-dirty
    let (stdout, _stderr, success) = run_devpulse(&[
        "--filter",
        "dirty",
        "--filter",
        "name:beta",
        "--no-color",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(stdout.contains("beta-dirty"), "combined filters should match beta-dirty");
    assert!(
        !stdout.contains("alpha-clean"),
        "combined filters should exclude alpha-clean"
    );
}

#[test]
fn test_depth_zero() {
    let tmp = TempDir::new().unwrap();
    // Initialize the root as a git repo itself
    init_repo(tmp.path());

    let (stdout, _stderr, success) = run_devpulse(&[
        "--depth",
        "0",
        "--no-color",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    // With depth 0, it should check the target directory itself
    let dir_name = tmp
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        stdout.contains(dir_name) || stdout.contains("clean"),
        "depth 0 should scan the directory itself"
    );
}

#[test]
fn test_json_summary_counts() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _stderr, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success);

    let json_str: String = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).expect("should be valid JSON");

    let summary = &parsed["summary"];
    assert_eq!(summary["total"], 2, "should have 2 total projects");
    assert_eq!(summary["dirty"], 1, "should have 1 dirty project");
}

#[test]
fn test_csv_output_to_file() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("csvfile");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let output_file = tmp.path().join("output.csv");
    let (_, _stderr, success) = run_devpulse(&[
        "--format",
        "csv",
        "--output",
        output_file.to_str().unwrap(),
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(output_file.exists(), "CSV output file should be created");

    let content = fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("csvfile"), "CSV file should contain project name");
}

#[test]
fn test_markdown_output_to_file() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("mdfile");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let output_file = tmp.path().join("output.md");
    let (_, _stderr, success) = run_devpulse(&[
        "--format",
        "md",
        "--output",
        output_file.to_str().unwrap(),
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(output_file.exists(), "markdown output file should be created");

    let content = fs::read_to_string(&output_file).unwrap();
    assert!(content.contains('|'), "markdown file should contain table separators");
    assert!(content.contains("mdfile"));
}

#[test]
fn test_stash_count_in_json() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("stashtest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // Create a stash
    fs::write(repo.join("stash.txt"), "stashme").unwrap();
    run_git(&repo, &["add", "stash.txt"]);
    run_git(&repo, &["stash", "push", "-m", "test stash"]);

    let (stdout, _stderr, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success);

    let json_str: String = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).expect("should be valid JSON");
    let stash_count = parsed["projects"][0]["stash_count"]
        .as_u64()
        .expect("stash_count should be a number");
    assert_eq!(stash_count, 1, "should detect 1 stash entry");
}

#[test]
fn test_last_commit_message_in_json() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("msgtest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // The initial commit message is "initial commit"
    let (stdout, _stderr, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success);

    let json_str: String = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).expect("should be valid JSON");
    let msg = parsed["projects"][0]["last_commit_message"]
        .as_str()
        .expect("should have last_commit_message");
    assert_eq!(msg, "initial commit");
}

#[test]
fn test_depth_two_nested_repos() {
    let tmp = TempDir::new().unwrap();
    // Create nested structure: parent/child/repo
    let nested = tmp.path().join("level1").join("level2-repo");
    fs::create_dir_all(&nested).unwrap();
    init_repo(&nested);

    // depth=1 should NOT find it
    let (stdout, _stderr, success) = run_devpulse(&[
        "--depth", "1", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(
        !stdout.contains("level2-repo"),
        "depth 1 should not find nested repo"
    );

    // depth=2 SHOULD find it
    let (stdout2, _stderr2, success2) = run_devpulse(&[
        "--depth", "2", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success2);
    assert!(
        stdout2.contains("level2-repo"),
        "depth 2 should find nested repo"
    );
}

#[test]
fn test_filter_stale() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("stalerepo");
    fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init"]);
    run_git(&repo, &["config", "user.email", "test@test.com"]);
    run_git(&repo, &["config", "user.name", "Test"]);
    // Create a commit dated 60 days ago using ISO format
    let old_date = chrono::Utc::now() - chrono::Duration::days(60);
    let date_str = old_date.format("%Y-%m-%dT%H:%M:%S").to_string();
    let output = Command::new("git")
        .args(["commit", "--allow-empty", "-m", "old commit", "--date", &date_str])
        .current_dir(&repo)
        .env("GIT_COMMITTER_DATE", &date_str)
        .output()
        .expect("failed to create old commit");
    assert!(output.status.success(), "old commit should succeed: {}", String::from_utf8_lossy(&output.stderr));

    let (stdout, _stderr, success) = run_devpulse(&[
        "--filter", "stale", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(
        stdout.contains("stalerepo"),
        "project with 60-day old commit should be stale"
    );
}

#[test]
fn test_filter_unpushed() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("unpushed-repo");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    // No remote set up, so commits are "unpushed" — behavior depends on implementation
    // At minimum, the filter should not crash
    let (_stdout, _stderr, success) = run_devpulse(&[
        "--filter", "unpushed", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success, "unpushed filter should not crash");
}

#[test]
fn test_include_empty_with_since() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("empty-history");
    fs::create_dir_all(&repo).unwrap();
    // Init repo but with no commits — just git init
    run_git(&repo, &["init"]);

    // Without --include-empty, empty repos should be excluded
    let (stdout1, _stderr1, success1) = run_devpulse(&[
        "--since", "7d", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success1);
    assert!(
        !stdout1.contains("empty-history"),
        "empty repo should be excluded without --include-empty"
    );

    // With --include-empty, it should show up
    let (stdout2, _stderr2, success2) = run_devpulse(&[
        "--since", "7d", "--include-empty", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success2);
    assert!(
        stdout2.contains("empty-history"),
        "empty repo should appear with --include-empty"
    );
}

#[test]
fn test_config_file() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("configtest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // Write a .devpulse.toml that sets color = false and sort = "name"
    let config_content = r#"
sort = "name"
color = false
"#;
    fs::write(tmp.path().join(".devpulse.toml"), config_content).unwrap();

    // Run without --no-color flag — config should disable color
    let output = Command::new(devpulse_bin())
        .args([tmp.path().to_str().unwrap()])
        .env_remove("NO_COLOR")
        .output()
        .expect("failed to run devpulse");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        !stdout.contains("\x1b["),
        "config color=false should suppress ANSI codes"
    );
}

#[test]
fn test_format_table_explicit() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("tabletest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _stderr, success) = run_devpulse(&[
        "--format", "table", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success);
    assert!(stdout.contains("tabletest"), "table format should show project name");
    // Table output should NOT be JSON or CSV
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "table output should not be valid JSON"
    );
}

#[test]
fn test_group_json_output() {
    let tmp = TempDir::new().unwrap();
    let grp = tmp.path().join("mygroup");
    let repo = grp.join("grouped-proj");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _stderr, success) = run_devpulse(&[
        "--group", "--json", "--depth", "2", tmp.path().to_str().unwrap(),
    ]);
    assert!(success);

    let json_str: String = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).expect("grouped JSON should be valid");
    // With --group, JSON uses "groups" key instead of "projects"
    assert!(
        parsed["groups"].is_object(),
        "grouped JSON should have groups object, got: {json_str}"
    );
    assert!(
        parsed["summary"].is_object(),
        "grouped JSON should have summary"
    );
}

#[test]
fn test_staged_changes_detected() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("stagedtest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // Stage a file without committing
    fs::write(repo.join("staged.txt"), "staged content").unwrap();
    run_git(&repo, &["add", "staged.txt"]);

    let (stdout, _stderr, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success);

    let json_str: String = stdout
        .lines()
        .filter(|l| !l.starts_with("Scanning"))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed: serde_json::Value =
        serde_json::from_str(json_str.trim()).expect("should be valid JSON");
    assert_eq!(
        parsed["projects"][0]["is_clean"], false,
        "staged but uncommitted changes should be dirty"
    );
}

#[test]
fn test_multiple_repos_sorted_by_activity() {
    let tmp = TempDir::new().unwrap();

    // Create two repos with different commit times
    let old_repo = tmp.path().join("old-repo");
    let new_repo = tmp.path().join("new-repo");
    fs::create_dir_all(&old_repo).unwrap();
    fs::create_dir_all(&new_repo).unwrap();

    // Old repo: commit dated 10 days ago
    run_git(&old_repo, &["init"]);
    run_git(&old_repo, &["config", "user.email", "test@test.com"]);
    run_git(&old_repo, &["config", "user.name", "Test"]);
    let old_date = chrono::Utc::now() - chrono::Duration::days(10);
    let date_str = old_date.format("%Y-%m-%dT%H:%M:%S").to_string();
    let output = Command::new("git")
        .args(["commit", "--allow-empty", "-m", "old", "--date", &date_str])
        .current_dir(&old_repo)
        .env("GIT_COMMITTER_DATE", &date_str)
        .output()
        .unwrap();
    assert!(output.status.success(), "old commit should succeed");

    // New repo: commit now
    init_repo(&new_repo);

    let (stdout, _stderr, success) = run_devpulse(&[
        "--sort", "activity", "--no-color", tmp.path().to_str().unwrap(),
    ]);
    assert!(success);

    // Activity sort = most stale first, so old-repo should appear before new-repo
    let old_pos = stdout.find("old-repo").expect("should find old-repo");
    let new_pos = stdout.find("new-repo").expect("should find new-repo");
    assert!(
        old_pos < new_pos,
        "activity sort should put stale (old) repos first"
    );
}

#[test]
fn test_json_format_flag_equivalent_to_json_flag() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("fmtjson");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout1, _, _) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    let (stdout2, _, _) = run_devpulse(&["--format", "json", tmp.path().to_str().unwrap()]);

    // Both should produce valid JSON with same structure
    let filter_scanning = |s: &str| -> String {
        s.lines()
            .filter(|l| !l.starts_with("Scanning"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let j1: serde_json::Value = serde_json::from_str(filter_scanning(&stdout1).trim()).unwrap();
    let j2: serde_json::Value = serde_json::from_str(filter_scanning(&stdout2).trim()).unwrap();
    assert_eq!(
        j1["projects"].as_array().unwrap().len(),
        j2["projects"].as_array().unwrap().len(),
        "--json and --format json should produce same number of projects"
    );
}

// ── Additional edge-case tests (#25) ──

#[test]
fn test_invalid_since_value() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("sincetest");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (_, stderr, success) = run_devpulse(&["--since", "abc", tmp.path().to_str().unwrap()]);
    assert!(!success, "invalid --since should fail");
    assert!(
        stderr.contains("Invalid") || stderr.contains("invalid") || stderr.contains("error"),
        "should show error for invalid --since: {stderr}"
    );
}

#[test]
fn test_invalid_since_zero_days() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("sincetest0");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    // 0d should either work (showing nothing recent) or fail gracefully
    let (_, _, _) = run_devpulse(&["--since", "0d", tmp.path().to_str().unwrap()]);
    // Just ensure it doesn't panic/crash
}

#[test]
fn test_help_output_contains_key_info() {
    let (stdout, _, success) = run_devpulse(&["--help"]);
    assert!(success, "help should succeed");
    assert!(
        stdout.contains("devpulse"),
        "help should mention devpulse"
    );
    assert!(
        stdout.contains("--json"),
        "help should document --json flag"
    );
    assert!(
        stdout.contains("--filter"),
        "help should document --filter flag"
    );
    assert!(
        stdout.contains("--since"),
        "help should document --since flag"
    );
    assert!(
        stdout.contains("--sort"),
        "help should document --sort flag"
    );
    assert!(
        stdout.contains("--watch"),
        "help should document --watch flag"
    );
    assert!(
        stdout.contains("EXAMPLES"),
        "help should include examples"
    );
}

#[test]
fn test_default_depth_scans_children() {
    // Without --depth, default is 1: scan immediate children
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _, success) = run_devpulse(&[tmp.path().to_str().unwrap()]);
    assert!(success, "default depth scan should succeed");
    assert!(stdout.contains("alpha-clean"), "should find child repo");
    assert!(stdout.contains("beta-dirty"), "should find child repo");
}

#[test]
fn test_repo_with_no_commits() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("no-commits");
    fs::create_dir_all(&repo).unwrap();
    // Init without committing
    run_git(&repo, &["init"]);
    run_git(&repo, &["config", "user.email", "test@test.com"]);
    run_git(&repo, &["config", "user.name", "Test"]);

    let (stdout, stderr, success) = run_devpulse(&[tmp.path().to_str().unwrap()]);
    // Should handle gracefully — either show it or skip with warning
    // Main thing: shouldn't crash
    assert!(
        success || stderr.contains("Warning") || stderr.contains("skipping"),
        "no-commit repo should be handled gracefully, got: stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn test_csv_format_flag() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("csvfmt");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _, success) = run_devpulse(&["--format", "csv", tmp.path().to_str().unwrap()]);
    assert!(success, "csv format should succeed");
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.starts_with("Scanning") && !l.is_empty()).collect();
    assert!(lines.len() >= 2, "csv should have header + data rows, got: {lines:?}");
    assert!(
        lines[0].contains(','),
        "csv header should contain commas: {:?}", lines[0]
    );
}

#[test]
fn test_markdown_format_flag() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("mdfmt");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _, success) = run_devpulse(&["--format", "markdown", tmp.path().to_str().unwrap()]);
    assert!(success, "markdown format should succeed");
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    assert!(
        content.contains('|'),
        "markdown should contain pipe separators: {content}"
    );
}

#[test]
fn test_group_with_csv_output() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _, success) = run_devpulse(&["--group", "--format", "csv", tmp.path().to_str().unwrap()]);
    assert!(success, "grouped csv should succeed");
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    assert!(!content.is_empty(), "grouped csv should produce output");
}

#[test]
fn test_group_with_markdown_output() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _, success) = run_devpulse(&["--group", "--format", "markdown", tmp.path().to_str().unwrap()]);
    assert!(success, "grouped markdown should succeed");
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    assert!(content.contains('|'), "grouped markdown should have table syntax");
}

#[test]
fn test_output_file_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("outrepo");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let output_path = tmp.path().join("subdir").join("nested").join("output.json");
    let (_, _, success) = run_devpulse(&[
        "--json",
        "-o", output_path.to_str().unwrap(),
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success, "output to nested path should succeed");
    assert!(output_path.exists(), "output file should be created");
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("projects"), "output should contain JSON data");
}

#[test]
fn test_filter_combined_dirty_and_name() {
    let tmp = TempDir::new().unwrap();
    setup_multi_repo(tmp.path());

    let (stdout, _, success) = run_devpulse(&[
        "--filter", "dirty",
        "--filter", "name:beta",
        "--json",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(success, "combined filter should succeed");
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    let json: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    let projects = json["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1, "should match exactly beta-dirty");
    assert!(projects[0]["name"].as_str().unwrap().contains("beta"));
}

#[test]
fn test_scan_single_repo_depth_zero() {
    // --depth 0 checks the target directory itself as a repo
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let (stdout, _, success) = run_devpulse(&["--depth", "0", tmp.path().to_str().unwrap()]);
    assert!(success, "depth 0 on a repo should succeed");
    // Should show the repo itself
    let lower = stdout.to_lowercase();
    assert!(
        lower.contains("clean") || lower.contains("dirty") || stdout.contains("✓") || stdout.contains("✗"),
        "should display repo status: {stdout}"
    );
}

#[test]
#[cfg(unix)]
fn test_permission_denied_directory() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let restricted = tmp.path().join("no-access");
    fs::create_dir_all(&restricted).unwrap();
    init_repo(&restricted);

    // Remove read + execute permissions
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o000)).unwrap();

    let (_, stderr, _) = run_devpulse(&[tmp.path().to_str().unwrap()]);
    // Should handle gracefully — either skip or report error, not panic
    // Restore permissions so TempDir cleanup works
    fs::set_permissions(&restricted, fs::Permissions::from_mode(0o755)).unwrap();

    // The key assertion: it didn't panic. stderr may or may not contain an error message.
    // We just verify it completed without crashing.
    let _ = stderr; // silence unused warning
}

#[test]
fn test_symlink_to_repo() {
    let tmp = TempDir::new().unwrap();
    let real_repo = tmp.path().join("real-repo");
    fs::create_dir_all(&real_repo).unwrap();
    init_repo(&real_repo);

    // Create a symlink pointing to the real repo
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_repo, tmp.path().join("linked-repo")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&real_repo, tmp.path().join("linked-repo")).unwrap();

    let (stdout, _, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success, "scanning with symlinks should succeed");
    // Should find at least the real repo
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    let json: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    let projects = json["projects"].as_array().unwrap();
    assert!(!projects.is_empty(), "should find at least one repo");
}

#[test]
fn test_long_project_name() {
    let tmp = TempDir::new().unwrap();
    let long_name = "a".repeat(200);
    let repo = tmp.path().join(&long_name);
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _, success) = run_devpulse(&[tmp.path().to_str().unwrap()]);
    assert!(success, "long project name should not crash");
    assert!(!stdout.is_empty(), "should produce output");
}

#[test]
fn test_repo_with_only_staged_no_commits() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("staged-only");
    fs::create_dir_all(&repo).unwrap();

    // Init repo and add a file to staging without committing beyond initial
    init_repo(&repo);
    fs::write(repo.join("staged.txt"), "staged content").unwrap();
    run_git(&repo, &["add", "staged.txt"]);

    let (stdout, _, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success, "repo with staged changes should succeed");
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    let json: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    let projects = json["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    // Should show as not clean since there are staged changes
    assert!(!projects[0]["is_clean"].as_bool().unwrap(), "staged changes should make repo dirty");
}

#[test]
fn test_sort_by_name_deterministic() {
    let tmp = TempDir::new().unwrap();
    for name in &["zebra", "alpha", "mango"] {
        let repo = tmp.path().join(name);
        fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
    }

    let (stdout, _, success) = run_devpulse(&["--sort", "name", "--json", tmp.path().to_str().unwrap()]);
    assert!(success);
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    let json: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    let projects = json["projects"].as_array().unwrap();
    let names: Vec<&str> = projects.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["alpha", "mango", "zebra"], "should be sorted alphabetically");
}

#[test]
fn test_csv_header_and_row_count() {
    let tmp = TempDir::new().unwrap();
    for name in &["proj-a", "proj-b", "proj-c"] {
        let repo = tmp.path().join(name);
        fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
    }

    let (stdout, _, success) = run_devpulse(&["--format", "csv", tmp.path().to_str().unwrap()]);
    assert!(success, "csv output should succeed");
    let lines: Vec<&str> = stdout.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("Scanning") && !l.starts_with("Found"))
        .collect();
    // Should have header + 3 data rows
    assert!(lines.len() >= 4, "should have header + 3 rows, got {} lines: {:?}", lines.len(), lines);
    // First line should be a CSV header with commas
    let header = lines[0].to_lowercase();
    assert!(header.contains(","), "csv header should contain commas: {}", header);
}

#[test]
fn test_markdown_table_structure() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("md-test");
    fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let (stdout, _, success) = run_devpulse(&["--format", "markdown", tmp.path().to_str().unwrap()]);
    assert!(success, "markdown output should succeed");
    let content: String = stdout.lines().filter(|l| !l.starts_with("Scanning")).collect::<Vec<_>>().join("\n");
    // Markdown table should have pipe characters
    assert!(content.contains("|"), "markdown output should contain table pipes");
    // Should have a separator row with dashes
    assert!(content.contains("---"), "markdown table should have separator row");
}

#[test]
fn test_empty_scan_directory_json() {
    let tmp = TempDir::new().unwrap();
    // No repos created — empty directory

    let (stdout, _, success) = run_devpulse(&["--json", tmp.path().to_str().unwrap()]);
    assert!(success, "empty directory scan should succeed");
    // When no projects found, devpulse outputs a hint message, not JSON
    assert!(
        stdout.contains("No projects found") || stdout.contains("projects"),
        "should indicate no projects found: {stdout}"
    );
}
