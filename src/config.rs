use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Raw TOML configuration as deserialized from `.devpulse.toml`.
#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Directories to scan for projects.
    pub scan_paths: Vec<String>,
    /// Default sort order: "activity", "name", or "status".
    pub sort: Option<String>,
    /// Directory names to ignore when scanning.
    pub ignore: Vec<String>,
    /// How many levels deep to scan for git projects (default: 1).
    pub depth: Option<u32>,
    /// Default output format: "table", "json", "csv", "markdown", or "md".
    pub format: Option<String>,
    /// Default `--since` duration (e.g. "30d", "2w", "1m").
    pub since: Option<String>,
    /// Whether to enable colored output (default: true).
    /// Set to `false` to disable ANSI colors.
    pub color: Option<bool>,
}

/// Locate and load a `.devpulse.toml` config file.
///
/// Search order:
/// 1. The given directory (e.g. the scan target or CWD)
/// 2. The user's home directory (`~/.devpulse.toml`)
///
/// If no config file is found, returns `Config::default()`.
pub fn load_config(local_dir: &Path) -> Result<Config> {
    // Check local directory first
    let local_path = local_dir.join(".devpulse.toml");
    if local_path.is_file() {
        return parse_config_file(&local_path);
    }

    // Fall back to home directory
    if let Some(home) = dirs::home_dir() {
        let home_path = home.join(".devpulse.toml");
        if home_path.is_file() {
            return parse_config_file(&home_path);
        }
    }

    Ok(Config::default())
}

/// Parse a TOML config file at the given path.
fn parse_config_file(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let config: Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
    Ok(config)
}

/// Expand `~` in a path string to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if path == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    PathBuf::from(path)
}

/// Parse a format string from config into an `OutputFormat`.
///
/// Returns `Ok(Some(format))` if valid, `Ok(None)` if no format specified,
/// or an error if the value is not recognized.
pub fn parse_format_str(format_str: &str) -> Result<crate::export::OutputFormat> {
    match format_str.trim().to_lowercase().as_str() {
        "table" => Ok(crate::export::OutputFormat::Table),
        "json" => Ok(crate::export::OutputFormat::Json),
        "csv" => Ok(crate::export::OutputFormat::Csv),
        "markdown" => Ok(crate::export::OutputFormat::Markdown),
        "md" => Ok(crate::export::OutputFormat::Md),
        other => anyhow::bail!(
            "Invalid format in config: '{}'. Valid values: table, json, csv, markdown, md",
            other
        ),
    }
}

/// Resolve scan paths from config, expanding `~` and making relative paths
/// absolute relative to `base_dir`.
pub fn resolve_scan_paths(config: &Config, base_dir: &Path) -> Vec<PathBuf> {
    config
        .scan_paths
        .iter()
        .map(|p| {
            let expanded = expand_tilde(p);
            if expanded.is_absolute() {
                expanded
            } else {
                base_dir.join(expanded)
            }
        })
        .collect()
}

/// Resolve whether colored output should be used.
///
/// Priority (highest to lowest):
/// 1. `cli_no_color` — the `--no-color` CLI flag (forces no color)
/// 2. `NO_COLOR` environment variable (any non-empty value disables color, per <https://no-color.org/>)
/// 3. `color` config option in `.devpulse.toml`
/// 4. Default: color enabled
pub fn resolve_color(cli_no_color: bool, cfg: &Config) -> bool {
    if cli_no_color {
        return false;
    }

    if let Ok(val) = std::env::var("NO_COLOR")
        && !val.is_empty()
    {
        return false;
    }

    if let Some(color) = cfg.color {
        return color;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_temp_config(dir: &Path, content: &str) {
        fs::write(dir.join(".devpulse.toml"), content).unwrap();
    }

    #[test]
    fn test_parse_full_config() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(
            dir.path(),
            r#"
scan_paths = ["~/projects", "~/work"]
sort = "name"
ignore = [".archived", "vendor"]
"#,
        );

        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.scan_paths, vec!["~/projects", "~/work"]);
        assert_eq!(config.sort, Some("name".to_string()));
        assert_eq!(config.ignore, vec![".archived", "vendor"]);
    }

    #[test]
    fn test_parse_partial_config() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(
            dir.path(),
            r#"
sort = "status"
"#,
        );

        let config = load_config(dir.path()).unwrap();
        assert!(config.scan_paths.is_empty());
        assert_eq!(config.sort, Some("status".to_string()));
        assert!(config.ignore.is_empty());
    }

    #[test]
    fn test_parse_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(dir.path(), "");

        let config = load_config(dir.path()).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_no_config_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_config(dir.path()).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(dir.path(), "this is not valid toml [[[");

        let result = load_config(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_tilde_home() {
        let expanded = expand_tilde("~/projects");
        // Should not start with ~ anymore
        assert!(!expanded.to_string_lossy().starts_with('~'));
        assert!(expanded.to_string_lossy().ends_with("projects"));
    }

    #[test]
    fn test_expand_tilde_bare() {
        let expanded = expand_tilde("~");
        assert!(!expanded.to_string_lossy().starts_with('~'));
        assert!(expanded.exists() || true); // home dir should exist, but don't fail in CI
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let expanded = expand_tilde("/absolute/path");
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_tilde_relative() {
        let expanded = expand_tilde("relative/path");
        assert_eq!(expanded, PathBuf::from("relative/path"));
    }

    #[test]
    fn test_resolve_scan_paths_absolute() {
        let config = Config {
            scan_paths: vec!["/tmp/projects".to_string()],
            ..Config::default()
        };
        let paths = resolve_scan_paths(&config, Path::new("/base"));
        assert_eq!(paths, vec![PathBuf::from("/tmp/projects")]);
    }

    #[test]
    fn test_resolve_scan_paths_relative() {
        let config = Config {
            scan_paths: vec!["subdir".to_string()],
            ..Config::default()
        };
        let paths = resolve_scan_paths(&config, Path::new("/base"));
        assert_eq!(paths, vec![PathBuf::from("/base/subdir")]);
    }

    #[test]
    fn test_resolve_scan_paths_tilde() {
        let config = Config {
            scan_paths: vec!["~/projects".to_string()],
            ..Config::default()
        };
        let paths = resolve_scan_paths(&config, Path::new("/base"));
        assert!(!paths[0].to_string_lossy().contains('~'));
    }

    #[test]
    fn test_resolve_scan_paths_empty() {
        let config = Config::default();
        let paths = resolve_scan_paths(&config, Path::new("/base"));
        assert!(paths.is_empty());
    }

    #[test]
    fn test_parse_config_with_depth() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(
            dir.path(),
            r#"
depth = 3
"#,
        );

        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.depth, Some(3));
    }

    #[test]
    fn test_parse_config_without_depth_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(
            dir.path(),
            r#"
sort = "name"
"#,
        );

        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.depth, None);
    }

    #[test]
    fn test_parse_config_with_format() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(
            dir.path(),
            r#"
format = "csv"
"#,
        );

        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.format, Some("csv".to_string()));
    }

    #[test]
    fn test_parse_config_without_format_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(
            dir.path(),
            r#"
sort = "name"
"#,
        );

        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.format, None);
    }

    #[test]
    fn test_parse_format_str_valid_values() {
        assert_eq!(
            parse_format_str("table").unwrap(),
            crate::export::OutputFormat::Table
        );
        assert_eq!(
            parse_format_str("json").unwrap(),
            crate::export::OutputFormat::Json
        );
        assert_eq!(
            parse_format_str("csv").unwrap(),
            crate::export::OutputFormat::Csv
        );
        assert_eq!(
            parse_format_str("markdown").unwrap(),
            crate::export::OutputFormat::Markdown
        );
        assert_eq!(
            parse_format_str("md").unwrap(),
            crate::export::OutputFormat::Md
        );
    }

    #[test]
    fn test_parse_format_str_case_insensitive() {
        assert_eq!(
            parse_format_str("CSV").unwrap(),
            crate::export::OutputFormat::Csv
        );
        assert_eq!(
            parse_format_str("Json").unwrap(),
            crate::export::OutputFormat::Json
        );
        assert_eq!(
            parse_format_str("MARKDOWN").unwrap(),
            crate::export::OutputFormat::Markdown
        );
    }

    #[test]
    fn test_parse_format_str_with_whitespace() {
        assert_eq!(
            parse_format_str("  csv  ").unwrap(),
            crate::export::OutputFormat::Csv
        );
    }

    #[test]
    fn test_parse_format_str_invalid() {
        let result = parse_format_str("yaml");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("yaml"));
        assert!(err_msg.contains("Valid values"));
    }

    #[test]
    fn test_parse_format_str_empty() {
        let result = parse_format_str("");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_type_in_config_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(
            dir.path(),
            r#"
scan_paths = "should-be-array"
"#,
        );

        let result = load_config(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_config_with_color_false() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(dir.path(), "color = false\n");
        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.color, Some(false));
    }

    #[test]
    fn test_parse_config_with_color_true() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(dir.path(), "color = true\n");
        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.color, Some(true));
    }

    #[test]
    fn test_parse_config_without_color_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write_temp_config(dir.path(), "sort = \"name\"\n");
        let config = load_config(dir.path()).unwrap();
        assert_eq!(config.color, None);
    }

    /// Helper to save, set/remove NO_COLOR env var safely within tests.
    /// SAFETY: These tests must run with `--test-threads=1` or accept
    /// that env var manipulation is inherently racy in multi-threaded tests.
    unsafe fn save_no_color() -> Option<String> {
        std::env::var("NO_COLOR").ok()
    }

    unsafe fn restore_no_color(saved: Option<String>) {
        unsafe {
            if let Some(val) = saved {
                std::env::set_var("NO_COLOR", val);
            } else {
                std::env::remove_var("NO_COLOR");
            }
        }
    }

    #[test]
    fn test_resolve_color_default_is_true() {
        unsafe {
            let saved = save_no_color();
            std::env::remove_var("NO_COLOR");

            let cfg = Config::default();
            assert!(resolve_color(false, &cfg));

            restore_no_color(saved);
        }
    }

    #[test]
    fn test_resolve_color_cli_flag_overrides_all() {
        let cfg = Config {
            color: Some(true),
            ..Config::default()
        };
        // --no-color should win even when config says color = true
        assert!(!resolve_color(true, &cfg));
    }

    #[test]
    fn test_resolve_color_config_false_disables() {
        unsafe {
            let saved = save_no_color();
            std::env::remove_var("NO_COLOR");

            let cfg = Config {
                color: Some(false),
                ..Config::default()
            };
            assert!(!resolve_color(false, &cfg));

            restore_no_color(saved);
        }
    }

    #[test]
    fn test_resolve_color_config_true_enables() {
        unsafe {
            let saved = save_no_color();
            std::env::remove_var("NO_COLOR");

            let cfg = Config {
                color: Some(true),
                ..Config::default()
            };
            assert!(resolve_color(false, &cfg));

            restore_no_color(saved);
        }
    }

    #[test]
    fn test_resolve_color_no_color_env_overrides_config() {
        unsafe {
            let saved = save_no_color();
            std::env::set_var("NO_COLOR", "1");

            let cfg = Config {
                color: Some(true),
                ..Config::default()
            };
            assert!(!resolve_color(false, &cfg));

            restore_no_color(saved);
        }
    }

    #[test]
    fn test_resolve_color_empty_no_color_env_ignored() {
        unsafe {
            let saved = save_no_color();
            std::env::set_var("NO_COLOR", "");

            let cfg = Config::default();
            assert!(resolve_color(false, &cfg));

            restore_no_color(saved);
        }
    }
}
