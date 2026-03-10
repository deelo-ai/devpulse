use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Maximum allowed scan depth to prevent runaway recursion.
const MAX_DEPTH_CAP: u32 = 10;

/// Discover projects by looking for directories containing .git folders.
/// Only checks top-level entries (depth=1). Skips hidden directories.
pub fn discover_projects(dir: &Path) -> Result<Vec<PathBuf>> {
    discover_projects_filtered(dir, &[])
}

/// Discover projects, skipping directories whose names match any entry in `ignore`.
/// Uses the default depth of 1 (immediate children only).
pub fn discover_projects_filtered(dir: &Path, ignore: &[String]) -> Result<Vec<PathBuf>> {
    discover_projects_with_depth(dir, ignore, 1)
}

/// Discover projects up to `depth` levels deep.
///
/// - `depth = 0`: only check if `dir` itself is a git project
/// - `depth = 1`: current default behavior (immediate children)
/// - `depth = 2+`: recursive scanning up to N levels
///
/// Caps at `MAX_DEPTH_CAP` to prevent runaway recursion.
/// Once a .git directory is found, does not recurse into that project
/// (avoids scanning submodules/nested repos inside a project).
pub fn discover_projects_with_depth(
    dir: &Path,
    ignore: &[String],
    depth: u32,
) -> Result<Vec<PathBuf>> {
    let capped_depth = depth.min(MAX_DEPTH_CAP);
    let mut projects = Vec::new();
    scan_recursive(dir, ignore, capped_depth, &mut projects)?;
    projects.sort();
    Ok(projects)
}

/// Recursively scan directories for git projects.
fn scan_recursive(
    dir: &Path,
    ignore: &[String],
    remaining_depth: u32,
    projects: &mut Vec<PathBuf>,
) -> Result<()> {
    // Depth 0: check if this directory itself is a git project
    if remaining_depth == 0 {
        if dir.join(".git").exists() {
            projects.push(dir.to_path_buf());
        }
        return Ok(());
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            // Permission denied or other read errors — warn and skip
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("  Warning: permission denied: {}", dir.display());
                return Ok(());
            }
            return Err(e).with_context(|| format!("Failed to read directory: {}", dir.display()));
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        // Skip non-directories and symlinks (avoid loops)
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
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

        // Skip ignored directory names
        if ignore.iter().any(|ig| ig == &name) {
            continue;
        }

        // If this directory is a git project, add it and don't recurse deeper
        // (avoids scanning submodules or nested repos within a project)
        if path.join(".git").exists() {
            projects.push(path);
        } else if remaining_depth > 1 {
            // Not a git project — recurse deeper
            scan_recursive(&path, ignore, remaining_depth - 1, projects)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a fake git project at the given path.
    fn create_fake_project(base: &Path, name: &str) {
        let project = base.join(name);
        fs::create_dir_all(project.join(".git")).unwrap();
    }

    /// Helper: create a plain directory (not a git project).
    fn create_plain_dir(base: &Path, name: &str) {
        fs::create_dir_all(base.join(name)).unwrap();
    }

    #[test]
    fn test_depth_1_finds_immediate_children() {
        let dir = tempfile::tempdir().unwrap();
        create_fake_project(dir.path(), "alpha");
        create_fake_project(dir.path(), "beta");
        create_plain_dir(dir.path(), "not-a-repo");

        let projects = discover_projects_with_depth(dir.path(), &[], 1).unwrap();
        assert_eq!(projects.len(), 2);
        let names: Vec<_> = projects
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn test_depth_0_checks_dir_itself() {
        let dir = tempfile::tempdir().unwrap();
        // The temp dir itself is not a git project
        let projects = discover_projects_with_depth(dir.path(), &[], 0).unwrap();
        assert!(projects.is_empty());

        // Make it a git project
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let projects = discover_projects_with_depth(dir.path(), &[], 0).unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[test]
    fn test_depth_2_finds_nested_projects() {
        let dir = tempfile::tempdir().unwrap();
        // Top-level project
        create_fake_project(dir.path(), "top-repo");
        // Nested: group/nested-repo
        let group = dir.path().join("group");
        fs::create_dir_all(&group).unwrap();
        create_fake_project(&group, "nested-repo");

        // Depth 1 should only find top-repo
        let projects = discover_projects_with_depth(dir.path(), &[], 1).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].ends_with("top-repo"));

        // Depth 2 should find both
        let projects = discover_projects_with_depth(dir.path(), &[], 2).unwrap();
        assert_eq!(projects.len(), 2);
    }

    #[test]
    fn test_does_not_recurse_into_git_projects() {
        let dir = tempfile::tempdir().unwrap();
        // Create a project with a nested project inside (e.g. submodule)
        create_fake_project(dir.path(), "outer");
        let outer = dir.path().join("outer");
        create_fake_project(&outer, "inner-sub");

        // Even with depth=3, should only find "outer" (not recurse into it)
        let projects = discover_projects_with_depth(dir.path(), &[], 3).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].ends_with("outer"));
    }

    #[test]
    fn test_ignores_hidden_directories() {
        let dir = tempfile::tempdir().unwrap();
        create_fake_project(dir.path(), ".hidden-repo");
        create_fake_project(dir.path(), "visible-repo");

        let projects = discover_projects_with_depth(dir.path(), &[], 1).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].ends_with("visible-repo"));
    }

    #[test]
    fn test_ignores_specified_directories() {
        let dir = tempfile::tempdir().unwrap();
        create_fake_project(dir.path(), "keep-me");
        create_fake_project(dir.path(), "ignore-me");

        let ignore = vec!["ignore-me".to_string()];
        let projects = discover_projects_with_depth(dir.path(), &ignore, 1).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].ends_with("keep-me"));
    }

    #[test]
    fn test_ignore_applies_at_all_depth_levels() {
        let dir = tempfile::tempdir().unwrap();
        let group = dir.path().join("group");
        fs::create_dir_all(&group).unwrap();
        create_fake_project(&group, "good-project");
        create_fake_project(&group, "vendor");

        let ignore = vec!["vendor".to_string()];
        let projects = discover_projects_with_depth(dir.path(), &ignore, 2).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].ends_with("good-project"));
    }

    #[test]
    fn test_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let projects = discover_projects_with_depth(dir.path(), &[], 1).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_empty_directory_deep() {
        let dir = tempfile::tempdir().unwrap();
        let projects = discover_projects_with_depth(dir.path(), &[], 5).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_non_utf8_names_skipped() {
        // This test verifies the code doesn't panic on entries where
        // file_name().to_str() returns None. On most platforms, creating
        // truly non-UTF-8 names is hard, so we just verify the function
        // handles an empty dir without panicking.
        let dir = tempfile::tempdir().unwrap();
        let projects = discover_projects_with_depth(dir.path(), &[], 1).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_results_are_sorted() {
        let dir = tempfile::tempdir().unwrap();
        create_fake_project(dir.path(), "zebra");
        create_fake_project(dir.path(), "alpha");
        create_fake_project(dir.path(), "mango");

        let projects = discover_projects_with_depth(dir.path(), &[], 1).unwrap();
        let names: Vec<_> = projects
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["alpha", "mango", "zebra"]);
    }

    #[test]
    fn test_depth_capped_at_max() {
        let dir = tempfile::tempdir().unwrap();
        // Just verify it doesn't panic with absurd depth
        let projects = discover_projects_with_depth(dir.path(), &[], 999).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_nonexistent_directory_returns_error() {
        let result = discover_projects_with_depth(Path::new("/nonexistent/path/xyz"), &[], 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_projects_uses_depth_1() {
        let dir = tempfile::tempdir().unwrap();
        create_fake_project(dir.path(), "repo");
        let group = dir.path().join("group");
        fs::create_dir_all(&group).unwrap();
        create_fake_project(&group, "nested");

        // discover_projects (no depth arg) should only find depth-1
        let projects = discover_projects(dir.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].ends_with("repo"));
    }

    #[test]
    fn test_deep_nesting_depth_3() {
        let dir = tempfile::tempdir().unwrap();
        // level1/level2/project
        let l1 = dir.path().join("level1");
        let l2 = l1.join("level2");
        fs::create_dir_all(&l2).unwrap();
        create_fake_project(&l2, "deep-project");

        // Depth 2 shouldn't reach it (dir -> level1 -> level2, need depth 3 to check level2's children)
        let projects = discover_projects_with_depth(dir.path(), &[], 2).unwrap();
        assert!(projects.is_empty());

        // Depth 3 should find it
        let projects = discover_projects_with_depth(dir.path(), &[], 3).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(projects[0].ends_with("deep-project"));
    }
}
