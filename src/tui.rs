use std::io;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use git2::Repository;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, TableState};

use crate::ci::CiStatus;
use crate::theme::Theme;
use crate::types::ProjectStatus;
use crate::{git, scanner};

/// Which view is shown in the detail panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailMode {
    Summary,
    GitLog,
}

/// A single commit entry for the git log view.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub short_hash: String,
    pub message: String,
    pub relative_time: String,
    pub is_merge: bool,
    /// Seconds since epoch, used for "recent" styling.
    pub commit_epoch: i64,
}

/// Application state for the TUI.
pub struct App {
    statuses: Vec<ProjectStatus>,
    table_state: TableState,
    list_state: ListState,
    should_quit: bool,
    search_mode: bool,
    search_query: String,
    filtered_indices: Vec<usize>,
    detail_mode: DetailMode,
    log_entries: Vec<LogEntry>,
    log_scroll: usize,
    /// Track which project index the log was fetched for, to know when to re-fetch.
    log_project_idx: Option<usize>,
}

impl App {
    /// Create a new App by scanning the given directory.
    pub fn new(statuses: Vec<ProjectStatus>) -> Self {
        let mut table_state = TableState::default();
        let mut list_state = ListState::default();
        let filtered_indices: Vec<usize> = (0..statuses.len()).collect();
        if !statuses.is_empty() {
            table_state.select(Some(0));
            list_state.select(Some(0));
        }
        Self {
            statuses,
            table_state,
            list_state,
            should_quit: false,
            search_mode: false,
            search_query: String::new(),
            filtered_indices,
            detail_mode: DetailMode::Summary,
            log_entries: Vec::new(),
            log_scroll: 0,
            log_project_idx: None,
        }
    }

    /// Move selection up within the filtered list.
    pub fn previous(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_indices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
        self.list_state.select(Some(i));
    }

    /// Move selection down within the filtered list.
    pub fn next(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered_indices.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
        self.list_state.select(Some(i));
    }

    /// Get the currently selected project status, if any.
    pub fn selected_project(&self) -> Option<&ProjectStatus> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_indices.get(i))
            .and_then(|&idx| self.statuses.get(idx))
    }

    /// Enter search mode: clear the query and show all projects.
    pub fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.rebuild_filtered_indices();
    }

    /// Exit search mode: clear the query and show all projects.
    pub fn exit_search_mode(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.rebuild_filtered_indices();
    }

    /// Rebuild the filtered indices based on the current search query.
    fn rebuild_filtered_indices(&mut self) {
        let query = self.search_query.to_lowercase();
        self.filtered_indices = self
            .statuses
            .iter()
            .enumerate()
            .filter(|(_, s)| query.is_empty() || s.name.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        // Reset selection to first filtered item
        if self.filtered_indices.is_empty() {
            self.table_state.select(None);
            self.list_state.select(None);
        } else {
            self.table_state.select(Some(0));
            self.list_state.select(Some(0));
        }
    }

    /// Open the selected project's remote URL in the default browser.
    pub fn open_selected_url(&self) -> Result<()> {
        if let Some(project) = self.selected_project()
            && let Some(ref url) = project.remote_url
        {
            open_url(url)?;
        }
        Ok(())
    }

    /// Toggle the detail panel to git log mode, or back to summary.
    pub fn toggle_git_log(&mut self) {
        match self.detail_mode {
            DetailMode::Summary => {
                self.detail_mode = DetailMode::GitLog;
                self.log_scroll = 0;
                self.refresh_log_if_needed();
            }
            DetailMode::GitLog => {
                self.detail_mode = DetailMode::Summary;
            }
        }
    }

    /// Fetch the git log for the currently selected project (if changed).
    pub fn refresh_log_if_needed(&mut self) {
        let current_idx = self
            .list_state
            .selected()
            .and_then(|i| self.filtered_indices.get(i).copied());

        if current_idx == self.log_project_idx && !self.log_entries.is_empty() {
            return; // already have log for this project
        }

        self.log_project_idx = current_idx;
        self.log_entries.clear();
        self.log_scroll = 0;

        if let Some(project) = self.selected_project() {
            self.log_entries = fetch_git_log(&project.path);
        }
    }

    /// Scroll the git log down by one line.
    pub fn scroll_log_down(&mut self) {
        if !self.log_entries.is_empty() {
            self.log_scroll = (self.log_scroll + 1).min(self.log_entries.len().saturating_sub(1));
        }
    }

    /// Scroll the git log up by one line.
    pub fn scroll_log_up(&mut self) {
        self.log_scroll = self.log_scroll.saturating_sub(1);
    }
}

/// Fetch the last 50 commits from a git repository using git2.
fn fetch_git_log(path: &Path) -> Vec<LogEntry> {
    let repo = match Repository::open(path) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Vec::new(), // empty repo
    };

    let head_oid = match head.target() {
        Some(oid) => oid,
        None => return Vec::new(),
    };

    let mut revwalk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(_) => return Vec::new(),
    };

    if revwalk.push(head_oid).is_err() {
        return Vec::new();
    }

    let mut entries = Vec::new();

    for oid_result in revwalk {
        if entries.len() >= 50 {
            break;
        }
        let oid = match oid_result {
            Ok(o) => o,
            Err(_) => continue,
        };
        let commit = match repo.find_commit(oid) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let short_hash = format!("{}", oid).chars().take(7).collect::<String>();

        let message = commit
            .message()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .to_string();

        let commit_epoch = commit.time().seconds();
        let dt = Utc
            .timestamp_opt(commit_epoch, 0)
            .single()
            .unwrap_or_else(Utc::now);
        let relative_time = format_relative_time(dt);

        let is_merge = message.starts_with("Merge");

        entries.push(LogEntry {
            short_hash,
            message,
            relative_time,
            is_merge,
            commit_epoch,
        });
    }

    entries
}

/// Open a URL in the default browser.
fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .context("Failed to open URL")?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("Failed to open URL")?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .context("Failed to open URL")?;
    }
    Ok(())
}

/// Scan projects from the given path and return their statuses.
/// Uses rayon for parallel git status collection across projects.
pub fn scan_projects(scan_path: &Path) -> Result<Vec<ProjectStatus>> {
    use rayon::prelude::*;
    let project_paths = scanner::discover_projects(scan_path)?;
    let statuses: Vec<ProjectStatus> = project_paths
        .par_iter()
        .filter_map(|path| git::get_project_status(path).ok())
        .collect();
    Ok(statuses)
}

/// Format a relative time string from a chrono DateTime.
fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now - dt;
    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{}m ago", minutes)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else if days < 30 {
        format!("{}d ago", days)
    } else if days < 365 {
        format!("{}mo ago", days / 30)
    } else {
        format!("{}y ago", days / 365)
    }
}

/// Render the TUI frame with split-pane layout.
fn render(frame: &mut ratatui::Frame, app: &mut App, theme: &Theme) {
    // Vertical: header (1 line + borders = 3) | main area | footer (3)
    let outer = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .split(frame.area());

    render_header(frame, app, outer[0], theme);

    // Horizontal split: left pane ~35%, right pane ~65%
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    render_project_list(frame, app, panes[0], theme);
    render_detail_panel(frame, app, panes[1], theme);
    render_footer(frame, app, outer[2], theme);
}

/// Render the header bar with summary stats.
fn render_header(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    let total = app.statuses.len();
    let filtered = app.filtered_indices.len();
    let dirty = app.statuses.iter().filter(|s| !s.is_clean).count();
    let stale = app.statuses.iter().filter(|s| is_stale(s)).count();

    let project_count = if filtered < total {
        format!("  {} of {} projects", filtered, total)
    } else {
        format!("  {} projects", total)
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "  devpulse",
            Style::default()
                .fg(theme.accent.to_ratatui())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            project_count,
            Style::default().fg(theme.header.to_ratatui()),
        ),
        Span::styled(
            format!("  {} dirty", dirty),
            Style::default().fg(theme.dirty.to_ratatui()),
        ),
        Span::styled(
            format!("  {} stale", stale),
            Style::default().fg(theme.stale.to_ratatui()),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, area);
}

/// Check if a project is stale (last commit > 30 days ago).
fn is_stale(status: &ProjectStatus) -> bool {
    match status.last_commit {
        Some(dt) => {
            let days = (chrono::Utc::now() - dt).num_days();
            days > 30
        }
        None => false,
    }
}

/// Render the left pane: project list with status dots.
fn render_project_list(frame: &mut ratatui::Frame, app: &mut App, area: Rect, theme: &Theme) {
    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&idx| {
            let s = &app.statuses[idx];
            let dot_color = if s.is_clean {
                theme.clean.to_ratatui()
            } else {
                theme.dirty.to_ratatui()
            };
            let line = Line::from(vec![
                Span::styled("● ", Style::default().fg(dot_color)),
                Span::raw(&s.name),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Projects "))
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg.to_ratatui())
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

/// Render the right pane: detail panel for the selected project.
fn render_detail_panel(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    match app.detail_mode {
        DetailMode::Summary => render_summary_panel(frame, app, area, theme),
        DetailMode::GitLog => render_git_log_panel(frame, app, area, theme),
    }
}

/// Render the summary view in the detail panel.
fn render_summary_panel(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::default().borders(Borders::ALL).title(" Details ");

    let Some(project) = app.selected_project() else {
        let empty = Paragraph::new("No project selected")
            .style(Style::default().fg(theme.dim.to_ratatui()))
            .block(block);
        frame.render_widget(empty, area);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Project name as title
    lines.push(Line::from(Span::styled(
        &project.name,
        Style::default()
            .fg(theme.accent.to_ratatui())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "──────────────────────────────────────",
        Style::default().fg(theme.dim.to_ratatui()),
    )));
    lines.push(Line::from(""));

    // Branch + upstream with ahead/behind
    let ahead_behind = if project.ahead == 0 && project.behind == 0 {
        String::new()
    } else {
        format!(" (↑{} ↓{})", project.ahead, project.behind)
    };
    let upstream = if project.remote_url.is_some() {
        format!("origin/{}", project.branch)
    } else {
        "no remote".to_string()
    };
    lines.push(Line::from(vec![
        Span::styled("Branch:    ", Style::default().fg(theme.dim.to_ratatui())),
        Span::styled(
            &project.branch,
            Style::default().fg(theme.header.to_ratatui()),
        ),
        Span::styled(
            format!(" → {upstream}{ahead_behind}"),
            Style::default().fg(theme.dim.to_ratatui()),
        ),
    ]));

    // Status with file count
    let (status_text, status_color) = if project.is_clean {
        ("clean".to_string(), theme.clean.to_ratatui())
    } else {
        (
            format!("● {} uncommitted files", project.changed_files),
            theme.dirty.to_ratatui(),
        )
    };
    lines.push(Line::from(vec![
        Span::styled("Status:    ", Style::default().fg(theme.dim.to_ratatui())),
        Span::styled(status_text, Style::default().fg(status_color)),
    ]));

    // Last commit
    let last_commit_str = match project.last_commit {
        Some(dt) => format_relative_time(dt),
        None => "no commits".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled("Last commit: ", Style::default().fg(theme.dim.to_ratatui())),
        Span::raw(&last_commit_str),
    ]));

    // Commit message
    if let Some(ref msg) = project.last_commit_message {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("\"{msg}\""),
                Style::default()
                    .fg(theme.header.to_ratatui())
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
    }

    lines.push(Line::from(""));

    // Remote URL (shortened)
    let remote_display = match &project.remote_url {
        Some(url) => url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url),
        None => "—",
    };
    lines.push(Line::from(vec![
        Span::styled("Remote:    ", Style::default().fg(theme.dim.to_ratatui())),
        Span::raw(remote_display),
    ]));

    // Stash count
    lines.push(Line::from(vec![
        Span::styled("Stashes:   ", Style::default().fg(theme.dim.to_ratatui())),
        Span::raw(format!("{}", project.stash_count)),
    ]));

    // CI status
    let (ci_text, ci_color) = match project.ci_status {
        CiStatus::Pass => ("Pass", theme.clean.to_ratatui()),
        CiStatus::Fail => ("Fail", theme.stale.to_ratatui()),
        CiStatus::Pending => ("Pending", theme.dirty.to_ratatui()),
        CiStatus::Unknown => ("—", theme.dim.to_ratatui()),
    };
    lines.push(Line::from(vec![
        Span::styled("CI:        ", Style::default().fg(theme.dim.to_ratatui())),
        Span::styled(ci_text, Style::default().fg(ci_color)),
    ]));

    let detail = Paragraph::new(lines).block(block);
    frame.render_widget(detail, area);
}

/// Render the git log view in the detail panel.
fn render_git_log_panel(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    let project_name = app
        .selected_project()
        .map(|p| p.name.as_str())
        .unwrap_or("—");
    let title = format!(" git log — {} ", project_name);
    let block = Block::default().borders(Borders::ALL).title(title);

    if app.log_entries.is_empty() {
        let empty = Paragraph::new("no commits found")
            .style(Style::default().fg(theme.dim.to_ratatui()))
            .block(block);
        frame.render_widget(empty, area);
        return;
    }

    // Available height inside the block (subtract 2 for top/bottom borders, 1 for status line)
    let inner_height = area.height.saturating_sub(4) as usize;
    let total = app.log_entries.len();
    let start = app.log_scroll;
    let end = (start + inner_height).min(total);
    let now_epoch = Utc::now().timestamp();

    let mut lines: Vec<Line> = Vec::new();
    for entry in &app.log_entries[start..end] {
        let is_recent = (now_epoch - entry.commit_epoch) < 86400; // < 1 day

        let hash_style = Style::default().fg(theme.accent.to_ratatui());
        let time_style = if is_recent {
            Style::default()
                .fg(theme.clean.to_ratatui())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.dim.to_ratatui())
        };
        let msg_style = if entry.is_merge {
            Style::default().fg(theme.dim.to_ratatui())
        } else {
            Style::default().fg(theme.header.to_ratatui())
        };

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(&entry.short_hash, hash_style),
            Span::raw("  "),
            Span::styled(format!("{:>8}", entry.relative_time), time_style),
            Span::raw("  "),
            Span::styled(&entry.message, msg_style),
        ]));
    }

    // Status line at the bottom
    lines.push(Line::from(""));
    let status = format!(
        "  showing {}-{} of {} commits",
        if total == 0 { 0 } else { start + 1 },
        end,
        total
    );
    lines.push(Line::from(Span::styled(
        status,
        Style::default().fg(theme.dim.to_ratatui()),
    )));

    let detail = Paragraph::new(lines).block(block);
    frame.render_widget(detail, area);
}

/// Render the footer with key hints or search bar.
fn render_footer(frame: &mut ratatui::Frame, app: &App, area: Rect, theme: &Theme) {
    let content = if app.search_mode {
        Line::from(vec![
            Span::styled(
                " / search: ",
                Style::default()
                    .fg(theme.accent.to_ratatui())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(&app.search_query),
            Span::styled("█", Style::default().fg(theme.accent.to_ratatui())),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " ↑↓ ",
                Style::default()
                    .fg(theme.accent.to_ratatui())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Navigate  "),
            Span::styled(
                " / ",
                Style::default()
                    .fg(theme.accent.to_ratatui())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Search  "),
            Span::styled(
                " Enter ",
                Style::default()
                    .fg(theme.accent.to_ratatui())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open URL  "),
            Span::styled(
                " l ",
                Style::default()
                    .fg(theme.accent.to_ratatui())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Log  "),
            Span::styled(
                " q ",
                Style::default()
                    .fg(theme.accent.to_ratatui())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Quit"),
        ])
    };

    let footer = Paragraph::new(content).block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, area);
}

/// Handle keyboard input events. Returns Ok(true) if the app should continue.
fn handle_event(app: &mut App) -> Result<bool> {
    if event::poll(Duration::from_millis(250))?
        && let Event::Key(key) = event::read()?
    {
        if key.kind != KeyEventKind::Press {
            return Ok(true);
        }

        if app.search_mode {
            match key.code {
                KeyCode::Esc => {
                    app.exit_search_mode();
                }
                KeyCode::Backspace => {
                    app.search_query.pop();
                    app.rebuild_filtered_indices();
                }
                KeyCode::Char(c) => {
                    app.search_query.push(c);
                    app.rebuild_filtered_indices();
                }
                KeyCode::Down => app.next(),
                KeyCode::Up => app.previous(),
                KeyCode::Enter => {
                    // Exit search mode but keep the filter active
                    app.search_mode = false;
                }
                _ => {}
            }
        } else if app.detail_mode == DetailMode::GitLog {
            match key.code {
                KeyCode::Char('q') => {
                    app.should_quit = true;
                    return Ok(false);
                }
                KeyCode::Char('l') | KeyCode::Esc => {
                    app.detail_mode = DetailMode::Summary;
                }
                KeyCode::Down | KeyCode::Char('j') => app.scroll_log_down(),
                KeyCode::Up | KeyCode::Char('k') => app.scroll_log_up(),
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.should_quit = true;
                    return Ok(false);
                }
                KeyCode::Char('/') => {
                    app.enter_search_mode();
                }
                KeyCode::Char('l') => {
                    app.toggle_git_log();
                }
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.previous(),
                KeyCode::Enter => {
                    app.open_selected_url()?;
                }
                _ => {}
            }
        }
    }
    Ok(true)
}

/// Run the interactive TUI.
pub fn run_tui(scan_path: &Path, theme: &Theme) -> Result<()> {
    let statuses = scan_projects(scan_path)?;

    if statuses.is_empty() {
        println!(
            "No projects found in {}.\n\
             Hint: devpulse looks for directories containing a .git folder.",
            scan_path.display()
        );
        return Ok(());
    }

    let mut app = App::new(statuses);

    // Set up terminal
    terminal::enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    stdout
        .execute(EnterAlternateScreen)
        .context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Main loop
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| render(f, &mut app, theme))?;
            if !handle_event(&mut app)? {
                break;
            }
        }
        Ok(())
    })();

    // Restore terminal — always do this even if there was an error
    terminal::disable_raw_mode().context("Failed to disable raw mode")?;
    terminal
        .backend_mut()
        .execute(LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProjectStatus;
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_status(name: &str, is_clean: bool, remote_url: Option<&str>) -> ProjectStatus {
        ProjectStatus {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{}", name)),
            branch: "main".to_string(),
            is_clean,
            changed_files: if is_clean { 0 } else { 3 },
            last_commit: Some(Utc::now()),
            ahead: 0,
            behind: 0,
            remote_url: remote_url.map(|s| s.to_string()),
            stash_count: 0,
            last_commit_message: None,
            ci_status: crate::ci::CiStatus::Unknown,
        }
    }

    #[test]
    fn test_app_new_empty() {
        let app = App::new(vec![]);
        assert!(app.statuses.is_empty());
        assert!(app.table_state.selected().is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn test_app_new_selects_first() {
        let statuses = vec![make_status("alpha", true, None)];
        let app = App::new(statuses);
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_app_next_wraps_around() {
        let statuses = vec![
            make_status("a", true, None),
            make_status("b", true, None),
            make_status("c", true, None),
        ];
        let mut app = App::new(statuses);
        assert_eq!(app.table_state.selected(), Some(0));

        app.next();
        assert_eq!(app.table_state.selected(), Some(1));

        app.next();
        assert_eq!(app.table_state.selected(), Some(2));

        // Wrap around
        app.next();
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_app_previous_wraps_around() {
        let statuses = vec![
            make_status("a", true, None),
            make_status("b", true, None),
            make_status("c", true, None),
        ];
        let mut app = App::new(statuses);
        assert_eq!(app.table_state.selected(), Some(0));

        // Wrap to end
        app.previous();
        assert_eq!(app.table_state.selected(), Some(2));

        app.previous();
        assert_eq!(app.table_state.selected(), Some(1));

        app.previous();
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_app_next_on_empty() {
        let mut app = App::new(vec![]);
        app.next(); // should not panic
        assert!(app.table_state.selected().is_none());
    }

    #[test]
    fn test_app_previous_on_empty() {
        let mut app = App::new(vec![]);
        app.previous(); // should not panic
        assert!(app.table_state.selected().is_none());
    }

    #[test]
    fn test_selected_project() {
        let statuses = vec![
            make_status("alpha", true, Some("https://github.com/user/alpha")),
            make_status("beta", false, None),
        ];
        let app = App::new(statuses);
        let selected = app.selected_project().expect("should have selection");
        assert_eq!(selected.name, "alpha");
        assert_eq!(
            selected.remote_url.as_deref(),
            Some("https://github.com/user/alpha")
        );
    }

    #[test]
    fn test_selected_project_after_navigation() {
        let statuses = vec![
            make_status("first", true, None),
            make_status("second", true, Some("https://example.com")),
        ];
        let mut app = App::new(statuses);
        app.next();
        let selected = app.selected_project().expect("should have selection");
        assert_eq!(selected.name, "second");
    }

    #[test]
    fn test_selected_project_empty() {
        let app = App::new(vec![]);
        assert!(app.selected_project().is_none());
    }

    #[test]
    fn test_format_relative_time_just_now() {
        let now = Utc::now();
        assert_eq!(format_relative_time(now), "just now");
    }

    #[test]
    fn test_format_relative_time_minutes() {
        let then = Utc::now() - chrono::Duration::minutes(5);
        assert_eq!(format_relative_time(then), "5m ago");
    }

    #[test]
    fn test_format_relative_time_hours() {
        let then = Utc::now() - chrono::Duration::hours(3);
        assert_eq!(format_relative_time(then), "3h ago");
    }

    #[test]
    fn test_format_relative_time_days() {
        let then = Utc::now() - chrono::Duration::days(15);
        assert_eq!(format_relative_time(then), "15d ago");
    }

    #[test]
    fn test_format_relative_time_months() {
        let then = Utc::now() - chrono::Duration::days(60);
        assert_eq!(format_relative_time(then), "2mo ago");
    }

    #[test]
    fn test_format_relative_time_years() {
        let then = Utc::now() - chrono::Duration::days(400);
        assert_eq!(format_relative_time(then), "1y ago");
    }

    #[test]
    fn test_enter_search_mode() {
        let statuses = vec![
            make_status("alpha", true, None),
            make_status("beta", false, None),
        ];
        let mut app = App::new(statuses);
        assert!(!app.search_mode);

        app.enter_search_mode();
        assert!(app.search_mode);
        assert!(app.search_query.is_empty());
        assert_eq!(app.filtered_indices.len(), 2);
    }

    #[test]
    fn test_search_filters_list() {
        let statuses = vec![
            make_status("alpha", true, None),
            make_status("beta", false, None),
            make_status("alphabet", true, None),
        ];
        let mut app = App::new(statuses);
        app.enter_search_mode();

        app.search_query.push_str("alph");
        app.rebuild_filtered_indices();

        assert_eq!(app.filtered_indices.len(), 2);
        assert_eq!(app.filtered_indices, vec![0, 2]);
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_search_case_insensitive() {
        let statuses = vec![
            make_status("MyProject", true, None),
            make_status("other", false, None),
        ];
        let mut app = App::new(statuses);
        app.enter_search_mode();

        app.search_query.push_str("myproj");
        app.rebuild_filtered_indices();

        assert_eq!(app.filtered_indices.len(), 1);
        assert_eq!(app.filtered_indices[0], 0);
    }

    #[test]
    fn test_esc_clears_search() {
        let statuses = vec![
            make_status("alpha", true, None),
            make_status("beta", false, None),
        ];
        let mut app = App::new(statuses);
        app.enter_search_mode();
        app.search_query.push_str("alpha");
        app.rebuild_filtered_indices();
        assert_eq!(app.filtered_indices.len(), 1);

        app.exit_search_mode();
        assert!(!app.search_mode);
        assert!(app.search_query.is_empty());
        assert_eq!(app.filtered_indices.len(), 2);
    }

    #[test]
    fn test_empty_query_shows_all() {
        let statuses = vec![
            make_status("alpha", true, None),
            make_status("beta", false, None),
            make_status("gamma", true, None),
        ];
        let mut app = App::new(statuses);
        app.enter_search_mode();

        // Empty query → all visible
        assert_eq!(app.filtered_indices.len(), 3);

        // Type something, then delete it
        app.search_query.push_str("alpha");
        app.rebuild_filtered_indices();
        assert_eq!(app.filtered_indices.len(), 1);

        app.search_query.clear();
        app.rebuild_filtered_indices();
        assert_eq!(app.filtered_indices.len(), 3);
    }

    #[test]
    fn test_navigation_within_filtered_list() {
        let statuses = vec![
            make_status("alpha", true, None),
            make_status("beta", false, None),
            make_status("alphabet", true, None),
        ];
        let mut app = App::new(statuses);
        app.enter_search_mode();
        app.search_query.push_str("alph");
        app.rebuild_filtered_indices();

        // Filtered list has 2 items (indices 0, 2)
        assert_eq!(app.list_state.selected(), Some(0));
        assert_eq!(app.selected_project().unwrap().name, "alpha");

        app.next();
        assert_eq!(app.list_state.selected(), Some(1));
        assert_eq!(app.selected_project().unwrap().name, "alphabet");

        // Wrap around
        app.next();
        assert_eq!(app.list_state.selected(), Some(0));
        assert_eq!(app.selected_project().unwrap().name, "alpha");
    }

    #[test]
    fn test_search_no_matches() {
        let statuses = vec![
            make_status("alpha", true, None),
            make_status("beta", false, None),
        ];
        let mut app = App::new(statuses);
        app.enter_search_mode();
        app.search_query.push_str("zzz");
        app.rebuild_filtered_indices();

        assert!(app.filtered_indices.is_empty());
        assert!(app.selected_project().is_none());
        assert!(app.list_state.selected().is_none());
    }

    // --- DetailMode and git log tests ---

    #[test]
    fn test_default_detail_mode_is_summary() {
        let app = App::new(vec![make_status("a", true, None)]);
        assert_eq!(app.detail_mode, DetailMode::Summary);
    }

    #[test]
    fn test_toggle_git_log_switches_mode() {
        let mut app = App::new(vec![make_status("a", true, None)]);
        assert_eq!(app.detail_mode, DetailMode::Summary);

        app.toggle_git_log();
        assert_eq!(app.detail_mode, DetailMode::GitLog);

        app.toggle_git_log();
        assert_eq!(app.detail_mode, DetailMode::Summary);
    }

    #[test]
    fn test_toggle_git_log_resets_scroll() {
        let mut app = App::new(vec![make_status("a", true, None)]);
        app.toggle_git_log();
        app.log_scroll = 5;

        // Toggle back and forth should reset scroll
        app.toggle_git_log(); // back to summary
        app.toggle_git_log(); // back to git log
        assert_eq!(app.log_scroll, 0);
    }

    #[test]
    fn test_scroll_log_down_clamps() {
        let mut app = App::new(vec![make_status("a", true, None)]);
        app.log_entries = vec![
            LogEntry {
                short_hash: "abc1234".to_string(),
                message: "first".to_string(),
                relative_time: "1h ago".to_string(),
                is_merge: false,
                commit_epoch: 0,
            },
            LogEntry {
                short_hash: "def5678".to_string(),
                message: "second".to_string(),
                relative_time: "2h ago".to_string(),
                is_merge: false,
                commit_epoch: 0,
            },
        ];

        app.scroll_log_down();
        assert_eq!(app.log_scroll, 1);

        // Should clamp at last entry
        app.scroll_log_down();
        assert_eq!(app.log_scroll, 1);

        app.scroll_log_down();
        assert_eq!(app.log_scroll, 1);
    }

    #[test]
    fn test_scroll_log_up_clamps_at_zero() {
        let mut app = App::new(vec![make_status("a", true, None)]);
        app.log_entries = vec![LogEntry {
            short_hash: "abc1234".to_string(),
            message: "first".to_string(),
            relative_time: "1h ago".to_string(),
            is_merge: false,
            commit_epoch: 0,
        }];

        app.scroll_log_up();
        assert_eq!(app.log_scroll, 0);

        app.log_scroll = 3;
        app.scroll_log_up();
        assert_eq!(app.log_scroll, 2);
    }

    #[test]
    fn test_scroll_log_empty_entries() {
        let mut app = App::new(vec![make_status("a", true, None)]);
        // No log entries
        app.scroll_log_down();
        assert_eq!(app.log_scroll, 0);

        app.scroll_log_up();
        assert_eq!(app.log_scroll, 0);
    }

    #[test]
    fn test_log_entry_is_merge() {
        let merge_entry = LogEntry {
            short_hash: "abc1234".to_string(),
            message: "Merge branch 'feature'".to_string(),
            relative_time: "1h ago".to_string(),
            is_merge: true,
            commit_epoch: 0,
        };
        assert!(merge_entry.is_merge);

        let normal_entry = LogEntry {
            short_hash: "def5678".to_string(),
            message: "feat: add feature".to_string(),
            relative_time: "2h ago".to_string(),
            is_merge: false,
            commit_epoch: 0,
        };
        assert!(!normal_entry.is_merge);
    }

    #[test]
    fn test_fetch_git_log_with_real_repo() {
        use std::process::Command;
        use tempfile::TempDir;

        let dir = TempDir::new().expect("tmpdir");
        let repo = git2::Repository::init(dir.path()).expect("init");

        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Test User").expect("set name");
        config
            .set_str("user.email", "test@example.com")
            .expect("set email");

        // Create 3 commits
        for i in 0..3 {
            let filename = format!("file{}.txt", i);
            std::fs::write(dir.path().join(&filename), format!("content {}", i))
                .expect("write file");
            Command::new("git")
                .args(["add", &filename])
                .current_dir(dir.path())
                .status()
                .expect("git add");
            Command::new("git")
                .args(["commit", "-m", &format!("commit {}", i)])
                .current_dir(dir.path())
                .status()
                .expect("git commit");
        }

        let entries = fetch_git_log(dir.path());
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].message, "commit 2"); // most recent first
        assert_eq!(entries[1].message, "commit 1");
        assert_eq!(entries[2].message, "commit 0");
        assert!(!entries[0].is_merge);
        assert_eq!(entries[0].short_hash.len(), 7);
    }

    #[test]
    fn test_fetch_git_log_empty_repo() {
        let dir = tempfile::TempDir::new().expect("tmpdir");
        git2::Repository::init(dir.path()).expect("init");

        let entries = fetch_git_log(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_fetch_git_log_not_a_repo() {
        let dir = tempfile::TempDir::new().expect("tmpdir");
        let entries = fetch_git_log(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_fetch_git_log_merge_commit() {
        use std::process::Command;
        use tempfile::TempDir;

        let dir = TempDir::new().expect("tmpdir");
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(dir.path())
            .status()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir.path())
            .status()
            .expect("config");
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .status()
            .expect("config");

        // Initial commit
        std::fs::write(dir.path().join("a.txt"), "a").expect("write");
        Command::new("git")
            .args(["add", "a.txt"])
            .current_dir(dir.path())
            .status()
            .expect("add");
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .status()
            .expect("commit");

        // Create a branch and commit
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .status()
            .expect("checkout");
        std::fs::write(dir.path().join("b.txt"), "b").expect("write");
        Command::new("git")
            .args(["add", "b.txt"])
            .current_dir(dir.path())
            .status()
            .expect("add");
        Command::new("git")
            .args(["commit", "-m", "feature work"])
            .current_dir(dir.path())
            .status()
            .expect("commit");

        // Go back to main and merge
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .status()
            .expect("checkout");
        Command::new("git")
            .args([
                "merge",
                "--no-ff",
                "feature",
                "-m",
                "Merge branch 'feature'",
            ])
            .current_dir(dir.path())
            .status()
            .expect("merge");

        let entries = fetch_git_log(dir.path());
        assert!(entries.len() >= 2);
        assert!(entries[0].is_merge);
        assert!(entries[0].message.starts_with("Merge"));
    }
}
