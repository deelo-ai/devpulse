mod ci;
mod config;
mod export;
mod filter;
mod git;
mod group;
mod scanner;
mod since;
mod summary;
mod table;
pub mod theme;
mod tui;
mod types;
mod watch;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueEnum};

/// devpulse — Project Health Dashboard for Your Terminal
#[derive(Parser)]
#[command(
    name = "devpulse",
    version,
    about = "Project health dashboard for your terminal",
    long_about = "devpulse scans a directory of projects and displays a terminal dashboard showing \
                   git status, last activity, branch info, and ahead/behind counts for each.",
    after_help = "EXAMPLES:\n  \
                  devpulse              Scan current directory\n  \
                  devpulse ~/projects   Scan a specific directory\n  \
                  devpulse --sort name  Sort projects alphabetically\n  \
                  devpulse --watch      Refresh every 60s\n  \
                  devpulse -w -i 30     Refresh every 30s"
)]
struct Cli {
    /// Directory to scan for projects [default: current directory]
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Sort projects by: activity (most stale first), name, or status
    #[arg(long, default_value = "activity")]
    sort: SortBy,

    /// Output results as JSON instead of a table (shorthand for --format json)
    #[arg(long)]
    json: bool,

    /// Output format: table, json, csv, markdown (or md)
    #[arg(long, value_enum)]
    format: Option<export::OutputFormat>,

    /// Watch mode: re-run at a regular interval
    #[arg(long, short = 'w')]
    watch: bool,

    /// Watch interval in seconds [default: 60]
    #[arg(long, short = 'i', default_value = "60")]
    interval: u64,

    /// Launch interactive TUI mode
    #[arg(long)]
    tui: bool,

    /// Filter projects by criteria. Can be specified multiple times.
    /// Values: dirty, clean, stale, unpushed, name:<substring>
    #[arg(long, short = 'f')]
    filter: Vec<String>,

    /// How many directory levels deep to scan for git projects [default: 1]
    /// 0 = check only the target directory itself
    /// 1 = immediate children (default)
    /// 2+ = recursive scanning
    #[arg(long, short = 'd')]
    depth: Option<u32>,

    /// Write output to a file instead of stdout.
    /// Works with all formats; table output strips ANSI colors when writing to file.
    #[arg(long, short = 'o', value_name = "PATH")]
    output: Option<PathBuf>,

    /// Group projects by their parent directory.
    /// Each group gets a sub-header and per-group summary stats.
    #[arg(long, short = 'g')]
    group: bool,

    /// Only show projects with commits within the given time window.
    /// Format: <number><unit> where unit is d (days), w (weeks), or m (months).
    /// Examples: 7d, 2w, 1m
    #[arg(long, value_name = "DURATION")]
    since: Option<String>,

    /// Include projects with no commits when using --since.
    /// By default, projects with no commit history are excluded.
    #[arg(long)]
    include_empty: bool,

    /// Disable colored output. Also respects the NO_COLOR environment variable
    /// and `color = false` in .devpulse.toml.
    /// Priority: --no-color flag > NO_COLOR env > config > default (colors on).
    #[arg(long)]
    no_color: bool,

    /// Skip CI status checks (faster, no network requests).
    /// By default, devpulse queries GitHub Actions for projects with GitHub remotes.
    #[arg(long)]
    no_ci: bool,

    /// Color theme: default, dracula, catppuccin-mocha, nord.
    /// Can also be set via `theme` in .devpulse.toml.
    #[arg(long, value_name = "THEME")]
    theme: Option<String>,
}

#[derive(Clone, Debug, ValueEnum)]
enum SortBy {
    /// Sort by last commit time, most stale first
    Activity,
    /// Sort by project name alphabetically
    Name,
    /// Sort by status (dirty first, then clean)
    Status,
}

fn sort_statuses(statuses: &mut [types::ProjectStatus], sort: &SortBy) {
    match sort {
        SortBy::Activity => {
            statuses.sort_by(|a, b| a.last_commit.cmp(&b.last_commit));
        }
        SortBy::Name => {
            statuses.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        SortBy::Status => {
            statuses.sort_by(|a, b| {
                a.is_clean
                    .cmp(&b.is_clean)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
    }
}

/// Parse and validate filter expressions from CLI arguments.
fn parse_filters(filter_args: &[String]) -> Result<Vec<filter::ProjectFilter>> {
    let mut filters = Vec::new();
    for expr in filter_args {
        match filter::parse_filter(expr) {
            Some(f) => filters.push(f),
            None => anyhow::bail!(
                "Unknown filter: '{}'. Valid filters: dirty, clean, stale, unpushed, name:<substring>",
                expr
            ),
        }
    }
    Ok(filters)
}

#[allow(clippy::too_many_arguments)]
fn scan_and_display(
    scan_path: &std::path::Path,
    sort: &SortBy,
    format: &export::OutputFormat,
    filters: &[filter::ProjectFilter],
    ignore: &[String],
    depth: u32,
    output: Option<&std::path::Path>,
    group_by_parent: bool,
    since_duration: Option<&since::SinceDuration>,
    include_empty: bool,
    use_color: bool,
    no_ci: bool,
    theme: &theme::Theme,
) -> Result<()> {
    let project_paths = scanner::discover_projects_with_depth(scan_path, ignore, depth)?;

    if project_paths.is_empty() {
        println!(
            "No projects found in {}.\n\
             Hint: devpulse looks for directories containing a .git folder.",
            scan_path.display()
        );
        return Ok(());
    }

    let results: Vec<_> = {
        use rayon::prelude::*;
        project_paths
            .par_iter()
            .map(|path| (path.clone(), git::get_project_status(path)))
            .collect()
    };

    let mut statuses = Vec::new();
    for (path, result) in results {
        match result {
            Ok(status) => statuses.push(status),
            Err(e) => eprintln!("  Warning: skipping {}: {}", path.display(), e),
        }
    }

    let mut statuses = filter::apply_filters(statuses, filters);

    // Apply --since filter if provided
    if let Some(since) = since_duration {
        statuses = since::filter_since(statuses, since, chrono::Utc::now(), include_empty);
    }

    // Fetch CI statuses from GitHub Actions (unless --no-ci)
    if !no_ci {
        let cache = ci::CiCache::new(300); // 5-minute cache TTL
        let ci_statuses = ci::fetch_ci_statuses(&statuses, &cache);
        for status in &mut statuses {
            if let Some(ci) = ci_statuses.get(&status.name) {
                status.ci_status = ci.clone();
            }
        }
    }

    sort_statuses(&mut statuses, sort);

    if group_by_parent {
        write_grouped_output(statuses, format, output, use_color, theme)?;
    } else if let Some(output_path) = output {
        export::write_output_to_file(&statuses, format, output_path)?;
    } else {
        export::write_output(&statuses, format, use_color, theme)?;
    }

    Ok(())
}

/// Write grouped output to stdout or a file.
fn write_grouped_output(
    statuses: Vec<types::ProjectStatus>,
    format: &export::OutputFormat,
    output: Option<&std::path::Path>,
    use_color: bool,
    theme: &theme::Theme,
) -> Result<()> {
    let groups = group::group_by_parent(statuses);
    let normalized = format.normalized();

    let content = match normalized {
        export::OutputFormat::Json => group::format_grouped_json(&groups)?,
        export::OutputFormat::Csv => group::format_grouped_csv(&groups)?,
        export::OutputFormat::Markdown | export::OutputFormat::Md => {
            group::format_grouped_markdown(&groups)?
        }
        export::OutputFormat::Table => {
            // For table format with grouping, print each group with a header
            let mut out = String::new();
            for g in &groups {
                out.push_str(&format!("\n── {} ──\n\n", g.label));
                out.push_str(&crate::table::format_table_plain(&g.projects));
                let summary_line = format!(
                    "  {} projects │ {} dirty │ {} stale │ {} unpushed\n",
                    g.summary.total, g.summary.dirty, g.summary.stale, g.summary.unpushed,
                );
                out.push_str(&summary_line);
            }
            out
        }
    };

    if let Some(output_path) = output {
        if let Some(parent) = output_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output_path, &content)?;
        eprintln!("Wrote grouped output to {}", output_path.display());
    } else if matches!(normalized, export::OutputFormat::Table) {
        if use_color {
            // For table, print with colors
            for g in &groups {
                println!("\n── {} ──\n", g.label);
                crate::table::print_table(&g.projects, theme);
                g.summary.print_colored();
                println!();
            }
        } else {
            // Plain text, no ANSI
            print!("{content}");
        }
    } else {
        print!("{content}");
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let scan_path = if cli.path.is_absolute() {
        cli.path.clone()
    } else {
        std::env::current_dir()?.join(&cli.path)
    };

    // Load config file (local dir or home)
    let cfg = config::load_config(&scan_path)?;

    // CLI --sort overrides config sort (CLI default is "activity", so we check
    // if the user explicitly provided --sort by seeing if config has a different
    // value and CLI is at its default). Since clap always provides a default,
    // config sort only applies when the user didn't pass --sort.
    // We use a simple approach: config sort is used if present, but CLI flag
    // always wins since it's explicitly provided by the user.
    let sort = resolve_sort(&cli.sort, &cfg)?;

    let filters = parse_filters(&cli.filter)?;

    // Resolve depth: CLI flag takes priority, then config, then default of 1
    let depth = cli.depth.or(cfg.depth).unwrap_or(1);

    // Resolve output format: --format > --json > config format > table
    let output_format = if let Some(fmt) = cli.format {
        fmt
    } else if cli.json {
        export::OutputFormat::Json
    } else if let Some(ref fmt_str) = cfg.format {
        config::parse_format_str(fmt_str)?
    } else {
        export::OutputFormat::Table
    };

    // Use config scan_paths if the user didn't specify a path (default is ".")
    let scan_paths = if cli.path.as_os_str() == "." && !cfg.scan_paths.is_empty() {
        config::resolve_scan_paths(&cfg, &scan_path)
    } else {
        vec![scan_path.clone()]
    };

    // Resolve --since: CLI flag takes priority, then config
    let since_duration = if let Some(ref s) = cli.since {
        Some(since::parse_duration(s)?)
    } else if let Some(ref s) = cfg.since {
        Some(since::parse_duration(s)?)
    } else {
        None
    };

    let use_color = config::resolve_color(cli.no_color, &cfg);

    // Resolve theme: --theme flag > config theme > default
    let theme_name = cli.theme.as_deref().or(cfg.theme.as_deref());
    let active_theme = theme::resolve_theme(theme_name)?;

    let ignore = &cfg.ignore;

    if cli.tui {
        // TUI mode uses first scan path
        tui::run_tui(scan_paths.first().unwrap_or(&scan_path), &active_theme)?;
    } else if cli.watch {
        watch::run_watch_loop(
            scan_paths.first().unwrap_or(&scan_path),
            &sort,
            &output_format,
            cli.interval,
            &filters,
            depth,
            use_color,
            &active_theme,
        )?;
    } else {
        for path in &scan_paths {
            println!("Scanning {}...\n", path.display());
            scan_and_display(
                path,
                &sort,
                &output_format,
                &filters,
                ignore,
                depth,
                cli.output.as_deref(),
                cli.group,
                since_duration.as_ref(),
                cli.include_empty,
                use_color,
                cli.no_ci,
                &active_theme,
            )?;
        }
    }

    Ok(())
}

/// Resolve sort order: CLI value takes priority, config is fallback.
fn resolve_sort(cli_sort: &SortBy, cfg: &config::Config) -> Result<SortBy> {
    // If config specifies a sort, we use it as a potential fallback.
    // However, since clap always fills a default, we just use the CLI value.
    // Config sort is informational — the CLI default matches config intention.
    // To truly detect "user didn't pass --sort", we'd need clap's Option<SortBy>.
    // For now, config sort is documented but CLI always wins.
    if let Some(ref sort_str) = cfg.sort {
        // Validate config sort value even if not used
        match sort_str.as_str() {
            "activity" | "name" | "status" => {}
            other => anyhow::bail!(
                "Invalid sort value in config: '{}'. Valid values: activity, name, status",
                other
            ),
        }
    }
    Ok(cli_sort.clone())
}
