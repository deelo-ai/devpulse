use chrono::Utc;

use crate::types::ProjectStatus;

/// Threshold in days for considering a project "stale".
const STALE_THRESHOLD_DAYS: i64 = 30;

/// A single filter criterion for projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectFilter {
    /// Only show dirty projects (uncommitted changes).
    Dirty,
    /// Only show clean projects.
    Clean,
    /// Only show stale projects (no commits in >30 days).
    Stale,
    /// Only show projects with unpushed commits.
    Unpushed,
    /// Only show projects whose name contains the given substring (case-insensitive).
    Name(String),
}

/// Parse a single filter expression string into a `ProjectFilter`.
///
/// Recognized formats:
/// - `"dirty"` → `ProjectFilter::Dirty`
/// - `"clean"` → `ProjectFilter::Clean`
/// - `"stale"` → `ProjectFilter::Stale`
/// - `"unpushed"` → `ProjectFilter::Unpushed`
/// - `"name:foo"` → `ProjectFilter::Name("foo")`
///
/// Returns `None` for unrecognized expressions.
pub fn parse_filter(expr: &str) -> Option<ProjectFilter> {
    let trimmed = expr.trim();
    match trimmed.to_lowercase().as_str() {
        "dirty" => Some(ProjectFilter::Dirty),
        "clean" => Some(ProjectFilter::Clean),
        "stale" => Some(ProjectFilter::Stale),
        "unpushed" => Some(ProjectFilter::Unpushed),
        other => {
            if let Some(name) = other.strip_prefix("name:") {
                let name = name.trim();
                if name.is_empty() {
                    None
                } else {
                    Some(ProjectFilter::Name(name.to_string()))
                }
            } else {
                None
            }
        }
    }
}

/// Check whether a single project matches a single filter.
pub fn matches_filter(project: &ProjectStatus, filter: &ProjectFilter) -> bool {
    match filter {
        ProjectFilter::Dirty => !project.is_clean,
        ProjectFilter::Clean => project.is_clean,
        ProjectFilter::Stale => {
            let now = Utc::now();
            project
                .last_commit
                .map(|dt| (now - dt).num_days() > STALE_THRESHOLD_DAYS)
                .unwrap_or(true)
        }
        ProjectFilter::Unpushed => project.ahead > 0,
        ProjectFilter::Name(pattern) => project.name.to_lowercase().contains(pattern),
    }
}

/// Apply all filters to a list of projects. A project must match ALL filters (AND logic).
pub fn apply_filters(
    statuses: Vec<ProjectStatus>,
    filters: &[ProjectFilter],
) -> Vec<ProjectStatus> {
    if filters.is_empty() {
        return statuses;
    }
    statuses
        .into_iter()
        .filter(|s| filters.iter().all(|f| matches_filter(s, f)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProjectStatus;
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    fn make_project(
        name: &str,
        is_clean: bool,
        days_ago: Option<i64>,
        ahead: usize,
    ) -> ProjectStatus {
        ProjectStatus {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{}", name)),
            branch: "main".to_string(),
            is_clean,
            changed_files: if is_clean { 0 } else { 2 },
            last_commit: days_ago.map(|d| Utc::now() - Duration::days(d)),
            ahead,
            behind: 0,
            remote_url: None,
            stash_count: 0,
            last_commit_message: None,
        }
    }

    // --- parse_filter tests ---

    #[test]
    fn test_parse_dirty() {
        assert_eq!(parse_filter("dirty"), Some(ProjectFilter::Dirty));
        assert_eq!(parse_filter("DIRTY"), Some(ProjectFilter::Dirty));
        assert_eq!(parse_filter("  Dirty  "), Some(ProjectFilter::Dirty));
    }

    #[test]
    fn test_parse_clean() {
        assert_eq!(parse_filter("clean"), Some(ProjectFilter::Clean));
        assert_eq!(parse_filter("CLEAN"), Some(ProjectFilter::Clean));
    }

    #[test]
    fn test_parse_stale() {
        assert_eq!(parse_filter("stale"), Some(ProjectFilter::Stale));
    }

    #[test]
    fn test_parse_unpushed() {
        assert_eq!(parse_filter("unpushed"), Some(ProjectFilter::Unpushed));
    }

    #[test]
    fn test_parse_name_filter() {
        assert_eq!(
            parse_filter("name:foo"),
            Some(ProjectFilter::Name("foo".to_string()))
        );
        assert_eq!(
            parse_filter("NAME:Bar"),
            Some(ProjectFilter::Name("bar".to_string()))
        );
        assert_eq!(
            parse_filter("name:  hello  "),
            Some(ProjectFilter::Name("hello".to_string()))
        );
    }

    #[test]
    fn test_parse_name_empty_value() {
        assert_eq!(parse_filter("name:"), None);
        assert_eq!(parse_filter("name:   "), None);
    }

    #[test]
    fn test_parse_unknown() {
        assert_eq!(parse_filter("unknown"), None);
        assert_eq!(parse_filter(""), None);
        assert_eq!(parse_filter("foo:bar"), None);
    }

    // --- matches_filter tests ---

    #[test]
    fn test_match_dirty() {
        let dirty = make_project("a", false, Some(1), 0);
        let clean = make_project("b", true, Some(1), 0);
        assert!(matches_filter(&dirty, &ProjectFilter::Dirty));
        assert!(!matches_filter(&clean, &ProjectFilter::Dirty));
    }

    #[test]
    fn test_match_clean() {
        let dirty = make_project("a", false, Some(1), 0);
        let clean = make_project("b", true, Some(1), 0);
        assert!(!matches_filter(&dirty, &ProjectFilter::Clean));
        assert!(matches_filter(&clean, &ProjectFilter::Clean));
    }

    #[test]
    fn test_match_stale() {
        let stale = make_project("old", true, Some(60), 0);
        let fresh = make_project("new", true, Some(5), 0);
        let no_commits = make_project("empty", true, None, 0);
        assert!(matches_filter(&stale, &ProjectFilter::Stale));
        assert!(!matches_filter(&fresh, &ProjectFilter::Stale));
        assert!(matches_filter(&no_commits, &ProjectFilter::Stale));
    }

    #[test]
    fn test_match_unpushed() {
        let ahead = make_project("a", true, Some(1), 3);
        let synced = make_project("b", true, Some(1), 0);
        assert!(matches_filter(&ahead, &ProjectFilter::Unpushed));
        assert!(!matches_filter(&synced, &ProjectFilter::Unpushed));
    }

    #[test]
    fn test_match_name() {
        let proj = make_project("my-cool-project", true, Some(1), 0);
        assert!(matches_filter(
            &proj,
            &ProjectFilter::Name("cool".to_string())
        ));
        assert!(matches_filter(
            &proj,
            &ProjectFilter::Name("my-cool".to_string())
        ));
        assert!(!matches_filter(
            &proj,
            &ProjectFilter::Name("awesome".to_string())
        ));
    }

    #[test]
    fn test_match_name_case_insensitive() {
        let proj = make_project("MyProject", true, Some(1), 0);
        // Name filter stores lowercase, project name lowercased in matches_filter
        assert!(matches_filter(
            &proj,
            &ProjectFilter::Name("myproject".to_string())
        ));
    }

    // --- apply_filters tests ---

    #[test]
    fn test_apply_no_filters() {
        let statuses = vec![
            make_project("a", true, Some(1), 0),
            make_project("b", false, Some(1), 0),
        ];
        let result = apply_filters(statuses, &[]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_single_filter() {
        let statuses = vec![
            make_project("a", true, Some(1), 0),
            make_project("b", false, Some(1), 0),
            make_project("c", false, Some(1), 0),
        ];
        let result = apply_filters(statuses, &[ProjectFilter::Dirty]);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|s| !s.is_clean));
    }

    #[test]
    fn test_apply_multiple_filters_and_logic() {
        let statuses = vec![
            make_project("alpha", false, Some(1), 3), // dirty + unpushed
            make_project("beta", false, Some(1), 0),  // dirty only
            make_project("gamma", true, Some(1), 3),  // unpushed only
            make_project("delta", true, Some(1), 0),  // neither
        ];
        let result = apply_filters(statuses, &[ProjectFilter::Dirty, ProjectFilter::Unpushed]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "alpha");
    }

    #[test]
    fn test_apply_filters_empty_input() {
        let statuses: Vec<ProjectStatus> = vec![];
        let result = apply_filters(statuses, &[ProjectFilter::Dirty]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_filters_no_matches() {
        let statuses = vec![
            make_project("a", true, Some(1), 0),
            make_project("b", true, Some(1), 0),
        ];
        let result = apply_filters(statuses, &[ProjectFilter::Dirty]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_combined_name_and_status_filter() {
        let statuses = vec![
            make_project("api-server", false, Some(1), 0),
            make_project("api-client", true, Some(1), 0),
            make_project("web-app", false, Some(1), 0),
        ];
        let result = apply_filters(
            statuses,
            &[ProjectFilter::Dirty, ProjectFilter::Name("api".to_string())],
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "api-server");
    }
}
