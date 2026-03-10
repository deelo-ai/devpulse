use std::io;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::types::ProjectStatus;
use crate::{git, scanner};

/// Application state for the TUI.
pub struct App {
    statuses: Vec<ProjectStatus>,
    table_state: TableState,
    should_quit: bool,
}

impl App {
    /// Create a new App by scanning the given directory.
    pub fn new(statuses: Vec<ProjectStatus>) -> Self {
        let mut table_state = TableState::default();
        if !statuses.is_empty() {
            table_state.select(Some(0));
        }
        Self {
            statuses,
            table_state,
            should_quit: false,
        }
    }

    /// Move selection up.
    pub fn previous(&mut self) {
        if self.statuses.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.statuses.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    /// Move selection down.
    pub fn next(&mut self) {
        if self.statuses.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= self.statuses.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    /// Get the currently selected project status, if any.
    pub fn selected_project(&self) -> Option<&ProjectStatus> {
        self.table_state
            .selected()
            .and_then(|i| self.statuses.get(i))
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

/// Render the TUI frame.
fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).split(frame.area());

    render_table(frame, app, chunks[0]);
    render_footer(frame, app, chunks[1]);
}

/// Render the project table.
fn render_table(frame: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let header_cells = [
        "Project",
        "Branch",
        "Status",
        "Changed",
        "Last Commit",
        "↑/↓",
        "Remote",
    ]
    .iter()
    .map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    });
    let header = Row::new(header_cells).height(1);

    let rows = app.statuses.iter().map(|s| {
        let status_style = if s.is_clean {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Yellow)
        };

        let last_commit_str = match s.last_commit {
            Some(dt) => format_relative_time(dt),
            None => "no commits".to_string(),
        };

        let ahead_behind = if s.ahead == 0 && s.behind == 0 {
            "—".to_string()
        } else {
            format!("↑{} ↓{}", s.ahead, s.behind)
        };

        let has_remote = if s.remote_url.is_some() { "✓" } else { "—" };

        Row::new(vec![
            Cell::from(s.name.clone()),
            Cell::from(s.branch.clone()),
            Cell::from(if s.is_clean { "clean" } else { "dirty" }).style(status_style),
            Cell::from(format!("{}", s.changed_files)),
            Cell::from(last_commit_str),
            Cell::from(ahead_behind),
            Cell::from(has_remote),
        ])
    });

    let widths = [
        Constraint::Min(15),
        Constraint::Min(12),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" devpulse — Project Health Dashboard "),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

/// Render the footer with key hints and selected project info.
fn render_footer(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let selected_info = match app.selected_project() {
        Some(p) => match &p.remote_url {
            Some(url) => format!("  {}  │  {}", p.name, url),
            None => format!("  {}  │  no remote", p.name),
        },
        None => String::new(),
    };

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Navigate  "),
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Open URL  "),
        Span::styled(
            " q ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Quit"),
        Span::styled(&selected_info, Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::ALL));

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
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                app.should_quit = true;
                return Ok(false);
            }
            KeyCode::Down | KeyCode::Char('j') => app.next(),
            KeyCode::Up | KeyCode::Char('k') => app.previous(),
            KeyCode::Enter => {
                app.open_selected_url()?;
            }
            _ => {}
        }
    }
    Ok(true)
}

/// Run the interactive TUI.
pub fn run_tui(scan_path: &Path) -> Result<()> {
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
            terminal.draw(|f| render(f, &mut app))?;
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
}
