use chrono::Utc;
use crossterm::style::{Color, Stylize};

use crate::types::ProjectStatus;

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
        "  {:<name_w$}  {:<branch_w$}  {:>8}  {:>7}  {:>14}  {:>12}  {}",
        "Project",
        "Branch",
        "Status",
        "Changed",
        "Last Commit",
        "Ahead/Behind",
        "Stash",
        name_w = name_width,
        branch_w = branch_width,
    );
    println!("{}", header.bold());
    println!(
        "  {}",
        "─".repeat(name_width + branch_width + 8 + 7 + 14 + 12 + 8 + 14)
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

        println!(
            "  {:<name_w$}  {:<branch_w$}  {}  {}  {}  {:>12}  {}",
            s.name,
            s.branch,
            status_colored,
            changed_colored,
            last_commit_colored,
            ahead_behind,
            stash_colored,
            name_w = name_width,
            branch_w = branch_width,
        );
    }

    println!();
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
