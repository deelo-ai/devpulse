use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use git2::{Repository, StatusOptions};

use crate::types::ProjectStatus;

/// Gather git status information for a project at the given path.
pub fn get_project_status(path: &Path) -> Result<ProjectStatus> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let repo = Repository::open(path)
        .with_context(|| format!("Failed to open git repo at {}", path.display()))?;

    let branch = get_branch_name(&repo);
    let (changed_files, is_clean) = get_dirty_count(&repo)?;
    let last_commit = get_last_commit_time(&repo)?;
    let (ahead, behind) = get_ahead_behind(&repo);
    let remote_url = get_remote_url(&repo);
    let stash_count = get_stash_count(&repo);
    let last_commit_message = get_last_commit_message(&repo);

    Ok(ProjectStatus {
        name,
        path: path.to_path_buf(),
        branch,
        is_clean,
        changed_files,
        last_commit,
        ahead,
        behind,
        remote_url,
        stash_count,
        last_commit_message,
    })
}

/// Get the current branch name, or "HEAD (detached)" if detached.
fn get_branch_name(repo: &Repository) -> String {
    if let Ok(head) = repo.head()
        && let Some(name) = head.shorthand()
    {
        return name.to_string();
    }
    "HEAD (detached)".to_string()
}

/// Count the number of dirty (modified, new, deleted) files.
fn get_dirty_count(repo: &Repository) -> Result<(usize, bool)> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("Failed to get repo status")?;

    let count = statuses.len();
    Ok((count, count == 0))
}

/// Get the timestamp of the last commit (HEAD).
fn get_last_commit_time(repo: &Repository) -> Result<Option<DateTime<Utc>>> {
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Ok(None), // empty repo, no commits
    };

    let commit = head
        .peel_to_commit()
        .context("Failed to peel HEAD to commit")?;

    let time = commit.time();
    let dt = Utc.timestamp_opt(time.seconds(), 0).single();
    Ok(dt)
}

/// Get the remote URL for the "origin" remote, converting SSH URLs to HTTPS.
fn get_remote_url(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    let url = remote.url()?.to_string();
    Some(normalize_remote_url(&url))
}

/// Normalize a git remote URL to an HTTPS browser URL.
/// Converts `git@github.com:user/repo.git` → `https://github.com/user/repo`
pub fn normalize_remote_url(url: &str) -> String {
    let mut url = url.to_string();

    // Convert SSH format: git@host:user/repo.git → https://host/user/repo
    if url.starts_with("git@") {
        url = url.replacen("git@", "https://", 1);
        if let Some(colon_pos) = url.find(':') {
            // Only replace if it's the host:path separator (not in https://)
            let after_scheme = &url["https://".len()..];
            if let Some(rel_pos) = after_scheme.find(':') {
                let abs_pos = "https://".len() + rel_pos;
                url.replace_range(abs_pos..abs_pos + 1, "/");
            } else {
                url.replace_range(colon_pos..colon_pos + 1, "/");
            }
        }
    }

    // Strip trailing .git
    if url.ends_with(".git") {
        url.truncate(url.len() - 4);
    }

    url
}

/// Count the number of stash entries in the repository.
fn get_stash_count(repo: &Repository) -> usize {
    // git2's stash_foreach requires a mutable repo reference,
    // so we re-open to avoid borrow issues with the caller.
    let path = repo.workdir().or_else(|| Some(repo.path()));
    let repo_path = match path {
        Some(p) => p.to_path_buf(),
        None => return 0,
    };

    let mut repo = match Repository::open(&repo_path) {
        Ok(r) => r,
        Err(_) => return 0,
    };

    let mut count: usize = 0;
    // stash_foreach calls the callback for each stash entry
    let _ = repo.stash_foreach(|_index, _message, _oid| {
        count += 1;
        true // continue iterating
    });
    count
}

/// Get the subject line (first line) of the last commit message.
/// Returns `None` for empty repos with no commits.
fn get_last_commit_message(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let message = commit.message()?;
    // Take only the first line (subject)
    let subject = message.lines().next().unwrap_or("").trim().to_string();
    if subject.is_empty() {
        None
    } else {
        Some(subject)
    }
}

/// Truncate a string to `max_len` characters, appending "…" if truncated.
/// Operates on char boundaries to avoid splitting multi-byte characters.
pub fn truncate_message(msg: &str, max_len: usize) -> String {
    if msg.chars().count() <= max_len {
        msg.to_string()
    } else {
        let truncated: String = msg.chars().take(max_len.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

/// Get ahead/behind counts relative to upstream tracking branch.
fn get_ahead_behind(repo: &Repository) -> (usize, usize) {
    let result = (|| -> Result<(usize, usize)> {
        let head = repo.head()?;
        let local_oid = head.target().context("HEAD has no target")?;

        let branch_name = head.shorthand().context("No branch name")?;

        let local_branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
        let upstream = local_branch.upstream()?;
        let upstream_oid = upstream.get().target().context("Upstream has no target")?;

        let (ahead, behind) = repo.graph_ahead_behind(local_oid, upstream_oid)?;
        Ok((ahead, behind))
    })();

    result.unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Helper: create a temporary git repo with an initial commit.
    fn setup_temp_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let repo = Repository::init(dir.path()).expect("Failed to init repo");

        // Configure user for commits
        let mut config = repo.config().expect("Failed to get config");
        config
            .set_str("user.name", "Test User")
            .expect("Failed to set user.name");
        config
            .set_str("user.email", "test@example.com")
            .expect("Failed to set user.email");

        // Create an initial commit so HEAD exists
        {
            let sig = repo.signature().expect("Failed to create signature");
            let tree_id = {
                let mut index = repo.index().expect("Failed to get index");
                index.write_tree().expect("Failed to write tree")
            };
            let tree = repo.find_tree(tree_id).expect("Failed to find tree");
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .expect("Failed to create initial commit");
        }

        (dir, repo)
    }

    #[test]
    fn test_stash_count_empty_repo() {
        let (_dir, repo) = setup_temp_repo();
        assert_eq!(get_stash_count(&repo), 0);
    }

    #[test]
    fn test_stash_count_with_stashes() {
        let (dir, _repo) = setup_temp_repo();

        // Create a file, add it, then stash
        std::fs::write(dir.path().join("file.txt"), "hello").expect("write failed");
        let status = Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git add failed");
        assert!(status.success());

        let status = Command::new("git")
            .args(["stash", "push", "-m", "first stash"])
            .current_dir(dir.path())
            .status()
            .expect("git stash failed");
        assert!(status.success());

        // Create another file and stash again
        std::fs::write(dir.path().join("file2.txt"), "world").expect("write failed");
        let status = Command::new("git")
            .args(["add", "file2.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git add failed");
        assert!(status.success());

        let status = Command::new("git")
            .args(["stash", "push", "-m", "second stash"])
            .current_dir(dir.path())
            .status()
            .expect("git stash failed");
        assert!(status.success());

        let repo = Repository::open(dir.path()).expect("Failed to open repo");
        assert_eq!(get_stash_count(&repo), 2);
    }

    #[test]
    fn test_stash_count_after_pop() {
        let (dir, _repo) = setup_temp_repo();

        // Create, add, stash
        std::fs::write(dir.path().join("file.txt"), "data").expect("write failed");
        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git add failed");
        Command::new("git")
            .args(["stash", "push", "-m", "to pop"])
            .current_dir(dir.path())
            .status()
            .expect("git stash failed");

        // Pop it
        Command::new("git")
            .args(["stash", "pop"])
            .current_dir(dir.path())
            .status()
            .expect("git stash pop failed");

        let repo = Repository::open(dir.path()).expect("Failed to open repo");
        assert_eq!(get_stash_count(&repo), 0);
    }

    #[test]
    fn test_normalize_ssh_url() {
        assert_eq!(
            normalize_remote_url("git@github.com:user/repo.git"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_https_url_with_git_suffix() {
        assert_eq!(
            normalize_remote_url("https://github.com/user/repo.git"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_https_url_without_git_suffix() {
        assert_eq!(
            normalize_remote_url("https://github.com/user/repo"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_ssh_url_gitlab() {
        assert_eq!(
            normalize_remote_url("git@gitlab.com:org/project.git"),
            "https://gitlab.com/org/project"
        );
    }

    #[test]
    fn test_normalize_plain_url() {
        assert_eq!(
            normalize_remote_url("https://example.com/repo"),
            "https://example.com/repo"
        );
    }

    // --- last commit message tests ---

    #[test]
    fn test_last_commit_message_from_initial_commit() {
        let (_dir, repo) = setup_temp_repo();
        let msg = get_last_commit_message(&repo);
        assert_eq!(msg, Some("Initial commit".to_string()));
    }

    #[test]
    fn test_last_commit_message_custom() {
        let (dir, _repo) = setup_temp_repo();
        // Make a second commit with a custom message
        std::fs::write(dir.path().join("file.txt"), "content").expect("write failed");
        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git add failed");
        Command::new("git")
            .args(["commit", "-m", "feat: add awesome feature"])
            .current_dir(dir.path())
            .status()
            .expect("git commit failed");

        let repo = Repository::open(dir.path()).expect("reopen");
        let msg = get_last_commit_message(&repo);
        assert_eq!(msg, Some("feat: add awesome feature".to_string()));
    }

    #[test]
    fn test_last_commit_message_multiline_takes_first_line() {
        let (dir, _repo) = setup_temp_repo();
        std::fs::write(dir.path().join("file.txt"), "data").expect("write failed");
        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git add failed");
        Command::new("git")
            .args(["commit", "-m", "Subject line\n\nBody paragraph here."])
            .current_dir(dir.path())
            .status()
            .expect("git commit failed");

        let repo = Repository::open(dir.path()).expect("reopen");
        let msg = get_last_commit_message(&repo);
        assert_eq!(msg, Some("Subject line".to_string()));
    }

    #[test]
    fn test_last_commit_message_empty_repo() {
        let dir = TempDir::new().expect("tmpdir");
        let repo = Repository::init(dir.path()).expect("init");
        // No commits — should return None
        let msg = get_last_commit_message(&repo);
        assert_eq!(msg, None);
    }

    // --- truncate_message tests ---

    #[test]
    fn test_truncate_short_message() {
        assert_eq!(truncate_message("hello", 50), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        let msg = "a".repeat(50);
        assert_eq!(truncate_message(&msg, 50), msg);
    }

    #[test]
    fn test_truncate_long_message() {
        let msg = "a".repeat(60);
        let result = truncate_message(&msg, 50);
        assert_eq!(result.chars().count(), 50);
        assert!(result.ends_with('…'));
        assert_eq!(&result[..49], &"a".repeat(49));
    }

    #[test]
    fn test_truncate_unicode() {
        // Each emoji is one char
        let msg = "🎉".repeat(55);
        let result = truncate_message(&msg, 50);
        assert_eq!(result.chars().count(), 50);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_empty_string() {
        assert_eq!(truncate_message("", 50), "");
    }

    #[test]
    fn test_truncate_max_len_1() {
        assert_eq!(truncate_message("hello", 1), "…");
    }

    #[test]
    fn test_project_status_includes_message() {
        let (dir, _repo) = setup_temp_repo();
        let status = get_project_status(dir.path()).unwrap();
        assert_eq!(
            status.last_commit_message,
            Some("Initial commit".to_string())
        );
    }
}
