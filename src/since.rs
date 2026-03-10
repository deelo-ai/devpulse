use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};

use crate::types::ProjectStatus;

/// A parsed duration used for the `--since` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinceDuration {
    /// The number of units.
    pub count: u64,
    /// The unit of time.
    pub unit: DurationUnit,
}

/// Supported time units for `--since`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    /// Days (`d`)
    Days,
    /// Weeks (`w`)
    Weeks,
    /// Months (`m`)
    Months,
}

impl SinceDuration {
    /// Convert to a `chrono::Duration`.
    ///
    /// Months are approximated as 30 days.
    pub fn to_chrono_duration(self) -> Duration {
        let days = match self.unit {
            DurationUnit::Days => self.count as i64,
            DurationUnit::Weeks => self.count as i64 * 7,
            DurationUnit::Months => self.count as i64 * 30,
        };
        Duration::days(days)
    }
}

/// Parse a duration string like `7d`, `2w`, `1m` into a `SinceDuration`.
///
/// The format is `<number><unit>` where unit is one of:
/// - `d` for days
/// - `w` for weeks
/// - `m` for months
///
/// # Errors
///
/// Returns an error if the format is invalid, the number is zero, or the unit
/// is not recognized.
pub fn parse_duration(s: &str) -> Result<SinceDuration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("Duration cannot be empty. Use format: <number><unit> (e.g. 7d, 2w, 1m)");
    }

    // Find where the numeric part ends
    let num_end = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i);

    let (num_str, unit_str) = match num_end {
        Some(i) if i > 0 => (&s[..i], &s[i..]),
        Some(0) => {
            bail!(
                "Invalid duration '{}': must start with a number. Use format: <number><unit> (e.g. 7d, 2w, 1m)",
                s
            );
        }
        None => {
            bail!(
                "Invalid duration '{}': missing unit. Use format: <number><unit> where unit is d (days), w (weeks), or m (months)",
                s
            );
        }
        // unreachable but satisfies the compiler
        _ => bail!("Invalid duration '{}'", s),
    };

    let count: u64 = num_str.parse().map_err(|_| {
        anyhow::anyhow!(
            "Invalid number in duration '{}'. Use format: <number><unit> (e.g. 7d, 2w, 1m)",
            s
        )
    })?;

    if count == 0 {
        bail!("Duration must be greater than zero. Got: '{}'", s);
    }

    let unit = match unit_str.trim().to_lowercase().as_str() {
        "d" | "day" | "days" => DurationUnit::Days,
        "w" | "week" | "weeks" => DurationUnit::Weeks,
        "m" | "month" | "months" => DurationUnit::Months,
        other => {
            bail!(
                "Unknown duration unit '{}'. Valid units: d (days), w (weeks), m (months)",
                other
            );
        }
    };

    Ok(SinceDuration { count, unit })
}

/// Filter projects to only include those with a last commit within the given duration.
///
/// Projects with no last commit are excluded unless `include_empty` is true.
pub fn filter_since(
    statuses: Vec<ProjectStatus>,
    since: &SinceDuration,
    now: DateTime<Utc>,
    include_empty: bool,
) -> Vec<ProjectStatus> {
    let cutoff = now - since.to_chrono_duration();
    statuses
        .into_iter()
        .filter(|s| match s.last_commit {
            Some(dt) => dt >= cutoff,
            None => include_empty,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProjectStatus;
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    fn make_project(name: &str, days_ago: Option<i64>) -> ProjectStatus {
        ProjectStatus {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{}", name)),
            branch: "main".to_string(),
            is_clean: true,
            changed_files: 0,
            last_commit: days_ago.map(|d| Utc::now() - Duration::days(d)),
            ahead: 0,
            behind: 0,
            remote_url: None,
            stash_count: 0,
            last_commit_message: None,
        }
    }

    // --- parse_duration tests ---

    #[test]
    fn test_parse_days() {
        let d = parse_duration("7d").unwrap();
        assert_eq!(d.count, 7);
        assert_eq!(d.unit, DurationUnit::Days);
    }

    #[test]
    fn test_parse_weeks() {
        let d = parse_duration("2w").unwrap();
        assert_eq!(d.count, 2);
        assert_eq!(d.unit, DurationUnit::Weeks);
    }

    #[test]
    fn test_parse_months() {
        let d = parse_duration("1m").unwrap();
        assert_eq!(d.count, 1);
        assert_eq!(d.unit, DurationUnit::Months);
    }

    #[test]
    fn test_parse_long_unit_names() {
        assert_eq!(parse_duration("3days").unwrap().unit, DurationUnit::Days);
        assert_eq!(parse_duration("1day").unwrap().unit, DurationUnit::Days);
        assert_eq!(parse_duration("2weeks").unwrap().unit, DurationUnit::Weeks);
        assert_eq!(parse_duration("1week").unwrap().unit, DurationUnit::Weeks);
        assert_eq!(
            parse_duration("6months").unwrap().unit,
            DurationUnit::Months
        );
        assert_eq!(parse_duration("1month").unwrap().unit, DurationUnit::Months);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(parse_duration("7D").unwrap().unit, DurationUnit::Days);
        assert_eq!(parse_duration("2W").unwrap().unit, DurationUnit::Weeks);
        assert_eq!(parse_duration("1M").unwrap().unit, DurationUnit::Months);
    }

    #[test]
    fn test_parse_with_whitespace() {
        let d = parse_duration("  7d  ").unwrap();
        assert_eq!(d.count, 7);
        assert_eq!(d.unit, DurationUnit::Days);
    }

    #[test]
    fn test_parse_empty_string() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn test_parse_no_number() {
        assert!(parse_duration("d").is_err());
    }

    #[test]
    fn test_parse_no_unit() {
        assert!(parse_duration("7").is_err());
    }

    #[test]
    fn test_parse_zero_duration() {
        assert!(parse_duration("0d").is_err());
    }

    #[test]
    fn test_parse_invalid_unit() {
        assert!(parse_duration("7x").is_err());
        assert!(parse_duration("3y").is_err());
        assert!(parse_duration("5h").is_err());
    }

    #[test]
    fn test_parse_large_number() {
        let d = parse_duration("365d").unwrap();
        assert_eq!(d.count, 365);
    }

    // --- to_chrono_duration tests ---

    #[test]
    fn test_chrono_duration_days() {
        let d = SinceDuration {
            count: 7,
            unit: DurationUnit::Days,
        };
        assert_eq!(d.to_chrono_duration().num_days(), 7);
    }

    #[test]
    fn test_chrono_duration_weeks() {
        let d = SinceDuration {
            count: 2,
            unit: DurationUnit::Weeks,
        };
        assert_eq!(d.to_chrono_duration().num_days(), 14);
    }

    #[test]
    fn test_chrono_duration_months() {
        let d = SinceDuration {
            count: 1,
            unit: DurationUnit::Months,
        };
        assert_eq!(d.to_chrono_duration().num_days(), 30);
    }

    // --- filter_since tests ---

    #[test]
    fn test_filter_includes_recent_projects() {
        let statuses = vec![
            make_project("recent", Some(3)),
            make_project("old", Some(60)),
        ];
        let since = parse_duration("7d").unwrap();
        let result = filter_since(statuses, &since, Utc::now(), false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "recent");
    }

    #[test]
    fn test_filter_excludes_no_commit_by_default() {
        let statuses = vec![make_project("empty", None), make_project("recent", Some(1))];
        let since = parse_duration("7d").unwrap();
        let result = filter_since(statuses, &since, Utc::now(), false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "recent");
    }

    #[test]
    fn test_filter_includes_no_commit_when_flag_set() {
        let statuses = vec![make_project("empty", None), make_project("recent", Some(1))];
        let since = parse_duration("7d").unwrap();
        let result = filter_since(statuses, &since, Utc::now(), true);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_boundary_exact_cutoff() {
        // Project committed exactly 7 days ago should be included (>= cutoff)
        let now = Utc::now();
        let statuses = vec![ProjectStatus {
            name: "boundary".to_string(),
            path: PathBuf::from("/tmp/boundary"),
            branch: "main".to_string(),
            is_clean: true,
            changed_files: 0,
            last_commit: Some(now - Duration::days(7)),
            ahead: 0,
            behind: 0,
            remote_url: None,
            stash_count: 0,
            last_commit_message: None,
        }];
        let since = parse_duration("7d").unwrap();
        let result = filter_since(statuses, &since, now, false);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_empty_input() {
        let statuses: Vec<ProjectStatus> = vec![];
        let since = parse_duration("7d").unwrap();
        let result = filter_since(statuses, &since, Utc::now(), false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_all_excluded() {
        let statuses = vec![
            make_project("old1", Some(60)),
            make_project("old2", Some(90)),
        ];
        let since = parse_duration("7d").unwrap();
        let result = filter_since(statuses, &since, Utc::now(), false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_all_included() {
        let statuses = vec![
            make_project("a", Some(1)),
            make_project("b", Some(2)),
            make_project("c", Some(3)),
        ];
        let since = parse_duration("30d").unwrap();
        let result = filter_since(statuses, &since, Utc::now(), false);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_weeks_duration() {
        let statuses = vec![
            make_project("recent", Some(10)),
            make_project("old", Some(30)),
        ];
        let since = parse_duration("2w").unwrap(); // 14 days
        let result = filter_since(statuses, &since, Utc::now(), false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "recent");
    }

    #[test]
    fn test_filter_months_duration() {
        let statuses = vec![
            make_project("recent", Some(20)),
            make_project("old", Some(45)),
        ];
        let since = parse_duration("1m").unwrap(); // 30 days
        let result = filter_since(statuses, &since, Utc::now(), false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "recent");
    }
}
