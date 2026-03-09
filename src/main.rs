mod filter;
mod git;
mod scanner;
mod summary;
mod table;
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

    /// Output results as JSON instead of a table
    #[arg(long)]
    json: bool,

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
}

#[derive(Clone, ValueEnum)]
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

fn scan_and_display(
    scan_path: &std::path::Path,
    sort: &SortBy,
    json: bool,
    filters: &[filter::ProjectFilter],
) -> Result<()> {
    let project_paths = scanner::discover_projects(scan_path)?;

    if project_paths.is_empty() {
        println!(
            "No projects found in {}.\n\
             Hint: devpulse looks for directories containing a .git folder.",
            scan_path.display()
        );
        return Ok(());
    }

    let mut statuses = Vec::new();
    for path in &project_paths {
        match git::get_project_status(path) {
            Ok(status) => statuses.push(status),
            Err(e) => eprintln!("  Warning: skipping {}: {}", path.display(), e),
        }
    }

    let mut statuses = filter::apply_filters(statuses, filters);

    sort_statuses(&mut statuses, sort);

    if json {
        let summary = summary::Summary::from_statuses(&statuses);
        let output = serde_json::json!({
            "projects": statuses,
            "summary": summary,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        table::print_table(&statuses);
        let summary = summary::Summary::from_statuses(&statuses);
        summary.print_colored();
        println!();
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

    let filters = parse_filters(&cli.filter)?;

    if cli.tui {
        tui::run_tui(&scan_path)?;
    } else if cli.watch {
        watch::run_watch_loop(&scan_path, &cli.sort, cli.json, cli.interval, &filters)?;
    } else {
        println!("Scanning {}...\n", scan_path.display());
        scan_and_display(&scan_path, &cli.sort, cli.json, &filters)?;
    }

    Ok(())
}
