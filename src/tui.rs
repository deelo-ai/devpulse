use std::io;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
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

/// Application state for the TUI.
pub struct App {
    statuses: Vec<ProjectStatus>,
    table_state: TableState,
    list_state: ListState,
    should_quit: bool,
    search_mode: bool,
    search_query: String,
    filtered_indices: Vec<usize>,
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
        } else {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.should_quit = true;
                    return Ok(false);
                }
                KeyCode::Char('/') => {
                    app.enter_search_mode();
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
}
