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

    let projects = scanner::discover_projects(&scan_path)?;
    println!("Found {} projects:\n", projects.len());
    for project in &projects {
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        println!("  {}", name);
    }

    Ok(())
}
