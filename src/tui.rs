use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, LineGauge, List, ListItem, ListState, Padding,
        Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Sparkline, Tabs, Wrap,
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
    sort_index: usize,
}

impl PaneState {
    fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            tasks: Vec::new(),
            list_state,
            sort_index: 0,
        }
    }

    fn jump(&mut self, n: usize) {
        if self.tasks.is_empty() {
            return;
        }
        let len = self.tasks.len();
        let i = match self.list_state.selected() {
            Some(i) => (i + n) % len,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn jump_back(&mut self, n: usize) {
        if self.tasks.is_empty() {
            return;
        }
        let len = self.tasks.len();
        let i = match self.list_state.selected() {
            Some(i) => (i + len - (n % len)) % len,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn jump_to_first(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn jump_to_last(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(self.tasks.len() - 1));
        }
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
    AddTask,
    MoveTask,
    ConfirmDelete,
    EditTask,
    EditTaskField,
    NoteView,
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

const EDIT_FIELD_LABELS: [&str; 9] = [
    "Title", "Project", "Context", "Tags", "Estimate", "Deadline", "Scheduled", "Priority", "Notes",
];

struct App<'a> {
    panes: [PaneState; 4],
    active_pane: usize,
    running_task: Option<String>,
    db: &'a Database,
    mode: AppMode,
    // Tabs & report
    tab: TuiTab,
    report_range: ReportRange,
    report: Option<ReportData>,
    tick_count: u64,
    frame_count: u64,
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
    edit_field_values: [String; 9],
    edit_field_input: String,
    // Note view
    note_lines: Vec<String>,
    note_selected: usize,
    note_editing: bool,
    // Vim count prefix & g key
    count_prefix: Option<usize>,
    pending_g: bool,
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
            running_task: None,
            db,
            mode: AppMode::Normal,
            tab: TuiTab::Tasks,
            report_range: ReportRange::Day,
            report: None,
            tick_count: 0,
            frame_count: 0,
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
            note_lines: Vec::new(),
            note_selected: 0,
            note_editing: false,
            count_prefix: None,
            pending_g: false,
        }
    }

    fn cycle_sort(&mut self) {
        let pane = &mut self.panes[self.active_pane];
        pane.sort_index = (pane.sort_index + 1) % SORT_MODES.len();
        let sort = SORT_MODES[pane.sort_index];
        let is_done = self.active_pane == 3;
        pane.tasks.sort_by(|a, b| sort_tasks(a, b, sort, is_done));
    }

    fn refresh_all(&mut self) -> Result<()> {
        let all_tasks = self.db.list_all_tasks(SortBy::Created)?;

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
            let sort = SORT_MODES[self.panes[i].sort_index];
            let is_done = i == 3;
            self.panes[i].tasks.sort_by(|a, b| sort_tasks(a, b, sort, is_done));
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

    fn open_note_quick(&mut self) {
        self.start_edit_task();
        if self.mode == AppMode::EditTask {
            self.edit_field_index = 8;
            self.edit_field_input.clear();
            let notes = &self.edit_field_values[8];
            if notes.is_empty() {
                // No notes — go straight to append input
                self.mode = AppMode::EditTaskField;
            } else {
                // Has notes — enter NoteView for browsing/editing
                self.note_lines = notes.lines().map(|l| l.to_string()).collect();
                self.note_selected = 0;
                self.note_editing = false;
                self.mode = AppMode::NoteView;
            }
        }
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

    fn move_task_quick(&mut self, direction: i32) -> Result<()> {
        if self.active_pane == 3 {
            return Ok(());
        }
        let target = (self.active_pane as i32 + direction).clamp(0, 2) as usize;
        if target == self.active_pane {
            return Ok(());
        }
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            if task.status == TaskStatus::Done {
                return Ok(());
            }
            let task_id = task.id.clone();
            let today = chrono::Local::now().date_naive();
            let date = match target {
                0 => today + chrono::Duration::days(8),
                1 => today + chrono::Duration::days(1),
                _ => today,
            };
            self.db.update_task_scheduled(&task_id, date)?;
            self.refresh_all()?;
            self.active_pane = target;
            if let Some(pos) = self.panes[target].tasks.iter().position(|t| t.id == task_id) {
                self.panes[target].list_state.select(Some(pos));
            }
        }
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
                task.notes.clone().unwrap_or_default(),
            ];
            self.edit_field_input.clear();
            self.mode = AppMode::EditTask;
        }
    }

    fn enter_edit_field(&mut self) {
        if self.edit_field_index == 8 {
            let notes = &self.edit_field_values[8];
            if notes.is_empty() {
                // No notes — go straight to append input
                self.edit_field_input.clear();
                self.mode = AppMode::EditTaskField;
            } else {
                // Has notes — enter NoteView
                self.note_lines = notes.lines().map(|l| l.to_string()).collect();
                self.note_selected = 0;
                self.note_editing = false;
                self.mode = AppMode::NoteView;
            }
        } else {
            self.edit_field_input = self.edit_field_values[self.edit_field_index].clone();
            self.mode = AppMode::EditTaskField;
        }
    }

    fn save_notes(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.edit_task_id {
            let full = self.note_lines.join("\n");
            self.db.update_notes_by_id(task_id, &full)?;
            self.edit_field_values[8] = full;
            self.refresh_all()?;
        }
        Ok(())
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
                8 => {
                    // Notes (append)
                    if !self.edit_field_input.is_empty() {
                        self.db
                            .append_note_by_id(task_id, &self.edit_field_input)?;
                        let notes = self.db.get_task_notes_by_id(task_id)?;
                        self.edit_field_values[8] = notes.unwrap_or_default();
                    }
                }
                _ => {}
            }
            self.refresh_all()?;
        }
        // After appending a note, return to NoteView so the user sees the updated list
        if idx == 8 && !self.edit_field_values[8].is_empty() {
            self.note_lines = self.edit_field_values[8]
                .lines()
                .map(|l| l.to_string())
                .collect();
            self.note_selected = self.note_lines.len().saturating_sub(1);
            self.note_editing = false;
            self.mode = AppMode::NoteView;
        } else {
            self.mode = AppMode::EditTask;
        }
        Ok(())
    }
}

// ── Sorting ──────────────────────────────────────────────────────────

fn sort_tasks(a: &Task, b: &Task, sort: SortBy, is_done: bool) -> std::cmp::Ordering {
    let ord = match sort {
        SortBy::Created | SortBy::Area => a.created.cmp(&b.created),
        SortBy::Modified => {
            let a_mod = a.modified_at.unwrap_or(a.created);
            let b_mod = b.modified_at.unwrap_or(b.created);
            a_mod.cmp(&b_mod)
        }
        SortBy::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
    };
    if is_done { ord.reverse() } else { ord }
}

fn sort_label(sort: SortBy) -> &'static str {
    match sort {
        SortBy::Created => "created",
        SortBy::Modified => "modified",
        SortBy::Title => "title",
        SortBy::Area => "area",
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
        app.frame_count = app.frame_count.wrapping_add(1);
        terminal.draw(|f| draw_ui(f, app))?;

        if crossterm::event::poll(poll_rate)? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('t') => {
                            app.tab = TuiTab::Tasks;
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Char('r') => {
                            app.tab = TuiTab::Report;
                            let _ = app.refresh_report();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Tab => {
                            if app.tab == TuiTab::Tasks {
                                app.tab = TuiTab::Report;
                                let _ = app.refresh_report();
                            } else {
                                app.tab = TuiTab::Tasks;
                            }
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        _ => {
                            if app.tab == TuiTab::Tasks {
                                // Handle pending 'g' for gg (jump to first)
                                if app.pending_g {
                                    app.pending_g = false;
                                    if key.code == KeyCode::Char('g') {
                                        app.panes[app.active_pane].jump_to_first();
                                        app.count_prefix = None;
                                        // Skip further processing
                                    } else {
                                        // g followed by non-g, ignore the g
                                        // and fall through to handle this key normally
                                        handle_tasks_key(app, key.code);
                                    }
                                } else {
                                    // Accumulate digit count prefix
                                    match key.code {
                                        KeyCode::Char(c @ '1'..='9')
                                            if app.count_prefix.is_none() =>
                                        {
                                            app.count_prefix =
                                                Some(c.to_digit(10).unwrap() as usize);
                                        }
                                        KeyCode::Char(c @ '0'..='9')
                                            if app.count_prefix.is_some() =>
                                        {
                                            let current = app.count_prefix.unwrap_or(0);
                                            app.count_prefix = Some(
                                                current * 10 + c.to_digit(10).unwrap() as usize,
                                            );
                                        }
                                        _ => {
                                            handle_tasks_key(app, key.code);
                                        }
                                    }
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
                                    _ => {}
                                }
                            }
                        }
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
                            if app.edit_field_index < 8 {
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
                            if app.edit_field_index == 8
                                && key.modifiers.contains(event::KeyModifiers::ALT)
                            {
                                app.edit_field_input.push('\n');
                            } else {
                                let _ = app.save_edit_field();
                            }
                        }
                        KeyCode::Backspace => {
                            app.edit_field_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.edit_field_input.push(c);
                        }
                        _ => {}
                    },
                    AppMode::NoteView => {
                        if app.note_editing {
                            match key.code {
                                KeyCode::Esc => {
                                    app.note_editing = false;
                                }
                                KeyCode::Enter => {
                                    if key.modifiers.contains(event::KeyModifiers::ALT) {
                                        app.edit_field_input.push('\n');
                                    } else {
                                        // Save edited line back
                                        if app.note_selected < app.note_lines.len() {
                                            app.note_lines[app.note_selected] =
                                                app.edit_field_input.clone();
                                            let _ = app.save_notes();
                                        }
                                        app.note_editing = false;
                                    }
                                }
                                KeyCode::Backspace => {
                                    app.edit_field_input.pop();
                                }
                                KeyCode::Char(c) => {
                                    app.edit_field_input.push(c);
                                }
                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Esc => {
                                    app.mode = AppMode::EditTask;
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if !app.note_lines.is_empty()
                                        && app.note_selected < app.note_lines.len() - 1
                                    {
                                        app.note_selected += 1;
                                    }
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    if app.note_selected > 0 {
                                        app.note_selected -= 1;
                                    }
                                }
                                KeyCode::Enter | KeyCode::Char('e') => {
                                    if app.note_selected < app.note_lines.len() {
                                        app.edit_field_input =
                                            app.note_lines[app.note_selected].clone();
                                        app.note_editing = true;
                                    }
                                }
                                KeyCode::Char('a') => {
                                    // Append new note — switch to EditTaskField for notes
                                    app.edit_field_index = 8;
                                    app.edit_field_input.clear();
                                    app.mode = AppMode::EditTaskField;
                                }
                                KeyCode::Char('d') | KeyCode::Delete => {
                                    if app.note_selected < app.note_lines.len() {
                                        app.note_lines.remove(app.note_selected);
                                        if app.note_selected >= app.note_lines.len()
                                            && app.note_selected > 0
                                        {
                                            app.note_selected -= 1;
                                        }
                                        let _ = app.save_notes();
                                        if app.note_lines.is_empty() {
                                            app.mode = AppMode::EditTask;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
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

fn handle_tasks_key(app: &mut App, code: KeyCode) {
    let count = app.count_prefix.take().unwrap_or(1);
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.panes[app.active_pane].jump(count);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.panes[app.active_pane].jump_back(count);
        }
        KeyCode::Char('h') | KeyCode::Left => app.move_pane_left(),
        KeyCode::Char('l') | KeyCode::Right => app.move_pane_right(),
        KeyCode::PageDown => {
            app.panes[app.active_pane].jump(10);
        }
        KeyCode::PageUp => {
            app.panes[app.active_pane].jump_back(10);
        }
        KeyCode::Char('G') => {
            app.panes[app.active_pane].jump_to_last();
        }
        KeyCode::Char('g') => {
            app.pending_g = true;
        }
        KeyCode::Char('s') => {
            let _ = app.toggle_selected();
        }
        KeyCode::Char('d') => {
            let _ = app.done();
        }
        KeyCode::Char('o') => app.cycle_sort(),
        KeyCode::Char('n') => {
            app.open_note_quick();
        }
        KeyCode::Char('a') => {
            app.start_add_task();
        }
        KeyCode::Char('m') => {
            app.start_move_task();
        }
        KeyCode::Char('<') => {
            let _ = app.move_task_quick(-1);
        }
        KeyCode::Char('>') => {
            let _ = app.move_task_quick(1);
        }
        KeyCode::Enter => {
            app.start_edit_task();
        }
        KeyCode::Backspace | KeyCode::Delete => {
            app.start_delete();
        }
        _ => {}
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
    let tab_titles: Vec<Line> = vec![Line::from(" Tasks (t) "), Line::from(" Report (r) ")];
    let tab_index = if app.tab == TuiTab::Tasks { 0 } else { 1 };
    let tabs = Tabs::new(tab_titles)
        .select(tab_index)
        .style(Style::default().fg(FG_OVERLAY))
        .highlight_style(
            Style::default()
                .fg(Color::Rgb(30, 30, 46))
                .bg(FG_TEXT)
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
        AppMode::ConfirmDelete => draw_delete_modal(f, app),
        AppMode::EditTask | AppMode::EditTaskField => draw_edit_modal(f, app),
        AppMode::NoteView => draw_note_view_modal(f, app),
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

    let header_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(FG_OVERLAY))
        .border_type(BorderType::Rounded);
    let inner = header_block.inner(area);
    f.render_widget(header_block, area);

    let muted = Style::default().fg(FG_OVERLAY);
    let legend = Line::from(vec![
        Span::styled("\u{25CB}", muted),
        Span::styled("pend ", muted),
        Span::styled("\u{25B6}", Style::default().fg(ACCENT_GREEN)),
        Span::styled("run ", muted),
        Span::styled("\u{23F8}", Style::default().fg(ACCENT_YELLOW)),
        Span::styled("pause ", muted),
        Span::styled("\u{2713}", Style::default().fg(ACCENT_TEAL)),
        Span::styled("done ", muted),
        Span::styled("+proj ", Style::default().fg(ACCENT_MAUVE)),
        Span::styled("@ctx ", Style::default().fg(ACCENT_TEAL)),
        Span::styled("~est ", muted),
        Span::styled("^dead ", Style::default().fg(ACCENT_PEACH)),
        Span::styled("=sched ", Style::default().fg(ACCENT_TEAL)),
        Span::styled("!pri ", Style::default().fg(ACCENT_RED)),
    ]);

    let legend_width: u16 = legend.spans.iter().map(|s| s.content.len() as u16).sum();

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(legend_width)])
        .split(inner);

    let left = Line::from(vec![
        Span::styled(
            " DODO ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(running_info, running_style),
    ]);
    f.render_widget(Paragraph::new(left), cols[0]);
    f.render_widget(Paragraph::new(legend).alignment(Alignment::Right), cols[1]);
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
                ("</>", "move"),
                ("\u{21B5}", "detail"),
                ("\u{232B}", "del"),
                ("s", "start"),
                ("d", "done"),
                ("n", "note"),
                ("o", "sort"),
                ("r", "report"),
                ("q", "quit"),
            ],
        }
    } else {
        vec![
            ("h/l", "range"),
            ("t", "tasks"),
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
    let now = chrono::Local::now();
    let today = now.date_naive();
    let tomorrow = today + chrono::Duration::days(1);
    let week_end = today + chrono::Duration::days(7);

    let headers = [
        "LONG TERM".to_string(),
        format!("THIS WEEK \u{2014} {}\u{2013}{}", tomorrow.format("%b%d"), week_end.format("%b%d")),
        format!("TODAY \u{2014} {}", today.format("%b%d")),
        "DONE".to_string(),
    ];

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
        let sl = sort_label(SORT_MODES[app.panes[i].sort_index]);
        draw_pane(f, &app.panes[i], &headers[i], is_active, app.frame_count, sl, pane_chunks[i]);
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
        .constraints([Constraint::Length(12), Constraint::Min(0)])
        .split(cols[0]);

    // Summary stats with Gauge
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

    let summary_block = Block::bordered()
        .title(Span::styled(
            " Summary ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FG_OVERLAY))
        .padding(Padding::horizontal(1));

    let summary_inner = summary_block.inner(left_layout[0]);
    f.render_widget(summary_block, left_layout[0]);

    let summary_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(summary_inner);

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
    f.render_widget(Paragraph::new(summary_lines), summary_chunks[0]);

    // Productivity Gauge: tasks done ratio (capped at 100%)
    let done_ratio = if report.tasks_done > 0 { 1.0_f64.min(report.tasks_done as f64 / (report.tasks_done as f64 + 1.0)) } else { 0.0 };
    let done_gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT_GREEN).bg(Color::Rgb(40, 42, 54)))
        .ratio(done_ratio)
        .label(format!("{} done", report.tasks_done))
        .use_unicode(true);
    f.render_widget(done_gauge, summary_chunks[1]);

    // Productivity section with Sparkline
    let prod_block = Block::bordered()
        .title(Span::styled(
            " Productivity ",
            Style::default()
                .fg(ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FG_OVERLAY))
        .padding(Padding::horizontal(1));

    let prod_inner = prod_block.inner(left_layout[1]);
    f.render_widget(prod_block, left_layout[1]);

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

    // Sparkline for hours worked distribution
    if !report.by_hour.is_empty() {
        prod_lines.push(Line::from(""));
        prod_lines.push(Line::from(Span::styled(
            "  Hours worked:",
            Style::default().fg(FG_SUBTEXT),
        )));

        // Build 24-hour sparkline data
        let mut hour_data = vec![0u64; 24];
        for (hour, secs) in &report.by_hour {
            if (*hour as usize) < 24 {
                hour_data[*hour as usize] = *secs as u64;
            }
        }

        // Render text stats first, then sparkline
        let prod_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(prod_lines.len() as u16),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(prod_inner);

        f.render_widget(Paragraph::new(prod_lines), prod_chunks[0]);

        let sparkline = Sparkline::default()
            .data(&hour_data)
            .style(Style::default().fg(ACCENT_TEAL));
        f.render_widget(sparkline, prod_chunks[1]);

        // Hour labels below sparkline
        let hour_labels = Line::from(vec![
            Span::styled("  0", Style::default().fg(FG_OVERLAY)),
            Span::styled("     6", Style::default().fg(FG_OVERLAY)),
            Span::styled("      12", Style::default().fg(FG_OVERLAY)),
            Span::styled("     18", Style::default().fg(FG_OVERLAY)),
            Span::styled("    23", Style::default().fg(FG_OVERLAY)),
        ]);
        if prod_chunks[2].height > 0 {
            let label_area = Rect::new(prod_chunks[2].x, prod_chunks[2].y, prod_chunks[2].width, 1);
            f.render_widget(Paragraph::new(hour_labels), label_area);
        }
    } else {
        f.render_widget(Paragraph::new(prod_lines), prod_inner);
    }

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

// ── Pane Drawing ─────────────────────────────────────────────────────

/// Apply neon sign sweep effect: a glow spot travels left→right→left.
fn apply_neon(line: Line<'static>, frame_count: u64, width: u16) -> Line<'static> {
    let t = (frame_count as f64 * 0.025).sin() * 0.5 + 0.5;
    let wave_center = t * width as f64;
    let sigma = width as f64 * 0.25;

    let mut result: Vec<Span<'static>> = Vec::new();
    let mut x: f64 = 0.0;

    for span in line.spans {
        let base_style = span.style;
        for ch in span.content.chars() {
            let d = x - wave_center;
            let intensity = (-0.5 * (d / sigma).powi(2)).exp();
            let bg = Color::Rgb(
                (20.0 + intensity * 80.0) as u8,
                (15.0 + intensity * 65.0) as u8,
                (40.0 + intensity * 215.0) as u8,
            );
            result.push(Span::styled(ch.to_string(), base_style.bg(bg)));
            x += 1.0;
        }
    }

    // Fill remaining row width with the glow
    while (x as u16) < width.saturating_sub(2) {
        let d = x - wave_center;
        let intensity = (-0.5 * (d / sigma).powi(2)).exp();
        let bg = Color::Rgb(
            (20.0 + intensity * 80.0) as u8,
            (15.0 + intensity * 65.0) as u8,
            (40.0 + intensity * 215.0) as u8,
        );
        result.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1.0;
    }

    Line::from(result)
}

fn draw_pane(
    f: &mut Frame,
    pane: &PaneState,
    label: &str,
    is_active: bool,
    frame_count: u64,
    sort_label_str: &str,
    area: Rect,
) {
    let border_color = if is_active { ACCENT_BLUE } else { FG_OVERLAY };
    let border_style = Style::default().fg(border_color);

    let title_style = if is_active {
        Style::default()
            .fg(ACCENT_BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(FG_SUBTEXT)
    };

    let block = Block::bordered()
        .title(Span::styled(format!(" {} ", label), title_style))
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // Split inner into stats sub-header (2 lines) and task list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);

    // Stats sub-header with right-aligned sort label
    let (elapsed, estimate, done, total) = pane.stats();
    let stats_text = build_pane_stats(elapsed, estimate, done, total);
    let left_text = format!(" {}", stats_text);
    let right_text = format!("{} ", sort_label_str);
    let pad = (chunks[0].width as usize).saturating_sub(left_text.len() + right_text.len());
    let stats_line = Line::from(vec![
        Span::styled(left_text, Style::default().fg(FG_SUBTEXT)),
        Span::raw(" ".repeat(pad)),
        Span::styled(right_text, Style::default().fg(FG_OVERLAY)),
    ]);
    let stats_area = Rect::new(chunks[0].x, chunks[0].y, chunks[0].width, 1);
    f.render_widget(Paragraph::new(stats_line), stats_area);

    // LineGauge progress bar
    let ratio = if estimate > 0 {
        (elapsed as f64 / estimate as f64).min(1.0)
    } else {
        0.0
    };
    let gauge_color = if ratio >= 1.0 {
        ACCENT_RED
    } else if ratio >= 0.75 {
        ACCENT_YELLOW
    } else {
        ACCENT_GREEN
    };
    let gauge = LineGauge::default()
        .filled_style(Style::default().fg(gauge_color))
        .unfilled_style(Style::default().fg(Color::Rgb(40, 42, 54)))
        .ratio(ratio);
    let gauge_area = Rect::new(chunks[0].x, chunks[0].y + 1, chunks[0].width, 1);
    f.render_widget(gauge, gauge_area);

    // Task list area
    let list_area = chunks[1];
    let today = chrono::Local::now().date_naive();

    let selected_idx = pane.list_state.selected();
    let neon_width = list_area.width;

    let items: Vec<ListItem> = pane
        .tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| {
            let is_selected = is_active && selected_idx == Some(idx);
            let is_running = task.status == TaskStatus::Running;
            let is_neon = is_running;
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
                (
                    Style::default()
                        .fg(ACCENT_GREEN)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(ACCENT_GREEN)
                        .add_modifier(Modifier::BOLD),
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

            if is_neon {
                // Neon sign sweep for running task
                let neon_line1 = apply_neon(line1, frame_count, neon_width);
                if meta_spans.is_empty() {
                    ListItem::new(vec![neon_line1])
                } else {
                    let mut line2_spans = vec![Span::raw("       ")];
                    line2_spans.extend(meta_spans);
                    let line2 = Line::from(line2_spans);
                    let neon_line2 = apply_neon(line2, frame_count, neon_width);
                    ListItem::new(vec![neon_line1, neon_line2])
                }
            } else if is_selected {
                // Selected cursor: static highlight background
                let bg = Color::Rgb(65, 75, 120);
                let item = if meta_spans.is_empty() {
                    ListItem::new(vec![line1])
                } else {
                    let mut line2_spans = vec![Span::raw("       ")];
                    line2_spans.extend(meta_spans);
                    let line2 = Line::from(line2_spans);
                    ListItem::new(vec![line1, line2])
                };
                item.style(Style::default().bg(bg))
            } else {
                if meta_spans.is_empty() {
                    ListItem::new(vec![line1])
                } else {
                    let mut line2_spans = vec![Span::raw("       ")];
                    line2_spans.extend(meta_spans);
                    let line2 = Line::from(line2_spans);
                    ListItem::new(vec![line1, line2])
                }
            }
        })
        .collect();

    let list = List::new(items);
    let list = if is_active {
        list.highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("\u{258C} ")
    } else {
        list.highlight_symbol("  ")
    };

    f.render_stateful_widget(list, list_area, &mut pane.list_state.clone());

    // Scrollbar (only when tasks exceed visible area)
    // Each task item is ~2 lines, so approximate visible count
    let visible_approx = list_area.height as usize / 2;
    if pane.tasks.len() > visible_approx && list_area.height > 0 {
        let mut scrollbar_state = ScrollbarState::new(pane.tasks.len())
            .position(pane.list_state.selected().unwrap_or(0));
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(FG_OVERLAY));
        f.render_stateful_widget(scrollbar, list_area, &mut scrollbar_state);
    }
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

    // Priority
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

    // Project
    if let Some(ref p) = task.project {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        spans.push(Span::styled(
            format!("+{}", p),
            Style::default().fg(ACCENT_MAUVE),
        ));
    }

    // Contexts
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

    // Elapsed (before estimate)
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

    // Estimate (after elapsed)
    if let Some(est) = task.estimate_minutes {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        spans.push(Span::styled(format!("~{}", format_est(est)), muted));
    }

    // Scheduled (before deadline)
    if let Some(ref sc) = task.scheduled {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        let sc_style = if task.status != TaskStatus::Done && *sc < today {
            Style::default()
                .bg(ACCENT_RED)
                .fg(Color::Rgb(30, 30, 46))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(ACCENT_TEAL)
        };
        spans.push(Span::styled(format!("={}", sc.format("%b%d")), sc_style));
    }

    // Deadline (after scheduled)
    if let Some(ref dl) = task.deadline {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        let dl_style = if task.status != TaskStatus::Done && *dl < today {
            Style::default()
                .bg(ACCENT_RED)
                .fg(Color::Rgb(30, 30, 46))
                .add_modifier(Modifier::BOLD)
        } else if *dl <= seven_days {
            Style::default().fg(ACCENT_PEACH)
        } else {
            muted
        };
        spans.push(Span::styled(format!("^{}", dl.format("%b%d")), dl_style));
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

const EDIT_FIELD_HINTS: [&str; 9] = [
    "Task name (plain text)",
    "Project name (no + prefix needed)",
    "Comma-separated, e.g.: work, laptop",
    "Comma-separated, e.g.: urgent, frontend",
    "Duration, e.g.: 30m, 1h, 2h30m, 1d",
    "Date, e.g.: today, tmr, fri, 0215, 2025-05-02",
    "Date, e.g.: today, tmr, 3d, mon",
    "! to !!!! (1-4 levels)",
    "Type to append. Alt+Enter for newline",
];

fn draw_edit_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 70, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let title_text = if app.mode == AppMode::EditTaskField {
        format!(" Edit: {} ", EDIT_FIELD_LABELS[app.edit_field_index])
    } else {
        " Task Detail ".to_string()
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

    let notes_content = &app.edit_field_values[8];
    let on_notes_field = app.edit_field_index == 8;

    if app.mode == AppMode::EditTaskField {
        if on_notes_field {
            // Editing Notes: show existing notes above, input below
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(4)])
                .split(inner);

            let content = if notes_content.is_empty() {
                "(no notes yet)".to_string()
            } else {
                notes_content.clone()
            };
            let notes_widget = Paragraph::new(content)
                .style(Style::default().fg(FG_SUBTEXT))
                .wrap(Wrap { trim: false });
            f.render_widget(notes_widget, chunks[0]);

            let input_block = Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(ACCENT_YELLOW))
                .border_type(BorderType::Rounded);
            let input_lines = vec![
                Line::from(Span::styled(
                    format!("  {}", EDIT_FIELD_HINTS[8]),
                    Style::default().fg(FG_OVERLAY),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("\u{276F} {}\u{2588}", app.edit_field_input),
                    Style::default().fg(FG_TEXT),
                )),
            ];
            let input_widget = Paragraph::new(input_lines).block(input_block);
            f.render_widget(input_widget, chunks[1]);
        } else {
            // Editing a regular field: fields above, input below
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(4)])
                .split(inner);

            let mut lines: Vec<Line> = vec![];
            for (i, label) in EDIT_FIELD_LABELS[..8].iter().enumerate() {
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
                        Style::default().fg(FG_OVERLAY),
                    ),
                ]));
            }
            f.render_widget(Paragraph::new(lines), chunks[0]);

            let input_block = Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(ACCENT_BLUE))
                .border_type(BorderType::Rounded);
            let hint = EDIT_FIELD_HINTS[app.edit_field_index];
            let input_lines = vec![
                Line::from(Span::styled(
                    format!("  {}", hint),
                    Style::default().fg(FG_OVERLAY),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("\u{276F} {}\u{2588}", app.edit_field_input),
                    Style::default().fg(FG_TEXT),
                )),
            ];
            let input_widget = Paragraph::new(input_lines).block(input_block);
            f.render_widget(input_widget, chunks[1]);
        }
    } else {
        // Field list view — split into fields + notes section
        let show_notes = on_notes_field && !notes_content.is_empty();
        let constraints = if show_notes {
            vec![Constraint::Length(18), Constraint::Min(1)]
        } else {
            vec![Constraint::Min(1)]
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

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

            // For Notes field, show line count preview instead of full content
            let display_value = if i == 8 {
                let v = &app.edit_field_values[8];
                if v.is_empty() {
                    "(no notes)".to_string()
                } else {
                    let line_count = v.lines().count();
                    format!("({} line{})", line_count, if line_count == 1 { "" } else { "s" })
                }
            } else {
                let v = &app.edit_field_values[i];
                if v.is_empty() {
                    "(empty)".to_string()
                } else {
                    v.clone()
                }
            };

            lines.push(Line::from(vec![
                Span::styled(indicator, Style::default().fg(ACCENT_BLUE)),
                Span::styled(format!("{:<12}", label), label_style),
                Span::styled(display_value, value_style),
            ]));
            if is_selected {
                lines.push(Line::from(Span::styled(
                    format!("    {}", EDIT_FIELD_HINTS[i]),
                    Style::default().fg(FG_OVERLAY),
                )));
            } else {
                lines.push(Line::from(""));
            }
        }
        f.render_widget(Paragraph::new(lines), chunks[0]);

        // Show notes content below fields when Notes is selected
        if show_notes {
            let notes_block = Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(FG_OVERLAY))
                .border_type(BorderType::Rounded);
            let notes_widget = Paragraph::new(notes_content.clone())
                .style(Style::default().fg(FG_SUBTEXT))
                .wrap(Wrap { trim: false })
                .block(notes_block);
            f.render_widget(notes_widget, chunks[1]);
        }
    }
}

fn draw_note_view_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 70, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let help_text = if app.note_editing {
        " Enter:save  Alt+Enter:newline  Esc:cancel "
    } else {
        " j/k:navigate  e:edit  a:add  d:delete  Esc:back "
    };

    let block = Block::bordered()
        .title(Span::styled(
            " Notes ",
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

    if app.note_lines.is_empty() {
        f.render_widget(
            Paragraph::new("(no notes)")
                .style(Style::default().fg(FG_OVERLAY))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    if app.note_editing {
        // Notes above, input below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(4)])
            .split(inner);

        let mut lines: Vec<Line> = Vec::new();
        for (i, line) in app.note_lines.iter().enumerate() {
            let style = if i == app.note_selected {
                Style::default()
                    .fg(FG_TEXT)
                    .bg(ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_SUBTEXT)
            };
            lines.push(Line::from(Span::styled(format!("  {}", line), style)));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), chunks[0]);

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(ACCENT_YELLOW))
            .border_type(BorderType::Rounded);
        let input_lines = vec![
            Line::from(Span::styled(
                "  Editing note. Alt+Enter for newline",
                Style::default().fg(FG_OVERLAY),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("\u{276F} {}\u{2588}", app.edit_field_input),
                Style::default().fg(FG_TEXT),
            )),
        ];
        f.render_widget(Paragraph::new(input_lines).block(input_block), chunks[1]);
    } else {
        // List of note lines with selection highlight
        let mut lines: Vec<Line> = Vec::new();
        for (i, line) in app.note_lines.iter().enumerate() {
            let style = if i == app.note_selected {
                Style::default()
                    .fg(FG_TEXT)
                    .bg(Color::Rgb(65, 75, 120))
            } else {
                Style::default().fg(FG_SUBTEXT)
            };
            lines.push(Line::from(Span::styled(format!("  {}", line), style)));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
            " e.g.: fix login +backend @laptop ~2h ^fri !!! ",
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
