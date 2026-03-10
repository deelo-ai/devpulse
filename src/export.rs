use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::ValueEnum;

use crate::summary::Summary;
use crate::types::ProjectStatus;

/// Supported output formats for project status data.
#[derive(Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    /// Terminal table with colors (default)
    Table,
    /// JSON output
    Json,
    /// Comma-separated values
    Csv,
    /// Markdown table
    Markdown,
    /// Markdown table (alias)
    Md,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Table => write!(f, "table"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Csv => write!(f, "csv"),
            OutputFormat::Markdown => write!(f, "markdown"),
            OutputFormat::Md => write!(f, "md"),
        }
    }
}

impl OutputFormat {
    /// Normalize aliases (md -> markdown).
    pub fn normalized(&self) -> &OutputFormat {
        match self {
            OutputFormat::Md => &OutputFormat::Markdown,
            other => other,
        }
    }
}

/// CSV column headers.
const CSV_HEADERS: &[&str] = &[
    "Project",
    "Branch",
    "Status",
    "Changed",
    "Last Commit",
    "Ahead",
    "Behind",
    "Stash",
    "Remote URL",
    "Last Commit Message",
];

/// Escape a CSV field: wrap in quotes if it contains commas, quotes, or newlines.
fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Format project statuses as CSV.
pub fn format_csv(statuses: &[ProjectStatus]) -> Result<String> {
    let mut out = String::new();

    // Header row
    out.push_str(&CSV_HEADERS.join(","));
    out.push('\n');

    for s in statuses {
        let status_str = if s.is_clean { "clean" } else { "dirty" };
        let last_commit_str = match s.last_commit {
            Some(dt) => dt.to_rfc3339(),
            None => String::new(),
        };
        let remote = s.remote_url.as_deref().unwrap_or("");

        let message = s.last_commit_message.as_deref().unwrap_or("");

        let row = [
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

    Ok(out)
}

/// Format project statuses as a Markdown table with summary row.
pub fn format_markdown(statuses: &[ProjectStatus]) -> Result<String> {
    let mut out = String::new();

    // Header
    out.push_str(
        "| Project | Branch | Status | Changed | Last Commit | Ahead/Behind | Stash | Message |\n",
    );
    out.push_str(
        "|---------|--------|--------|--------:|-------------|-------------:|------:|---------|\n",
    );

    let now = Utc::now();

    for s in statuses {
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

    // Summary row
    let summary = Summary::from_statuses(statuses);
    out.push('\n');
    out.push_str(&format!(
        "**{}** projects · **{}** dirty · **{}** stale · **{}** unpushed\n",
        summary.total, summary.dirty, summary.stale, summary.unpushed,
    ));

    Ok(out)
}

/// Format as JSON (same as existing --json behavior).
pub fn format_json(statuses: &[ProjectStatus]) -> Result<String> {
    let summary = Summary::from_statuses(statuses);
    let output = serde_json::json!({
        "projects": statuses,
        "summary": summary,
    });
    serde_json::to_string_pretty(&output).context("Failed to serialize JSON output")
}

/// Write formatted output to stdout.
pub fn write_output(
    statuses: &[ProjectStatus],
    format: &OutputFormat,
    use_color: bool,
) -> Result<()> {
    let normalized = format.normalized();
    match normalized {
        OutputFormat::Table => {
            if use_color {
                crate::table::print_table(statuses);
                let summary = Summary::from_statuses(statuses);
                summary.print_colored();
            } else {
                let plain = crate::table::format_table_plain(statuses);
                print!("{plain}");
            }
            println!();
            Ok(())
        }
        OutputFormat::Json => {
            let output = format_json(statuses)?;
            println!("{output}");
            Ok(())
        }
        OutputFormat::Csv => {
            let output = format_csv(statuses)?;
            print!("{output}");
            io::stdout().flush().context("Failed to flush stdout")?;
            Ok(())
        }
        OutputFormat::Markdown | OutputFormat::Md => {
            let output = format_markdown(statuses)?;
            print!("{output}");
            io::stdout().flush().context("Failed to flush stdout")?;
            Ok(())
        }
    }
}

/// Format output as a string for any format (table uses plain text, no ANSI).
pub fn format_output(statuses: &[ProjectStatus], format: &OutputFormat) -> Result<String> {
    let normalized = format.normalized();
    match normalized {
        OutputFormat::Table => Ok(crate::table::format_table_plain(statuses)),
        OutputFormat::Json => format_json(statuses),
        OutputFormat::Csv => format_csv(statuses),
        OutputFormat::Markdown | OutputFormat::Md => format_markdown(statuses),
    }
}

/// Write formatted output to a file, creating parent directories as needed.
/// Prints a confirmation message to stderr on success.
pub fn write_output_to_file(
    statuses: &[ProjectStatus],
    format: &OutputFormat,
    path: &Path,
) -> Result<()> {
    let content = format_output(statuses, format)?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create parent directories for {}", path.display())
        })?;
    }

    fs::write(path, &content)
        .with_context(|| format!("Failed to write output to {}", path.display()))?;

    eprintln!("Wrote {} projects to {}", statuses.len(), path.display());

    Ok(())
}

/// Escape pipe characters in markdown table cells.
fn md_escape(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Format a duration between now and a past timestamp as a human-readable string.
fn format_relative_time(now: chrono::DateTime<Utc>, then: chrono::DateTime<Utc>) -> String {
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

    fn make_project(
        name: &str,
        branch: &str,
        is_clean: bool,
        days_ago: Option<i64>,
        ahead: usize,
        behind: usize,
        stash: usize,
    ) -> ProjectStatus {
        ProjectStatus {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{}", name)),
            branch: branch.to_string(),
            is_clean,
            changed_files: if is_clean { 0 } else { 3 },
            last_commit: days_ago.map(|d| Utc::now() - Duration::days(d)),
            ahead,
            behind,
            remote_url: Some("https://github.com/example/repo".to_string()),
            stash_count: stash,
            last_commit_message: None,
        }
    }

    #[test]
    fn test_csv_empty_statuses() {
        let result = format_csv(&[]).unwrap();
        assert_eq!(
            result,
            "Project,Branch,Status,Changed,Last Commit,Ahead,Behind,Stash,Remote URL,Last Commit Message\n"
        );
    }

    #[test]
    fn test_csv_single_project() {
        let statuses = vec![make_project("myapp", "main", true, Some(1), 0, 0, 0)];
        let result = format_csv(&statuses).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("Project,"));
        assert!(lines[1].starts_with("myapp,main,clean,0,"));
    }

    #[test]
    fn test_csv_dirty_project() {
        let statuses = vec![make_project("dirty-app", "dev", false, Some(5), 2, 1, 3)];
        let result = format_csv(&statuses).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines[1].contains("dirty"));
        assert!(lines[1].contains(",3,")); // changed_files
        assert!(lines[1].contains(",2,")); // ahead
    }

    #[test]
    fn test_csv_escape_commas() {
        assert_eq!(csv_escape("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn test_csv_escape_quotes() {
        assert_eq!(csv_escape("say \"hello\""), "\"say \"\"hello\"\"\"");
    }

    #[test]
    fn test_csv_escape_newlines() {
        assert_eq!(csv_escape("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn test_csv_no_escape_needed() {
        assert_eq!(csv_escape("simple"), "simple");
    }

    #[test]
    fn test_csv_no_last_commit() {
        let statuses = vec![make_project("new", "main", true, None, 0, 0, 0)];
        let result = format_csv(&statuses).unwrap();
        // No last_commit should produce empty field
        assert!(result.contains(",,"));
    }

    #[test]
    fn test_markdown_empty_statuses() {
        let result = format_markdown(&[]).unwrap();
        assert!(result.contains("| Project |"));
        assert!(result.contains("|---------|"));
        assert!(result.contains("**0** projects"));
    }

    #[test]
    fn test_markdown_single_clean_project() {
        let statuses = vec![make_project("myapp", "main", true, Some(1), 0, 0, 0)];
        let result = format_markdown(&statuses).unwrap();
        assert!(result.contains("| myapp |"));
        assert!(result.contains("✅ clean"));
        assert!(result.contains("**1** projects"));
        assert!(result.contains("**0** dirty"));
    }

    #[test]
    fn test_markdown_dirty_project() {
        let statuses = vec![make_project("broken", "feat", false, Some(2), 3, 1, 0)];
        let result = format_markdown(&statuses).unwrap();
        assert!(result.contains("⚠️ dirty"));
        assert!(result.contains("↑3 ↓1"));
        assert!(result.contains("**1** dirty"));
    }

    #[test]
    fn test_markdown_stash_display() {
        let statuses = vec![make_project("stashed", "main", true, Some(1), 0, 0, 5)];
        let result = format_markdown(&statuses).unwrap();
        assert!(result.contains("| 5 |"));
    }

    #[test]
    fn test_markdown_zero_stash_shows_dash() {
        let statuses = vec![make_project("clean", "main", true, Some(1), 0, 0, 0)];
        let result = format_markdown(&statuses).unwrap();
        assert!(result.contains("| — |"));
    }

    #[test]
    fn test_markdown_no_ahead_behind_shows_dash() {
        let statuses = vec![make_project("synced", "main", true, Some(1), 0, 0, 0)];
        let result = format_markdown(&statuses).unwrap();
        // The ahead/behind column should show "—"
        let lines: Vec<&str> = result.lines().collect();
        let data_line = lines[2]; // first data row after header + separator
        assert!(data_line.contains("—"));
    }

    #[test]
    fn test_markdown_pipe_escaping() {
        assert_eq!(md_escape("no pipes"), "no pipes");
        assert_eq!(md_escape("has|pipe"), "has\\|pipe");
    }

    #[test]
    fn test_markdown_project_with_pipe_in_name() {
        let mut proj = make_project("my|app", "main", true, Some(1), 0, 0, 0);
        proj.name = "my|app".to_string();
        let result = format_markdown(&[proj]).unwrap();
        assert!(result.contains("my\\|app"));
    }

    #[test]
    fn test_json_output_structure() {
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        let result = format_json(&statuses).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["projects"].is_array());
        assert!(parsed["summary"].is_object());
        assert_eq!(parsed["summary"]["total"], 1);
    }

    #[test]
    fn test_json_empty() {
        let result = format_json(&[]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["projects"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["summary"]["total"], 0);
    }

    #[test]
    fn test_format_display() {
        assert_eq!(format!("{}", OutputFormat::Table), "table");
        assert_eq!(format!("{}", OutputFormat::Json), "json");
        assert_eq!(format!("{}", OutputFormat::Csv), "csv");
        assert_eq!(format!("{}", OutputFormat::Markdown), "markdown");
        assert_eq!(format!("{}", OutputFormat::Md), "md");
    }

    #[test]
    fn test_format_normalized() {
        assert_eq!(OutputFormat::Md.normalized(), &OutputFormat::Markdown);
        assert_eq!(OutputFormat::Markdown.normalized(), &OutputFormat::Markdown);
        assert_eq!(OutputFormat::Csv.normalized(), &OutputFormat::Csv);
        assert_eq!(OutputFormat::Json.normalized(), &OutputFormat::Json);
        assert_eq!(OutputFormat::Table.normalized(), &OutputFormat::Table);
    }

    #[test]
    fn test_csv_multiple_projects() {
        let statuses = vec![
            make_project("alpha", "main", true, Some(1), 0, 0, 0),
            make_project("beta", "dev", false, Some(10), 5, 2, 1),
            make_project("gamma", "release", true, Some(60), 0, 0, 3),
        ];
        let result = format_csv(&statuses).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 4); // header + 3 data rows
    }

    #[test]
    fn test_markdown_summary_counts() {
        let statuses = vec![
            make_project("a", "main", true, Some(1), 0, 0, 0),
            make_project("b", "main", false, Some(1), 1, 0, 0),
            make_project("c", "main", true, Some(60), 0, 0, 0), // stale
        ];
        let result = format_markdown(&statuses).unwrap();
        assert!(result.contains("**3** projects"));
        assert!(result.contains("**1** dirty"));
        assert!(result.contains("**1** stale"));
        assert!(result.contains("**1** unpushed"));
    }

    #[test]
    fn test_relative_time_just_now() {
        let now = Utc::now();
        assert_eq!(format_relative_time(now, now), "just now");
    }

    #[test]
    fn test_relative_time_minutes() {
        let now = Utc::now();
        let then = now - Duration::minutes(15);
        assert_eq!(format_relative_time(now, then), "15m ago");
    }

    #[test]
    fn test_relative_time_hours() {
        let now = Utc::now();
        let then = now - Duration::hours(5);
        assert_eq!(format_relative_time(now, then), "5h ago");
    }

    #[test]
    fn test_relative_time_days() {
        let now = Utc::now();
        let then = now - Duration::days(15);
        assert_eq!(format_relative_time(now, then), "15d ago");
    }

    #[test]
    fn test_relative_time_months() {
        let now = Utc::now();
        let then = now - Duration::days(90);
        assert_eq!(format_relative_time(now, then), "3mo ago");
    }

    #[test]
    fn test_relative_time_years() {
        let now = Utc::now();
        let then = now - Duration::days(400);
        assert_eq!(format_relative_time(now, then), "1y ago");
    }

    #[test]
    fn test_format_output_csv() {
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        let result = format_output(&statuses, &OutputFormat::Csv).unwrap();
        assert!(result.starts_with("Project,"));
        assert!(result.contains("app,main,clean"));
    }

    #[test]
    fn test_format_output_json() {
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        let result = format_output(&statuses, &OutputFormat::Json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["projects"].is_array());
    }

    #[test]
    fn test_format_output_markdown() {
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        let result = format_output(&statuses, &OutputFormat::Markdown).unwrap();
        assert!(result.contains("| app |"));
    }

    #[test]
    fn test_format_output_md_alias() {
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        let result = format_output(&statuses, &OutputFormat::Md).unwrap();
        assert!(result.contains("| app |"));
    }

    #[test]
    fn test_format_output_table_plain() {
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        let result = format_output(&statuses, &OutputFormat::Table).unwrap();
        assert!(result.contains("Project"));
        assert!(result.contains("app"));
        // Should NOT contain ANSI escape codes
        assert!(!result.contains("\x1b["));
    }

    #[test]
    fn test_format_output_empty() {
        let result = format_output(&[], &OutputFormat::Table).unwrap();
        assert!(result.contains("No projects found"));
    }

    #[test]
    fn test_write_output_to_file_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.csv");
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        write_output_to_file(&statuses, &OutputFormat::Csv, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("Project,"));
        assert!(content.contains("app,main,clean"));
    }

    #[test]
    fn test_write_output_to_file_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        write_output_to_file(&statuses, &OutputFormat::Json, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["summary"]["total"], 1);
    }

    #[test]
    fn test_write_output_to_file_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("report.md");
        let statuses = vec![make_project("app", "main", true, Some(1), 0, 0, 0)];
        write_output_to_file(&statuses, &OutputFormat::Markdown, &path).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("| app |"));
    }

    #[test]
    fn test_write_output_to_file_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.csv");
        std::fs::write(&path, "old content").unwrap();
        let statuses = vec![make_project("new-app", "main", true, Some(1), 0, 0, 0)];
        write_output_to_file(&statuses, &OutputFormat::Csv, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("old content"));
        assert!(content.contains("new-app"));
    }

    #[test]
    fn test_write_output_to_file_table_no_ansi() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.txt");
        let statuses = vec![
            make_project("clean-app", "main", true, Some(1), 0, 0, 0),
            make_project("dirty-app", "dev", false, Some(5), 2, 1, 3),
        ];
        write_output_to_file(&statuses, &OutputFormat::Table, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("clean-app"));
        assert!(content.contains("dirty-app"));
        // No ANSI escape sequences
        assert!(!content.contains("\x1b["));
    }

    #[test]
    fn test_write_output_to_file_empty_statuses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.csv");
        write_output_to_file(&[], &OutputFormat::Csv, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // Should have header only
        assert!(content.starts_with("Project,"));
        assert_eq!(content.lines().count(), 1);
    }
}
