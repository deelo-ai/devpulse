mod git;
mod scanner;
mod types;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

/// devpulse — Project Health Dashboard for Your Terminal
#[derive(Parser)]
#[command(name = "devpulse", version, about)]
struct Cli {
    /// Directory to scan for projects (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let scan_path = if cli.path.is_absolute() {
        cli.path
    } else {
        std::env::current_dir()?.join(&cli.path)
    };

    println!("Scanning {}...", scan_path.display());

    let project_paths = scanner::discover_projects(&scan_path)?;
    println!("Found {} projects\n", project_paths.len());

    let mut statuses = Vec::new();
    for path in &project_paths {
        match git::get_project_status(path) {
            Ok(status) => statuses.push(status),
            Err(e) => eprintln!("  Warning: skipping {}: {}", path.display(), e),
        }
    }

    for s in &statuses {
        let status_label = if s.is_clean { "clean" } else { "dirty" };
        let last = s
            .last_commit
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "no commits".to_string());
        println!(
            "  {} [{}] {} | {} changed | last commit: {} | ahead: {} behind: {}",
            s.name, s.branch, status_label, s.changed_files, last, s.ahead, s.behind
        );
    }

    Ok(())
}
