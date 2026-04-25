use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use ratatui::widgets::ListState;
use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc;

use dodo::cli::SortBy;
use dodo::config::{PreferencesConfig, WeekStart};
use dodo::db::Database;
use dodo::notation::{parse_date, parse_duration, parse_filter_days, prepare_task};
use dodo::task::{Area, Task, TaskStatus};

use super::constants::*;
use super::format::*;

#[derive(Clone)]
pub(super) enum SyncStatus {
    Disabled,                   // sync not configured
    Idle,                       // sync configured but no sync attempted yet
    Syncing,                    // sync in progress
    Synced(std::time::Instant), // last successful sync timestamp
    Error(String),              // last sync failed
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum TasksView {
    Panes,
    Daily,
    Weekly,
    Calendar,
}

impl TasksView {
    pub(super) fn next(self) -> Self {
        match self {
            TasksView::Panes => TasksView::Daily,
            TasksView::Daily => TasksView::Weekly,
            TasksView::Weekly => TasksView::Calendar,
            TasksView::Calendar => TasksView::Panes,
        }
    }

    pub(super) fn prev(self) -> Self {
        match self {
            TasksView::Panes => TasksView::Calendar,
            TasksView::Daily => TasksView::Panes,
            TasksView::Weekly => TasksView::Daily,
            TasksView::Calendar => TasksView::Weekly,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            TasksView::Panes => "Panes",
            TasksView::Daily => "Daily",
            TasksView::Weekly => "Weekly",
            TasksView::Calendar => "Calendar",
        }
    }
}

pub(super) enum DailyEntry {
    Header {
        date: NaiveDate,
        task_count: usize,
        is_today: bool,
    },
    Task(Task),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum CalendarFocus {
    Grid,
    TaskList,
}

pub(super) struct RunningTaskInfo {
    pub(super) title: String,
    pub(super) elapsed_seconds: i64,
    pub(super) estimate_minutes: Option<i64>,
}

pub(super) struct PaneState {
    pub(super) tasks: Vec<Task>,
    pub(super) list_state: ListState,
    pub(super) sort_index: usize,
    pub(super) sort_ascending: bool,
}

impl PaneState {
    pub(super) fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            tasks: Vec::new(),
            list_state,
            sort_index: 0,
            sort_ascending: true,
        }
    }

    pub(super) fn jump(&mut self, n: usize) {
        if self.tasks.is_empty() {
            return;
        }
        let len = self.tasks.len();
        let i = match self.list_state.selected() {
            Some(i) => (i + n).min(len - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub(super) fn jump_back(&mut self, n: usize) {
        if self.tasks.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(n),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub(super) fn jump_to_first(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub(super) fn jump_to_last(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(self.tasks.len() - 1));
        }
    }

    pub(super) fn selected_task(&self) -> Option<&Task> {
        self.list_state.selected().and_then(|i| self.tasks.get(i))
    }

    pub(super) fn stats(&self) -> (i64, i64, usize, usize, usize, usize) {
        let mut elapsed = 0i64;
        let mut estimate = 0i64;
        let mut done = 0usize;
        let mut on_time = 0usize;
        let mut overdue = 0usize;
        for task in &self.tasks {
            elapsed += task.elapsed_seconds.unwrap_or(0);
            estimate += task.estimate_minutes.unwrap_or(0) * 60;
            if task.status == TaskStatus::Done {
                done += 1;
                // on_time = elapsed within estimate, overdue = elapsed exceeded estimate
                let task_elapsed = task.elapsed_seconds.unwrap_or(0);
                let task_estimate = task.estimate_minutes.unwrap_or(0) * 60;
                if task_estimate > 0 {
                    if task_elapsed <= task_estimate {
                        on_time += 1;
                    } else {
                        overdue += 1;
                    }
                } else {
                    on_time += 1;
                }
            }
        }
        (elapsed, estimate, done, self.tasks.len(), on_time, overdue)
    }
}

#[derive(PartialEq)]
pub(super) enum AppMode {
    Normal,
    AddTask,
    MoveTask,
    ConfirmDelete,
    EditTask,
    EditTaskField,
    EditElapsed,
    NoteView,
    Search,
    RecAddTemplate,
    RecConfirmDelete,
    EditConfig,
    EditConfigField,
    Help,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum TuiTab {
    Tasks,
    Recurring,
    Report,
    Settings,
}

// ReportRange imported from dodo::cli, re-exported for draw.rs
pub(super) use dodo::cli::ReportRange;

pub(super) struct ReportData {
    pub(super) tasks_done: i64,
    pub(super) total_seconds: i64,
    pub(super) active_days: i64,
    pub(super) by_hour: Vec<(i64, i64)>,
    pub(super) by_weekday: Vec<(i64, i64)>,
    pub(super) by_project: Vec<(String, i64)>,
    pub(super) done_tasks: Vec<(String, i64)>,
    pub(super) streak: i64,
    pub(super) total_tasks: i64,
}

pub(super) struct App<'a> {
    pub(super) panes: [PaneState; 4],
    pub(super) active_pane: usize,
    pub(super) running_task: Option<RunningTaskInfo>,
    pub(super) db: &'a Database,
    pub(super) mode: AppMode,
    pub(super) last_ding_tick: u64,
    // Tabs & report
    pub(super) tab: TuiTab,
    pub(super) report_range: ReportRange,
    pub(super) report_offset: i64, // 0 = current period, 1 = one period ago, etc.
    pub(super) report: Option<ReportData>,
    pub(super) tick_count: u64,
    pub(super) frame_count: u64, // still used by event.rs tick logic
    pub(super) start_time: std::time::Instant, // wall-clock anchor for animations
    // Add task
    pub(super) add_input: String,
    // Move task
    pub(super) move_task_id: Option<String>,
    pub(super) move_source: usize,
    pub(super) move_target: usize,
    // Delete task
    pub(super) delete_task_id: Option<String>,
    pub(super) delete_task_title: String,
    pub(super) delete_task_is_recurring_instance: bool,
    // Edit task
    pub(super) edit_task_id: Option<String>,
    pub(super) edit_field_index: usize,
    pub(super) edit_field_values: [String; 9],
    pub(super) edit_field_input: String,
    pub(super) edit_is_template: bool, // true when editing a recurring template
    // Elapsed editing modal
    pub(super) elapsed_edit_input: String,
    pub(super) elapsed_return_to_edit: bool, // true when opened from EditTask dialog
    // Done visibility toggle (Daily/Weekly/Calendar only; Panes always shows Done pane)
    pub(super) show_done: bool,
    // Note view
    pub(super) note_lines: Vec<String>,
    pub(super) note_selected: usize,
    pub(super) note_editing: bool,
    // Search
    pub(super) search_input: String,
    // Vim count prefix & g key
    pub(super) count_prefix: Option<usize>,
    pub(super) pending_g: bool,
    // Recurring tab
    pub(super) templates: Vec<Task>,
    pub(super) template_selected: usize,
    pub(super) rec_add_input: String,
    // Backup tab
    pub(super) backup_entries: Vec<dodo::backup::BackupEntry>,
    pub(super) backup_selected: usize,
    pub(super) backup_config: dodo::config::BackupConfig,
    pub(super) sync_config: dodo::config::SyncConfig,
    pub(super) sync_status: SyncStatus,
    pub(super) sync_receiver: Option<mpsc::Receiver<Result<()>>>,
    pub(super) last_sync_tick: u64,
    pub(super) last_backup_check_tick: Option<u64>,
    pub(super) backup_status_msg: Option<String>,
    pub(super) backup_status_msg_at: Option<std::time::Instant>,
    pub(super) config_test_result: Option<String>,
    // Config editor
    pub(super) config_field_index: usize,
    pub(super) config_field_values: [String; CONFIG_FIELD_COUNT],
    pub(super) config_field_input: String,
    pub(super) config_scroll: usize,
    // Help modal
    pub(super) help_scroll: usize,
    // Tasks view mode
    pub(super) tasks_view: TasksView,
    // Daily view
    pub(super) daily_entries: Vec<DailyEntry>,
    pub(super) daily_cursor: usize,
    pub(super) daily_scroll: usize, // Scroll offset for natural navigation
    // Weekly view
    pub(super) weekly_panes: [PaneState; 8],
    pub(super) weekly_active: usize,
    pub(super) week_start_date: NaiveDate,
    // Calendar view
    pub(super) calendar_year: i32,
    pub(super) calendar_month: u32,
    pub(super) calendar_selected: NaiveDate,
    pub(super) calendar_focus: CalendarFocus,
    pub(super) calendar_tasks: Vec<Task>,
    pub(super) calendar_task_selected: usize,
    pub(super) calendar_task_counts: HashMap<NaiveDate, usize>,
    pub(super) calendar_tasks_by_date: HashMap<NaiveDate, Vec<Task>>,
    // Preferences
    pub(super) preferences: PreferencesConfig,
    // Email config
    pub(super) email_config: dodo::config::EmailConfig,
}

/// Split note text into entries grouped by timestamp.
/// Each entry starts with a strict `[YYYY-MM-DD HH:MM]` timestamp prefix.
/// Uses strict matching to avoid misidentifying pasted text (e.g. `[2025-01-15 note]`).
/// Continuation lines (without a timestamp prefix) are grouped with the previous entry.
pub(super) fn split_note_entries(text: &str) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for line in text.lines() {
        // Strict check: must match [YYYY-MM-DD HH:MM] exactly (19+ chars)
        let is_timestamp = line.len() >= 19
            && line.starts_with('[')
            && line.get(1..5).is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))  // YYYY
            && line.get(5..6) == Some("-")
            && line.get(6..8).is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))  // MM
            && line.get(8..9) == Some("-")
            && line.get(9..11).is_some_and(|s| s.chars().all(|c| c.is_ascii_digit())) // DD
            && line.get(11..12) == Some(" ")
            && line.get(12..14).is_some_and(|s| s.chars().all(|c| c.is_ascii_digit())) // HH
            && line.get(14..15) == Some(":")
            && line.get(15..17).is_some_and(|s| s.chars().all(|c| c.is_ascii_digit())) // MM
            && line.get(17..18) == Some("]");
        if is_timestamp {
            entries.push(line.to_string());
        } else if let Some(last) = entries.last_mut() {
            last.push('\n');
            last.push_str(line);
        } else {
            entries.push(line.to_string());
        }
    }
    entries
}

impl<'a> App<'a> {
    pub(super) fn new(db: &'a Database) -> Self {
        let mut panes = [
            PaneState::new(),
            PaneState::new(),
            PaneState::new(),
            PaneState::new(),
        ];
        panes[3].sort_index = 1; // DONE pane defaults to modified
        panes[3].sort_ascending = false; // descending (newest done first)
        let config = dodo::config::Config::load().unwrap_or_default();
        let today = dodo::today();
        let initial_view = match config.preferences.last_view.as_str() {
            "daily" => TasksView::Daily,
            "weekly" => TasksView::Weekly,
            "calendar" => TasksView::Calendar,
            _ => TasksView::Panes,
        };
        Self {
            panes,
            active_pane: 2,
            running_task: None,
            db,
            mode: AppMode::Normal,
            last_ding_tick: 0,
            tab: TuiTab::Tasks,
            report_range: ReportRange::Month,
            report_offset: 0,
            report: None,
            tick_count: 0,
            frame_count: 0,
            start_time: std::time::Instant::now(),
            add_input: String::new(),
            move_task_id: None,
            move_source: 0,
            move_target: 0,
            delete_task_id: None,
            delete_task_title: String::new(),
            delete_task_is_recurring_instance: false,
            edit_task_id: None,
            edit_field_index: 0,
            edit_field_values: Default::default(),
            edit_field_input: String::new(),
            edit_is_template: false,
            elapsed_edit_input: String::new(),
            elapsed_return_to_edit: false,
            show_done: true,
            note_lines: Vec::new(),
            note_selected: 0,
            note_editing: false,
            search_input: String::new(),
            count_prefix: None,
            pending_g: false,
            templates: Vec::new(),
            template_selected: 0,
            rec_add_input: String::new(),
            backup_entries: Vec::new(),
            backup_selected: 0,
            backup_config: config.backup,
            sync_config: config.sync.clone(),
            sync_status: if config.sync.is_ready() {
                SyncStatus::Idle
            } else {
                SyncStatus::Disabled
            },
            sync_receiver: None,
            last_sync_tick: 0,
            last_backup_check_tick: None,
            backup_status_msg: None,
            backup_status_msg_at: None,
            config_test_result: None,
            config_field_index: 0,
            config_field_values: Default::default(),
            config_field_input: String::new(),
            config_scroll: 0,
            help_scroll: 0,
            tasks_view: initial_view,
            daily_entries: Vec::new(),
            daily_cursor: 0,
            daily_scroll: 0,
            weekly_panes: [
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
                PaneState::new(),
            ],
            weekly_active: 0,
            week_start_date: today,
            calendar_year: today.year(),
            calendar_month: today.month(),
            calendar_selected: today,
            calendar_focus: CalendarFocus::Grid,
            calendar_tasks: Vec::new(),
            calendar_task_selected: 0,
            calendar_task_counts: HashMap::new(),
            calendar_tasks_by_date: HashMap::new(),
            preferences: config.preferences,
            email_config: config.email,
        }
    }

    /// Virtual frame count at 60fps based on wall-clock time.
    /// Using real elapsed time instead of render frame_count means the animation
    /// runs at a consistent visual speed regardless of the actual poll/render rate.
    pub(super) fn anim_frame(&self) -> u64 {
        (self.start_time.elapsed().as_millis() / 16) as u64
    }

    pub(super) fn adjust_selected_date(&mut self, days: i64) {
        if let Some(task) = self.current_selected_task() {
            let today = dodo::today();
            let current = task.scheduled.unwrap_or(today);
            let new_date = current + chrono::Duration::days(days);
            let task_id = task.id.clone();
            let _ = self.db.update_task_scheduled(&task_id, new_date);
            let _ = self.refresh_current_view();
            // Follow cursor to task's new position in all views
            self.follow_task_cursor(&task_id);
        }
    }

    pub(super) fn sync_enabled(&self) -> bool {
        !matches!(self.sync_status, SyncStatus::Disabled)
    }

    pub(super) fn set_backup_status(&mut self, msg: String) {
        self.backup_status_msg = Some(msg);
        self.backup_status_msg_at = Some(std::time::Instant::now());
    }

    pub(super) fn trigger_sync(&mut self) {
        if !self.sync_enabled() {
            return;
        }
        // Skip if already syncing
        if matches!(self.sync_status, SyncStatus::Syncing) {
            return;
        }
        self.sync_status = SyncStatus::Syncing;
        // Safety: sync_enabled() implies sync_config.is_ready() which guarantees these are Some
        let url = self.sync_config.turso_url.clone().unwrap_or_default();
        let token = self.sync_config.turso_token.clone().unwrap_or_default();
        self.sync_receiver = Some(Database::sync_with_remote(url, token));
    }

    pub(super) fn check_sync_result(&mut self) {
        if let Some(ref rx) = self.sync_receiver {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.sync_status = SyncStatus::Synced(std::time::Instant::now());
                    self.sync_receiver = None;
                }
                Ok(Err(e)) => {
                    self.sync_status = SyncStatus::Error(format!("{}", e));
                    self.sync_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still syncing, do nothing
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.sync_status = SyncStatus::Error("Sync thread disconnected".to_string());
                    self.sync_receiver = None;
                }
            }
        }
    }

    pub(super) fn cycle_sort(&mut self) {
        let pane = &mut self.panes[self.active_pane];
        if pane.sort_ascending {
            pane.sort_ascending = false;
        } else {
            pane.sort_index = (pane.sort_index + 1) % SORT_MODES.len();
            pane.sort_ascending = true;
        }
        let sort = SORT_MODES[pane.sort_index];
        let ascending = pane.sort_ascending;
        pane.tasks.sort_by(|a, b| sort_tasks(a, b, sort, ascending));
    }

    /// Follow the cursor to a specific task in the current view after a status/schedule change.
    /// This single method replaces the ad-hoc cursor-follow blocks that used to be duplicated
    /// in done(), toggle_selected(), and adjust_selected_date().
    pub(super) fn follow_task_cursor(&mut self, task_id: &str) {
        match self.tasks_view {
            TasksView::Panes => {
                for pane_idx in 0..4 {
                    if let Some(pos) = self.panes[pane_idx]
                        .tasks
                        .iter()
                        .position(|t| t.id == task_id)
                    {
                        self.active_pane = pane_idx;
                        self.panes[pane_idx].list_state.select(Some(pos));
                        break;
                    }
                }
            }
            TasksView::Daily => {
                for (i, entry) in self.daily_entries.iter().enumerate() {
                    if let DailyEntry::Task(ref t) = entry {
                        if t.id == task_id {
                            self.daily_cursor = i;
                            break;
                        }
                    }
                }
            }
            TasksView::Weekly => {
                for (pane_idx, pane) in self.weekly_panes.iter().enumerate() {
                    if pane.tasks.iter().any(|t| t.id == task_id) {
                        self.weekly_active = pane_idx;
                        break;
                    }
                }
            }
            TasksView::Calendar => {
                let found = self
                    .calendar_tasks_by_date
                    .iter()
                    .find_map(|(date, tasks)| {
                        tasks
                            .iter()
                            .position(|t| t.id == task_id)
                            .map(|pos| (*date, pos))
                    });
                if let Some((date, pos)) = found {
                    self.calendar_selected = date;
                    self.calendar_task_selected = pos;
                    self.calendar_focus = CalendarFocus::TaskList;
                }
            }
        }
    }

    pub(super) fn current_selected_task(&self) -> Option<&Task> {
        match self.tasks_view {
            TasksView::Panes => self.panes[self.active_pane].selected_task(),
            TasksView::Daily => self.daily_entries.get(self.daily_cursor).and_then(|e| {
                if let DailyEntry::Task(ref t) = e {
                    Some(t)
                } else {
                    None
                }
            }),
            TasksView::Weekly => self.weekly_panes[self.weekly_active].selected_task(),
            TasksView::Calendar => {
                if self.calendar_focus == CalendarFocus::TaskList {
                    self.calendar_tasks.get(self.calendar_task_selected)
                } else {
                    None
                }
            }
        }
    }

    pub(super) fn matches_search(&self, task: &Task) -> bool {
        if self.search_input.is_empty() {
            return true;
        }
        let query = self.search_input.to_lowercase();
        let today = dodo::today();
        for token in query.split_whitespace() {
            if let Some(proj) = token.strip_prefix('+') {
                let task_proj = task.project.as_deref().unwrap_or("").to_lowercase();
                if !task_proj.contains(proj) {
                    return false;
                }
            } else if let Some(ctx) = token.strip_prefix('@') {
                let task_ctx = task.context.as_deref().unwrap_or("").to_lowercase();
                if !task_ctx.contains(ctx) {
                    return false;
                }
            } else if token.chars().all(|c| c == '!') && !token.is_empty() {
                // Priority: !! means priority >= 2
                let min_pri = token.len() as i64;
                if task.priority.unwrap_or(0) < min_pri {
                    return false;
                }
            } else if let Some(rest) = token.strip_prefix("=<") {
                // Scheduled within N days: =<10d
                if let Some(days) = parse_filter_days(rest) {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.scheduled {
                        Some(sc) if sc <= cutoff => {}
                        _ => return false,
                    }
                }
            } else if let Some(rest) = token.strip_prefix("=>") {
                // Scheduled beyond N days: =>3d
                if let Some(days) = parse_filter_days(rest) {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.scheduled {
                        Some(sc) if sc >= cutoff => {}
                        _ => return false,
                    }
                }
            } else if let Some(rest) = token.strip_prefix("^<") {
                // Deadline within N days: ^<3d
                if let Some(days) = parse_filter_days(rest) {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.deadline {
                        Some(dl) if dl <= cutoff => {}
                        _ => return false,
                    }
                }
            } else if let Some(rest) = token.strip_prefix("^>") {
                // Deadline beyond N days: ^>5d
                if let Some(days) = parse_filter_days(rest) {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.deadline {
                        Some(dl) if dl >= cutoff => {}
                        _ => return false,
                    }
                }
            } else if let Some(rest) = token.strip_prefix('^') {
                // ^3d is shorthand for ^<3d
                if let Some(days) = parse_filter_days(rest) {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.deadline {
                        Some(dl) if dl <= cutoff => {}
                        _ => return false,
                    }
                }
            } else if let Some(rest) = token.strip_prefix('=') {
                // =1w is shorthand for =<1w
                if let Some(days) = parse_filter_days(rest) {
                    let cutoff = today + chrono::Duration::days(days);
                    match task.scheduled {
                        Some(sc) if sc <= cutoff => {}
                        _ => return false,
                    }
                }
            } else {
                let title = task.title.to_lowercase();
                if !title.contains(token) {
                    return false;
                }
            }
        }
        true
    }

    pub(super) fn load_running_task(&mut self) {
        self.running_task = if let Ok(Some((title, elapsed, estimate))) = self.db.get_running_task()
        {
            Some(RunningTaskInfo {
                title,
                elapsed_seconds: elapsed,
                estimate_minutes: estimate,
            })
        } else {
            None
        };
    }

    pub(super) fn refresh_all(&mut self) -> Result<()> {
        let all_tasks = self.db.list_all_tasks(SortBy::Created)?;

        let mut groups: [Vec<Task>; 4] = [vec![], vec![], vec![], vec![]];
        for task in all_tasks {
            if !self.matches_search(&task) {
                continue;
            }
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
            let ascending = self.panes[i].sort_ascending;
            self.panes[i]
                .tasks
                .sort_by(|a, b| sort_tasks(a, b, sort, ascending));
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

        self.load_running_task();

        Ok(())
    }

    /// Compute (from_rfc3339, to_rfc3339) adjusted by report_offset periods into the past.
    /// For ReportRange::All, offset is ignored.
    pub(super) fn report_date_range(&self) -> (String, String) {
        use chrono::Utc;
        let today = dodo::today();
        let period_days: i64 = match self.report_range {
            ReportRange::Day => 1,
            ReportRange::Week => 7,
            ReportRange::Month => 30,
            ReportRange::Year => 365,
            ReportRange::All => {
                // No offset for All — return the same as the plain date_range
                return self.report_range.date_range();
            }
        };
        // Shift the window into the past by offset * period_days.
        let to_date = today - chrono::Duration::days(self.report_offset * period_days);
        let from_date = to_date - chrono::Duration::days(period_days - 1);
        let from = from_date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap()
            .to_rfc3339();
        let to = (to_date + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Utc)
            .unwrap()
            .to_rfc3339();
        (from, to)
    }

    /// Human-readable label for the current report period (used in draw_report_tab).
    pub(super) fn report_period_label(&self) -> String {
        if let ReportRange::All = self.report_range {
            return "All time".to_string();
        }
        let today = dodo::today();
        let period_days: i64 = match self.report_range {
            ReportRange::Day => 1,
            ReportRange::Week => 7,
            ReportRange::Month => 30,
            ReportRange::Year => 365,
            ReportRange::All => unreachable!(),
        };
        let to_date = today - chrono::Duration::days(self.report_offset * period_days);
        let from_date = to_date - chrono::Duration::days(period_days - 1);
        if self.report_offset == 0 {
            match self.report_range {
                ReportRange::Day => format!("Today ({})", today.format("%b %d")),
                _ => format!(
                    "{} \u{2013} {}",
                    from_date.format("%b %d"),
                    to_date.format("%b %d, %Y")
                ),
            }
        } else {
            format!(
                "{} \u{2013} {}",
                from_date.format("%b %d"),
                to_date.format("%b %d, %Y")
            )
        }
    }

    pub(super) fn refresh_report(&mut self) -> Result<()> {
        let (from, to) = self.report_date_range();
        self.report = Some(ReportData {
            tasks_done: self.db.report_tasks_done(&from, &to)?,
            total_seconds: self.db.report_total_seconds(&from, &to)?,
            active_days: self.db.report_active_days(&from, &to)?,
            by_hour: self.db.report_by_hour(&from, &to)?,
            by_weekday: self.db.report_by_weekday(&from, &to)?,
            by_project: self.db.report_by_project(&from, &to)?,
            done_tasks: self.db.report_done_tasks(&from, &to, 20)?,
            streak: self.db.report_streak()?,
            total_tasks: self.db.report_total_tasks(&from, &to)?,
        });
        Ok(())
    }

    pub(super) fn refresh_templates(&mut self) -> Result<()> {
        self.templates = self.db.list_templates()?;
        if self.templates.is_empty() {
            self.template_selected = 0;
        } else if self.template_selected >= self.templates.len() {
            self.template_selected = self.templates.len() - 1;
        }
        Ok(())
    }

    pub(super) fn refresh_backups(&mut self) {
        if self.backup_config.is_ready() {
            match dodo::backup::list_backups(&self.backup_config) {
                Ok(entries) => {
                    self.backup_entries = entries;
                    if self.backup_entries.is_empty() {
                        self.backup_selected = 0;
                    } else if self.backup_selected >= self.backup_entries.len() {
                        self.backup_selected = self.backup_entries.len() - 1;
                    }
                }
                Err(e) => {
                    self.set_backup_status(format!("Error: {}", e));
                }
            }
        }
    }

    pub(super) fn refresh_daily(&mut self) -> Result<()> {
        let all_tasks = self.db.list_all_tasks(SortBy::Created)?;
        let today = dodo::today();
        self.load_running_task();

        // Group tasks by scheduled date (None → today)
        let mut by_date: BTreeMap<NaiveDate, Vec<Task>> = BTreeMap::new();
        for task in all_tasks {
            if !self.matches_search(&task) {
                continue;
            }
            if !self.show_done && task.status == TaskStatus::Done {
                continue;
            }
            let date = task.scheduled.unwrap_or(today);
            by_date.entry(date).or_default().push(task);
        }

        // Ensure ±7 days around today have entries even if empty
        for offset in -7..=7 {
            let d = today + chrono::Duration::days(offset);
            by_date.entry(d).or_default();
        }

        // Sort tasks within each date by status priority then created
        for tasks in by_date.values_mut() {
            tasks.sort_by(|a, b| {
                let status_order = |s: &TaskStatus| match s {
                    TaskStatus::Running => 0,
                    TaskStatus::Paused => 1,
                    TaskStatus::Pending => 2,
                    TaskStatus::Done => 3,
                };
                status_order(&a.status)
                    .cmp(&status_order(&b.status))
                    .then(a.created.cmp(&b.created))
            });
        }

        // Build flat Vec<DailyEntry>
        let mut entries = Vec::new();
        for (date, tasks) in &by_date {
            entries.push(DailyEntry::Header {
                date: *date,
                task_count: tasks.len(),
                is_today: *date == today,
            });
            for task in tasks {
                entries.push(DailyEntry::Task(task.clone()));
            }
        }

        self.daily_entries = entries;
        // Preserve daily_scroll — draw.rs adjusts it every frame to keep the cursor visible.
        // Clamp cursor to valid range
        if self.daily_entries.is_empty() {
            self.daily_cursor = 0;
        } else if self.daily_cursor >= self.daily_entries.len() {
            self.daily_cursor = self.daily_entries.len() - 1;
        }
        // Ensure cursor lands on a Task entry, not a Header (can shift after task removal).
        while self.daily_cursor < self.daily_entries.len()
            && !matches!(self.daily_entries[self.daily_cursor], DailyEntry::Task(_))
        {
            self.daily_cursor += 1;
        }
        if self.daily_cursor >= self.daily_entries.len() {
            // Scanned past end — find the last Task entry
            self.daily_cursor = self
                .daily_entries
                .iter()
                .enumerate()
                .rev()
                .find(|(_, e)| matches!(e, DailyEntry::Task(_)))
                .map(|(i, _)| i)
                .unwrap_or(0);
        }

        Ok(())
    }

    pub(super) fn refresh_weekly(&mut self) -> Result<()> {
        let all_tasks = self.db.list_all_tasks(SortBy::Created)?;
        let today = dodo::today();
        self.load_running_task();

        // Distribute tasks into 8 panes by date
        let dates: Vec<NaiveDate> = (0..8)
            .map(|i| self.week_start_date + chrono::Duration::days(i))
            .collect();

        for pane in &mut self.weekly_panes {
            pane.tasks.clear();
        }

        for task in all_tasks {
            if !self.matches_search(&task) {
                continue;
            }
            if !self.show_done && task.status == TaskStatus::Done {
                continue;
            }
            let task_date = task.scheduled.unwrap_or(today);
            for (i, date) in dates.iter().enumerate() {
                if task_date == *date {
                    self.weekly_panes[i].tasks.push(task.clone());
                    break;
                }
            }
        }

        // Sort each pane
        for pane in &mut self.weekly_panes {
            let sort = SORT_MODES[pane.sort_index];
            let ascending = pane.sort_ascending;
            pane.tasks.sort_by(|a, b| sort_tasks(a, b, sort, ascending));
            let len = pane.tasks.len();
            if len == 0 {
                pane.list_state.select(None);
            } else if let Some(sel) = pane.list_state.selected() {
                if sel >= len {
                    pane.list_state.select(Some(len - 1));
                }
            } else {
                pane.list_state.select(Some(0));
            }
        }

        Ok(())
    }

    pub(super) fn refresh_calendar(&mut self) -> Result<()> {
        let all_tasks = self.db.list_all_tasks(SortBy::Created)?;
        let today = dodo::today();
        self.load_running_task();

        // Compute task counts per date and build tasks-by-date map
        self.calendar_task_counts.clear();
        self.calendar_tasks_by_date.clear();
        let mut selected_tasks = Vec::new();

        let status_order = |s: &TaskStatus| match s {
            TaskStatus::Running => 0,
            TaskStatus::Paused => 1,
            TaskStatus::Pending => 2,
            TaskStatus::Done => 3,
        };

        for task in all_tasks {
            if !self.matches_search(&task) {
                continue;
            }
            if !self.show_done && task.status == TaskStatus::Done {
                continue;
            }
            let task_date = task.scheduled.unwrap_or(today);
            *self.calendar_task_counts.entry(task_date).or_insert(0) += 1;
            self.calendar_tasks_by_date
                .entry(task_date)
                .or_default()
                .push(task.clone());
            if task_date == self.calendar_selected {
                selected_tasks.push(task);
            }
        }

        // Sort tasks by status priority within each date
        for tasks in self.calendar_tasks_by_date.values_mut() {
            tasks.sort_by(|a, b| {
                status_order(&a.status)
                    .cmp(&status_order(&b.status))
                    .then(a.created.cmp(&b.created))
            });
        }

        // Sort selected tasks by status then created
        selected_tasks.sort_by(|a, b| {
            status_order(&a.status)
                .cmp(&status_order(&b.status))
                .then(a.created.cmp(&b.created))
        });

        self.calendar_tasks = selected_tasks;
        if self.calendar_tasks.is_empty() {
            self.calendar_task_selected = 0;
        } else if self.calendar_task_selected >= self.calendar_tasks.len() {
            self.calendar_task_selected = self.calendar_tasks.len() - 1;
        }

        Ok(())
    }

    pub(super) fn refresh_current_view(&mut self) -> Result<()> {
        match self.tasks_view {
            TasksView::Panes => self.refresh_all(),
            TasksView::Daily => self.refresh_daily(),
            TasksView::Weekly => self.refresh_weekly(),
            TasksView::Calendar => self.refresh_calendar(),
        }
    }

    pub(super) fn daily_jump_to_today(&mut self) {
        let today = dodo::today();

        // Find today's header index so we can set scroll to show it at the top
        let today_header_idx = self
            .daily_entries
            .iter()
            .position(|e| matches!(e, DailyEntry::Header { is_today: true, .. }));

        // Find the first Task entry on today's date (skip the header)
        for (i, entry) in self.daily_entries.iter().enumerate() {
            if let DailyEntry::Task(ref t) = entry {
                if t.scheduled.unwrap_or(today) == today {
                    self.daily_cursor = i;
                    // Scroll to show today's header at the top of the viewport
                    self.daily_scroll = today_header_idx.unwrap_or(i.saturating_sub(1));
                    return;
                }
            }
        }
        // Fallback: find the today header
        for (i, entry) in self.daily_entries.iter().enumerate() {
            if let DailyEntry::Header { is_today: true, .. } = entry {
                // Try to select the next task entry after the header
                if i + 1 < self.daily_entries.len() {
                    self.daily_cursor = i + 1;
                } else {
                    self.daily_cursor = i;
                }
                self.daily_scroll = i; // show header at top
                return;
            }
        }
    }

    pub(super) fn move_pane_left(&mut self) {
        if self.active_pane > 0 {
            self.active_pane -= 1;
        }
    }

    pub(super) fn move_pane_right(&mut self) {
        if self.active_pane < 3 {
            self.active_pane += 1;
        }
    }

    pub(super) fn toggle_selected(&mut self) -> Result<()> {
        let task_info = self
            .current_selected_task()
            .map(|t| (t.id.clone(), t.status, t.num_id, t.scheduled));
        if let Some((id, status, num_id, _scheduled)) = task_info {
            let task_id = id.clone();
            if status == TaskStatus::Running {
                self.db.pause_timer()?;
            } else {
                let num_str = num_id.map(|n| n.to_string()).unwrap_or_default();
                if !num_str.is_empty() {
                    let today = dodo::today();
                    self.db.update_task_scheduled(&id, today)?;
                    let _ = self.db.start_timer(&num_str);
                }
            }
            self.refresh_current_view()?;
            // Follow cursor to task's new position in all views
            self.follow_task_cursor(&task_id);
        }
        Ok(())
    }

    pub(super) fn save_last_view(&mut self) {
        self.preferences.last_view = match self.tasks_view {
            TasksView::Panes => "panes".to_string(),
            TasksView::Daily => "daily".to_string(),
            TasksView::Weekly => "weekly".to_string(),
            TasksView::Calendar => "calendar".to_string(),
        };
    }

    pub(super) fn done(&mut self) -> Result<()> {
        let task_info = self
            .current_selected_task()
            .map(|t| (t.id.clone(), t.status));
        if let Some((ref id, ref status)) = task_info {
            if *status == TaskStatus::Done {
                self.db.uncomplete_task_by_id(id)?;
            } else {
                // If running, stop timer display immediately before DB call
                if *status == TaskStatus::Running {
                    self.running_task = None;
                }
                self.db.complete_task_by_id(id)?;
            }
        }
        self.refresh_current_view()?;
        // Follow task to its new position in all views via shared helper
        if let Some((ref id, _)) = task_info {
            self.follow_task_cursor(id);
        }
        Ok(())
    }

    pub(super) fn open_note_quick(&mut self) {
        self.start_edit_task(); // sets edit_is_template = false
        if self.mode == AppMode::EditTask {
            self.edit_field_index = 8;
            self.edit_field_input.clear();
            let notes = &self.edit_field_values[8];
            if notes.is_empty() {
                // No notes — go straight to append input
                self.mode = AppMode::EditTaskField;
            } else {
                // Has notes — enter NoteView for browsing/editing
                self.note_lines = split_note_entries(notes);
                self.note_selected = 0;
                self.note_editing = false;
                self.mode = AppMode::NoteView;
            }
        }
    }

    pub(super) fn start_add_task(&mut self) {
        self.add_input.clear();
        self.mode = AppMode::AddTask;
    }

    pub(super) fn confirm_add_task(&mut self) -> Result<()> {
        if !self.add_input.is_empty() {
            let mut prep = prepare_task(&self.add_input);
            // If estimate is the hardcoded default (60) and user has a different pref, override
            if prep.estimate_minutes == Some(60) && self.preferences.default_estimate != 60 {
                prep.estimate_minutes = Some(self.preferences.default_estimate as i64);
            }
            self.db.add_task(
                &prep.title,
                Area::Today,
                prep.project,
                prep.context,
                prep.estimate_minutes,
                prep.deadline,
                prep.scheduled,
                prep.tags,
                prep.priority,
            )?;
            self.refresh_current_view()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    pub(super) fn start_move_task(&mut self) {
        if let Some(task) = self.current_selected_task() {
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

    pub(super) fn next_move_target(&self, current: usize) -> usize {
        let mut t = (current + 1) % 3; // 0,1,2 only (skip DONE=3)
        if t == self.move_source {
            t = (t + 1) % 3;
        }
        t
    }

    pub(super) fn prev_move_target(&self, current: usize) -> usize {
        let mut t = if current == 0 { 2 } else { current - 1 };
        if t == self.move_source {
            t = if t == 0 { 2 } else { t - 1 };
        }
        t
    }

    pub(super) fn confirm_move_task(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.move_task_id {
            let area = match self.move_target {
                0 => Area::LongTerm,
                1 => Area::ThisWeek,
                _ => Area::Today,
            };
            self.db
                .update_task_scheduled(task_id, area.to_scheduled_date())?;
            self.refresh_current_view()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    pub(super) fn move_task_quick(&mut self, direction: i32) -> Result<()> {
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
            let area = match target {
                0 => Area::LongTerm,
                1 => Area::ThisWeek,
                _ => Area::Today,
            };
            self.db
                .update_task_scheduled(&task_id, area.to_scheduled_date())?;
            self.refresh_current_view()?;
            self.active_pane = target;
            if let Some(pos) = self.panes[target]
                .tasks
                .iter()
                .position(|t| t.id == task_id)
            {
                self.panes[target].list_state.select(Some(pos));
            }
        }
        Ok(())
    }

    /// Returns true if the task to be deleted is a recurring instance (has a template_id).
    /// Checks the stored delete_task_id against all pane/daily task lists.
    #[allow(dead_code)]
    pub(super) fn selected_is_recurring_instance(&self) -> bool {
        if let Some(ref task_id) = self.delete_task_id {
            // Search the active pane lists for the task
            for pane in &self.panes {
                if let Some(t) = pane.tasks.iter().find(|t| &t.id == task_id) {
                    return t.template_id.is_some();
                }
            }
            // Also check daily / weekly panes
            for entry in &self.daily_entries {
                if let DailyEntry::Task(t) = entry {
                    if &t.id == task_id {
                        return t.template_id.is_some();
                    }
                }
            }
        }
        false
    }

    pub(super) fn start_delete(&mut self) {
        let info = self
            .current_selected_task()
            .map(|t| (t.id.clone(), t.title.clone(), t.template_id.is_some()));
        if let Some((id, title, is_recurring)) = info {
            self.delete_task_id = Some(id);
            self.delete_task_title = title;
            self.delete_task_is_recurring_instance = is_recurring;
            self.mode = AppMode::ConfirmDelete;
        }
    }

    pub(super) fn confirm_delete(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.delete_task_id {
            // If this is a recurring instance, generate the next occurrence before deleting
            // so that deletion acts as "skip" rather than terminating the recurrence.
            let _ = self.db.complete_recurring_instance(task_id);
            self.db.delete_task_by_id(task_id)?;
            self.refresh_current_view()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    pub(super) fn start_edit_task(&mut self) {
        if let Some(task) = self.current_selected_task().cloned() {
            self.edit_task_id = Some(task.id.clone());
            self.edit_field_index = 0;
            self.edit_is_template = false;
            self.edit_field_values = [
                task.title.clone(),
                task.project.clone().unwrap_or_default(),
                task.context.clone().unwrap_or_default(),
                task.tags.clone().unwrap_or_default(),
                task.estimate_minutes.map(format_est).unwrap_or_default(),
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

    pub(super) fn enter_edit_field(&mut self) {
        // Field 8 for regular tasks = Notes (special NoteView)
        // Field 8 for recurring templates = Recurrence (plain text)
        if self.edit_field_index == 8 && !self.edit_is_template {
            let notes = &self.edit_field_values[8];
            if notes.is_empty() {
                // No notes — go straight to append input
                self.edit_field_input.clear();
                self.mode = AppMode::EditTaskField;
            } else {
                // Has notes — enter NoteView
                self.note_lines = split_note_entries(notes);
                self.note_selected = 0;
                self.note_editing = false;
                self.mode = AppMode::NoteView;
            }
        } else {
            self.edit_field_input = self.edit_field_values[self.edit_field_index].clone();
            self.mode = AppMode::EditTaskField;
        }
    }

    pub(super) fn save_notes(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.edit_task_id {
            let full = self.note_lines.join("\n");
            self.db.update_notes_by_id(task_id, &full)?;
            self.edit_field_values[8] = full;
            self.refresh_current_view()?;
        }
        Ok(())
    }

    pub(super) fn save_edit_field(&mut self) -> Result<()> {
        let idx = self.edit_field_index;
        self.edit_field_values[idx] = self.edit_field_input.clone();
        let saved_input = self.edit_field_input.clone();

        if let Some(ref task_id) = self.edit_task_id {
            let val = &self.edit_field_values[idx];
            match idx {
                0
                    // Title
                    if !val.is_empty() => {
                        self.db.update_task_title_by_id(task_id, val)?;
                    }
                1 => {
                    // Project
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if !val.is_empty() {
                        parsed.project = Some(val.clone());
                    } else {
                        parsed.project = Some(String::new());
                    }
                    self.db.update_task_fields_by_id(task_id, &parsed, None)?;
                }
                2 => {
                    // Context
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if !val.is_empty() {
                        parsed.contexts = val.split(',').map(|s| s.trim().to_string()).collect();
                    } else {
                        parsed.contexts = vec![String::new()];
                    }
                    self.db.update_task_fields_by_id(task_id, &parsed, None)?;
                }
                3 => {
                    // Tags
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if !val.is_empty() {
                        parsed.tags = val.split(',').map(|s| s.trim().to_string()).collect();
                    } else {
                        parsed.tags = vec![String::new()];
                    }
                    self.db.update_task_fields_by_id(task_id, &parsed, None)?;
                }
                4 => {
                    // Estimate
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if let Some(mins) = parse_duration(val) {
                        parsed.estimate_minutes = Some(mins);
                        self.db.update_task_fields_by_id(task_id, &parsed, None)?;
                    }
                }
                5 => {
                    // Deadline
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if let Some(date) = parse_date(val) {
                        parsed.deadline = Some(date);
                        self.db.update_task_fields_by_id(task_id, &parsed, None)?;
                        self.edit_field_values[5] = date.format("%Y-%m-%d").to_string();
                    }
                }
                6 => {
                    // Scheduled
                    let mut parsed = dodo::notation::ParsedInput::default();
                    if let Some(date) = parse_date(val) {
                        parsed.scheduled = Some(date);
                        self.db.update_task_fields_by_id(task_id, &parsed, None)?;
                        self.edit_field_values[6] = date.format("%Y-%m-%d").to_string();
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
                        self.db.update_task_fields_by_id(task_id, &parsed, None)?;
                    }
                }
                8 if self.edit_is_template
                    // Recurrence pattern for recurring templates
                    && !saved_input.is_empty() => {
                        self.db.update_recurrence_by_id(task_id, &saved_input)?;
                    }
                8
                    // Notes (append) for regular tasks
                    if !saved_input.is_empty() => {
                        self.db.append_note_by_id(task_id, &saved_input)?;
                        let notes = self.db.get_task_notes_by_id(task_id)?;
                        self.edit_field_values[8] = notes.unwrap_or_default();
                    }
                _ => {}
            }
            self.refresh_current_view()?;
        }
        // Always clear the input field when returning to EditTask to prevent stale-input bug:
        // without this, pressing Enter again would try to re-save the previous field's value.
        self.edit_field_input.clear();
        // After appending a note, return to NoteView so the user sees the updated list
        if idx == 8 && !self.edit_is_template && !self.edit_field_values[8].is_empty() {
            self.note_lines = split_note_entries(&self.edit_field_values[8]);
            self.note_selected = self.note_lines.len().saturating_sub(1);
            self.note_editing = false;
            self.mode = AppMode::NoteView;
        } else {
            self.mode = AppMode::EditTask;
        }
        Ok(())
    }

    pub(super) fn start_elapsed_edit(&mut self) {
        // Pre-fill with current elapsed formatted as duration
        if let Some(task) = self.current_selected_task() {
            let elapsed = task.elapsed_seconds.unwrap_or(0);
            let task_id = task.id.clone();
            let mins = elapsed / 60;
            self.elapsed_edit_input = if mins > 0 {
                let h = mins / 60;
                let m = mins % 60;
                if h > 0 && m > 0 {
                    format!("{}h{}m", h, m)
                } else if h > 0 {
                    format!("{}h", h)
                } else {
                    format!("{}m", m)
                }
            } else {
                String::new()
            };
            // Store the task id (may not be set if called from Normal mode, not EditTask dialog)
            self.edit_task_id = Some(task_id);
            // Remember whether to return to the edit dialog or back to Normal view
            self.elapsed_return_to_edit = self.mode == AppMode::EditTask;
            self.mode = AppMode::EditElapsed;
        }
    }

    pub(super) fn save_elapsed_edit(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.edit_task_id.clone() {
            if let Some(mins) = dodo::notation::parse_duration(&self.elapsed_edit_input) {
                self.db.set_elapsed_seconds_by_id(task_id, mins * 60)?;
                self.refresh_current_view()?;
            }
        }
        self.elapsed_edit_input.clear();
        // Return to the edit dialog if we came from it, otherwise back to Normal task view
        self.mode = if self.elapsed_return_to_edit {
            AppMode::EditTask
        } else {
            AppMode::Normal
        };
        Ok(())
    }

    pub(super) fn start_edit_config(&mut self) {
        self.config_field_index = 0;
        self.config_scroll = 0;
        self.config_field_values = [
            // Sync fields (0-3)
            if self.sync_config.enabled {
                "true".to_string()
            } else {
                "false".to_string()
            },
            self.sync_config.turso_url.clone().unwrap_or_default(),
            self.sync_config.turso_token.clone().unwrap_or_default(),
            self.sync_config.sync_interval.to_string(),
            // Backup fields (4-12)
            if self.backup_config.enabled {
                "true".to_string()
            } else {
                "false".to_string()
            },
            self.backup_config.endpoint.clone().unwrap_or_default(),
            self.backup_config.bucket.clone().unwrap_or_default(),
            self.backup_config.prefix.clone(),
            self.backup_config.access_key.clone().unwrap_or_default(),
            self.backup_config.secret_key.clone().unwrap_or_default(),
            self.backup_config.region.clone().unwrap_or_default(),
            self.backup_config.schedule_days.to_string(),
            self.backup_config.max_backups.to_string(),
            // Preferences fields (13-17)
            match self.preferences.week_start {
                WeekStart::Sunday => "sunday".to_string(),
                WeekStart::Monday => "monday".to_string(),
            },
            if self.preferences.sound_enabled {
                "true".to_string()
            } else {
                "false".to_string()
            },
            self.preferences.timer_sound_interval.to_string(),
            self.preferences.default_view.clone(),
            self.preferences.default_estimate.to_string(),
            self.preferences.timezone.clone().unwrap_or_default(),
            // Email fields (19-23)
            if self.email_config.enabled {
                "true".to_string()
            } else {
                "false".to_string()
            },
            self.email_config.api_key.clone().unwrap_or_default(),
            self.email_config.from.clone().unwrap_or_default(),
            self.email_config.to.clone().unwrap_or_default(),
            self.email_config.digest_time.clone(),
        ];
        self.mode = AppMode::EditConfig;
    }

    pub(super) fn enter_config_field(&mut self) {
        if CONFIG_FIELD_TYPES[self.config_field_index] == ConfigFieldType::Boolean {
            // Toggle boolean immediately
            let new_val = self.config_field_values[self.config_field_index] != "true";
            self.config_field_values[self.config_field_index] = if new_val {
                "true".to_string()
            } else {
                "false".to_string()
            };
            self.apply_config_field(self.config_field_index);
            let _ = self.save_config();
        } else {
            self.config_field_input = self.config_field_values[self.config_field_index].clone();
            self.mode = AppMode::EditConfigField;
        }
    }

    pub(super) fn save_config_field(&mut self) {
        let idx = self.config_field_index;
        self.config_field_values[idx] = self.config_field_input.clone();
        self.apply_config_field(idx);
        let _ = self.save_config();
        self.mode = AppMode::EditConfig;
    }

    pub(super) fn apply_config_field(&mut self, idx: usize) {
        let val = &self.config_field_values[idx];
        let opt = if val.is_empty() {
            None
        } else {
            Some(val.clone())
        };
        match idx {
            0 => self.sync_config.enabled = val == "true",
            1 => self.sync_config.turso_url = opt,
            2 => self.sync_config.turso_token = opt,
            3 => self.sync_config.sync_interval = val.parse().unwrap_or(10),
            4 => self.backup_config.enabled = val == "true",
            5 => self.backup_config.endpoint = opt,
            6 => self.backup_config.bucket = opt,
            7 => {
                self.backup_config.prefix = if val.is_empty() {
                    "dodo/".to_string()
                } else {
                    val.clone()
                }
            }
            8 => self.backup_config.access_key = opt,
            9 => self.backup_config.secret_key = opt,
            10 => self.backup_config.region = opt,
            11 => self.backup_config.schedule_days = val.parse().unwrap_or(7),
            12 => self.backup_config.max_backups = val.parse().unwrap_or(10),
            13 => {
                self.preferences.week_start = if val.to_lowercase() == "monday" {
                    WeekStart::Monday
                } else {
                    WeekStart::Sunday
                };
            }
            14 => self.preferences.sound_enabled = val == "true",
            15 => self.preferences.timer_sound_interval = val.parse().unwrap_or(10),
            16 => self.preferences.default_view = val.clone(),
            17 => self.preferences.default_estimate = val.parse().unwrap_or(60),
            18 => {
                self.preferences.timezone = opt;
                // Re-initialize timezone so it takes effect immediately
                dodo::init_timezone(self.preferences.timezone.as_deref());
            }
            19 => self.email_config.enabled = val == "true",
            20 => self.email_config.api_key = opt,
            21 => self.email_config.from = opt,
            22 => self.email_config.to = opt,
            23 => {
                self.email_config.digest_time = if val.is_empty() {
                    "07:00".to_string()
                } else {
                    val.clone()
                }
            }
            _ => {}
        }
    }

    pub(super) fn auto_scroll_config(&mut self, visible_height: usize) {
        // Compute the line offset for the selected field.
        // Layout: "── Sync ──" header (1 line), then fields 0-3 (2 lines each),
        // blank + "── Backup ──" before field 4, fields 4-12 (2 lines each),
        // blank + "── Preferences ──" before field 13, fields 13-18 (2 lines each),
        // blank + "── Email ──" before field 19, fields 19+ (2 lines each).
        let mut field_line: usize = 1; // "── Sync ──" header
        for i in 0..self.config_field_index {
            if i == 4 || i == 13 || i == 19 {
                field_line += 2; // blank + section header
            }
            field_line += 2; // label line + hint/blank line
        }
        if self.config_field_index == 4
            || self.config_field_index == 13
            || self.config_field_index == 19
        {
            field_line += 2; // section header for current field's section
        }
        if field_line < self.config_scroll {
            self.config_scroll = field_line;
        } else if field_line + 2 > self.config_scroll + visible_height {
            self.config_scroll = (field_line + 2).saturating_sub(visible_height);
        }
    }

    pub(super) fn save_config(&mut self) -> Result<()> {
        let config = dodo::config::Config {
            sync: self.sync_config.clone(),
            backup: self.backup_config.clone(),
            preferences: self.preferences.clone(),
            email: self.email_config.clone(),
        };
        config.save()?;
        if self.backup_config.is_ready() {
            self.refresh_backups();
        }
        // Update sync status when config changes
        if self.sync_config.is_ready() && matches!(self.sync_status, SyncStatus::Disabled) {
            self.sync_status = SyncStatus::Idle;
        } else if !self.sync_config.is_ready() {
            self.sync_status = SyncStatus::Disabled;
        }
        Ok(())
    }
}
