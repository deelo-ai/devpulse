# devpulse

**Project health dashboard for your terminal.**

Scan a directory of projects and instantly see the health of each one — git status, last activity, stale repos, dirty worktrees — all in a colored terminal table.

No servers, no config files. Just run `devpulse`.

![devpulse screenshot](https://github.com/deelo-ai/devpulse/assets/screenshot-placeholder.png)
<!-- TODO: Replace with actual screenshot -->

## Why?

If you work on multiple projects, it's easy to lose track of which repos have uncommitted changes, which are weeks behind, and which you forgot about entirely. `devpulse` gives you that overview in one command.

## Installation

### From source (cargo)

```bash
cargo install devpulse
```

### Pre-built binaries

Download the latest binary for your platform from [GitHub Releases](https://github.com/deelo-ai/devpulse/releases).

```bash
# Example: Linux x86_64
curl -L https://github.com/deelo-ai/devpulse/releases/latest/download/devpulse-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv devpulse /usr/local/bin/
```

### Homebrew (coming soon)

```bash
brew install deelo-ai/tap/devpulse
```

## Usage

```bash
# Scan the current directory for projects
devpulse

# Scan a specific directory
devpulse ~/projects

# Sort by name instead of activity
devpulse --sort name

# Sort by status (dirty repos first)
devpulse --sort status
```

### Output

Each project shows:

| Column | Description |
|--------|-------------|
| Project | Repository name |
| Branch | Current branch |
| Status | Clean or dirty (with uncommitted file count) |
| Last Commit | Time since last commit, color-coded |
| Remote | Ahead/behind remote tracking branch |

Colors indicate staleness: **green** = active (< 1 week), **yellow** = aging (< 1 month), **red** = stale (> 1 month).

## Building from source

```bash
git clone https://github.com/deelo-ai/devpulse.git
cd devpulse
cargo build --release
```

## Contributing

Contributions are welcome! Please:

1. Fork the repo and create a feature branch
2. Run `cargo fmt` and `cargo clippy` before committing
3. Add tests for new functionality
4. Open a PR with a clear description of the change

See [open issues](https://github.com/deelo-ai/devpulse/issues) for ideas on what to work on.

## License

[MIT](LICENSE) — Copyright 2026 deelo-ai
