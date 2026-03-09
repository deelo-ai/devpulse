mod git;
mod scanner;
mod table;
mod types;

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
                  devpulse --sort name  Sort projects alphabetically"
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    let scan_path = if cli.path.is_absolute() {
        cli.path
    } else {
        std::env::current_dir()?.join(&cli.path)
    };

    println!("Scanning {}...\n", scan_path.display());

    let project_paths = scanner::discover_projects(&scan_path)?;

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

    // Sort based on --sort flag
    match cli.sort {
        SortBy::Activity => {
            // Most stale first (oldest commit first, None at the top)
            statuses.sort_by(|a, b| a.last_commit.cmp(&b.last_commit));
        }
        SortBy::Name => {
            statuses.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        SortBy::Status => {
            // Dirty first, then clean; within same status, by name
            statuses.sort_by(|a, b| {
                a.is_clean
                    .cmp(&b.is_clean)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
    }

    if cli.json {
        let json = serde_json::to_string_pretty(&statuses)?;
        println!("{json}");
    } else {
        table::print_table(&statuses);
    }

    Ok(())
}
