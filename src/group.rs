use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::summary::Summary;
use crate::types::ProjectStatus;

/// A group of projects sharing the same parent directory.
#[derive(Debug, Serialize)]
pub struct ProjectGroup {
    /// Display name for this group (the parent directory path).
    pub label: String,
    /// Projects in this group.
    pub projects: Vec<ProjectStatus>,
    /// Summary statistics for this group.
    pub summary: Summary,
}

/// Group projects by their immediate parent directory.
///
/// Each project's path is inspected to determine its parent directory.
/// Projects sharing the same parent are collected into a single group.
/// Groups are sorted alphabetically by their parent path label.
pub fn group_by_parent(statuses: Vec<ProjectStatus>) -> Vec<ProjectGroup> {
    let mut buckets: BTreeMap<String, Vec<ProjectStatus>> = BTreeMap::new();

    for status in statuses {
        let parent_label = status
            .path
            .parent()
            .map(normalize_label)
            .unwrap_or_else(|| "/".to_string());

        buckets.entry(parent_label).or_default().push(status);
    }

    buckets
        .into_iter()
        .map(|(label, projects)| {
            let summary = Summary::from_statuses(&projects);
            ProjectGroup {
                label,
                projects,
                summary,
            }
        })
        .collect()
}

/// Normalize a parent path into a display label.
/// Replaces the home directory prefix with `~` for readability.
fn normalize_label(path: &Path) -> String {
    let display = path.display().to_string();

    if let Some(home) = home_dir()
        && let Some(rest) = display.strip_prefix(&home)
    {
        if rest.is_empty() {
            return "~".to_string();
        }
        return format!("~{rest}");
    }

    display
}

/// Get the home directory path as a string, if available.
fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| dirs::home_dir().map(|p| p.display().to_string()))
}

/// Format grouped output as JSON.
///
/// Produces: `{"groups": {"path": {"projects": [...], "summary": {...}}}}`
pub fn format_grouped_json(groups: &[ProjectGroup]) -> anyhow::Result<String> {
    let mut group_map = serde_json::Map::new();

    for group in groups {
        let value = serde_json::json!({
            "projects": group.projects,
            "summary": group.summary,
        });
        group_map.insert(group.label.clone(), value);
    }

    let overall_projects: Vec<&ProjectStatus> =
        groups.iter().flat_map(|g| g.projects.iter()).collect();
    let overall_summary = Summary::from_statuses(
        &overall_projects
            .iter()
            .map(|p| (*p).clone())
            .collect::<Vec<_>>(),
    );

    let output = serde_json::json!({
        "groups": group_map,
        "summary": overall_summary,
    });

    serde_json::to_string_pretty(&output).map_err(|e| anyhow::anyhow!("JSON error: {e}"))
}

/// Format grouped output as CSV.
///
/// Adds a "Group" column as the first field.
pub fn format_grouped_csv(groups: &[ProjectGroup]) -> anyhow::Result<String> {
    let mut out = String::new();
    out.push_str("Group,Project,Branch,Status,Changed,Last Commit,Ahead,Behind,Stash,Remote URL,Last Commit Message\n");

    for group in groups {
        for s in &group.projects {
            let status_str = if s.is_clean { "clean" } else { "dirty" };
            let last_commit_str = match s.last_commit {
                Some(dt) => dt.to_rfc3339(),
                None => String::new(),
            };
            let remote = s.remote_url.as_deref().unwrap_or("");
            let message = s.last_commit_message.as_deref().unwrap_or("");

            let row = [
                csv_escape(&group.label),
                csv_escape(&s.name),
                csv_escape(&s.branch),
                status_str.to_string(),
                s.changed_files.to_string(),
                last_commit_str,
                s.ahead.to_string(),
                s.behind.to_string(),
                s.stash_count.to_string(),
                csv_escape(remote),
                csv_escape(message),
            ];
            out.push_str(&row.join(","));
            out.push('\n');
        }
    }

    Ok(out)
}

/// Format grouped output as Markdown with sub-headers per group.
pub fn format_grouped_markdown(groups: &[ProjectGroup]) -> anyhow::Result<String> {
    let mut out = String::new();
    let now = chrono::Utc::now();

    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("### {}\n\n", group.label));
        out.push_str(
            "| Project | Branch | Status | Changed | Last Commit | Ahead/Behind | Stash | Message |\n",
        );
        out.push_str(
            "|---------|--------|--------|--------:|-------------|-------------:|------:|---------|\n",
        );

        for s in &group.projects {
            let status_str = if s.is_clean {
                "✅ clean"
            } else {
                "⚠️ dirty"
            };
            let last_commit_str = match s.last_commit {
                Some(dt) => format_relative_time(now, dt),
                None => "no commits".to_string(),
            };
            let ahead_behind = if s.ahead == 0 && s.behind == 0 {
                "—".to_string()
            } else {
                format!("↑{} ↓{}", s.ahead, s.behind)
            };
            let stash = if s.stash_count == 0 {
                "—".to_string()
            } else {
                s.stash_count.to_string()
            };
            let message = match &s.last_commit_message {
                Some(msg) => crate::git::truncate_message(msg, 50),
                None => "—".to_string(),
            };

            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                md_escape(&s.name),
                md_escape(&s.branch),
                status_str,
                s.changed_files,
                last_commit_str,
                ahead_behind,
                stash,
                md_escape(&message),
            ));
        }

        out.push_str(&format!(
            "\n**{}** projects · **{}** dirty · **{}** stale · **{}** unpushed\n",
            group.summary.total, group.summary.dirty, group.summary.stale, group.summary.unpushed,
        ));
    }

    // Overall summary if multiple groups
    if groups.len() > 1 {
        let all_projects: Vec<&ProjectStatus> =
            groups.iter().flat_map(|g| g.projects.iter()).collect();
        let total = all_projects.len();
        let dirty = all_projects.iter().filter(|s| !s.is_clean).count();
        out.push_str(&format!(
            "\n---\n\n**Overall:** {} projects · {} dirty\n",
            total, dirty,
        ));
    }

    Ok(out)
}

/// Escape a CSV field.
fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Escape pipe characters for markdown tables.
fn md_escape(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Format relative time (duplicated from export.rs to keep module self-contained).
fn format_relative_time(
    now: chrono::DateTime<chrono::Utc>,
    then: chrono::DateTime<chrono::Utc>,
) -> String {
    let duration = now - then;
    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{}m ago", minutes)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else if days < 30 {
        format!("{}d ago", days)
    } else if days < 365 {
        format!("{}mo ago", days / 30)
    } else {
        format!("{}y ago", days / 365)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProjectStatus;
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    fn make_project_at(name: &str, parent: &str, is_clean: bool) -> ProjectStatus {
        ProjectStatus {
            name: name.to_string(),
            path: PathBuf::from(parent).join(name),
            branch: "main".to_string(),
            is_clean,
            changed_files: if is_clean { 0 } else { 2 },
            last_commit: Some(Utc::now() - Duration::days(3)),
            ahead: 0,
            behind: 0,
            remote_url: None,
            stash_count: 0,
            last_commit_message: Some("initial commit".to_string()),
            ci_status: crate::ci::CiStatus::Unknown,
        }
    }

    #[test]
    fn test_group_by_parent_single_group() {
        let statuses = vec![
            make_project_at("alpha", "/tmp/projects", true),
            make_project_at("beta", "/tmp/projects", false),
        ];

        let groups = group_by_parent(statuses);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, "/tmp/projects");
        assert_eq!(groups[0].projects.len(), 2);
        assert_eq!(groups[0].summary.total, 2);
        assert_eq!(groups[0].summary.dirty, 1);
    }

    #[test]
    fn test_group_by_parent_multiple_groups() {
        let statuses = vec![
            make_project_at("app1", "/home/user/projects", true),
            make_project_at("app2", "/home/user/projects", true),
            make_project_at("lib1", "/home/user/work", false),
        ];

        let groups = group_by_parent(statuses);
        assert_eq!(groups.len(), 2);
        // BTreeMap sorts alphabetically
        assert_eq!(groups[0].label, "/home/user/projects");
        assert_eq!(groups[0].projects.len(), 2);
        assert_eq!(groups[1].label, "/home/user/work");
        assert_eq!(groups[1].projects.len(), 1);
    }

    #[test]
    fn test_group_by_parent_empty_input() {
        let groups = group_by_parent(vec![]);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_group_summary_stats() {
        let statuses = vec![
            make_project_at("clean1", "/tmp/a", true),
            make_project_at("dirty1", "/tmp/a", false),
            make_project_at("dirty2", "/tmp/a", false),
        ];

        let groups = group_by_parent(statuses);
        assert_eq!(groups[0].summary.total, 3);
        assert_eq!(groups[0].summary.dirty, 2);
        assert_eq!(groups[0].summary.clean, 1);
    }

    #[test]
    fn test_grouped_json_structure() {
        let statuses = vec![
            make_project_at("app", "/tmp/projects", true),
            make_project_at("lib", "/tmp/work", false),
        ];

        let groups = group_by_parent(statuses);
        let json_str = format_grouped_json(&groups).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(parsed["groups"].is_object());
        assert!(parsed["groups"]["/tmp/projects"].is_object());
        assert!(parsed["groups"]["/tmp/work"].is_object());
        assert!(parsed["groups"]["/tmp/projects"]["projects"].is_array());
        assert!(parsed["summary"].is_object());
        assert_eq!(parsed["summary"]["total"], 2);
    }

    #[test]
    fn test_grouped_json_empty() {
        let groups = group_by_parent(vec![]);
        let json_str = format_grouped_json(&groups).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed["groups"].as_object().unwrap().is_empty());
        assert_eq!(parsed["summary"]["total"], 0);
    }

    #[test]
    fn test_grouped_csv_has_group_column() {
        let statuses = vec![make_project_at("app", "/tmp/projects", true)];
        let groups = group_by_parent(statuses);
        let csv = format_grouped_csv(&groups).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[0].starts_with("Group,"));
        assert!(lines[1].starts_with("/tmp/projects,app,"));
    }

    #[test]
    fn test_grouped_csv_empty() {
        let groups = group_by_parent(vec![]);
        let csv = format_grouped_csv(&groups).unwrap();
        assert_eq!(csv.lines().count(), 1); // Header only
    }

    #[test]
    fn test_grouped_csv_multiple_groups() {
        let statuses = vec![
            make_project_at("a", "/tmp/g1", true),
            make_project_at("b", "/tmp/g2", false),
        ];
        let groups = group_by_parent(statuses);
        let csv = format_grouped_csv(&groups).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 data rows
    }

    #[test]
    fn test_grouped_markdown_has_subheaders() {
        let statuses = vec![
            make_project_at("app", "/tmp/projects", true),
            make_project_at("lib", "/tmp/work", false),
        ];
        let groups = group_by_parent(statuses);
        let md = format_grouped_markdown(&groups).unwrap();
        assert!(md.contains("### /tmp/projects"));
        assert!(md.contains("### /tmp/work"));
        assert!(md.contains("| app |"));
        assert!(md.contains("| lib |"));
    }

    #[test]
    fn test_grouped_markdown_overall_summary_with_multiple_groups() {
        let statuses = vec![
            make_project_at("a", "/tmp/g1", true),
            make_project_at("b", "/tmp/g2", false),
        ];
        let groups = group_by_parent(statuses);
        let md = format_grouped_markdown(&groups).unwrap();
        assert!(md.contains("**Overall:**"));
        assert!(md.contains("2 projects"));
    }

    #[test]
    fn test_grouped_markdown_single_group_no_overall() {
        let statuses = vec![
            make_project_at("a", "/tmp/g1", true),
            make_project_at("b", "/tmp/g1", false),
        ];
        let groups = group_by_parent(statuses);
        let md = format_grouped_markdown(&groups).unwrap();
        assert!(!md.contains("**Overall:**"));
    }

    #[test]
    fn test_grouped_markdown_empty() {
        let groups = group_by_parent(vec![]);
        let md = format_grouped_markdown(&groups).unwrap();
        assert!(md.is_empty());
    }

    #[test]
    fn test_csv_escape_in_group() {
        assert_eq!(csv_escape("normal"), "normal");
        assert_eq!(csv_escape("has,comma"), "\"has,comma\"");
        assert_eq!(csv_escape("has\"quote"), "\"has\"\"quote\"");
    }

    #[test]
    fn test_normalize_label_non_home() {
        let path = Path::new("/tmp/projects");
        let label = normalize_label(path);
        assert_eq!(label, "/tmp/projects");
    }

    #[test]
    fn test_normalize_label_uses_tilde() {
        // This test depends on HOME being set, which it normally is
        if let Ok(home) = std::env::var("HOME") {
            let path_str = format!("{}/projects", home);
            let path = Path::new(&path_str);
            let label = normalize_label(path);
            assert_eq!(label, "~/projects");
        }
    }

    #[test]
    fn test_normalize_label_home_itself() {
        if let Ok(home) = std::env::var("HOME") {
            let path = Path::new(&home);
            let label = normalize_label(path);
            assert_eq!(label, "~");
        }
    }
}
