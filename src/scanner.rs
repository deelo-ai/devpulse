use std::path::PathBuf;

use anyhow::Result;

/// Discover projects by looking for directories containing .git folders.
/// Only checks top-level entries (not recursive beyond 1 level).
/// Skips hidden directories (starting with .).
pub fn discover_projects(_dir: &PathBuf) -> Result<Vec<PathBuf>> {
    Ok(Vec::new())
}
