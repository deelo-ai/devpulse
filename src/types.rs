use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// Status information for a single project
#[derive(Serialize)]
pub struct ProjectStatus {
    /// Project name (directory name)
    pub name: String,
    /// Path to the project root
    #[allow(dead_code)]
    pub path: PathBuf,
    /// Current branch name
    pub branch: String,
    /// Whether the working tree is clean
    pub is_clean: bool,
    /// Number of changed (dirty) files
    pub changed_files: usize,
    /// Timestamp of the last commit
    pub last_commit: Option<DateTime<Utc>>,
    /// Commits ahead of upstream
    pub ahead: usize,
    /// Commits behind upstream
    pub behind: usize,
    /// Remote URL (e.g. GitHub URL) if available
    pub remote_url: Option<String>,
}
