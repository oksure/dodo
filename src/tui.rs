use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Tabs,
        Wrap,
    },
    Frame, Terminal,
};
use std::io;

use dodo::cli::{Area as CliArea, SortBy};
use dodo::db::Database;
use dodo::notation::{parse_date, parse_duration, parse_notation};
use dodo::task::{Area, Task, TaskStatus};

// ── Color Palette (Catppuccin Mocha-inspired) ────────────────────────

const BG_SURFACE: Color = Color::Rgb(49, 50, 68);
const FG_TEXT: Color = Color::Rgb(205, 214, 244);
const FG_SUBTEXT: Color = Color::Rgb(166, 173, 200);
const FG_OVERLAY: Color = Color::Rgb(108, 112, 134);
const ACCENT_BLUE: Color = Color::Rgb(137, 180, 250);
const ACCENT_GREEN: Color = Color::Rgb(166, 227, 161);
const ACCENT_YELLOW: Color = Color::Rgb(249, 226, 175);
const ACCENT_RED: Color = Color::Rgb(243, 139, 168);
const ACCENT_MAUVE: Color = Color::Rgb(203, 166, 247);
const ACCENT_TEAL: Color = Color::Rgb(148, 226, 213);
const ACCENT_PEACH: Color = Color::Rgb(250, 179, 135);

const PANE_LABELS: [&str; 4] = ["LONG TERM", "THIS WEEK", "TODAY", "DONE"];
const SORT_MODES: [SortBy; 3] = [SortBy::Created, SortBy::Modified, SortBy::Title];
const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

pub fn run_tui(db: &Database) -> Result<()> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db);
    app.refresh_all()?;

    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

// ── State ────────────────────────────────────────────────────────────

struct PaneState {
    tasks: Vec<Task>,
    list_state: ListState,
}

impl PaneState {
    fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            tasks: Vec::new(),
            list_state,
        }
    }

    fn next(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) if i >= self.tasks.len().saturating_sub(1) => 0,
            Some(i) => i + 1,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(0) | None => self.tasks.len().saturating_sub(1),
            Some(i) => i - 1,
        };
        self.list_state.select(Some(i));
    }

    fn selected_task(&self) -> Option<&Task> {
        self.list_state.selected().and_then(|i| self.tasks.get(i))
    }

    fn stats(&self) -> (i64, i64, usize, usize) {
        let mut elapsed = 0i64;
        let mut estimate = 0i64;
        let mut done = 0usize;
        for task in &self.tasks {
            elapsed += task.elapsed_seconds.unwrap_or(0);
            estimate += task.estimate_minutes.unwrap_or(0) * 60;
            if task.status == TaskStatus::Done {
                done += 1;
            }
        }
        (elapsed, estimate, done, self.tasks.len())
    }
}

#[derive(PartialEq)]
enum AppMode {
    Normal,
    NoteView,
    NoteEdit,
    AddTask,
    MoveTask,
    ConfirmDelete,
    EditTask,
    EditTaskField,
}

#[derive(Clone, Copy, PartialEq)]
enum TuiTab {
    Tasks,
    Report,
}

#[derive(Clone, Copy, PartialEq)]
enum ReportRange {
    Day,
    Week,
    Month,
    Year,
    All,
}

impl ReportRange {
    fn label(self) -> &'static str {
        match self {
            ReportRange::Day => "DAY",
            ReportRange::Week => "WEEK",
            ReportRange::Month => "MONTH",
            ReportRange::Year => "YEAR",
            ReportRange::All => "ALL",
        }
    }

    fn next(self) -> Self {
        match self {
            ReportRange::Day => ReportRange::Week,
            ReportRange::Week => ReportRange::Month,
            ReportRange::Month => ReportRange::Year,
            ReportRange::Year => ReportRange::All,
            ReportRange::All => ReportRange::Day,
        }
    }

    fn prev(self) -> Self {
        match self {
            ReportRange::Day => ReportRange::All,
            ReportRange::Week => ReportRange::Day,
            ReportRange::Month => ReportRange::Week,
            ReportRange::Year => ReportRange::Month,
            ReportRange::All => ReportRange::Year,
        }
    }

    fn date_range(self) -> (String, String) {
        let now = chrono::Local::now();
        let today = now.date_naive();
        let to = (today + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Utc)
            .unwrap()
            .to_rfc3339();
        let from_date = match self {
            ReportRange::Day => today,
            ReportRange::Week => today - chrono::Duration::days(7),
            ReportRange::Month => today - chrono::Duration::days(30),
            ReportRange::Year => today - chrono::Duration::days(365),
            ReportRange::All => chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
        };
        let from = from_date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Utc)
            .unwrap()
            .to_rfc3339();
        (from, to)
    }
}

struct ReportData {
    tasks_done: i64,
    total_seconds: i64,
    active_days: i64,
    by_hour: Vec<(i64, i64)>,
    by_weekday: Vec<(i64, i64)>,
    by_project: Vec<(String, i64)>,
    done_tasks: Vec<(String, i64)>,
}

const EDIT_FIELD_LABELS: [&str; 8] = [
    "Title", "Project", "Context", "Tags", "Estimate", "Deadline", "Scheduled", "Priority",
];

struct App<'a> {
    panes: [PaneState; 4],
    active_pane: usize,
    sort_index: usize,
    running_task: Option<String>,
    db: &'a Database,
    mode: AppMode,
    // Note modal
    note_task_id: Option<String>,
    note_task_title: String,
    note_content: String,
    note_input: String,
    // Tabs & report
    tab: TuiTab,
    report_range: ReportRange,
    report: Option<ReportData>,
    tick_count: u64,
    // Add task
    add_input: String,
    // Move task
    move_task_id: Option<String>,
    move_source: usize,
    move_target: usize,
    // Delete task
    delete_task_id: Option<String>,
    delete_task_title: String,
    // Edit task
    edit_task_id: Option<String>,
    edit_field_index: usize,
    edit_field_values: [String; 8],
    edit_field_input: String,
}

impl<'a> App<'a> {
    fn new(db: &'a Database) -> Self {
        Self {
            panes: [
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
            ],
            active_pane: 2,
            sort_index: 0,
            running_task: None,
            db,
            mode: AppMode::Normal,
            note_task_id: None,
            note_task_title: String::new(),
            note_content: String::new(),
            note_input: String::new(),
            tab: TuiTab::Tasks,
            report_range: ReportRange::Day,
            report: None,
            tick_count: 0,
            add_input: String::new(),
            move_task_id: None,
            move_source: 0,
            move_target: 0,
            delete_task_id: None,
            delete_task_title: String::new(),
            edit_task_id: None,
            edit_field_index: 0,
            edit_field_values: Default::default(),
            edit_field_input: String::new(),
        }
    }

    fn current_sort(&self) -> SortBy {
        SORT_MODES[self.sort_index]
    }

    fn cycle_sort(&mut self) {
        self.sort_index = (self.sort_index + 1) % SORT_MODES.len();
        let _ = self.refresh_all();
    }

    fn refresh_all(&mut self) -> Result<()> {
        let sort = self.current_sort();
        let all_tasks = self.db.list_all_tasks(sort)?;

        let mut groups: [Vec<Task>; 4] = [vec![], vec![], vec![], vec![]];
        for task in all_tasks {
            let effective = task.effective_area();
            let idx = match effective {
                Area::LongTerm => 0,
                Area::ThisWeek => 1,
                Area::Today => 2,
                Area::Completed => 3,
            };
            groups[idx].push(task);
        }

        for (i, group) in groups.into_iter().enumerate() {
            self.panes[i].tasks = group;
            let len = self.panes[i].tasks.len();
            if len == 0 {
                self.panes[i].list_state.select(None);
            } else if let Some(sel) = self.panes[i].list_state.selected() {
                if sel >= len {
                    self.panes[i].list_state.select(Some(len - 1));
                }
            } else {
                self.panes[i].list_state.select(Some(0));
            }
        }

        self.running_task = if let Ok(Some((title, _))) = self.db.get_running_task() {
            Some(title)
        } else {
            None
        };

        Ok(())
    }

    fn refresh_report(&mut self) -> Result<()> {
        let (from, to) = self.report_range.date_range();
        self.report = Some(ReportData {
            tasks_done: self.db.report_tasks_done(&from, &to)?,
            total_seconds: self.db.report_total_seconds(&from, &to)?,
            active_days: self.db.report_active_days(&from, &to)?,
            by_hour: self.db.report_by_hour(&from, &to)?,
            by_weekday: self.db.report_by_weekday(&from, &to)?,
            by_project: self.db.report_by_project(&from, &to)?,
            done_tasks: self.db.report_done_tasks(&from, &to, 20)?,
        });
        Ok(())
    }

    fn move_pane_left(&mut self) {
        if self.active_pane > 0 {
            self.active_pane -= 1;
        }
    }

    fn move_pane_right(&mut self) {
        if self.active_pane < 3 {
            self.active_pane += 1;
        }
    }

    fn toggle_selected(&mut self) -> Result<()> {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            if task.status == TaskStatus::Running {
                self.db.pause_timer()?;
            } else {
                let num_id = task.num_id.map(|n| n.to_string()).unwrap_or_default();
                if !num_id.is_empty() {
                    let today = chrono::Local::now().date_naive();
                    self.db.update_task_scheduled(&task.id, today)?;
                    let _ = self.db.start_timer(&num_id);
                }
            }
            self.refresh_all()?;
        }
        Ok(())
    }

    fn done(&mut self) -> Result<()> {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            if task.status == TaskStatus::Done {
                self.db.uncomplete_task_by_id(&task.id)?;
            } else {
                self.db.complete_task_by_id(&task.id)?;
            }
        }
        self.refresh_all()?;
        Ok(())
    }

    fn open_note_view(&mut self) {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            self.note_task_id = Some(task.id.clone());
            self.note_task_title = task.title.clone();
            self.note_content = task.notes.clone().unwrap_or_default();
            self.note_input.clear();
            self.mode = AppMode::NoteView;
        }
    }

    fn enter_note_edit(&mut self) {
        self.note_input.clear();
        self.mode = AppMode::NoteEdit;
    }

    fn save_note(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.note_task_id {
            if !self.note_input.is_empty() {
                self.db.append_note_by_id(task_id, &self.note_input)?;
                let notes = self.db.get_task_notes_by_id(task_id)?;
                self.note_content = notes.unwrap_or_default();
                self.note_input.clear();
                self.refresh_all()?;
            }
        }
        self.mode = AppMode::NoteView;
        Ok(())
    }

    fn start_add_task(&mut self) {
        self.add_input.clear();
        self.mode = AppMode::AddTask;
    }

    fn confirm_add_task(&mut self) -> Result<()> {
        if !self.add_input.is_empty() {
            let parsed = parse_notation(&self.add_input);
            let title = if parsed.title.is_empty() {
                self.add_input.clone()
            } else {
                parsed.title
            };
            let context = if !parsed.contexts.is_empty() {
                Some(parsed.contexts.join(","))
            } else {
                None
            };
            let tags = if !parsed.tags.is_empty() {
                Some(parsed.tags.join(","))
            } else {
                None
            };
            let estimate = parsed.estimate_minutes.or(Some(60));
            let scheduled = parsed
                .scheduled
                .or_else(|| Some(chrono::Local::now().date_naive()));

            self.db.add_task(
                &title,
                CliArea::Today,
                parsed.project,
                context,
                estimate,
                parsed.deadline,
                scheduled,
                tags,
                parsed.priority,
            )?;
            self.refresh_all()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    fn start_move_task(&mut self) {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            if task.status == TaskStatus::Done {
                return; // Can't move done tasks
            }
            self.move_task_id = Some(task.id.clone());
            self.move_source = self.active_pane;
            // Pick first valid target (skip current pane and DONE)
            self.move_target = self.next_move_target(self.active_pane);
            self.mode = AppMode::MoveTask;
        }
    }

    fn next_move_target(&self, current: usize) -> usize {
        let mut t = (current + 1) % 3; // 0,1,2 only (skip DONE=3)
        if t == self.move_source {
            t = (t + 1) % 3;
        }
        t
    }

    fn prev_move_target(&self, current: usize) -> usize {
        let mut t = if current == 0 { 2 } else { current - 1 };
        if t == self.move_source {
            t = if t == 0 { 2 } else { t - 1 };
        }
        t
    }

    fn confirm_move_task(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.move_task_id {
            let today = chrono::Local::now().date_naive();
            let date = match self.move_target {
                0 => today + chrono::Duration::days(8), // LONG TERM
                1 => today + chrono::Duration::days(1), // THIS WEEK (tomorrow)
                _ => today,                             // TODAY
            };
            self.db.update_task_scheduled(task_id, date)?;
            self.refresh_all()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    fn start_delete(&mut self) {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            self.delete_task_id = Some(task.id.clone());
            self.delete_task_title = task.title.clone();
            self.mode = AppMode::ConfirmDelete;
        }
    }

    fn confirm_delete(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.delete_task_id {
            self.db.delete_task_by_id(task_id)?;
            self.refresh_all()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    fn start_edit_task(&mut self) {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            self.edit_task_id = Some(task.id.clone());
            self.edit_field_index = 0;
            self.edit_field_values = [
                task.title.clone(),
                task.project.clone().unwrap_or_default(),
                task.context.clone().unwrap_or_default(),
                task.tags.clone().unwrap_or_default(),
                task.estimate_minutes
                    .map(|m| format_est(m))
                    .unwrap_or_default(),
                task.deadline
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default(),
                task.scheduled
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default(),
                task.priority
                    .map(|p| "!".repeat(p.clamp(1, 4) as usize))
                    .unwrap_or_default(),
            ];
            self.edit_field_input.clear();
            self.mode = AppMode::EditTask;
        }
    }

    fn enter_edit_field(&mut self) {
        self.edit_field_input = self.edit_field_values[self.edit_field_index].clone();
        self.mode = AppMode::EditTaskField;
    }

    fn save_edit_field(&mut self) -> Result<()> {
        let idx = self.edit_field_index;
        self.edit_field_values[idx] = self.edit_field_input.clone();

        if let Some(ref task_id) = self.edit_task_id {
            let val = &self.edit_field_values[idx];
            match idx {
                0 => {
                    // Title
                    if !val.is_empty() {
                        self.db.update_task_title_by_id(task_id, val)?;
                    }
                }
                1 => {
                    // Project
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if !val.is_empty() {
                        parsed.project = Some(val.clone());
                    } else {
                        parsed.project = Some(String::new());
                    }
                    self.db
                        .update_task_fields_by_id(task_id, &parsed, None)?;
                }
                2 => {
                    // Context
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if !val.is_empty() {
                        parsed.contexts = val.split(',').map(|s| s.trim().to_string()).collect();
                    } else {
                        parsed.contexts = vec![String::new()];
                    }
                    self.db
                        .update_task_fields_by_id(task_id, &parsed, None)?;
                }
                3 => {
                    // Tags
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if !val.is_empty() {
                        parsed.tags = val.split(',').map(|s| s.trim().to_string()).collect();
                    } else {
                        parsed.tags = vec![String::new()];
                    }
                    self.db
                        .update_task_fields_by_id(task_id, &parsed, None)?;
                }
                4 => {
                    // Estimate
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if let Some(mins) = parse_duration(val) {
                        parsed.estimate_minutes = Some(mins);
                        self.db
                            .update_task_fields_by_id(task_id, &parsed, None)?;
                    }
                }
                5 => {
                    // Deadline
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if let Some(date) = parse_date(val) {
                        parsed.deadline = Some(date);
                        self.db
                            .update_task_fields_by_id(task_id, &parsed, None)?;
                    }
                }
                6 => {
                    // Scheduled
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if let Some(date) = parse_date(val) {
                        parsed.scheduled = Some(date);
                        self.db
                            .update_task_fields_by_id(task_id, &parsed, None)?;
                    }
                }
                7 => {
                    // Priority
                    let mut parsed = dodo::notation::ParsedInput::default();
                    let p = val.len() as i64;
                    if p > 0 && val.chars().all(|c| c == '!') {
                        parsed.priority = Some(p.clamp(1, 4));
                    } else if val.is_empty() {
                        parsed.priority = Some(0);
                    }
                    if parsed.priority.is_some() {
                        self.db
                            .update_task_fields_by_id(task_id, &parsed, None)?;
                    }
                }
                _ => {}
            }
            self.refresh_all()?;
        }
        self.mode = AppMode::EditTask;
        Ok(())
    }
}

// ── Event Loop ───────────────────────────────────────────────────────

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let mut last_data_refresh = std::time::Instant::now();
    let poll_rate = std::time::Duration::from_millis(16);
    let data_refresh_rate = std::time::Duration::from_secs(1);

    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if crossterm::event::poll(poll_rate)? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('1') => {
                            app.tab = TuiTab::Tasks;
                        }
                        KeyCode::Char('2') => {
                            app.tab = TuiTab::Report;
                            let _ = app.refresh_report();
                        }
                        KeyCode::Tab => {
                            if app.tab == TuiTab::Tasks {
                                app.tab = TuiTab::Report;
                                let _ = app.refresh_report();
                            } else {
                                app.tab = TuiTab::Tasks;
                            }
                        }
                        _ => {
                            if app.tab == TuiTab::Tasks {
                                match key.code {
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        app.panes[app.active_pane].next()
                                    }
                                    KeyCode::Char('k') | KeyCode::Up => {
                                        app.panes[app.active_pane].previous()
                                    }
                                    KeyCode::Char('h') | KeyCode::Left => app.move_pane_left(),
                                    KeyCode::Char('l') | KeyCode::Right => app.move_pane_right(),
                                    KeyCode::Char('s') => {
                                        let _ = app.toggle_selected();
                                    }
                                    KeyCode::Char('d') => {
                                        let _ = app.done();
                                    }
                                    KeyCode::Char('o') => app.cycle_sort(),
                                    KeyCode::Char('r') => {
                                        let _ = app.refresh_all();
                                    }
                                    KeyCode::Char('n') => {
                                        app.open_note_view();
                                    }
                                    KeyCode::Char('a') => {
                                        app.start_add_task();
                                    }
                                    KeyCode::Char('m') => {
                                        app.start_move_task();
                                    }
                                    KeyCode::Enter => {
                                        app.start_edit_task();
                                    }
                                    KeyCode::Backspace | KeyCode::Delete => {
                                        app.start_delete();
                                    }
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        app.report_range = app.report_range.next();
                                        let _ = app.refresh_report();
                                    }
                                    KeyCode::Char('h') | KeyCode::Left => {
                                        app.report_range = app.report_range.prev();
                                        let _ = app.refresh_report();
                                    }
                                    KeyCode::Char('r') => {
                                        let _ = app.refresh_report();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    },
                    AppMode::NoteView => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Char('e') => {
                            app.enter_note_edit();
                        }
                        _ => {}
                    },
                    AppMode::NoteEdit => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::NoteView;
                        }
                        KeyCode::Enter => {
                            let _ = app.save_note();
                        }
                        KeyCode::Backspace => {
                            app.note_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.note_input.push(c);
                        }
                        _ => {}
                    },
                    AppMode::AddTask => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Enter => {
                            let _ = app.confirm_add_task();
                        }
                        KeyCode::Backspace => {
                            app.add_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.add_input.push(c);
                        }
                        _ => {}
                    },
                    AppMode::MoveTask => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Enter => {
                            let _ = app.confirm_move_task();
                        }
                        KeyCode::Char('l') | KeyCode::Right => {
                            app.move_target = app.next_move_target(app.move_target);
                        }
                        KeyCode::Char('h') | KeyCode::Left => {
                            app.move_target = app.prev_move_target(app.move_target);
                        }
                        _ => {}
                    },
                    AppMode::ConfirmDelete => match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => {
                            let _ = app.confirm_delete();
                        }
                        KeyCode::Char('n') | KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    },
                    AppMode::EditTask => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            if app.edit_field_index < 7 {
                                app.edit_field_index += 1;
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if app.edit_field_index > 0 {
                                app.edit_field_index -= 1;
                            }
                        }
                        KeyCode::Enter => {
                            app.enter_edit_field();
                        }
                        _ => {}
                    },
                    AppMode::EditTaskField => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::EditTask;
                        }
                        KeyCode::Enter => {
                            let _ = app.save_edit_field();
                        }
                        KeyCode::Backspace => {
                            app.edit_field_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.edit_field_input.push(c);
                        }
                        _ => {}
                    },
                }
            }
        }

        if last_data_refresh.elapsed() >= data_refresh_rate {
            app.tick_count = app.tick_count.wrapping_add(1);
            if app.tab == TuiTab::Tasks {
                let _ = app.refresh_all();
            } else {
                let _ = app.refresh_report();
            }
            last_data_refresh = std::time::Instant::now();
        }
    }
}

// ── Drawing ──────────────────────────────────────────────────────────

fn draw_ui(f: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header
            Constraint::Length(1), // Tab bar
            Constraint::Min(0),   // Content
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    // Header
    draw_header(f, app, outer[0]);

    // Tab bar
    let tab_titles: Vec<Line> = vec![Line::from(" Tasks "), Line::from(" Report ")];
    let tab_index = if app.tab == TuiTab::Tasks { 0 } else { 1 };
    let tabs = Tabs::new(tab_titles)
        .select(tab_index)
        .style(Style::default().fg(FG_OVERLAY))
        .highlight_style(
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" | ", Style::default().fg(FG_OVERLAY)));
    f.render_widget(tabs, outer[1]);

    // Content
    match app.tab {
        TuiTab::Tasks => draw_tasks_tab(f, app, outer[2]),
        TuiTab::Report => draw_report_tab(f, app, outer[2]),
    }

    // Footer
    draw_footer(f, app, outer[3]);

    // Modal overlays
    match app.mode {
        AppMode::NoteView | AppMode::NoteEdit => draw_note_modal(f, app),
        AppMode::ConfirmDelete => draw_delete_modal(f, app),
        AppMode::EditTask | AppMode::EditTaskField => draw_edit_modal(f, app),
        AppMode::AddTask => draw_add_bar(f, app),
        AppMode::MoveTask => draw_move_bar(f, app),
        _ => {}
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let running_info = if let Some(ref task) = app.running_task {
        format!(" \u{25B6} {} ", task)
    } else {
        String::new()
    };

    let running_style = if app.running_task.is_some() {
        let phase = app.tick_count % 3;
        match phase {
            0 => Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
            1 => Style::default()
                .fg(Color::Rgb(180, 240, 180))
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .fg(ACCENT_TEAL)
                .add_modifier(Modifier::BOLD),
        }
    } else {
        Style::default()
    };

    let sort_label = match app.current_sort() {
        SortBy::Created => "created",
        SortBy::Modified => "modified",
        SortBy::Title => "title",
        SortBy::Area => "area",
    };

    let text = Line::from(vec![
        Span::styled(
            " DODO ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(running_info, running_style),
        Span::styled(
            format!("  sort:{}", sort_label),
            Style::default().fg(FG_OVERLAY),
        ),
    ]);

    let header = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(FG_OVERLAY))
            .border_type(BorderType::Rounded),
    );
    f.render_widget(header, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys: Vec<(&str, &str)> = if app.tab == TuiTab::Tasks {
        match app.mode {
            AppMode::AddTask => vec![
                ("Enter", "add"),
                ("Esc", "cancel"),
            ],
            AppMode::MoveTask => vec![
                ("h/l", "select"),
                ("Enter", "move"),
                ("Esc", "cancel"),
            ],
            _ => vec![
                ("a", "add"),
                ("m", "move"),
                ("\u{21B5}", "edit"),
                ("\u{232B}", "del"),
                ("s", "start"),
                ("d", "done"),
                ("n", "note"),
                ("o", "sort"),
                ("q", "quit"),
            ],
        }
    } else {
        vec![
            ("h/l", "range"),
            ("r", "refresh"),
            ("q", "quit"),
        ]
    };

    let mut spans: Vec<Span> = vec![Span::styled(" ", Style::default())];
    for (i, (key, action)) in keys.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default().fg(FG_TEXT).bg(BG_SURFACE),
        ));
        spans.push(Span::styled(
            format!(" {} ", action),
            Style::default().fg(FG_SUBTEXT),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_tasks_tab(f: &mut Frame, app: &App, area: Rect) {
    let pane_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    for i in 0..4 {
        let is_active = i == app.active_pane;
        let pane_widget = build_pane(&app.panes[i], PANE_LABELS[i], is_active, app.tick_count);
        f.render_stateful_widget(
            pane_widget,
            pane_chunks[i],
            &mut app.panes[i].list_state.clone(),
        );
    }
}

fn draw_report_tab(f: &mut Frame, app: &App, area: Rect) {
    let report = match &app.report {
        Some(r) => r,
        None => {
            let msg = Paragraph::new("Loading report...")
                .style(Style::default().fg(FG_OVERLAY));
            f.render_widget(msg, area);
            return;
        }
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    // Range selector
    let ranges = [
        ReportRange::Day,
        ReportRange::Week,
        ReportRange::Month,
        ReportRange::Year,
        ReportRange::All,
    ];
    let range_spans: Vec<Span> = ranges
        .iter()
        .map(|r| {
            if *r == app.report_range {
                Span::styled(
                    format!(" {} ", r.label()),
                    Style::default()
                        .fg(FG_TEXT)
                        .bg(ACCENT_BLUE)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(format!(" {} ", r.label()), Style::default().fg(FG_SUBTEXT))
            }
        })
        .collect();
    let mut all_spans = vec![Span::styled(
        "  Range: ",
        Style::default().fg(FG_OVERLAY),
    )];
    for (i, s) in range_spans.into_iter().enumerate() {
        all_spans.push(s);
        if i < ranges.len() - 1 {
            all_spans.push(Span::styled("  ", Style::default()));
        }
    }
    f.render_widget(Paragraph::new(Line::from(all_spans)), layout[0]);

    // Report body
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[1]);

    let left_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(cols[0]);

    // Summary stats
    let avg_per_task = if report.tasks_done > 0 {
        report.total_seconds / report.tasks_done
    } else {
        0
    };
    let avg_per_day = if report.active_days > 0 {
        report.total_seconds / report.active_days
    } else {
        0
    };

    let summary_lines = vec![
        Line::from(vec![
            Span::styled("  Tasks done:    ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(
                format!("{}", report.tasks_done),
                Style::default()
                    .fg(ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Total time:    ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(
                format_dur(report.total_seconds),
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Active days:   ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(
                format!("{}", report.active_days),
                Style::default().fg(ACCENT_YELLOW),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Avg/task:      ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(format_dur(avg_per_task), Style::default().fg(FG_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Avg/day:       ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(format_dur(avg_per_day), Style::default().fg(FG_TEXT)),
        ]),
    ];

    let summary = Paragraph::new(summary_lines).block(
        Block::bordered()
            .title(Span::styled(
                " Summary ",
                Style::default()
                    .fg(ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(FG_OVERLAY))
            .padding(Padding::horizontal(1)),
    );
    f.render_widget(summary, left_layout[0]);

    // Productivity
    let mut prod_lines: Vec<Line> = vec![];

    if let Some((hour, secs)) = report.by_hour.iter().max_by_key(|(_h, s)| *s) {
        prod_lines.push(Line::from(vec![
            Span::styled("  Best hour:     ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(
                format!("{:02}:00", hour),
                Style::default()
                    .fg(ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({})", format_dur(*secs)),
                Style::default().fg(FG_OVERLAY),
            ),
        ]));
    }

    if let Some((dow, secs)) = report.by_weekday.iter().max_by_key(|(_d, s)| *s) {
        let day_name = DAY_NAMES.get(*dow as usize).unwrap_or(&"?");
        prod_lines.push(Line::from(vec![
            Span::styled("  Best day:      ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(
                day_name.to_string(),
                Style::default()
                    .fg(ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({})", format_dur(*secs)),
                Style::default().fg(FG_OVERLAY),
            ),
        ]));
    }

    if !report.by_hour.is_empty() {
        prod_lines.push(Line::from(""));
        prod_lines.push(Line::from(Span::styled(
            "  Hours worked:",
            Style::default().fg(FG_SUBTEXT),
        )));
        let max_secs = report
            .by_hour
            .iter()
            .map(|(_, s)| *s)
            .max()
            .unwrap_or(1)
            .max(1);
        for (hour, secs) in &report.by_hour {
            let bar_width = (*secs as f64 / max_secs as f64 * 20.0) as usize;
            let bar: String = "\u{2588}".repeat(bar_width);
            prod_lines.push(Line::from(vec![
                Span::styled(format!("  {:02}:00 ", hour), Style::default().fg(FG_OVERLAY)),
                Span::styled(bar, Style::default().fg(ACCENT_TEAL)),
                Span::styled(
                    format!(" {}", format_dur(*secs)),
                    Style::default().fg(FG_OVERLAY),
                ),
            ]));
        }
    }

    let prod = Paragraph::new(prod_lines).block(
        Block::bordered()
            .title(Span::styled(
                " Productivity ",
                Style::default()
                    .fg(ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(FG_OVERLAY))
            .padding(Padding::horizontal(1)),
    );
    f.render_widget(prod, left_layout[1]);

    // Right column
    let right_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(cols[1]);

    // Time by project
    let mut proj_lines: Vec<Line> = vec![];
    for (project, secs) in &report.by_project {
        let pct = if report.total_seconds > 0 {
            (*secs as f64 / report.total_seconds as f64 * 100.0) as u64
        } else {
            0
        };
        proj_lines.push(Line::from(vec![
            Span::styled(
                format!("  +{:<14}", project),
                Style::default().fg(ACCENT_MAUVE),
            ),
            Span::styled(
                format!("{:>8}", format_dur(*secs)),
                Style::default().fg(FG_TEXT),
            ),
            Span::styled(format!("  {:>3}%", pct), Style::default().fg(FG_OVERLAY)),
        ]));
    }
    if proj_lines.is_empty() {
        proj_lines.push(Line::from(Span::styled(
            "  (no data)",
            Style::default().fg(FG_OVERLAY),
        )));
    }

    let proj = Paragraph::new(proj_lines).block(
        Block::bordered()
            .title(Span::styled(
                " Time by Project ",
                Style::default()
                    .fg(ACCENT_MAUVE)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(FG_OVERLAY))
            .padding(Padding::horizontal(1)),
    );
    f.render_widget(proj, right_layout[0]);

    // Done tasks
    let mut done_lines: Vec<Line> = vec![];
    for (title, secs) in &report.done_tasks {
        done_lines.push(Line::from(vec![
            Span::styled("  \u{2713} ", Style::default().fg(ACCENT_GREEN)),
            Span::styled(title.clone(), Style::default().fg(FG_TEXT)),
            Span::styled(
                format!("  ({})", format_dur(*secs)),
                Style::default().fg(FG_OVERLAY),
            ),
        ]));
    }
    if done_lines.is_empty() {
        done_lines.push(Line::from(Span::styled(
            "  (no completed tasks)",
            Style::default().fg(FG_OVERLAY),
        )));
    }

    let done = Paragraph::new(done_lines).block(
        Block::bordered()
            .title(Span::styled(
                " Completed Tasks ",
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(FG_OVERLAY))
            .padding(Padding::horizontal(1)),
    );
    f.render_widget(done, right_layout[1]);
}

// ── Pane Building ────────────────────────────────────────────────────

fn build_pane(pane: &PaneState, label: &str, is_active: bool, tick_count: u64) -> List<'static> {
    let border_color = if is_active { ACCENT_BLUE } else { FG_OVERLAY };
    let border_style = Style::default().fg(border_color);

    let title_style = if is_active {
        Style::default()
            .fg(ACCENT_BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(FG_SUBTEXT)
    };

    let today = chrono::Local::now().date_naive();

    let items: Vec<ListItem> = pane
        .tasks
        .iter()
        .map(|task| {
            let is_running = task.status == TaskStatus::Running;
            let is_overdue = !is_running
                && task.status != TaskStatus::Done
                && is_task_overdue(task, today);

            let status_icon = match task.status {
                TaskStatus::Pending => "\u{25CB}", // ○
                TaskStatus::Running => "\u{25B6}", // ▶
                TaskStatus::Paused => "\u{23F8}",  // ⏸
                TaskStatus::Done => "\u{2713}",    // ✓
            };

            let num = task
                .num_id
                .map(|n| n.to_string())
                .unwrap_or_else(|| "?".into());
            let notes_mark = match &task.notes {
                Some(n) if !n.is_empty() => " *",
                _ => "",
            };

            let (num_style, title_style) = if is_running {
                let phase = tick_count % 3;
                let color = match phase {
                    0 => ACCENT_GREEN,
                    1 => Color::Rgb(180, 240, 180),
                    _ => ACCENT_TEAL,
                };
                (
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )
            } else if is_overdue {
                (
                    Style::default()
                        .fg(ACCENT_RED)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(ACCENT_RED)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (task_num_style(task), task_title_style(task))
            };

            let status_style = match task.status {
                TaskStatus::Running => Style::default().fg(ACCENT_GREEN),
                TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
                TaskStatus::Done => Style::default().fg(ACCENT_TEAL),
                TaskStatus::Pending => {
                    if is_overdue {
                        Style::default().fg(ACCENT_RED)
                    } else {
                        Style::default().fg(FG_OVERLAY)
                    }
                }
            };

            let line1 = Line::from(vec![
                Span::styled(format!(" {:>3} ", num), num_style),
                Span::styled(format!("{} ", status_icon), status_style),
                Span::styled(format!("{}{}", task.title, notes_mark), title_style),
            ]);

            let meta_spans = build_compact_meta(task, today);
            if meta_spans.is_empty() {
                ListItem::new(vec![line1])
            } else {
                let mut line2_spans = vec![Span::raw("       ")];
                line2_spans.extend(meta_spans);
                let line2 = Line::from(line2_spans);
                ListItem::new(vec![line1, line2])
            }
        })
        .collect();

    // Pane title with stats
    let (elapsed, estimate, done, total) = pane.stats();
    let stats = build_pane_stats(elapsed, estimate, done, total);
    let stats_span = Span::styled(format!(" {} ", stats), Style::default().fg(FG_OVERLAY));

    List::new(items)
        .block(
            Block::bordered()
                .title(Span::styled(format!(" {} ", label), title_style))
                .title_bottom(stats_span)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .padding(Padding::horizontal(0)),
        )
        .highlight_style(
            Style::default()
                .bg(BG_SURFACE)
                .fg(FG_TEXT)
                .add_modifier(Modifier::BOLD),
        )
}

fn is_task_overdue(task: &Task, today: chrono::NaiveDate) -> bool {
    if task.status == TaskStatus::Done {
        return false;
    }
    if let Some(dl) = task.deadline {
        if dl < today {
            return true;
        }
    }
    false
}

fn build_pane_stats(elapsed: i64, estimate: i64, done: usize, total: usize) -> String {
    if total == 0 {
        return "(0)".to_string();
    }

    let elapsed_str = format_dur_short(elapsed);
    let estimate_str = format_dur_short(estimate);

    let pct = if estimate > 0 {
        (elapsed as f64 / estimate as f64 * 100.0) as u64
    } else {
        0
    };

    if estimate > 0 {
        format!(
            "{}/{} | {}% | {}/{}",
            elapsed_str, estimate_str, pct, done, total
        )
    } else {
        format!("{} | {}/{}", elapsed_str, done, total)
    }
}

fn task_num_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(ACCENT_GREEN),
        TaskStatus::Done => Style::default().fg(FG_SUBTEXT),
        TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
        TaskStatus::Pending => Style::default().fg(FG_SUBTEXT),
    }
}

fn task_title_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default()
            .fg(ACCENT_GREEN)
            .add_modifier(Modifier::BOLD),
        TaskStatus::Done => Style::default().fg(FG_SUBTEXT),
        TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
        TaskStatus::Pending => Style::default().fg(FG_TEXT),
    }
}

fn build_compact_meta(task: &Task, today: chrono::NaiveDate) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = vec![];
    let muted = Style::default().fg(FG_OVERLAY);
    let seven_days = today + chrono::Duration::days(7);

    if let Some(p) = task.priority {
        if p > 0 {
            if !spans.is_empty() {
                spans.push(Span::styled(" ", muted));
            }
            let pri_style = match p {
                4 => Style::default()
                    .fg(ACCENT_RED)
                    .add_modifier(Modifier::BOLD),
                3 => Style::default().fg(ACCENT_RED),
                2 => Style::default().fg(ACCENT_YELLOW),
                _ => Style::default().fg(FG_SUBTEXT),
            };
            let indicator = match p {
                4 => "\u{25A0}\u{25A0}\u{25A0}\u{25A0}",
                3 => "\u{25A0}\u{25A0}\u{25A0}",
                2 => "\u{25A0}\u{25A0}",
                _ => "\u{25A0}",
            };
            spans.push(Span::styled(indicator, pri_style));
        }
    }
    if let Some(ref p) = task.project {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        spans.push(Span::styled(
            format!("+{}", p),
            Style::default().fg(ACCENT_MAUVE),
        ));
    }
    if let Some(ref c) = task.context {
        for ctx in c.split(',') {
            let ctx = ctx.trim();
            if !ctx.is_empty() {
                if !spans.is_empty() {
                    spans.push(Span::styled(" ", muted));
                }
                spans.push(Span::styled(
                    format!("@{}", ctx),
                    Style::default().fg(ACCENT_TEAL),
                ));
            }
        }
    }
    if let Some(est) = task.estimate_minutes {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        spans.push(Span::styled(format!("~{}", format_est(est)), muted));
    }

    let elapsed = task.elapsed_seconds.unwrap_or(0);
    if elapsed > 0 {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        let elapsed_style = match task.estimate_minutes {
            Some(est) if elapsed > est * 60 => Style::default().fg(ACCENT_RED),
            Some(est) if elapsed > est * 45 => Style::default().fg(ACCENT_YELLOW),
            _ => Style::default().fg(ACCENT_GREEN),
        };
        spans.push(Span::styled(
            format!("({})", format_dur(elapsed)),
            elapsed_style,
        ));
    }

    if let Some(ref dl) = task.deadline {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        let dl_style = if *dl < today {
            Style::default()
                .fg(ACCENT_RED)
                .add_modifier(Modifier::BOLD)
        } else if *dl <= seven_days {
            Style::default().fg(ACCENT_PEACH)
        } else {
            muted
        };
        spans.push(Span::styled(format!("^{}", dl.format("%b%d")), dl_style));
    }
    if let Some(ref sc) = task.scheduled {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        let sc_style = if task.status != TaskStatus::Done && *sc < today {
            Style::default().fg(ACCENT_RED)
        } else {
            Style::default().fg(ACCENT_TEAL)
        };
        spans.push(Span::styled(format!("={}", sc.format("%b%d")), sc_style));
    }

    spans
}

// ── Modals ───────────────────────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_shadow(f: &mut Frame, area: Rect) {
    if area.x + area.width < f.area().width && area.y + area.height < f.area().height {
        let shadow = Rect::new(area.x + 1, area.y + 1, area.width, area.height);
        let shadow_block = Block::default().style(Style::default().bg(Color::Rgb(20, 20, 30)));
        f.render_widget(shadow_block, shadow);
    }
}

fn draw_note_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let title = format!(" Notes: {} ", app.note_task_title);
    let help_text = if app.mode == AppMode::NoteEdit {
        " Enter:save  Esc:cancel "
    } else {
        " e:edit  Esc/q:close "
    };

    let block = Block::bordered()
        .title(Span::styled(
            title,
            Style::default()
                .fg(ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(help_text, Style::default().fg(FG_OVERLAY)))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_YELLOW))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.mode == AppMode::NoteEdit {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(3)])
            .split(inner);

        let content = if app.note_content.is_empty() {
            "(no notes yet)".to_string()
        } else {
            app.note_content.clone()
        };
        let notes_widget = Paragraph::new(content)
            .style(Style::default().fg(FG_SUBTEXT))
            .wrap(Wrap { trim: false });
        f.render_widget(notes_widget, chunks[0]);

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(FG_OVERLAY))
            .border_type(BorderType::Rounded);
        let input_text = format!("\u{276F} {}\u{2588}", app.note_input);
        let input_widget = Paragraph::new(input_text)
            .style(Style::default().fg(FG_TEXT))
            .block(input_block);
        f.render_widget(input_widget, chunks[1]);
    } else {
        let content = if app.note_content.is_empty() {
            "(no notes)".to_string()
        } else {
            app.note_content.clone()
        };
        let notes_widget = Paragraph::new(content)
            .style(Style::default().fg(FG_SUBTEXT))
            .wrap(Wrap { trim: false });
        f.render_widget(notes_widget, inner);
    }
}

fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .title(Span::styled(
            " Delete Task ",
            Style::default()
                .fg(ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_RED))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Delete ", Style::default().fg(FG_TEXT)),
            Span::styled(
                format!("\"{}\"", app.delete_task_title),
                Style::default()
                    .fg(FG_TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("?", Style::default().fg(FG_TEXT)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                " y ",
                Style::default().fg(FG_TEXT).bg(ACCENT_RED),
            ),
            Span::styled(" yes  ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(
                " n ",
                Style::default().fg(FG_TEXT).bg(BG_SURFACE),
            ),
            Span::styled(" no", Style::default().fg(FG_SUBTEXT)),
        ]),
    ];

    f.render_widget(Paragraph::new(text), inner);
}

fn draw_edit_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 70, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let title_text = if app.mode == AppMode::EditTaskField {
        format!(" Edit: {} ", EDIT_FIELD_LABELS[app.edit_field_index])
    } else {
        " Edit Task ".to_string()
    };

    let help_text = if app.mode == AppMode::EditTaskField {
        " Enter:save  Esc:cancel "
    } else {
        " j/k:navigate  Enter:edit  Esc:close "
    };

    let block = Block::bordered()
        .title(Span::styled(
            title_text,
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(help_text, Style::default().fg(FG_OVERLAY)))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_BLUE))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.mode == AppMode::EditTaskField {
        // Show the field being edited with input
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(3)])
            .split(inner);

        // Show all fields above for context (dimmed)
        let mut lines: Vec<Line> = vec![];
        for (i, label) in EDIT_FIELD_LABELS.iter().enumerate() {
            let style = if i == app.edit_field_index {
                Style::default()
                    .fg(ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_OVERLAY)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<12}", label), style),
                Span::styled(
                    app.edit_field_values[i].clone(),
                    if i == app.edit_field_index {
                        Style::default().fg(FG_OVERLAY)
                    } else {
                        Style::default().fg(FG_OVERLAY)
                    },
                ),
            ]));
        }
        f.render_widget(Paragraph::new(lines), chunks[0]);

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(ACCENT_BLUE))
            .border_type(BorderType::Rounded);
        let input_text = format!("\u{276F} {}\u{2588}", app.edit_field_input);
        let input_widget = Paragraph::new(input_text)
            .style(Style::default().fg(FG_TEXT))
            .block(input_block);
        f.render_widget(input_widget, chunks[1]);
    } else {
        // Show field list with selection highlight
        let mut lines: Vec<Line> = vec![];
        for (i, label) in EDIT_FIELD_LABELS.iter().enumerate() {
            let is_selected = i == app.edit_field_index;
            let (label_style, value_style) = if is_selected {
                (
                    Style::default()
                        .fg(ACCENT_BLUE)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    Style::default().fg(FG_SUBTEXT),
                    Style::default().fg(FG_TEXT),
                )
            };
            let indicator = if is_selected { "\u{25B6} " } else { "  " };
            let value = &app.edit_field_values[i];
            let display_value = if value.is_empty() {
                "(empty)".to_string()
            } else {
                value.clone()
            };
            lines.push(Line::from(vec![
                Span::styled(indicator, Style::default().fg(ACCENT_BLUE)),
                Span::styled(format!("{:<12}", label), label_style),
                Span::styled(display_value, value_style),
            ]));
            lines.push(Line::from(""));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}

fn draw_add_bar(f: &mut Frame, app: &App) {
    // Render input bar at the bottom of the content area, above the footer
    let area = f.area();
    if area.height < 5 {
        return;
    }
    let bar_area = Rect::new(area.x, area.height - 4, area.width, 3);
    f.render_widget(Clear, bar_area);

    let block = Block::bordered()
        .title(Span::styled(
            " Add Task ",
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " +project @context #tag ~estimate ^deadline =scheduled !priority ",
            Style::default().fg(FG_OVERLAY),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_GREEN))
        .padding(Padding::horizontal(1));

    let inner = block.inner(bar_area);
    f.render_widget(block, bar_area);

    let input_text = format!("\u{276F} {}\u{2588}", app.add_input);
    f.render_widget(
        Paragraph::new(input_text).style(Style::default().fg(FG_TEXT)),
        inner,
    );
}

fn draw_move_bar(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.height < 4 {
        return;
    }
    let bar_area = Rect::new(area.x, area.height - 3, area.width, 2);
    f.render_widget(Clear, bar_area);

    let targets = ["LONG TERM", "THIS WEEK", "TODAY"];
    let mut spans: Vec<Span> = vec![Span::styled(
        " Move to: ",
        Style::default()
            .fg(ACCENT_BLUE)
            .add_modifier(Modifier::BOLD),
    )];

    for (i, name) in targets.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        if i == app.move_source {
            spans.push(Span::styled(
                format!(" {} ", name),
                Style::default().fg(FG_OVERLAY),
            ));
        } else if i == app.move_target {
            spans.push(Span::styled(
                format!(" {} ", name),
                Style::default()
                    .fg(FG_TEXT)
                    .bg(ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {} ", name),
                Style::default().fg(FG_SUBTEXT),
            ));
        }
    }
    spans.push(Span::styled(
        "  Enter:move  Esc:cancel",
        Style::default().fg(FG_OVERLAY),
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), bar_area);
}

// ── Formatting helpers ───────────────────────────────────────────────

fn format_dur(seconds: i64) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{}h{}m{}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m{}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

fn format_dur_short(seconds: i64) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{}h{}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

fn format_est(minutes: i64) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 && mins > 0 {
        format!("{}h{}m", hours, mins)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        format!("{}m", mins)
    }
}
