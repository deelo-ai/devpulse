//! GitHub CI status integration.
//!
//! Queries the GitHub Actions API to fetch the latest workflow run status
//! for projects with GitHub remotes. Results are cached to avoid repeated
//! API calls in watch/TUI mode.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// CI status for a project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum CiStatus {
    /// CI passed (all checks green).
    Pass,
    /// CI failed.
    Fail,
    /// CI is currently running.
    Pending,
    /// No CI information available (no remote, not GitHub, API error).
    Unknown,
}

impl std::fmt::Display for CiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CiStatus::Pass => write!(f, "✅"),
            CiStatus::Fail => write!(f, "❌"),
            CiStatus::Pending => write!(f, "⏳"),
            CiStatus::Unknown => write!(f, "—"),
        }
    }
}

/// Cache entry for CI status lookups.
struct CacheEntry {
    status: CiStatus,
    fetched_at: Instant,
}

/// Thread-safe CI status cache.
/// Entries expire after `ttl` seconds.
pub struct CiCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
    ttl: Duration,
}

impl CiCache {
    /// Create a new cache with the given TTL in seconds.
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Get a cached status if it exists and hasn't expired.
    pub fn get(&self, key: &str) -> Option<CiStatus> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.fetched_at.elapsed() < self.ttl {
            Some(entry.status.clone())
        } else {
            None
        }
    }

    /// Insert or update a cache entry.
    pub fn set(&self, key: String, status: CiStatus) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(
                key,
                CacheEntry {
                    status,
                    fetched_at: Instant::now(),
                },
            );
        }
    }
}

/// Response from the GitHub Actions workflow runs API.
#[derive(Debug, Deserialize)]
struct WorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

/// A single workflow run from the API.
#[derive(Debug, Deserialize)]
struct WorkflowRun {
    /// "completed", "in_progress", "queued", etc.
    status: String,
    /// "success", "failure", "cancelled", "skipped", etc.
    /// Only present when status == "completed".
    conclusion: Option<String>,
}

/// Extract GitHub owner/repo from a normalized remote URL.
///
/// Expects URLs like `https://github.com/owner/repo`.
/// Returns `None` for non-GitHub URLs.
pub fn parse_github_repo(remote_url: &str) -> Option<(String, String)> {
    let url = remote_url.strip_prefix("https://github.com/")?;
    let parts: Vec<&str> = url.splitn(3, '/').collect();
    if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        // Strip any trailing path segments (e.g. /tree/main)
        let repo = parts[1].split('/').next().unwrap_or(parts[1]);
        Some((parts[0].to_string(), repo.to_string()))
    } else {
        None
    }
}

/// Fetch CI status for a single GitHub repository.
///
/// Queries the GitHub Actions API for the latest workflow run on the
/// default branch. Uses `GITHUB_TOKEN` env var for authentication
/// if available (higher rate limits).
pub fn fetch_ci_status(owner: &str, repo: &str, branch: &str) -> Result<CiStatus> {
    // Use a short timeout to avoid blocking the dashboard
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("Failed to build HTTP client")?;

    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs?branch={branch}&per_page=1"
    );

    let mut request = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "devpulse")
        .header("X-GitHub-Api-Version", "2022-11-28");

    // Use GITHUB_TOKEN if available for higher rate limits
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        request = request.header("Authorization", format!("Bearer {token}"));
    }

    let response = request.send().context("GitHub API request failed")?;

    if !response.status().is_success() {
        return Ok(CiStatus::Unknown);
    }

    let body: WorkflowRunsResponse = response
        .json()
        .context("Failed to parse GitHub API response")?;

    match body.workflow_runs.first() {
        None => Ok(CiStatus::Unknown),
        Some(run) => Ok(workflow_run_to_status(run)),
    }
}

/// Convert a workflow run to a CiStatus.
fn workflow_run_to_status(run: &WorkflowRun) -> CiStatus {
    match run.status.as_str() {
        "completed" => match run.conclusion.as_deref() {
            Some("success") => CiStatus::Pass,
            Some("skipped") => CiStatus::Pass,
            Some("failure") | Some("timed_out") | Some("cancelled") => CiStatus::Fail,
            _ => CiStatus::Unknown,
        },
        "in_progress" | "queued" | "waiting" | "requested" | "pending" => CiStatus::Pending,
        _ => CiStatus::Unknown,
    }
}

/// Fetch CI statuses for multiple projects, using the cache.
///
/// Only queries GitHub for projects that have a GitHub remote URL.
/// Returns a map from project name to CI status.
pub fn fetch_ci_statuses(
    projects: &[crate::types::ProjectStatus],
    cache: &CiCache,
) -> HashMap<String, CiStatus> {
    let mut results = HashMap::new();

    for project in projects {
        let remote = match &project.remote_url {
            Some(url) => url,
            None => {
                results.insert(project.name.clone(), CiStatus::Unknown);
                continue;
            }
        };

        // Check cache first
        if let Some(cached) = cache.get(remote) {
            results.insert(project.name.clone(), cached);
            continue;
        }

        let status = match parse_github_repo(remote) {
            Some((owner, repo)) => {
                fetch_ci_status(&owner, &repo, &project.branch).unwrap_or(CiStatus::Unknown)
            }
            None => CiStatus::Unknown,
        };

        cache.set(remote.clone(), status.clone());
        results.insert(project.name.clone(), status);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_github_repo tests ---

    #[test]
    fn test_parse_github_repo_standard() {
        let result = parse_github_repo("https://github.com/deelo-ai/devpulse");
        assert_eq!(
            result,
            Some(("deelo-ai".to_string(), "devpulse".to_string()))
        );
    }

    #[test]
    fn test_parse_github_repo_with_trailing_path() {
        let result = parse_github_repo("https://github.com/user/repo/tree/main");
        assert_eq!(result, Some(("user".to_string(), "repo".to_string())));
    }

    #[test]
    fn test_parse_github_repo_non_github() {
        let result = parse_github_repo("https://gitlab.com/user/repo");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_github_repo_empty_parts() {
        let result = parse_github_repo("https://github.com/");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_github_repo_only_owner() {
        let result = parse_github_repo("https://github.com/owner");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_github_repo_plain_http() {
        // We only support https://github.com
        let result = parse_github_repo("http://github.com/user/repo");
        assert_eq!(result, None);
    }

    // --- workflow_run_to_status tests ---

    #[test]
    fn test_status_success() {
        let run = WorkflowRun {
            status: "completed".to_string(),
            conclusion: Some("success".to_string()),
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Pass);
    }

    #[test]
    fn test_status_failure() {
        let run = WorkflowRun {
            status: "completed".to_string(),
            conclusion: Some("failure".to_string()),
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Fail);
    }

    #[test]
    fn test_status_cancelled() {
        let run = WorkflowRun {
            status: "completed".to_string(),
            conclusion: Some("cancelled".to_string()),
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Fail);
    }

    #[test]
    fn test_status_timed_out() {
        let run = WorkflowRun {
            status: "completed".to_string(),
            conclusion: Some("timed_out".to_string()),
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Fail);
    }

    #[test]
    fn test_status_skipped() {
        let run = WorkflowRun {
            status: "completed".to_string(),
            conclusion: Some("skipped".to_string()),
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Pass);
    }

    #[test]
    fn test_status_in_progress() {
        let run = WorkflowRun {
            status: "in_progress".to_string(),
            conclusion: None,
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Pending);
    }

    #[test]
    fn test_status_queued() {
        let run = WorkflowRun {
            status: "queued".to_string(),
            conclusion: None,
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Pending);
    }

    #[test]
    fn test_status_waiting() {
        let run = WorkflowRun {
            status: "waiting".to_string(),
            conclusion: None,
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Pending);
    }

    #[test]
    fn test_status_unknown_status() {
        let run = WorkflowRun {
            status: "something_new".to_string(),
            conclusion: None,
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Unknown);
    }

    #[test]
    fn test_status_completed_unknown_conclusion() {
        let run = WorkflowRun {
            status: "completed".to_string(),
            conclusion: Some("neutral".to_string()),
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Unknown);
    }

    #[test]
    fn test_status_completed_no_conclusion() {
        let run = WorkflowRun {
            status: "completed".to_string(),
            conclusion: None,
        };
        assert_eq!(workflow_run_to_status(&run), CiStatus::Unknown);
    }

    // --- CiStatus Display tests ---

    #[test]
    fn test_display_pass() {
        assert_eq!(format!("{}", CiStatus::Pass), "✅");
    }

    #[test]
    fn test_display_fail() {
        assert_eq!(format!("{}", CiStatus::Fail), "❌");
    }

    #[test]
    fn test_display_pending() {
        assert_eq!(format!("{}", CiStatus::Pending), "⏳");
    }

    #[test]
    fn test_display_unknown() {
        assert_eq!(format!("{}", CiStatus::Unknown), "—");
    }

    // --- CiCache tests ---

    #[test]
    fn test_cache_miss() {
        let cache = CiCache::new(300);
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn test_cache_hit() {
        let cache = CiCache::new(300);
        cache.set("key".to_string(), CiStatus::Pass);
        assert_eq!(cache.get("key"), Some(CiStatus::Pass));
    }

    #[test]
    fn test_cache_overwrite() {
        let cache = CiCache::new(300);
        cache.set("key".to_string(), CiStatus::Pass);
        cache.set("key".to_string(), CiStatus::Fail);
        assert_eq!(cache.get("key"), Some(CiStatus::Fail));
    }

    #[test]
    fn test_cache_expired() {
        let cache = CiCache::new(0); // 0 second TTL = instant expiry
        cache.set("key".to_string(), CiStatus::Pass);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(cache.get("key"), None);
    }

    #[test]
    fn test_cache_multiple_keys() {
        let cache = CiCache::new(300);
        cache.set("a".to_string(), CiStatus::Pass);
        cache.set("b".to_string(), CiStatus::Fail);
        cache.set("c".to_string(), CiStatus::Pending);
        assert_eq!(cache.get("a"), Some(CiStatus::Pass));
        assert_eq!(cache.get("b"), Some(CiStatus::Fail));
        assert_eq!(cache.get("c"), Some(CiStatus::Pending));
    }
}
