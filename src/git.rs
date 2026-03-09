use std::path::PathBuf;

use anyhow::Result;

use crate::types::ProjectStatus;

/// Gather git status information for a project at the given path.
pub fn get_project_status(_path: &PathBuf) -> Result<ProjectStatus> {
    anyhow::bail!("not yet implemented")
}
