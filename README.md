# devpulse

**Project health dashboard for your terminal.**

Scan a directory of projects and instantly see the health of each one — git status, last activity, stale repos, dirty worktrees — all in a colored terminal table.

No servers, no config files required. Just run `devpulse`.

## Why?

If you work on multiple projects, it's easy to lose track of which repos have uncommitted changes, which are weeks behind, and which you forgot about entirely. `devpulse` gives you that overview in one command.

## Installation

### Homebrew (macOS / Linux)

```bash
brew install deelo-ai/tap/devpulse
```

### From crates.io

```bash
cargo install devpulse
```

### Pre-built binaries

Download the latest binary for your platform from [GitHub Releases](https://github.com/deelo-ai/devpulse/releases).

```bash
# macOS (Apple Silicon)
curl -L https://github.com/deelo-ai/devpulse/releases/latest/download/devpulse-aarch64-apple-darwin.tar.gz | tar xz
sudo mv devpulse /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/deelo-ai/devpulse/releases/latest/download/devpulse-x86_64-apple-darwin.tar.gz | tar xz
sudo mv devpulse /usr/local/bin/

# Linux (x86_64)
curl -L https://github.com/deelo-ai/devpulse/releases/latest/download/devpulse-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv devpulse /usr/local/bin/

# Linux (aarch64)
curl -L https://github.com/deelo-ai/devpulse/releases/latest/download/devpulse-aarch64-unknown-linux-gnu.tar.gz | tar xz
sudo mv devpulse /usr/local/bin/
```

## Quick Start

```bash
# Scan the current directory for projects
devpulse

# Scan a specific directory
devpulse ~/projects

# Show only dirty repos, sorted by name
devpulse --filter dirty --sort name

# Export as JSON
devpulse --json

# Watch mode — refresh every 30 seconds
devpulse --watch --interval 30
```

## Output

Each project shows:

| Column | Description |
|--------|-------------|
| Project | Repository name |
| Branch | Current branch |
| Status | Clean or dirty (with uncommitted file count) |
| Last Commit | Time since last commit, color-coded |
| Remote | Ahead/behind remote tracking branch |

Colors indicate staleness: **green** = active (< 1 week), **yellow** = aging (< 1 month), **red** = stale (> 1 month).

## Flags Reference

### Sorting

```bash
devpulse --sort activity   # Most stale first (default)
devpulse --sort name       # Alphabetical
devpulse --sort status     # Dirty repos first, then clean
```

### Output Formats

```bash
devpulse --format table      # Terminal table with colors (default)
devpulse --format json       # JSON output
devpulse --json              # Shorthand for --format json
devpulse --format csv        # Comma-separated values
devpulse --format markdown   # Markdown table
devpulse --format md         # Alias for markdown
```

### Filtering

Filter projects by criteria. Multiple filters can be combined:

```bash
devpulse --filter dirty           # Only repos with uncommitted changes
devpulse --filter clean           # Only clean repos
devpulse --filter stale           # Repos with no commits in 30+ days
devpulse --filter unpushed        # Repos ahead of remote
devpulse --filter name:api        # Repos with "api" in the name
devpulse -f dirty -f name:web     # Combine: dirty repos matching "web"
```

### Time Window

Show only projects with recent activity:

```bash
devpulse --since 7d              # Active in last 7 days
devpulse --since 2w              # Active in last 2 weeks
devpulse --since 1m              # Active in last month
devpulse --since 7d --include-empty  # Also include repos with no commits
```

### Scan Depth

Control how deep to search for git repositories:

```bash
devpulse --depth 0    # Check only the target directory itself
devpulse --depth 1    # Immediate children (default)
devpulse --depth 2    # Two levels deep (nested project structures)
```

### Grouping

Group projects by their parent directory, with per-group summary stats:

```bash
devpulse ~/code --depth 2 --group
```

### File Output

Write results to a file instead of stdout (ANSI colors are stripped automatically):

```bash
devpulse --output report.json --format json
devpulse -o status.csv --format csv
devpulse -o dashboard.md --format markdown
```

### Watch Mode

Continuously re-scan at an interval:

```bash
devpulse --watch                 # Refresh every 60 seconds
devpulse -w -i 30                # Refresh every 30 seconds
```

### Interactive TUI

Launch a terminal UI for browsing projects:

```bash
devpulse --tui
```

### Colors

```bash
devpulse --no-color              # Disable ANSI colors
NO_COLOR=1 devpulse              # Also works via environment variable
```

Color priority: `--no-color` flag > `NO_COLOR` env var > config file > default (colors on).

### Shell Completions

Generate tab-completion scripts for your shell:

```bash
# Bash — add to ~/.bashrc
devpulse completions bash >> ~/.bashrc

# Zsh — add to fpath
devpulse completions zsh > ~/.zfunc/_devpulse
# Then add to ~/.zshrc: fpath=(~/.zfunc $fpath); autoload -Uz compinit && compinit

# Fish
devpulse completions fish > ~/.config/fish/completions/devpulse.fish
```

After installation, restart your shell or source the config file. Then `devpulse <tab>` will complete flags, subcommands, and values.

## Configuration

Create a `.devpulse.toml` file in your project directory or home directory (`~/.devpulse.toml`):

```toml
# Directories to scan (used when no path argument given)
scan_paths = ["~/projects", "~/work"]

# Default sort order: "activity", "name", or "status"
sort = "name"

# Default output format: "table", "json", "csv", "markdown"
format = "table"

# Default --since duration
since = "30d"

# Scan depth (default: 1)
depth = 2

# Directories to ignore when scanning
ignore = ["node_modules", "target", ".archive"]

# Disable colors (default: true)
color = true
```

CLI flags always take priority over config file values.

## Building from Source

```bash
git clone https://github.com/deelo-ai/devpulse.git
cd devpulse
cargo build --release
```

## Contributing

Contributions are welcome! Please:

1. Fork the repo and create a feature branch
2. Run `cargo fmt` and `cargo clippy -- -D warnings` before committing
3. Add tests for new functionality
4. Open a PR with a clear description of the change

See [open issues](https://github.com/deelo-ai/devpulse/issues) for ideas.

## License

[MIT](LICENSE) — Copyright 2026 deelo-ai
