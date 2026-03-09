# devpulse — Project Health Dashboard for Your Terminal

## What It Does

`devpulse` scans a directory of projects and displays a TUI dashboard showing the health of each:

- **Git status** — clean, dirty, ahead/behind remote, stale branches
- **Last activity** — time since last commit (color-coded: green < 1 week, yellow < 1 month, red > 1 month)
- **CI status** — GitHub Actions pass/fail (if it's a GitHub repo)
- **Dependency freshness** — outdated deps count (Cargo.toml, package.json, pyproject.toml, go.mod)
- **Repo size** — lines of code, number of files

## Architecture

Single Rust binary. No server, no config files needed. Just run `devpulse` or `devpulse ~/projects`.

### Core Modules

```
src/
├── main.rs          # CLI entry point (clap)
├── scanner.rs       # Walks directory, discovers projects
├── git.rs           # Git status checks (uses git2 crate)
├── ci.rs            # GitHub Actions status (gh CLI or GitHub API)
├── deps.rs          # Dependency freshness checking
├── tui.rs           # Terminal UI (ratatui)
└── types.rs         # Shared types
```

### Dependencies

- `clap` — CLI argument parsing
- `git2` — Git operations (libgit2 bindings, no shelling out)
- `ratatui` + `crossterm` — Terminal UI
- `tokio` — Async runtime (for parallel scanning + API calls)
- `reqwest` — HTTP client (GitHub API)
- `serde` + `serde_json` — JSON parsing
- `chrono` — Time handling
- `dirs` — Home directory resolution

## Development Rules

- **Incremental development.** One feature at a time, verify it works before moving on.
- **Each GitHub issue = one small, testable piece of work.**
- **Compile and test after every change.** No big bang commits.
- **Error handling:** Use `anyhow` for application errors. Never panic in production code.
- **No unwrap() in production code.** Use `?` or proper error handling.
- **Clippy clean.** Run `cargo clippy` before committing.
- **Format.** Run `cargo fmt` before committing.

## Build & Run

```bash
cargo build           # Dev build
cargo run             # Run (scans current directory's parent)
cargo run -- ~/projects  # Scan specific directory
cargo clippy          # Lint
cargo fmt             # Format
cargo test            # Test
```

## MVP Scope (v0.1.0)

The MVP is a **non-interactive table view** (not full TUI yet):

1. Scan a directory for projects (look for .git directories)
2. For each project, gather:
   - Git status (clean/dirty, branch, ahead/behind)
   - Last commit timestamp
   - Uncommitted file count
3. Display a colored table in the terminal
4. Sort by last activity (most stale first)

That's it. No CI, no deps, no TUI interactions yet. Ship the simplest useful thing first.

## Future Features (post-MVP)

- Interactive TUI with keyboard navigation
- CI status from GitHub Actions
- Dependency freshness checking
- Watch mode (auto-refresh)
- Config file for custom project paths
- Export to JSON/markdown
- Git branch cleanup suggestions
