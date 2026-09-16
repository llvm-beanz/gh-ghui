//! Interactive terminal UI, built on ratatui.

use std::error::Error;
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
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};

type DynError = Box<dyn Error>;
type Tui = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Default, PartialEq, Eq)]
struct App {
    mode: Mode,
    command: String,
    should_quit: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Normal,
    Command,
}

impl App {
    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match self.mode {
            Mode::Normal => {
                if key.code == KeyCode::Char(':') {
                    self.command.clear();
                    self.mode = Mode::Command;
                }
            }
            Mode::Command => self.handle_command_key(key.code),
        }
    }

    fn handle_command_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                self.command.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                if self.command.trim() == "q" {
                    self.should_quit = true;
                } else {
                    self.command.clear();
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Backspace => {
                self.command.pop();
            }
            KeyCode::Char(character) => self.command.push(character),
            _ => {}
        }
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
pub fn run() -> Result<(), DynError> {
    let mut session = TerminalSession::start()?;
    let mut app = App::default();

    while !app.should_quit {
        session.terminal.draw(|frame| render(frame, &app))?;
        if let Event::Key(key) = event::read()? {
            app.handle_key(key);
        }
    }

    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let [content_area, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    let content = Paragraph::new("GitHub projects")
        .block(Block::default().borders(Borders::ALL).title(" ghui "));
    frame.render_widget(content, content_area);

    let status = match app.mode {
        Mode::Normal => Line::styled(
            " NORMAL ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
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

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn colon_enters_command_mode() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char(':')));

        assert_eq!(app.mode, Mode::Command);
        assert!(app.command.is_empty());
    }

    #[test]
    fn q_command_quits() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char(':')));
        app.handle_key(key(KeyCode::Char('q')));
        app.handle_key(key(KeyCode::Enter));

        assert!(app.should_quit);
    }

    #[test]
    fn escape_cancels_command() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char(':')));
        app.handle_key(key(KeyCode::Char('q')));

        app.handle_key(key(KeyCode::Esc));

        assert_eq!(app.mode, Mode::Normal);
        assert!(app.command.is_empty());
        assert!(!app.should_quit);
    }

    #[test]
    fn unknown_command_returns_to_normal_mode() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char(':')));
        app.handle_key(key(KeyCode::Char('x')));

        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.mode, Mode::Normal);
        assert!(app.command.is_empty());
        assert!(!app.should_quit);
    }

    #[test]
    fn control_c_quits_from_any_mode() {
        let mut app = App {
            mode: Mode::Command,
            ..Default::default()
        };

        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        assert!(app.should_quit);
    }

    #[test]
    fn renders_command_line_across_terminal_viewport() {
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App {
            mode: Mode::Command,
            command: "q".into(),
            should_quit: false,
        };

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("ghui"));
        assert!(screen.contains("GitHub projects"));
        assert!(screen.contains(":q"));
    }
}
