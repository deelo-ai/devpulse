use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Discover projects by looking for directories containing .git folders.
/// Only checks top-level entries (not recursive beyond 1 level).
/// Skips hidden directories (starting with .).
pub fn discover_projects(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();

    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip non-directories
        if !path.is_dir() {
            continue;
        }

        // Skip hidden directories (starting with .)
        let name = match entry.file_name().to_str() {
            Some(n) => n.to_string(),
            None => continue,
        };
        if name.starts_with('.') {
            continue;
        }

        // Check if this directory contains a .git subdirectory
        if path.join(".git").exists() {
            projects.push(path);
        }
    }

    // Sort alphabetically by directory name
    projects.sort();

    Ok(projects)
}
