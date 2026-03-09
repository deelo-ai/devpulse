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

    Ok(ProjectStatus {
        name,
        path: path.to_path_buf(),
        branch,
        is_clean,
        changed_files,
        last_commit,
        ahead,
        behind,
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
