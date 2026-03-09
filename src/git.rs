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
}
