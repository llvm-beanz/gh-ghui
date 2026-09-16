//! Interactive terminal UI, built on ratatui.

use std::collections::HashMap;
use std::io::{self, Stdout};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};

use crate::commands::login;
use crate::github::{
    parse_project_url, DynError, GitHubProjectSource, Item, Kind, Project, ProjectSource,
};

type Tui = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Default, PartialEq, Eq)]
struct App {
    mode: Mode,
    command: String,
    project: Option<Project>,
    selected: Option<usize>,
    message: Option<String>,
    should_quit: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Normal,
    Command,
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Open(String),
}

impl App {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return None;
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key.code),
            Mode::Command => self.handle_command_key(key.code),
        }
    }

    fn handle_normal_key(&mut self, key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Char(':') => {
                self.command.clear();
                self.message = None;
                self.mode = Mode::Command;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Home | KeyCode::Char('g') => self.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.select_last(),
            _ => {}
        }
        None
    }

    fn handle_command_key(&mut self, key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Esc => {
                self.command.clear();
                self.mode = Mode::Normal;
                None
            }
            KeyCode::Enter => self.execute_command(),
            KeyCode::Backspace => {
                self.command.pop();
                None
            }
            KeyCode::Char(character) => {
                self.command.push(character);
                None
            }
            _ => None,
        }
    }

    fn execute_command(&mut self) -> Option<Action> {
        let command = self.command.trim().to_string();
        self.command.clear();
        self.mode = Mode::Normal;

        if command == "q" {
            self.should_quit = true;
            return None;
        }

        if let Some(url) = command.strip_prefix("e ").map(str::trim) {
            if !url.is_empty() {
                return Some(Action::Open(url.to_string()));
            }
        }

        self.message = Some(format!("Not an editor command: {command}"));
        None
    }

    fn move_selection(&mut self, amount: isize) {
        let Some(last_index) = self.last_item_index() else {
            self.selected = None;
            return;
        };
        let selected = self.selected.unwrap_or_default();
        self.selected = Some(selected.saturating_add_signed(amount).min(last_index));
    }

    fn select_first(&mut self) {
        self.selected = self.last_item_index().map(|_| 0);
    }

    fn select_last(&mut self) {
        self.selected = self.last_item_index();
    }

    fn last_item_index(&self) -> Option<usize> {
        self.project
            .as_ref()
            .and_then(|project| project.items.len().checked_sub(1))
    }
}

trait TokenProvider {
    fn token(&self) -> Option<String>;
}

struct SystemTokenProvider;

impl TokenProvider for SystemTokenProvider {
    fn token(&self) -> Option<String> {
        std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|token| !token.is_empty())
            .or_else(|| login::get_token().ok())
    }
}

struct TerminalSession {
    terminal: Tui,
}

impl TerminalSession {
    fn start() -> Result<Self, DynError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            disable_raw_mode()?;
            return Err(error.into());
        }

        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(error.into())
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Run the interactive UI until the user exits it.
pub fn run(initial_url: Option<&str>) -> Result<(), DynError> {
    let source = GitHubProjectSource::new()?;
    let token_provider = SystemTokenProvider;
    let mut session = TerminalSession::start()?;
    let mut app = App::default();

    if let Some(url) = initial_url {
        open_project_with_progress(
            &mut session.terminal,
            &mut app,
            url,
            &token_provider,
            &source,
        )?;
    }

    while !app.should_quit {
        session.terminal.draw(|frame| render(frame, &app))?;
        if let Event::Key(key) = event::read()? {
            if let Some(Action::Open(url)) = app.handle_key(key) {
                open_project_with_progress(
                    &mut session.terminal,
                    &mut app,
                    &url,
                    &token_provider,
                    &source,
                )?;
            }
        }
    }

    Ok(())
}

fn open_project_with_progress(
    terminal: &mut Tui,
    app: &mut App,
    url: &str,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
) -> Result<(), DynError> {
    app.message = Some("Opening project...".into());
    terminal.draw(|frame| render(frame, app))?;
    open_project(app, url, token_provider, source);
    Ok(())
}

fn open_project(
    app: &mut App,
    url: &str,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
) {
    let result = parse_project_url(url).and_then(|project_ref| {
        let token = token_provider
            .token()
            .ok_or("no GitHub token found; set GITHUB_TOKEN or run `ghui login`")?;
        source.fetch_project(&project_ref, &token)
    });

    match result {
        Ok(project) => {
            app.selected = (!project.items.is_empty()).then_some(0);
            app.project = Some(project);
            app.message = None;
        }
        Err(error) => app.message = Some(error.to_string()),
    }
}

fn render(frame: &mut Frame, app: &App) {
    let [content_area, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    match &app.project {
        Some(project) => render_project(frame, content_area, project, app.selected),
        None => frame.render_widget(
            Paragraph::new("No project open")
                .block(Block::default().borders(Borders::ALL).title(" ghui ")),
            content_area,
        ),
    }

    let status = match app.mode {
        Mode::Normal => normal_status(app.message.as_deref()),
        Mode::Command => Line::from(format!(":{}", app.command)),
    };
    frame.render_widget(Paragraph::new(status), status_area);

    if app.mode == Mode::Command {
        let cursor_x = status_area.x + 1 + app.command.chars().count() as u16;
        frame.set_cursor_position((
            cursor_x.min(status_area.right().saturating_sub(1)),
            status_area.y,
        ));
    }
}

fn render_project(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    project: &Project,
    selected: Option<usize>,
) {
    let block = Block::default().borders(Borders::ALL).title(format!(
        " {} - {} items ",
        project.title,
        project.items.len()
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [header_area, rows_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    let table = project_table(project);
    frame.render_widget(Paragraph::new(table.header), header_area);
    let rows =
        List::new(table.rows).highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(rows, rows_area, &mut state);
}

fn normal_status(message: Option<&str>) -> Line<'static> {
    let mode = ratatui::text::Span::styled(
        " NORMAL ",
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    match message {
        Some(message) => Line::from(vec![mode, " ".into(), message.to_string().into()]),
        None => Line::from(mode),
    }
}

struct ProjectTable {
    header: Vec<Line<'static>>,
    rows: Vec<ListItem<'static>>,
}

fn project_table(project: &Project) -> ProjectTable {
    let columns = field_columns(&project.items);
    let mut header: Vec<String> = vec!["#".into(), "Type".into(), "Title".into()];
    header.extend(columns.iter().cloned());

    let rows: Vec<Vec<String>> = project
        .items
        .iter()
        .map(|item| {
            let number = item
                .content
                .as_ref()
                .and_then(|content| content.number)
                .map(|number| number.to_string())
                .unwrap_or_else(|| "-".into());
            let title = item
                .content
                .as_ref()
                .and_then(|content| content.title.clone())
                .unwrap_or_else(|| "-".into());
            let values: HashMap<&str, &str> = item
                .fields
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            let mut row = vec![number, kind_label(item).to_string(), title];
            row.extend(columns.iter().map(|column| {
                values
                    .get(column.as_str())
                    .copied()
                    .unwrap_or_default()
                    .to_string()
            }));
            row
        })
        .collect();

    let mut widths: Vec<usize> = header.iter().map(|cell| cell.chars().count()).collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    let format_row = |cells: &[String]| {
        cells
            .iter()
            .enumerate()
            .map(|(index, cell)| format!("{:<width$}", cell, width = widths[index]))
            .collect::<Vec<_>>()
            .join("  ")
    };

    let header = vec![
        Line::styled(
            format_row(&header),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::from(
            widths
                .iter()
                .map(|width| "-".repeat(*width))
                .collect::<Vec<_>>()
                .join("  "),
        ),
    ];
    let rows = rows
        .iter()
        .map(|row| ListItem::new(format_row(row)))
        .collect();
    ProjectTable { header, rows }
}

fn field_columns(items: &[Item]) -> Vec<String> {
    let mut columns = Vec::new();
    for item in items {
        for (name, _) in &item.fields {
            if !columns.contains(name) {
                columns.push(name.clone());
            }
        }
    }
    columns
}

fn kind_label(item: &Item) -> &'static str {
    match item.content.as_ref().map(|content| content.kind) {
        Some(Kind::Issue) => "Issue",
        Some(Kind::PullRequest) => "PR",
        _ => "-",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::github::testing::MockProjectSource;
    use crate::github::Content;

    struct FixedToken(Option<String>);

    impl TokenProvider for FixedToken {
        fn token(&self) -> Option<String> {
            self.0.clone()
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_command(app: &mut App, command: &str) -> Option<Action> {
        app.handle_key(key(KeyCode::Char(':')));
        for character in command.chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter))
    }

    fn sample_project(item_count: usize) -> Project {
        Project {
            title: "Demo".into(),
            items: (0..item_count)
                .map(|index| Item {
                    content: Some(Content {
                        kind: Kind::Issue,
                        number: Some(index as u32 + 1),
                        title: Some(format!("Issue {}", index + 1)),
                        url: None,
                    }),
                    fields: vec![("Status".into(), "Todo".into())],
                })
                .collect(),
        }
    }

    #[test]
    fn q_command_quits() {
        let mut app = App::default();

        assert_eq!(type_command(&mut app, "q"), None);

        assert!(app.should_quit);
    }

    #[test]
    fn edit_command_returns_open_action() {
        let mut app = App::default();

        let action = type_command(&mut app, "e https://github.com/orgs/example/projects/1");

        assert_eq!(
            action,
            Some(Action::Open(
                "https://github.com/orgs/example/projects/1".into()
            ))
        );
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn opens_project_with_injected_dependencies() {
        let source = MockProjectSource::returning(sample_project(3));
        let mut app = App::default();

        open_project(
            &mut app,
            "https://github.com/orgs/example/projects/1",
            &FixedToken(Some("token".into())),
            &source,
        );

        assert_eq!(app.project.as_ref().unwrap().items.len(), 3);
        assert_eq!(source.requests()[0].1, "token");
        assert_eq!(app.selected, Some(0));
        assert_eq!(app.message, None);
    }

    #[test]
    fn failed_open_keeps_existing_project_and_shows_error() {
        let mut app = App {
            project: Some(sample_project(1)),
            ..Default::default()
        };

        open_project(
            &mut app,
            "https://github.com/orgs/example/projects/2",
            &FixedToken(Some("token".into())),
            &MockProjectSource::failing("request failed"),
        );

        assert_eq!(app.project.as_ref().unwrap().items.len(), 1);
        assert_eq!(app.message.as_deref(), Some("request failed"));
    }

    #[test]
    fn normal_mode_moves_selection() {
        let mut app = App {
            project: Some(sample_project(20)),
            selected: Some(0),
            ..Default::default()
        };

        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::PageDown));
        assert_eq!(app.selected, Some(11));

        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.selected, Some(10));

        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.selected, Some(0));

        app.handle_key(key(KeyCode::Char('G')));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.selected, Some(19));
    }

    #[test]
    fn escape_cancels_command() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char(':')));
        app.handle_key(key(KeyCode::Char('e')));

        app.handle_key(key(KeyCode::Esc));

        assert_eq!(app.mode, Mode::Normal);
        assert!(app.command.is_empty());
    }

    #[test]
    fn renders_loaded_project_and_command_line() {
        let backend = TestBackend::new(50, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App {
            mode: Mode::Command,
            command: "q".into(),
            project: Some(sample_project(2)),
            ..Default::default()
        };

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("Demo - 2 items"));
        assert!(screen.contains("Issue 1"));
        assert!(screen.contains(":q"));
    }

    #[test]
    fn scrolling_keeps_column_header_visible() {
        let backend = TestBackend::new(50, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App {
            project: Some(sample_project(10)),
            selected: Some(5),
            ..Default::default()
        };

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen
            .lines()
            .any(|line| line.contains('#') && line.contains("Type") && line.contains("Title")));
        assert!(screen.contains("Issue 6"));
        assert!(!screen.contains("Issue 1 "));

        let selected_row = buffer
            .content()
            .chunks(buffer.area.width as usize)
            .find(|row| {
                row.iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
                    .contains("Issue 6")
            })
            .unwrap();
        assert!(selected_row.iter().any(|cell| cell.bg == Color::Cyan));
    }
}
