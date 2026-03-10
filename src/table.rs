use chrono::Utc;
use crossterm::style::{Color, Stylize};

use crate::git::truncate_message;
use crate::types::ProjectStatus;

/// Maximum display width for commit messages in the table.
const MESSAGE_MAX_LEN: usize = 50;

/// Print a colored table of project statuses.
pub fn print_table(statuses: &[ProjectStatus]) {
    if statuses.is_empty() {
        println!("No projects found.");
        return;
    }

    // Calculate column widths
    let name_width = statuses
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(7)
        .max(7);
    let branch_width = statuses
        .iter()
        .map(|s| s.branch.len())
        .max()
        .unwrap_or(6)
        .max(6);

    // Print header
    let header = format!(
        "  {:<name_w$}  {:<branch_w$}  {:>8}  {:>7}  {:>14}  {:>12}  {:>5}  {}",
        "Project",
        "Branch",
        "Status",
        "Changed",
        "Last Commit",
        "Ahead/Behind",
        "Stash",
        "Message",
        name_w = name_width,
        branch_w = branch_width,
    );
    println!("{}", header.bold());
    println!(
        "  {}",
        "─".repeat(name_width + branch_width + 8 + 7 + 14 + 12 + 5 + MESSAGE_MAX_LEN + 20)
    );

    let now = Utc::now();

    for s in statuses {
        let status_str = if s.is_clean { "clean" } else { "dirty" };
        let status_colored = if s.is_clean {
            format!("{:>8}", status_str).with(Color::Green)
        } else {
            format!("{:>8}", status_str).with(Color::Yellow)
        };

        let last_commit_str = match s.last_commit {
            Some(dt) => format_relative_time(now, dt),
            None => "no commits".to_string(),
        };

        let last_commit_colored = match s.last_commit {
            Some(dt) => {
                let days = (now - dt).num_days();
                let text = format!("{:>14}", last_commit_str);
                if days < 7 {
                    text.with(Color::Green)
                } else if days < 30 {
                    text.with(Color::Yellow)
                } else {
                    text.with(Color::Red)
                }
            }
            None => format!("{:>14}", last_commit_str).with(Color::DarkGrey),
        };

        let changed_str = format!("{:>7}", s.changed_files);
        let changed_colored = if s.changed_files == 0 {
            changed_str.with(Color::Green)
        } else {
            changed_str.with(Color::Yellow)
        };

        let ahead_behind = if s.ahead == 0 && s.behind == 0 {
            "—".to_string()
        } else {
            format!("↑{} ↓{}", s.ahead, s.behind)
        };

        let stash_str = if s.stash_count == 0 {
            "—".to_string()
        } else {
            format!("{}", s.stash_count)
        };
        let stash_colored = if s.stash_count == 0 {
            format!("{:>5}", stash_str).with(Color::DarkGrey)
        } else {
            format!("{:>5}", stash_str).with(Color::Cyan)
        };

        let message_str = match &s.last_commit_message {
            Some(msg) => truncate_message(msg, MESSAGE_MAX_LEN),
            None => "—".to_string(),
        };
        let message_colored = match &s.last_commit_message {
            Some(_) => message_str.with(Color::White),
            None => message_str.with(Color::DarkGrey),
        };

        println!(
            "  {:<name_w$}  {:<branch_w$}  {}  {}  {}  {:>12}  {}  {}",
            s.name,
            s.branch,
            status_colored,
            changed_colored,
            last_commit_colored,
            ahead_behind,
            stash_colored,
            message_colored,
            name_w = name_width,
            branch_w = branch_width,
        );
    }

    println!();
}

/// Format a plain-text table of project statuses (no ANSI colors).
/// Used when writing table output to a file.
pub fn format_table_plain(statuses: &[ProjectStatus]) -> String {
    use crate::summary::Summary;

    if statuses.is_empty() {
        return "No projects found.\n".to_string();
    }

    let mut out = String::new();

    // Calculate column widths
    let name_width = statuses
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(7)
        .max(7);
    let branch_width = statuses
        .iter()
        .map(|s| s.branch.len())
        .max()
        .unwrap_or(6)
        .max(6);

    // Header
    out.push_str(&format!(
        "  {:<name_w$}  {:<branch_w$}  {:>8}  {:>7}  {:>14}  {:>12}  {:>5}  {}\n",
        "Project",
        "Branch",
        "Status",
        "Changed",
        "Last Commit",
        "Ahead/Behind",
        "Stash",
        "Message",
        name_w = name_width,
        branch_w = branch_width,
    ));
    out.push_str(&format!(
        "  {}\n",
        "-".repeat(name_width + branch_width + 8 + 7 + 14 + 12 + 5 + MESSAGE_MAX_LEN + 20)
    ));

    let now = Utc::now();

    for s in statuses {
        let status_str = if s.is_clean { "clean" } else { "dirty" };
        let last_commit_str = match s.last_commit {
            Some(dt) => format_relative_time(now, dt),
            None => "no commits".to_string(),
        };
        let ahead_behind = if s.ahead == 0 && s.behind == 0 {
            "—".to_string()
        } else {
            format!("↑{} ↓{}", s.ahead, s.behind)
        };
        let stash_str = if s.stash_count == 0 {
            "—".to_string()
        } else {
            format!("{}", s.stash_count)
        };
        let message_str = match &s.last_commit_message {
            Some(msg) => truncate_message(msg, MESSAGE_MAX_LEN),
            None => "—".to_string(),
        };

        out.push_str(&format!(
            "  {:<name_w$}  {:<branch_w$}  {:>8}  {:>7}  {:>14}  {:>12}  {:>5}  {}\n",
            s.name,
            s.branch,
            status_str,
            s.changed_files,
            last_commit_str,
            ahead_behind,
            stash_str,
            message_str,
            name_w = name_width,
            branch_w = branch_width,
        ));
    }

    out.push('\n');
    let summary = Summary::from_statuses(statuses);
    out.push_str(&format!("  {}\n", summary.to_plain_string()));

    out
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

    fn make_status(name: &str, is_clean: bool, changed: usize) -> ProjectStatus {
        let now = Utc::now();
        ProjectStatus {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{}", name)),
            branch: "main".to_string(),
            is_clean,
            changed_files: changed,
            last_commit: Some(now - Duration::hours(2)),
            ahead: 0,
            behind: 0,
            remote_url: None,
            stash_count: 0,
            last_commit_message: Some("initial commit".to_string()),
        }
    }

    // --- format_relative_time tests ---

    #[test]
    fn test_relative_time_just_now() {
        let now = Utc::now();
        assert_eq!(format_relative_time(now, now), "just now");
    }

    #[test]
    fn test_relative_time_seconds_ago() {
        let now = Utc::now();
        let then = now - Duration::seconds(30);
        assert_eq!(format_relative_time(now, then), "just now");
    }

    #[test]
    fn test_relative_time_minutes_ago() {
        let now = Utc::now();
        let then = now - Duration::minutes(5);
        assert_eq!(format_relative_time(now, then), "5m ago");
    }

    #[test]
    fn test_relative_time_59_minutes() {
        let now = Utc::now();
        let then = now - Duration::minutes(59);
        assert_eq!(format_relative_time(now, then), "59m ago");
    }

    #[test]
    fn test_relative_time_hours_ago() {
        let now = Utc::now();
        let then = now - Duration::hours(3);
        assert_eq!(format_relative_time(now, then), "3h ago");
    }

    #[test]
    fn test_relative_time_23_hours() {
        let now = Utc::now();
        let then = now - Duration::hours(23);
        assert_eq!(format_relative_time(now, then), "23h ago");
    }

    #[test]
    fn test_relative_time_days_ago() {
        let now = Utc::now();
        let then = now - Duration::days(5);
        assert_eq!(format_relative_time(now, then), "5d ago");
    }

    #[test]
    fn test_relative_time_29_days() {
        let now = Utc::now();
        let then = now - Duration::days(29);
        assert_eq!(format_relative_time(now, then), "29d ago");
    }

    #[test]
    fn test_relative_time_months_ago() {
        let now = Utc::now();
        let then = now - Duration::days(60);
        assert_eq!(format_relative_time(now, then), "2mo ago");
    }

    #[test]
    fn test_relative_time_11_months() {
        let now = Utc::now();
        let then = now - Duration::days(330);
        assert_eq!(format_relative_time(now, then), "11mo ago");
    }

    #[test]
    fn test_relative_time_years_ago() {
        let now = Utc::now();
        let then = now - Duration::days(400);
        assert_eq!(format_relative_time(now, then), "1y ago");
    }

    #[test]
    fn test_relative_time_multiple_years() {
        let now = Utc::now();
        let then = now - Duration::days(900);
        assert_eq!(format_relative_time(now, then), "2y ago");
    }

    // --- format_table_plain tests ---

    #[test]
    fn test_plain_table_empty() {
        let result = format_table_plain(&[]);
        assert_eq!(result, "No projects found.\n");
    }

    #[test]
    fn test_plain_table_single_clean_project() {
        let status = make_status("myapp", true, 0);
        let result = format_table_plain(&[status]);
        assert!(result.contains("myapp"));
        assert!(result.contains("clean"));
        assert!(result.contains("main"));
        assert!(result.contains("initial commit"));
        // Should have summary line
        assert!(result.contains("1 project"));
    }

    #[test]
    fn test_plain_table_single_dirty_project() {
        let status = make_status("myapp", false, 3);
        let result = format_table_plain(&[status]);
        assert!(result.contains("dirty"));
        assert!(result.contains("3"));
    }

    #[test]
    fn test_plain_table_multiple_projects() {
        let s1 = make_status("alpha", true, 0);
        let s2 = make_status("beta", false, 5);
        let result = format_table_plain(&[s1, s2]);
        assert!(result.contains("alpha"));
        assert!(result.contains("beta"));
        assert!(result.contains("2 projects"));
    }

    #[test]
    fn test_plain_table_contains_header() {
        let status = make_status("test", true, 0);
        let result = format_table_plain(&[status]);
        assert!(result.contains("Project"));
        assert!(result.contains("Branch"));
        assert!(result.contains("Status"));
        assert!(result.contains("Changed"));
        assert!(result.contains("Last Commit"));
        assert!(result.contains("Ahead/Behind"));
        assert!(result.contains("Stash"));
        assert!(result.contains("Message"));
    }

    #[test]
    fn test_plain_table_ahead_behind() {
        let mut status = make_status("myapp", true, 0);
        status.ahead = 3;
        status.behind = 1;
        let result = format_table_plain(&[status]);
        assert!(result.contains("↑3 ↓1"));
    }

    #[test]
    fn test_plain_table_no_ahead_behind_shows_dash() {
        let status = make_status("myapp", true, 0);
        let result = format_table_plain(&[status]);
        assert!(result.contains("—"));
    }

    #[test]
    fn test_plain_table_stash_count() {
        let mut status = make_status("myapp", true, 0);
        status.stash_count = 4;
        let result = format_table_plain(&[status]);
        assert!(result.contains("4"));
    }

    #[test]
    fn test_plain_table_no_last_commit() {
        let mut status = make_status("myapp", true, 0);
        status.last_commit = None;
        status.last_commit_message = None;
        let result = format_table_plain(&[status]);
        assert!(result.contains("no commits"));
    }

    #[test]
    fn test_plain_table_long_project_name_widens_column() {
        let status = make_status("a-very-long-project-name", true, 0);
        let result = format_table_plain(&[status]);
        // The full name should appear, not truncated
        assert!(result.contains("a-very-long-project-name"));
    }

    #[test]
    fn test_relative_time_boundary_60_minutes() {
        let now = Utc::now();
        let then = now - Duration::minutes(60);
        // 60 minutes = 1 hour
        assert_eq!(format_relative_time(now, then), "1h ago");
    }

    #[test]
    fn test_relative_time_boundary_24_hours() {
        let now = Utc::now();
        let then = now - Duration::hours(24);
        // 24 hours = 1 day
        assert_eq!(format_relative_time(now, then), "1d ago");
    }

    #[test]
    fn test_relative_time_boundary_30_days() {
        let now = Utc::now();
        let then = now - Duration::days(30);
        // 30 days = 1 month
        assert_eq!(format_relative_time(now, then), "1mo ago");
    }

    #[test]
    fn test_relative_time_boundary_365_days() {
        let now = Utc::now();
        let then = now - Duration::days(365);
        // 365 days = 1 year
        assert_eq!(format_relative_time(now, then), "1y ago");
    }
}
