use chrono::Utc;
use crossterm::style::{Color, Stylize};
use serde::Serialize;

use crate::types::ProjectStatus;

/// Threshold in days for considering a project "stale".
const STALE_THRESHOLD_DAYS: i64 = 30;

/// Summary statistics computed from a collection of project statuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Total number of projects scanned.
    pub total: usize,
    /// Number of projects with uncommitted changes.
    pub dirty: usize,
    /// Number of clean projects.
    pub clean: usize,
    /// Number of projects with no commits in >30 days.
    pub stale: usize,
    /// Number of projects with unpushed commits (ahead > 0).
    pub unpushed: usize,
}

impl Summary {
    /// Compute summary statistics from a slice of project statuses.
    pub fn from_statuses(statuses: &[ProjectStatus]) -> Self {
        let now = Utc::now();
        let total = statuses.len();
        let dirty = statuses.iter().filter(|s| !s.is_clean).count();
        let clean = total - dirty;
        let stale = statuses
            .iter()
            .filter(|s| {
                s.last_commit
                    .map(|dt| (now - dt).num_days() > STALE_THRESHOLD_DAYS)
                    .unwrap_or(true) // no commits = stale
            })
            .count();
        let unpushed = statuses.iter().filter(|s| s.ahead > 0).count();

        Self {
            total,
            dirty,
            clean,
            stale,
            unpushed,
        }
    }

    /// Format summary as a plain text string (no colors).
    /// Used in JSON output context and for testing.
    #[allow(dead_code)]
    pub fn to_plain_string(&self) -> String {
        format!(
            "{} projects │ {} dirty │ {} stale │ {} unpushed",
            self.total, self.dirty, self.stale, self.unpushed,
        )
    }

    /// Print the summary line with terminal colors.
    pub fn print_colored(&self) {
        let total_part = format!("{} projects", self.total);

        let dirty_part = format!("{} dirty", self.dirty);
        let dirty_colored = if self.dirty > 0 {
            dirty_part.with(Color::Yellow)
        } else {
            dirty_part.with(Color::Green)
        };

        let stale_part = format!("{} stale", self.stale);
        let stale_colored = if self.stale > 0 {
            stale_part.with(Color::Red)
        } else {
            stale_part.with(Color::Green)
        };

        let unpushed_part = format!("{} unpushed", self.unpushed);
        let unpushed_colored = if self.unpushed > 0 {
            unpushed_part.with(Color::Yellow)
        } else {
            unpushed_part.with(Color::Green)
        };

        println!(
            "  {} │ {} │ {} │ {}",
            total_part, dirty_colored, stale_colored, unpushed_colored,
        );
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

    #[test]
    fn test_empty_statuses() {
        let summary = Summary::from_statuses(&[]);
        assert_eq!(
            summary,
            Summary {
                total: 0,
                dirty: 0,
                clean: 0,
                stale: 0,
                unpushed: 0,
            }
        );
    }

    #[test]
    fn test_all_clean_active() {
        let statuses = vec![
            make_project("a", true, Some(1), 0),
            make_project("b", true, Some(5), 0),
        ];
        let summary = Summary::from_statuses(&statuses);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.dirty, 0);
        assert_eq!(summary.clean, 2);
        assert_eq!(summary.stale, 0);
        assert_eq!(summary.unpushed, 0);
    }

    #[test]
    fn test_mixed_statuses() {
        let statuses = vec![
            make_project("active-clean", true, Some(3), 0),
            make_project("active-dirty", false, Some(7), 0),
            make_project("stale-clean", true, Some(60), 0),
            make_project("unpushed", true, Some(1), 5),
        ];
        let summary = Summary::from_statuses(&statuses);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.dirty, 1);
        assert_eq!(summary.clean, 3);
        assert_eq!(summary.stale, 1);
        assert_eq!(summary.unpushed, 1);
    }

    #[test]
    fn test_no_commits_counts_as_stale() {
        let statuses = vec![make_project("empty", true, None, 0)];
        let summary = Summary::from_statuses(&statuses);
        assert_eq!(summary.stale, 1);
    }

    #[test]
    fn test_exactly_30_days_is_not_stale() {
        let statuses = vec![make_project("borderline", true, Some(30), 0)];
        let summary = Summary::from_statuses(&statuses);
        assert_eq!(summary.stale, 0);
    }

    #[test]
    fn test_31_days_is_stale() {
        let statuses = vec![make_project("old", true, Some(31), 0)];
        let summary = Summary::from_statuses(&statuses);
        assert_eq!(summary.stale, 1);
    }

    #[test]
    fn test_multiple_unpushed() {
        let statuses = vec![
            make_project("a", true, Some(1), 3),
            make_project("b", true, Some(1), 1),
            make_project("c", true, Some(1), 0),
        ];
        let summary = Summary::from_statuses(&statuses);
        assert_eq!(summary.unpushed, 2);
    }

    #[test]
    fn test_to_plain_string() {
        let summary = Summary {
            total: 12,
            dirty: 3,
            clean: 9,
            stale: 2,
            unpushed: 1,
        };
        assert_eq!(
            summary.to_plain_string(),
            "12 projects │ 3 dirty │ 2 stale │ 1 unpushed"
        );
    }

    #[test]
    fn test_to_plain_string_zeros() {
        let summary = Summary {
            total: 0,
            dirty: 0,
            clean: 0,
            stale: 0,
            unpushed: 0,
        };
        assert_eq!(
            summary.to_plain_string(),
            "0 projects │ 0 dirty │ 0 stale │ 0 unpushed"
        );
    }

    #[test]
    fn test_all_dirty_all_stale_all_unpushed() {
        let statuses = vec![
            make_project("a", false, Some(100), 2),
            make_project("b", false, Some(200), 1),
        ];
        let summary = Summary::from_statuses(&statuses);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.dirty, 2);
        assert_eq!(summary.clean, 0);
        assert_eq!(summary.stale, 2);
        assert_eq!(summary.unpushed, 2);
    }

    #[test]
    fn test_serialization() {
        let summary = Summary {
            total: 5,
            dirty: 2,
            clean: 3,
            stale: 1,
            unpushed: 0,
        };
        let json = serde_json::to_string(&summary).expect("should serialize");
        assert!(json.contains("\"total\":5"));
        assert!(json.contains("\"dirty\":2"));
        assert!(json.contains("\"stale\":1"));
    }

    #[test]
    fn test_print_colored_does_not_panic() {
        let summary = Summary {
            total: 3,
            dirty: 1,
            clean: 2,
            stale: 0,
            unpushed: 1,
        };
        // Just verify it doesn't panic
        summary.print_colored();
    }
}
