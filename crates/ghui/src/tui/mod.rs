//! Interactive terminal UI, built on ratatui.

mod history;
pub mod state;

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::{Frame, Terminal};
use unicode_width::UnicodeWidthStr;

use crate::commands::login;
use crate::github::{
    emoji_status, parse_project_url, DynError, EditableField, EditableFieldKind, FieldValue,
    GitHubProjectSource, Item, Kind, Project, ProjectSource, STATUS_COLUMN,
};
use state::{SessionState, ViewState};

type Tui = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct RuntimeTab {
    project: Option<Project>,
    view: ViewState,
}

#[derive(Debug, PartialEq, Eq)]
struct App {
    mode: Mode,
    command: String,
    command_history: Vec<String>,
    history_index: Option<usize>,
    history_draft: String,
    history_dirty: bool,
    tabs: Vec<RuntimeTab>,
    active_tab: usize,
    state_path: Option<PathBuf>,
    column_selected: usize,
    active_column: Option<String>,
    field_editor: Option<FieldEditor>,
    message: Option<String>,
    error_dialog: Option<String>,
    legend_dialog: bool,
    should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            command: String::new(),
            command_history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            history_dirty: false,
            tabs: vec![RuntimeTab::default()],
            active_tab: 0,
            state_path: None,
            column_selected: 0,
            active_column: None,
            field_editor: None,
            message: None,
            error_dialog: None,
            legend_dialog: false,
            should_quit: false,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Normal,
    Command,
    Columns,
    Active,
    EditField,
}

#[derive(Debug, PartialEq, Eq)]
enum FieldEditor {
    Input {
        field: EditableField,
        value: String,
    },
    Select {
        field: EditableField,
        selected: usize,
    },
}

#[derive(Debug, PartialEq)]
struct FieldUpdate {
    project_id: String,
    item_id: String,
    field_id: String,
    field_name: String,
    value: FieldValue,
    display_value: String,
}

#[derive(Debug, PartialEq)]
enum Action {
    Edit(String),
    Refresh,
    Write,
    WriteQuit,
    UpdateField(FieldUpdate),
    NewTab(Option<String>),
    CloseTab,
    NextTab,
    PreviousTab,
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

        if self.error_dialog.is_some() {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                self.error_dialog = None;
            }
            return None;
        }

        if self.legend_dialog {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('?')) {
                self.legend_dialog = false;
            }
            return None;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            return Some(Action::Refresh);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Tab {
            self.next_tab();
            return None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::BackTab {
            self.previous_tab();
            return None;
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key.code),
            Mode::Command => self.handle_command_key(key.code),
            Mode::Columns => self.handle_columns_key(key.code),
            Mode::Active => self.handle_active_key(key.code),
            Mode::EditField => self.handle_field_editor_key(key.code),
        }
    }

    fn handle_normal_key(&mut self, key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Char(':') => {
                self.command.clear();
                self.history_index = None;
                self.history_draft.clear();
                self.message = None;
                self.mode = Mode::Command;
            }
            KeyCode::Char('?') => self.legend_dialog = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Home | KeyCode::Char('g') => self.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.select_last(),
            KeyCode::Enter => self.activate_selected_row(),
            _ => {}
        }
        None
    }

    fn handle_command_key(&mut self, key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Esc => {
                self.command.clear();
                self.history_index = None;
                self.history_draft.clear();
                self.mode = Mode::Normal;
                None
            }
            KeyCode::Enter => self.execute_command(),
            KeyCode::Backspace => {
                self.detach_history();
                self.command.pop();
                None
            }
            KeyCode::Up => {
                self.previous_command();
                None
            }
            KeyCode::Down => {
                self.next_command();
                None
            }
            KeyCode::Char(character) => {
                self.detach_history();
                self.command.push(character);
                None
            }
            _ => None,
        }
    }

    fn handle_columns_key(&mut self, key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Esc | KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down => {
                let last = self.available_columns().len().saturating_sub(1);
                self.column_selected = self.column_selected.saturating_add(1).min(last);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.column_selected = self.column_selected.saturating_sub(1);
            }
            KeyCode::Char(' ') => self.toggle_selected_column(),
            KeyCode::Home | KeyCode::Char('g') => self.column_selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.column_selected = self.available_columns().len().saturating_sub(1);
            }
            _ => {}
        }
        None
    }

    fn handle_active_key(&mut self, key: KeyCode) -> Option<Action> {
        match key {
            KeyCode::Esc => {
                self.active_column = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => self.open_field_editor(),
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Tab => self.cycle_active_column(1),
            KeyCode::BackTab => self.cycle_active_column(-1),
            _ => {}
        }
        None
    }

    fn handle_field_editor_key(&mut self, key: KeyCode) -> Option<Action> {
        if key == KeyCode::Esc {
            self.field_editor = None;
            self.message = None;
            self.mode = Mode::Active;
            return None;
        }

        match self.field_editor.as_mut()? {
            FieldEditor::Input { value, .. } => match key {
                KeyCode::Enter => return self.field_update_action(),
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(character) => value.push(character),
                _ => {}
            },
            FieldEditor::Select { field, selected } => match key {
                KeyCode::Enter => return self.field_update_action(),
                KeyCode::Char('j') | KeyCode::Down => {
                    *selected = selected
                        .saturating_add(1)
                        .min(field.options.len().saturating_sub(1));
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                }
                KeyCode::Home | KeyCode::Char('g') => *selected = 0,
                KeyCode::End | KeyCode::Char('G') => {
                    *selected = field.options.len().saturating_sub(1);
                }
                _ => {}
            },
        }
        None
    }

    fn execute_command(&mut self) -> Option<Action> {
        let command = self.command.trim().to_string();
        self.command.clear();
        self.history_index = None;
        self.history_draft.clear();
        self.mode = Mode::Normal;

        self.history_dirty |= history::record(&mut self.command_history, command.clone());

        if command == "q" {
            self.should_quit = true;
            return None;
        }

        if command == "w" {
            return Some(Action::Write);
        }

        if command == "wq" {
            return Some(Action::WriteQuit);
        }

        if command == "refresh" {
            return Some(Action::Refresh);
        }

        if command == "legend" || command == "emoji" {
            self.legend_dialog = true;
            return None;
        }

        if command == "columns" {
            if self.project().is_some() {
                self.column_selected = 0;
                self.mode = Mode::Columns;
            } else {
                self.show_error("No project open");
            }
            return None;
        }

        if command == "tabnew" {
            return Some(Action::NewTab(None));
        }
        if let Some(target) = command.strip_prefix("tabnew ").map(str::trim) {
            if !target.is_empty() {
                return Some(Action::NewTab(Some(target.to_string())));
            }
        }
        if command == "tabnext" || command == "tabn" {
            return Some(Action::NextTab);
        }
        if command == "tabprevious" || command == "tabp" {
            return Some(Action::PreviousTab);
        }
        if command == "tabclose" || command == "tabc" {
            return Some(Action::CloseTab);
        }

        if let Some(target) = command.strip_prefix("e ").map(str::trim) {
            if !target.is_empty() {
                return Some(Action::Edit(target.to_string()));
            }
        }

        self.show_error(format!("Not an editor command: {command}"));
        None
    }

    fn move_selection(&mut self, amount: isize) {
        let Some(last_index) = self.last_item_index() else {
            self.view_mut().selected = None;
            return;
        };
        let selected = self.view().selected.unwrap_or_default();
        self.view_mut().selected = Some(selected.saturating_add_signed(amount).min(last_index));
    }

    fn previous_command(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let index = match self.history_index {
            Some(index) => index.saturating_sub(1),
            None => {
                self.history_draft = self.command.clone();
                self.command_history.len() - 1
            }
        };
        self.history_index = Some(index);
        self.command.clone_from(&self.command_history[index]);
    }

    fn next_command(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 < self.command_history.len() {
            let index = index + 1;
            self.history_index = Some(index);
            self.command.clone_from(&self.command_history[index]);
        } else {
            self.history_index = None;
            self.command.clone_from(&self.history_draft);
        }
    }

    fn detach_history(&mut self) {
        if self.history_index.take().is_some() {
            self.history_draft.clone_from(&self.command);
        }
    }

    fn select_first(&mut self) {
        self.view_mut().selected = self.last_item_index().map(|_| 0);
    }

    fn select_last(&mut self) {
        self.view_mut().selected = self.last_item_index();
    }

    fn last_item_index(&self) -> Option<usize> {
        self.project()
            .and_then(|project| project.items.len().checked_sub(1))
    }

    fn available_columns(&self) -> Vec<String> {
        self.project()
            .map(project_columns)
            .unwrap_or_default()
            .into_iter()
            .map(|column| column.name)
            .collect()
    }

    fn toggle_selected_column(&mut self) {
        let available = self.available_columns();
        let Some(column) = available.get(self.column_selected) else {
            return;
        };
        let enabled = self
            .view_mut()
            .columns
            .get_or_insert_with(|| available.clone());
        if enabled.contains(column) {
            enabled.retain(|candidate| candidate != column);
        } else {
            enabled.push(column.clone());
            enabled.sort_by_key(|candidate| {
                available
                    .iter()
                    .position(|available| available == candidate)
                    .unwrap_or(usize::MAX)
            });
        }
    }

    fn mutable_columns(&self) -> Vec<String> {
        let enabled = self.view().columns.as_deref();
        self.project()
            .map(project_columns)
            .unwrap_or_default()
            .into_iter()
            .filter(|column| {
                column.mutable && enabled.is_none_or(|enabled| enabled.contains(&column.name))
            })
            .map(|column| column.name)
            .collect()
    }

    fn activate_selected_row(&mut self) {
        if self.view().selected.is_none() {
            return;
        }
        let mut columns = self.mutable_columns().into_iter();
        if let Some(column) = columns.next() {
            self.active_column = Some(column);
            self.mode = Mode::Active;
        }
    }

    fn cycle_active_column(&mut self, amount: isize) {
        let columns = self.mutable_columns();
        if columns.is_empty() {
            self.active_column = None;
            self.mode = Mode::Normal;
            return;
        }
        let selected = self
            .active_column
            .as_ref()
            .and_then(|active| columns.iter().position(|column| column == active))
            .unwrap_or_default();
        let index = (selected as isize + amount).rem_euclid(columns.len() as isize) as usize;
        self.active_column = Some(columns[index].clone());
    }

    fn open_field_editor(&mut self) {
        let Some(project) = self.project() else {
            return;
        };
        let Some(field_name) = &self.active_column else {
            return;
        };
        let Some(field) = project
            .editable_fields
            .iter()
            .find(|field| &field.name == field_name)
            .cloned()
        else {
            self.show_error(format!("No editing metadata for {field_name}"));
            return;
        };
        let current = self
            .view()
            .selected
            .and_then(|index| project.items.get(index))
            .and_then(|item| item.fields.iter().find(|(name, _)| name == field_name))
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        self.field_editor = Some(match field.kind {
            EditableFieldKind::Text | EditableFieldKind::Number | EditableFieldKind::Date => {
                FieldEditor::Input {
                    field,
                    value: current,
                }
            }
            EditableFieldKind::SingleSelect | EditableFieldKind::Iteration => {
                let selected = field
                    .options
                    .iter()
                    .position(|option| option.name == current)
                    .unwrap_or_default();
                FieldEditor::Select { field, selected }
            }
        });
        self.message = None;
        self.mode = Mode::EditField;
    }

    fn field_update_action(&mut self) -> Option<Action> {
        let project = self.project()?;
        let item = project.items.get(self.view().selected?)?;
        let (field, value, display_value) = match self.field_editor.as_ref()? {
            FieldEditor::Input { field, value } => {
                let parsed = match field.kind {
                    EditableFieldKind::Text => FieldValue::Text(value.clone()),
                    EditableFieldKind::Number => match value.parse::<f64>() {
                        Ok(number) if number.is_finite() => FieldValue::Number(number),
                        _ => {
                            self.show_error("Enter a valid number");
                            return None;
                        }
                    },
                    EditableFieldKind::Date if valid_iso_date(value) => {
                        FieldValue::Date(value.clone())
                    }
                    EditableFieldKind::Date => {
                        self.show_error("Enter a date as YYYY-MM-DD");
                        return None;
                    }
                    _ => return None,
                };
                (field, parsed, value.clone())
            }
            FieldEditor::Select { field, selected } => {
                let option = field.options.get(*selected)?;
                let value = match field.kind {
                    EditableFieldKind::SingleSelect => FieldValue::SingleSelect(option.id.clone()),
                    EditableFieldKind::Iteration => FieldValue::Iteration(option.id.clone()),
                    _ => return None,
                };
                (field, value, option.name.clone())
            }
        };
        Some(Action::UpdateField(FieldUpdate {
            project_id: project.id.clone(),
            item_id: item.id.clone(),
            field_id: field.id.clone(),
            field_name: field.name.clone(),
            value,
            display_value,
        }))
    }

    fn tab(&self) -> &RuntimeTab {
        &self.tabs[self.active_tab]
    }

    fn tab_mut(&mut self) -> &mut RuntimeTab {
        &mut self.tabs[self.active_tab]
    }

    fn project(&self) -> Option<&Project> {
        self.tab().project.as_ref()
    }

    fn project_mut(&mut self) -> Option<&mut Project> {
        self.tab_mut().project.as_mut()
    }

    fn view(&self) -> &ViewState {
        &self.tab().view
    }

    fn view_mut(&mut self) -> &mut ViewState {
        &mut self.tab_mut().view
    }

    fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.leave_transient_mode();
    }

    fn new_tab(&mut self, duplicate_current: bool) {
        let tab = if duplicate_current {
            self.tab().clone()
        } else {
            RuntimeTab::default()
        };
        self.tabs.insert(self.active_tab + 1, tab);
        self.active_tab += 1;
        self.leave_transient_mode();
    }

    fn previous_tab(&mut self) {
        self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
        self.leave_transient_mode();
    }

    fn close_tab(&mut self) {
        if self.tabs.len() == 1 {
            self.tabs[0] = RuntimeTab::default();
            self.active_tab = 0;
        } else {
            self.tabs.remove(self.active_tab);
            self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        }
        self.leave_transient_mode();
    }

    fn leave_transient_mode(&mut self) {
        self.mode = Mode::Normal;
        self.active_column = None;
        self.field_editor = None;
    }

    fn session_state(&self) -> SessionState {
        SessionState::new(
            self.tabs.iter().map(|tab| tab.view.clone()).collect(),
            self.active_tab,
        )
    }

    fn show_error(&mut self, error: impl Into<String>) {
        self.message = None;
        self.error_dialog = Some(error.into());
    }
}

fn valid_iso_date(value: &str) -> bool {
    let mut parts = value.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        year.parse::<u32>(),
        month.parse::<u32>(),
        day.parse::<u32>(),
    ) else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
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
pub fn run(initial_target: Option<&str>) -> Result<(), DynError> {
    let source = GitHubProjectSource::new()?;
    let token_provider = SystemTokenProvider;
    let mut session = TerminalSession::start()?;
    let mut app = App::default();
    let history_path = history::default_path();

    if let Some(path) = history_path.as_deref() {
        match history::load(path) {
            Ok(entries) => app.command_history = entries,
            Err(error) => app.show_error(format!("Could not load command history: {error}")),
        }
    }

    if let Some(target) = initial_target {
        edit_target_with_progress(
            &mut session.terminal,
            &mut app,
            target,
            &token_provider,
            &source,
        )?;
    }

    while !app.should_quit {
        session.terminal.draw(|frame| render(frame, &app))?;
        if let Event::Key(key) = event::read()? {
            let action = app.handle_key(key);
            persist_command_history(&mut app, history_path.as_deref());
            match action {
                Some(Action::Edit(target)) => edit_target_with_progress(
                    &mut session.terminal,
                    &mut app,
                    &target,
                    &token_provider,
                    &source,
                )?,
                Some(Action::Refresh) => refresh_project_with_progress(
                    &mut session.terminal,
                    &mut app,
                    &token_provider,
                    &source,
                )?,
                Some(Action::Write) => {
                    write_and_maybe_quit(&mut app, false);
                }
                Some(Action::WriteQuit) => {
                    write_and_maybe_quit(&mut app, true);
                }
                Some(Action::UpdateField(update)) => {
                    update_field(&mut app, update, &token_provider, &source);
                }
                Some(Action::NewTab(target)) => {
                    app.new_tab(target.is_none());
                    if let Some(target) = target {
                        edit_target_with_progress(
                            &mut session.terminal,
                            &mut app,
                            &target,
                            &token_provider,
                            &source,
                        )?;
                    }
                }
                Some(Action::CloseTab) => app.close_tab(),
                Some(Action::NextTab) => app.next_tab(),
                Some(Action::PreviousTab) => app.previous_tab(),
                None => {}
            }
        }
    }

    Ok(())
}

fn persist_command_history(app: &mut App, path: Option<&Path>) {
    if !app.history_dirty {
        return;
    }
    app.history_dirty = false;
    let Some(path) = path else { return };
    if let Err(error) = history::save(path, &app.command_history) {
        app.show_error(format!("Could not save command history: {error}"));
    }
}

fn edit_target_with_progress(
    terminal: &mut Tui,
    app: &mut App,
    target: &str,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
) -> Result<(), DynError> {
    app.message = Some("Opening project...".into());
    app.error_dialog = None;
    terminal.draw(|frame| render(frame, app))?;
    edit_target(app, target, token_provider, source);
    Ok(())
}

fn refresh_project_with_progress(
    terminal: &mut Tui,
    app: &mut App,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
) -> Result<(), DynError> {
    app.message = Some("Refreshing project...".into());
    app.error_dialog = None;
    terminal.draw(|frame| render(frame, app))?;
    refresh_project(app, token_provider, source);
    Ok(())
}

fn refresh_project(app: &mut App, token_provider: &dyn TokenProvider, source: &dyn ProjectSource) {
    let Some(url) = app.view().project_url.clone() else {
        app.show_error("No project open");
        return;
    };
    let result = parse_project_url(&url).and_then(|project_ref| {
        let token = token_provider
            .token()
            .ok_or("no GitHub token found; set GITHUB_TOKEN or run `ghui login`")?;
        source.fetch_project(&project_ref, &token)
    });
    match result {
        Ok(project) => {
            let selected = match (app.view().selected, project.items.len().checked_sub(1)) {
                (Some(selected), Some(last_index)) => Some(selected.min(last_index)),
                _ => None,
            };
            app.tab_mut().project = Some(project);
            app.view_mut().selected = selected;
            app.leave_transient_mode();
            app.message = Some("Project refreshed".into());
        }
        Err(error) => app.show_error(error.to_string()),
    }
}

fn edit_target(
    app: &mut App,
    target: &str,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
) {
    if target.starts_with("https://") || target.starts_with("http://") {
        open_project(app, target, token_provider, source);
        return;
    }

    let path = Path::new(target);
    if path.exists() {
        match SessionState::load(path) {
            Ok(state) => {
                app.state_path = Some(path.to_path_buf());
                restore_session_state(app, state, token_provider, source);
            }
            Err(error) => app.show_error(error.to_string()),
        }
    } else {
        app.state_path = Some(path.to_path_buf());
        app.message = Some(format!("State will be saved to {}", path.display()));
    }
}

fn write_state(app: &mut App) -> bool {
    let Some(path) = &app.state_path else {
        app.show_error("No state path; use :e PATH first");
        return false;
    };

    match app.session_state().save(path) {
        Ok(()) => {
            app.message = Some(format!("Saved {}", path.display()));
            true
        }
        Err(error) => {
            app.show_error(error.to_string());
            false
        }
    }
}

fn write_and_maybe_quit(app: &mut App, quit: bool) {
    if write_state(app) && quit {
        app.should_quit = true;
    }
}

fn update_field(
    app: &mut App,
    update: FieldUpdate,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
) {
    let Some(token) = token_provider.token() else {
        app.show_error("No GitHub token found; set GITHUB_TOKEN or run `ghui login`");
        return;
    };
    match source.update_field(
        &update.project_id,
        &update.item_id,
        &update.field_id,
        update.value,
        &token,
    ) {
        Ok(()) => {
            if let Some(item) = app.project_mut().and_then(|project| {
                project
                    .items
                    .iter_mut()
                    .find(|item| item.id == update.item_id)
            }) {
                if let Some((_, value)) = item
                    .fields
                    .iter_mut()
                    .find(|(name, _)| name == &update.field_name)
                {
                    *value = update.display_value;
                } else {
                    item.fields
                        .push((update.field_name.clone(), update.display_value));
                }
            }
            app.field_editor = None;
            app.mode = Mode::Active;
            app.message = Some(format!("Updated {}", update.field_name));
        }
        Err(error) => app.show_error(error.to_string()),
    }
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
            app.view_mut().project_url = Some(url.to_string());
            app.view_mut().selected = (!project.items.is_empty()).then_some(0);
            app.tab_mut().project = Some(project);
            app.message = None;
        }
        Err(error) => app.show_error(error.to_string()),
    }
}

fn restore_session_state(
    app: &mut App,
    state: SessionState,
    token_provider: &dyn TokenProvider,
    source: &dyn ProjectSource,
) {
    let active_tab = state.active_tab;
    app.tabs = state
        .tabs
        .into_iter()
        .map(|view| RuntimeTab {
            project: None,
            view,
        })
        .collect();
    app.active_tab = active_tab.min(app.tabs.len() - 1);
    for index in 0..app.tabs.len() {
        app.active_tab = index;
        let Some(url) = app.view().project_url.clone() else {
            continue;
        };
        let saved_selection = app.view().selected;
        open_project(app, &url, token_provider, source);
        if app.error_dialog.is_none() {
            app.view_mut().selected = match (saved_selection, app.last_item_index()) {
                (Some(selected), Some(last_index)) => Some(selected.min(last_index)),
                _ => None,
            };
        }
    }
    app.active_tab = active_tab.min(app.tabs.len() - 1);
    app.leave_transient_mode();
}

fn render(frame: &mut Frame, app: &App) {
    let [tabs_area, content_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let titles = app.tabs.iter().enumerate().map(|(index, tab)| {
        let title = tab
            .project
            .as_ref()
            .map(|project| project.title.as_str())
            .or(tab.view.project_url.as_deref())
            .unwrap_or("New");
        Line::from(format!(" {}:{} ", index + 1, title))
    });
    frame.render_widget(
        Tabs::new(titles)
            .select(app.active_tab)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(""),
        tabs_area,
    );

    match app.project() {
        Some(project) => render_project(
            frame,
            content_area,
            project,
            app.view().selected,
            app.view().columns.as_deref(),
            (app.mode == Mode::Active)
                .then_some(app.active_column.as_deref())
                .flatten(),
        ),
        None => frame.render_widget(
            Paragraph::new("No project open")
                .block(Block::default().borders(Borders::ALL).title(" ghui ")),
            content_area,
        ),
    }

    let version = format!(" ghui v{} ", env!("CARGO_PKG_VERSION"));
    let [status_content_area, version_area] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(version.chars().count() as u16),
    ])
    .areas(status_area);
    let status = match app.mode {
        Mode::Normal => normal_status(app.message.as_deref()),
        Mode::Command => Line::from(format!(":{}", app.command)),
        Mode::Columns => Line::from(" COLUMNS  Space toggle  Enter/Esc close "),
        Mode::Active => Line::from(" ACTIVE  Tab cycle  Up/Down move  Enter/Esc close "),
        Mode::EditField => Line::from(
            app.message
                .as_deref()
                .unwrap_or(" EDIT FIELD  Enter save  Esc cancel "),
        ),
    };
    frame.render_widget(Paragraph::new(status), status_content_area);
    frame.render_widget(
        Paragraph::new(version).alignment(ratatui::layout::Alignment::Right),
        version_area,
    );

    if app.mode == Mode::Command {
        let cursor_x = status_content_area.x + 1 + app.command.chars().count() as u16;
        frame.set_cursor_position((
            cursor_x.min(status_content_area.right().saturating_sub(1)),
            status_content_area.y,
        ));
    }

    if app.mode == Mode::Columns {
        render_columns(frame, app);
    }
    if app.mode == Mode::EditField {
        render_field_editor(frame, app);
    }
    if let Some(error) = app.error_dialog.as_deref() {
        render_error_dialog(frame, error);
    }
    if app.legend_dialog {
        render_legend_dialog(frame);
    }
}

fn render_project(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    project: &Project,
    selected: Option<usize>,
    enabled_columns: Option<&[String]>,
    active_column: Option<&str>,
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
    let table = project_table(project, enabled_columns, selected, active_column);
    frame.render_widget(Paragraph::new(table.header), header_area);
    let mut rows = List::new(table.rows);
    if active_column.is_none() {
        rows = rows.highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
    }
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(rows, rows_area, &mut state);
}

fn render_columns(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 70, frame.area());
    frame.render_widget(Clear, area);
    let available = app.available_columns();
    let items = available.iter().map(|column| {
        let enabled = app
            .view()
            .columns
            .as_ref()
            .is_none_or(|columns| columns.contains(column));
        ListItem::new(format!("[{}] {column}", if enabled { "x" } else { " " }))
    });
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Columns "))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
    let mut state = ListState::default().with_selected(Some(app.column_selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_field_editor(frame: &mut Frame, app: &App) {
    let Some(editor) = &app.field_editor else {
        return;
    };
    let area = centered_rect(60, 35, frame.area());
    frame.render_widget(Clear, area);
    match editor {
        FieldEditor::Input { field, value } => {
            let hint = match field.kind {
                EditableFieldKind::Text => "Text",
                EditableFieldKind::Number => "Number",
                EditableFieldKind::Date => "Date (YYYY-MM-DD)",
                _ => "Value",
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} - {} ", field.name, hint));
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_widget(Paragraph::new(value.as_str()), inner);
            frame.set_cursor_position((
                (inner.x + value.chars().count() as u16).min(inner.right().saturating_sub(1)),
                inner.y,
            ));
        }
        FieldEditor::Select { field, selected } => {
            let items = field
                .options
                .iter()
                .map(|option| ListItem::new(option.name.clone()));
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" {} - Select value ", field.name)),
                )
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow));
            let mut state = ListState::default().with_selected(Some(*selected));
            frame.render_stateful_widget(list, area, &mut state);
        }
    }
}

fn render_error_dialog(frame: &mut Frame, error: &str) {
    let area = centered_rect(70, 40, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!("{error}\n\nEnter or Esc to dismiss"))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" Error "),
            ),
        area,
    );
}

fn render_legend_dialog(frame: &mut Frame) {
    let area = centered_rect(45, 55, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(
            "🟢  Open PR\n🛑  Closed PR\n🏁  Merged PR\n⚠️  Open Issue\n✅  Fixed Issue\n🏃  In Progress Issue\n❓  Duplicate Issue\n❌  Closed Issue\n\nEnter, Esc, or ? to dismiss",
        )
        .block(Block::default().borders(Borders::ALL).title(" Status legend ")),
        area,
    );
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    area: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let [vertical] = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(ratatui::layout::Flex::Center)
        .areas(area);
    let [horizontal] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(ratatui::layout::Flex::Center)
        .areas(vertical);
    horizontal
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

fn project_table(
    project: &Project,
    enabled_columns: Option<&[String]>,
    selected: Option<usize>,
    active_column: Option<&str>,
) -> ProjectTable {
    let columns = project_columns(project)
        .into_iter()
        .filter(|column| enabled_columns.is_none_or(|enabled| enabled.contains(&column.name)))
        .map(|column| column.name)
        .collect::<Vec<_>>();
    let header = columns.clone();

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
            columns
                .iter()
                .map(|column| match column.as_str() {
                    STATUS_COLUMN => emoji_status(item).to_string(),
                    "#" => number.clone(),
                    "Type" => kind_label(item).to_string(),
                    "Title" => title.clone(),
                    _ => values
                        .get(column.as_str())
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .collect();

    let mut widths: Vec<usize> = header.iter().map(|cell| cell.width()).collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.width());
        }
    }
    let format_row = |cells: &[String]| {
        cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                format!(
                    "{cell}{}",
                    " ".repeat(widths[index].saturating_sub(cell.width()))
                )
            })
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
        .enumerate()
        .map(|(row_index, row)| {
            let spans = row.iter().enumerate().map(|(column_index, cell)| {
                let suffix = if column_index + 1 == row.len() {
                    ""
                } else {
                    "  "
                };
                let content = format!(
                    "{cell}{}{suffix}",
                    " ".repeat(widths[column_index].saturating_sub(cell.width()))
                );
                if selected == Some(row_index)
                    && active_column == Some(columns[column_index].as_str())
                {
                    Span::styled(
                        content,
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else if selected == Some(row_index) && active_column.is_some() {
                    Span::styled(content, Style::default().fg(Color::Black).bg(Color::Cyan))
                } else {
                    Span::raw(content)
                }
            });
            ListItem::new(Line::from(spans.collect::<Vec<_>>()))
        })
        .collect();
    ProjectTable { header, rows }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectColumn {
    name: String,
    mutable: bool,
}

fn project_columns(project: &Project) -> Vec<ProjectColumn> {
    let mut columns = [STATUS_COLUMN, "#", "Type", "Title"]
        .into_iter()
        .map(|name| ProjectColumn {
            name: name.into(),
            mutable: false,
        })
        .collect::<Vec<_>>();
    columns.extend(
        project
            .field_names
            .iter()
            .filter(|name| name.as_str() != STATUS_COLUMN)
            .map(|name| ProjectColumn {
                name: name.clone(),
                mutable: project.mutable_field_names.contains(name),
            }),
    );
    for column in field_columns(&project.items) {
        if !columns.iter().any(|candidate| candidate.name == column) {
            columns.push(ProjectColumn {
                name: column,
                mutable: true,
            });
        }
    }
    columns
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use ratatui::backend::TestBackend;

    use super::*;
    use crate::github::testing::MockProjectSource;
    use crate::github::{Content, FieldOption};

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
            id: "project-id".into(),
            title: "Demo".into(),
            field_names: vec!["Status".into()],
            mutable_field_names: vec!["Status".into()],
            editable_fields: vec![EditableField {
                id: "status-field".into(),
                name: "Status".into(),
                kind: EditableFieldKind::SingleSelect,
                options: vec![
                    FieldOption {
                        id: "todo-option".into(),
                        name: "Todo".into(),
                    },
                    FieldOption {
                        id: "done-option".into(),
                        name: "Done".into(),
                    },
                ],
            }],
            items: (0..item_count)
                .map(|index| Item {
                    id: format!("item-{index}"),
                    content: Some(Content {
                        kind: Kind::Issue,
                        state: crate::github::ContentState::Open,
                        state_reason: None,
                        number: Some(index as u32 + 1),
                        title: Some(format!("Issue {}", index + 1)),
                        url: None,
                    }),
                    fields: vec![("Status".into(), "Todo".into())],
                })
                .collect(),
        }
    }

    fn temp_state_path(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{unique}.json", std::process::id()))
    }

    fn app_with_project(project: Project, view: ViewState) -> App {
        App {
            tabs: vec![RuntimeTab {
                project: Some(project),
                view,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn q_command_quits() {
        let mut app = App::default();

        assert_eq!(type_command(&mut app, "q"), None);

        assert!(app.should_quit);
    }

    #[test]
    fn legend_opens_from_shortcut_and_commands_and_blocks_input() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('?')));
        assert!(app.legend_dialog);
        app.handle_key(key(KeyCode::Char(':')));
        assert_eq!(app.mode, Mode::Normal);
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.legend_dialog);

        type_command(&mut app, "legend");
        assert!(app.legend_dialog);
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.legend_dialog);

        type_command(&mut app, "emoji");
        assert!(app.legend_dialog);

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .chunks(60)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        for description in [
            "Open PR",
            "Closed PR",
            "Merged PR",
            "Open Issue",
            "Fixed Issue",
            "In Progress Issue",
            "Duplicate Issue",
            "Closed Issue",
        ] {
            assert!(screen.contains(description));
        }
    }

    #[test]
    fn command_history_browses_and_restores_draft() {
        let mut app = App {
            command_history: vec!["columns".into(), "refresh".into()],
            ..Default::default()
        };
        app.handle_key(key(KeyCode::Char(':')));
        app.handle_key(key(KeyCode::Char('q')));

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.command, "refresh");
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.command, "columns");
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.command, "columns");
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.command, "refresh");
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.command, "q");
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.command, "q");
    }

    #[test]
    fn editing_recalled_command_detaches_from_history() {
        let mut app = App {
            command_history: vec!["refres".into()],
            ..Default::default()
        };
        app.handle_key(key(KeyCode::Char(':')));
        app.handle_key(key(KeyCode::Up));

        app.handle_key(key(KeyCode::Char('h')));
        app.handle_key(key(KeyCode::Down));

        assert_eq!(app.command, "refresh");
        assert_eq!(app.history_index, None);
    }

    #[test]
    fn executed_commands_are_recorded_once_when_consecutive() {
        let mut app = App::default();

        type_command(&mut app, "refresh");
        assert!(app.history_dirty);
        app.history_dirty = false;
        type_command(&mut app, "refresh");

        assert_eq!(app.command_history, ["refresh"]);
        assert!(!app.history_dirty);
    }

    #[test]
    fn edit_command_returns_open_action() {
        let mut app = App::default();

        let action = type_command(&mut app, "e https://github.com/orgs/example/projects/1");

        assert_eq!(
            action,
            Some(Action::Edit(
                "https://github.com/orgs/example/projects/1".into()
            ))
        );
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn tab_commands_return_navigation_actions() {
        let mut app = App::default();

        assert_eq!(type_command(&mut app, "tabnew"), Some(Action::NewTab(None)));
        assert_eq!(
            type_command(
                &mut app,
                "tabnew https://github.com/orgs/example/projects/2"
            ),
            Some(Action::NewTab(Some(
                "https://github.com/orgs/example/projects/2".into()
            )))
        );
        assert_eq!(type_command(&mut app, "tabnext"), Some(Action::NextTab));
        assert_eq!(
            type_command(&mut app, "tabprevious"),
            Some(Action::PreviousTab)
        );
        assert_eq!(type_command(&mut app, "tabclose"), Some(Action::CloseTab));
    }

    #[test]
    fn refresh_command_and_shortcut_return_refresh_action() {
        let mut app = App::default();

        assert_eq!(type_command(&mut app, "refresh"), Some(Action::Refresh));
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            Some(Action::Refresh)
        );
    }

    #[test]
    fn refresh_replaces_only_current_project_and_preserves_view() {
        let url = "https://github.com/orgs/example/projects/1";
        let first_project = sample_project(1);
        let mut stale_project = sample_project(5);
        stale_project.title = "Stale".into();
        let mut refreshed_project = sample_project(2);
        refreshed_project.title = "Fresh".into();
        let source = MockProjectSource::returning(refreshed_project);
        let mut active_view = ViewState::new(Some(url.into()), Some(4));
        active_view.columns = Some(vec!["Title".into()]);
        let mut app = App {
            tabs: vec![
                RuntimeTab {
                    project: Some(first_project),
                    view: ViewState::default(),
                },
                RuntimeTab {
                    project: Some(stale_project),
                    view: active_view,
                },
            ],
            active_tab: 1,
            ..Default::default()
        };

        refresh_project(&mut app, &FixedToken(Some("token".into())), &source);

        assert_eq!(app.tabs[0].project.as_ref().unwrap().title, "Demo");
        assert_eq!(app.project().unwrap().title, "Fresh");
        assert_eq!(app.view().selected, Some(1));
        assert_eq!(app.view().columns, Some(vec!["Title".into()]));
        assert_eq!(app.message.as_deref(), Some("Project refreshed"));
        assert_eq!(source.requests().len(), 1);
    }

    #[test]
    fn failed_refresh_keeps_current_project_and_shows_error() {
        let mut app = app_with_project(
            sample_project(2),
            ViewState::new(
                Some("https://github.com/orgs/example/projects/1".into()),
                Some(1),
            ),
        );

        refresh_project(
            &mut app,
            &FixedToken(Some("token".into())),
            &MockProjectSource::failing("refresh failed"),
        );

        assert_eq!(app.project().unwrap().items.len(), 2);
        assert_eq!(app.view().selected, Some(1));
        assert_eq!(app.error_dialog.as_deref(), Some("refresh failed"));
    }

    #[test]
    fn duplicate_tabs_have_independent_view_state() {
        let mut app = app_with_project(
            sample_project(3),
            ViewState::new(
                Some("https://github.com/orgs/example/projects/1".into()),
                Some(0),
            ),
        );

        app.new_tab(true);
        app.move_selection(1);

        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.tabs[0].view.selected, Some(0));
        assert_eq!(app.tabs[1].view.selected, Some(1));
        assert_eq!(app.tabs[0].view.project_url, app.tabs[1].view.project_url);
        app.previous_tab();
        assert_eq!(app.active_tab, 0);
        app.previous_tab();
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn close_tab_keeps_at_least_one_tab() {
        let mut app = App::default();
        app.new_tab(false);

        app.close_tab();
        assert_eq!(app.tabs.len(), 1);
        app.close_tab();

        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.tabs[0], RuntimeTab::default());
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

        assert_eq!(app.project().unwrap().items.len(), 3);
        assert_eq!(source.requests()[0].1, "token");
        assert_eq!(app.view().selected, Some(0));
        assert_eq!(
            app.view().project_url.as_deref(),
            Some("https://github.com/orgs/example/projects/1")
        );
        assert_eq!(app.message, None);
    }

    #[test]
    fn failed_open_keeps_existing_project_and_shows_error() {
        let mut app = app_with_project(sample_project(1), ViewState::default());

        open_project(
            &mut app,
            "https://github.com/orgs/example/projects/2",
            &FixedToken(Some("token".into())),
            &MockProjectSource::failing("request failed"),
        );

        assert_eq!(app.project().unwrap().items.len(), 1);
        assert_eq!(app.error_dialog.as_deref(), Some("request failed"));
    }

    #[test]
    fn normal_mode_moves_selection() {
        let mut app = app_with_project(sample_project(20), ViewState::new(None, Some(0)));

        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::PageDown));
        assert_eq!(app.view().selected, Some(11));

        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.view().selected, Some(10));

        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.view().selected, Some(0));

        app.handle_key(key(KeyCode::Char('G')));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.view().selected, Some(19));
    }

    #[test]
    fn classifies_project_fields_as_mutable() {
        let mut project = sample_project(1);
        project.field_names.push("Priority".into());
        project.mutable_field_names.push("Priority".into());

        let columns = project_columns(&project);

        assert_eq!(
            columns,
            vec![
                ProjectColumn {
                    name: STATUS_COLUMN.into(),
                    mutable: false,
                },
                ProjectColumn {
                    name: "#".into(),
                    mutable: false,
                },
                ProjectColumn {
                    name: "Type".into(),
                    mutable: false,
                },
                ProjectColumn {
                    name: "Title".into(),
                    mutable: false,
                },
                ProjectColumn {
                    name: "Status".into(),
                    mutable: true,
                },
                ProjectColumn {
                    name: "Priority".into(),
                    mutable: true,
                },
            ]
        );
    }

    #[test]
    fn active_mode_cycles_mutable_columns_and_moves_rows() {
        let mut project = sample_project(3);
        project.field_names.push("Priority".into());
        project.mutable_field_names.push("Priority".into());
        let mut app = app_with_project(project, ViewState::new(None, Some(0)));

        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Active);
        assert_eq!(app.active_column.as_deref(), Some("Status"));

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_column.as_deref(), Some("Priority"));
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.active_column.as_deref(), Some("Status"));
        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.active_column.as_deref(), Some("Priority"));

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.view().selected, Some(1));
        assert_eq!(app.mode, Mode::Active);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.view().selected, Some(0));
    }

    #[test]
    fn enter_on_active_selection_opens_and_updates_select_field() {
        let source = MockProjectSource::returning(sample_project(1));
        let mut app = App {
            tabs: vec![RuntimeTab {
                project: Some(sample_project(1)),
                view: ViewState::new(None, Some(0)),
            }],
            mode: Mode::Active,
            active_column: Some("Status".into()),
            ..Default::default()
        };

        assert_eq!(app.handle_key(key(KeyCode::Enter)), None);
        assert_eq!(app.mode, Mode::EditField);
        app.handle_key(key(KeyCode::Down));
        let action = app.handle_key(key(KeyCode::Enter)).unwrap();
        let Action::UpdateField(update) = action else {
            panic!("expected field update");
        };
        update_field(&mut app, update, &FixedToken(Some("token".into())), &source);

        assert_eq!(
            source.updates()[0].value,
            FieldValue::SingleSelect("done-option".into())
        );
        assert_eq!(app.project().unwrap().items[0].fields[0].1, "Done");
        assert_eq!(app.mode, Mode::Active);
    }

    #[test]
    fn free_form_editors_validate_number_and_date_formats() {
        let fields = [
            ("Estimate", EditableFieldKind::Number, "not-a-number", "3.5"),
            ("Due", EditableFieldKind::Date, "2026-02-30", "2026-02-28"),
        ];
        for (name, kind, invalid, valid) in fields {
            let mut project = sample_project(1);
            project.field_names.push(name.into());
            project.mutable_field_names.push(name.into());
            project.editable_fields.push(EditableField {
                id: format!("{name}-field"),
                name: name.into(),
                kind,
                options: vec![],
            });
            let mut app = App {
                tabs: vec![RuntimeTab {
                    project: Some(project),
                    view: ViewState::new(None, Some(0)),
                }],
                mode: Mode::Active,
                active_column: Some(name.into()),
                ..Default::default()
            };

            app.handle_key(key(KeyCode::Enter));
            if let Some(FieldEditor::Input { value, .. }) = app.field_editor.as_mut() {
                *value = invalid.into();
            }
            assert_eq!(app.handle_key(key(KeyCode::Enter)), None);
            assert!(app.error_dialog.is_some());
            app.handle_key(key(KeyCode::Esc));
            if let Some(FieldEditor::Input { value, .. }) = app.field_editor.as_mut() {
                *value = valid.into();
            }
            assert!(matches!(
                app.handle_key(key(KeyCode::Enter)),
                Some(Action::UpdateField(_))
            ));
        }
    }

    #[test]
    fn text_editor_prefills_current_value_and_accepts_free_form_text() {
        let mut project = sample_project(1);
        project.field_names.push("Notes".into());
        project.mutable_field_names.push("Notes".into());
        project.editable_fields.push(EditableField {
            id: "notes-field".into(),
            name: "Notes".into(),
            kind: EditableFieldKind::Text,
            options: vec![],
        });
        project.items[0]
            .fields
            .push(("Notes".into(), "hello".into()));
        let mut app = App {
            tabs: vec![RuntimeTab {
                project: Some(project),
                view: ViewState::new(None, Some(0)),
            }],
            mode: Mode::Active,
            active_column: Some("Notes".into()),
            ..Default::default()
        };

        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('!')));
        let Some(Action::UpdateField(update)) = app.handle_key(key(KeyCode::Enter)) else {
            panic!("expected field update");
        };

        assert_eq!(update.value, FieldValue::Text("hello!".into()));
        assert_eq!(update.display_value, "hello!");
    }

    #[test]
    fn selection_field_renders_option_dialog() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let project = sample_project(1);
        let mut app = App {
            tabs: vec![RuntimeTab {
                project: Some(project),
                view: ViewState::new(None, Some(0)),
            }],
            mode: Mode::Active,
            active_column: Some("Status".into()),
            ..Default::default()
        };
        app.handle_key(key(KeyCode::Enter));

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("Status - Select value"));
        assert!(screen.contains("Todo"));
        assert!(screen.contains("Done"));
    }

    #[test]
    fn active_mode_highlights_mutable_cell() {
        let backend = TestBackend::new(60, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App {
            mode: Mode::Active,
            active_column: Some("Status".into()),
            tabs: vec![RuntimeTab {
                project: Some(sample_project(2)),
                view: ViewState::new(None, Some(0)),
            }],
            ..Default::default()
        };

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let selected_row = buffer
            .content()
            .chunks(buffer.area.width as usize)
            .find(|row| {
                row.iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
                    .contains("Issue 1")
            })
            .unwrap();
        assert!(selected_row.iter().any(|cell| cell.bg == Color::Yellow));
    }

    #[test]
    fn columns_command_opens_checkbox_menu_and_toggles_columns() {
        let mut project = sample_project(2);
        project.field_names.push("Release notes".into());
        let mut app = app_with_project(project, ViewState::default());

        assert_eq!(type_command(&mut app, "columns"), None);
        assert_eq!(app.mode, Mode::Columns);
        assert_eq!(
            app.available_columns(),
            vec![
                STATUS_COLUMN,
                "#",
                "Type",
                "Title",
                "Status",
                "Release notes"
            ]
        );

        app.handle_key(key(KeyCode::Char(' ')));
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char(' ')));
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(
            app.view().columns,
            Some(vec![
                "Type".into(),
                "Title".into(),
                "Status".into(),
                "Release notes".into()
            ])
        );
    }

    #[test]
    fn columns_menu_renders_checkboxes_and_filters_project_table() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App {
            mode: Mode::Columns,
            tabs: vec![RuntimeTab {
                project: Some(sample_project(2)),
                view: ViewState {
                    columns: Some(vec!["Title".into()]),
                    ..Default::default()
                },
            }],
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
        assert!(screen.contains("[ ] #"));
        assert!(screen.contains("[x] Title"));
        assert!(screen.contains("Issue 1"));
        assert!(!screen.contains("Todo"));
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
            tabs: vec![RuntimeTab {
                project: Some(sample_project(2)),
                view: ViewState::default(),
            }],
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
        assert!(screen.contains(concat!("ghui v", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn credential_failure_renders_in_error_dialog() {
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();

        open_project(
            &mut app,
            "https://github.com/orgs/example/projects/1",
            &FixedToken(None),
            &MockProjectSource::returning(sample_project(1)),
        );
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let screen = buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("Error"));
        assert!(screen.contains("no GitHub token found"));
        assert!(screen.contains("Enter or Esc to dismiss"));
    }

    #[test]
    fn error_dialog_blocks_input_until_dismissed() {
        let mut app = app_with_project(sample_project(2), ViewState::new(None, Some(0)));
        app.show_error("request failed");

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.view().selected, Some(0));
        assert_eq!(app.error_dialog.as_deref(), Some("request failed"));

        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.error_dialog, None);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.view().selected, Some(1));
    }

    #[test]
    fn renders_all_open_tabs_and_highlights_active_tab() {
        let backend = TestBackend::new(70, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut second_project = sample_project(1);
        second_project.title = "Second".into();
        let app = App {
            tabs: vec![
                RuntimeTab {
                    project: Some(sample_project(1)),
                    view: ViewState::default(),
                },
                RuntimeTab {
                    project: Some(second_project),
                    view: ViewState::default(),
                },
            ],
            active_tab: 1,
            ..Default::default()
        };

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let first_row = &terminal.backend().buffer().content()
            [..terminal.backend().buffer().area.width as usize];
        let text = first_row
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("1:Demo"));
        assert!(text.contains("2:Second"));
        assert!(first_row.iter().any(|cell| cell.bg == Color::Cyan));
    }

    #[test]
    fn scrolling_keeps_column_header_visible() {
        let backend = TestBackend::new(50, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = app_with_project(sample_project(10), ViewState::new(None, Some(5)));

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

    #[test]
    fn restores_project_and_clamps_saved_selection() {
        let source = MockProjectSource::returning(sample_project(3));
        let state = SessionState::new(
            vec![ViewState::new(
                Some("https://github.com/orgs/example/projects/1".into()),
                Some(20),
            )],
            0,
        );
        let state = SessionState::from_text(&state.to_text().unwrap()).unwrap();
        let mut app = App::default();

        restore_session_state(&mut app, state, &FixedToken(Some("token".into())), &source);

        assert_eq!(app.project().unwrap().items.len(), 3);
        assert_eq!(app.view().selected, Some(2));
        assert_eq!(source.requests().len(), 1);
    }

    #[test]
    fn restores_all_saved_tabs_and_active_tab() {
        let source = MockProjectSource::returning(sample_project(4));
        let state = SessionState::new(
            vec![
                ViewState::new(
                    Some("https://github.com/orgs/example/projects/1".into()),
                    Some(1),
                ),
                ViewState::new(
                    Some("https://github.com/orgs/example/projects/2".into()),
                    Some(3),
                ),
            ],
            1,
        );
        let mut app = App::default();

        restore_session_state(&mut app, state, &FixedToken(Some("token".into())), &source);

        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.tabs[0].view.selected, Some(1));
        assert_eq!(app.tabs[1].view.selected, Some(3));
        assert!(app.tabs.iter().all(|tab| tab.project.is_some()));
        assert_eq!(source.requests().len(), 2);
    }

    #[test]
    fn edit_existing_path_loads_and_restores_state() {
        let path = temp_state_path("ghui-load-state");
        SessionState::new(
            vec![ViewState::new(
                Some("https://github.com/orgs/example/projects/1".into()),
                Some(1),
            )],
            0,
        )
        .save(&path)
        .unwrap();
        let source = MockProjectSource::returning(sample_project(3));
        let mut app = App::default();

        edit_target(
            &mut app,
            path.to_str().unwrap(),
            &FixedToken(Some("token".into())),
            &source,
        );

        std::fs::remove_file(&path).unwrap();
        assert_eq!(app.state_path.as_deref(), Some(path.as_path()));
        assert_eq!(app.view().selected, Some(1));
        assert_eq!(app.project().unwrap().items.len(), 3);
    }

    #[test]
    fn edit_missing_path_sets_destination_and_write_saves_state() {
        let path = temp_state_path("ghui-save-state");
        let _ = std::fs::remove_file(&path);
        let mut app = App::default();
        app.tabs[0].view = ViewState::new(
            Some("https://github.com/orgs/example/projects/1".into()),
            Some(2),
        );

        edit_target(
            &mut app,
            path.to_str().unwrap(),
            &FixedToken(None),
            &MockProjectSource::returning(sample_project(0)),
        );
        write_state(&mut app);

        assert_eq!(SessionState::load(&path).unwrap(), app.session_state());
        assert!(app.message.as_deref().unwrap().starts_with("Saved "));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn write_command_requires_a_state_path() {
        let mut app = App::default();

        assert_eq!(type_command(&mut app, "w"), Some(Action::Write));
        write_state(&mut app);

        assert_eq!(
            app.error_dialog.as_deref(),
            Some("No state path; use :e PATH first")
        );
    }

    #[test]
    fn write_quit_command_saves_and_quits() {
        let path = temp_state_path("ghui-write-quit-state");
        let mut app = App {
            state_path: Some(path.clone()),
            ..Default::default()
        };
        app.tabs[0].view = ViewState::new(
            Some("https://github.com/orgs/example/projects/1".into()),
            Some(2),
        );

        assert_eq!(type_command(&mut app, "wq"), Some(Action::WriteQuit));
        write_and_maybe_quit(&mut app, true);

        assert!(app.should_quit);
        assert_eq!(SessionState::load(&path).unwrap(), app.session_state());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn write_quit_stays_open_when_write_fails() {
        let mut app = App::default();

        assert_eq!(type_command(&mut app, "wq"), Some(Action::WriteQuit));
        write_and_maybe_quit(&mut app, true);

        assert!(!app.should_quit);
        assert_eq!(
            app.error_dialog.as_deref(),
            Some("No state path; use :e PATH first")
        );
    }
}
