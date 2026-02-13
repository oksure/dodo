use anyhow::Result;
use ratatui::widgets::ListState;
use std::sync::mpsc;

use dodo::cli::SortBy;
use dodo::db::Database;
use dodo::notation::{parse_date, parse_duration, parse_filter_days, prepare_task};
use dodo::task::{Area, Task, TaskStatus};

use super::constants::*;
use super::format::*;

#[derive(Clone)]
pub(super) enum SyncStatus {
    Disabled,                        // sync not configured
    Idle,                            // sync configured but no sync attempted yet
    Syncing,                         // sync in progress
    Synced(std::time::Instant),      // last successful sync timestamp
    Error(String),                   // last sync failed
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

    pub(super) fn stats(&self) -> (i64, i64, usize, usize) {
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
pub(super) enum AppMode {
    Normal,
    AddTask,
    MoveTask,
    ConfirmDelete,
    EditTask,
    EditTaskField,
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
    Backup,
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
}

pub(super) struct App<'a> {
    pub(super) panes: [PaneState; 4],
    pub(super) active_pane: usize,
    pub(super) running_task: Option<String>,
    pub(super) db: &'a Database,
    pub(super) mode: AppMode,
    // Tabs & report
    pub(super) tab: TuiTab,
    pub(super) report_range: ReportRange,
    pub(super) report: Option<ReportData>,
    pub(super) tick_count: u64,
    pub(super) frame_count: u64,
    // Add task
    pub(super) add_input: String,
    // Move task
    pub(super) move_task_id: Option<String>,
    pub(super) move_source: usize,
    pub(super) move_target: usize,
    // Delete task
    pub(super) delete_task_id: Option<String>,
    pub(super) delete_task_title: String,
    // Edit task
    pub(super) edit_task_id: Option<String>,
    pub(super) edit_field_index: usize,
    pub(super) edit_field_values: [String; 9],
    pub(super) edit_field_input: String,
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
    pub(super) backup_status_msg: Option<String>,
    pub(super) backup_status_msg_at: Option<std::time::Instant>,
    pub(super) config_test_result: Option<String>,
    // Config editor
    pub(super) config_field_index: usize,
    pub(super) config_field_values: [String; CONFIG_FIELD_COUNT],
    pub(super) config_field_input: String,
    // Help modal
    pub(super) help_scroll: usize,
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
        Self {
            panes,
            active_pane: 2,
            running_task: None,
            db,
            mode: AppMode::Normal,
            tab: TuiTab::Tasks,
            report_range: ReportRange::Month,
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
            sync_status: if config.sync.is_ready() { SyncStatus::Idle } else { SyncStatus::Disabled },
            sync_receiver: None,
            last_sync_tick: 0,
            backup_status_msg: None,
            backup_status_msg_at: None,
            config_test_result: None,
            config_field_index: 0,
            config_field_values: Default::default(),
            config_field_input: String::new(),
            help_scroll: 0,
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

    pub(super) fn matches_search(&self, task: &Task) -> bool {
        if self.search_input.is_empty() {
            return true;
        }
        let query = self.search_input.to_lowercase();
        let today = chrono::Local::now().date_naive();
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
            self.panes[i].tasks.sort_by(|a, b| sort_tasks(a, b, sort, ascending));
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

    pub(super) fn refresh_report(&mut self) -> Result<()> {
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

    pub(super) fn done(&mut self) -> Result<()> {
        let task_id = self.panes[self.active_pane]
            .selected_task()
            .map(|t| t.id.clone());
        if let Some(ref id) = task_id {
            let was_done = self.panes[self.active_pane]
                .selected_task()
                .map(|t| t.status == TaskStatus::Done)
                .unwrap_or(false);
            if was_done {
                self.db.uncomplete_task_by_id(id)?;
            } else {
                self.db.complete_task_by_id(id)?;
            }
        }
        self.refresh_all()?;
        // Follow the task to its new pane
        if let Some(ref id) = task_id {
            for pane_idx in 0..4 {
                if let Some(pos) = self.panes[pane_idx].tasks.iter().position(|t| t.id == *id) {
                    self.active_pane = pane_idx;
                    self.panes[pane_idx].list_state.select(Some(pos));
                    break;
                }
            }
        }
        Ok(())
    }

    pub(super) fn open_note_quick(&mut self) {
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

    pub(super) fn start_add_task(&mut self) {
        self.add_input.clear();
        self.mode = AppMode::AddTask;
    }

    pub(super) fn confirm_add_task(&mut self) -> Result<()> {
        if !self.add_input.is_empty() {
            let prep = prepare_task(&self.add_input);
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
            self.refresh_all()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    pub(super) fn start_move_task(&mut self) {
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
            self.db.update_task_scheduled(task_id, area.to_scheduled_date())?;
            self.refresh_all()?;
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
            self.db.update_task_scheduled(&task_id, area.to_scheduled_date())?;
            self.refresh_all()?;
            self.active_pane = target;
            if let Some(pos) = self.panes[target].tasks.iter().position(|t| t.id == task_id) {
                self.panes[target].list_state.select(Some(pos));
            }
        }
        Ok(())
    }

    pub(super) fn start_delete(&mut self) {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            self.delete_task_id = Some(task.id.clone());
            self.delete_task_title = task.title.clone();
            self.mode = AppMode::ConfirmDelete;
        }
    }

    pub(super) fn confirm_delete(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.delete_task_id {
            self.db.delete_task_by_id(task_id)?;
            self.refresh_all()?;
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    pub(super) fn start_edit_task(&mut self) {
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

    pub(super) fn enter_edit_field(&mut self) {
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

    pub(super) fn save_notes(&mut self) -> Result<()> {
        if let Some(ref task_id) = self.edit_task_id {
            let full = self.note_lines.join("\n");
            self.db.update_notes_by_id(task_id, &full)?;
            self.edit_field_values[8] = full;
            self.refresh_all()?;
        }
        Ok(())
    }

    pub(super) fn save_edit_field(&mut self) -> Result<()> {
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

    pub(super) fn start_edit_config(&mut self) {
        self.config_field_index = 0;
        self.config_field_values = [
            // Sync fields (0-3)
            if self.sync_config.enabled { "true".to_string() } else { "false".to_string() },
            self.sync_config.turso_url.clone().unwrap_or_default(),
            self.sync_config.turso_token.clone().unwrap_or_default(),
            self.sync_config.sync_interval.to_string(),
            // Backup fields (4-12)
            if self.backup_config.enabled { "true".to_string() } else { "false".to_string() },
            self.backup_config.endpoint.clone().unwrap_or_default(),
            self.backup_config.bucket.clone().unwrap_or_default(),
            self.backup_config.prefix.clone(),
            self.backup_config.access_key.clone().unwrap_or_default(),
            self.backup_config.secret_key.clone().unwrap_or_default(),
            self.backup_config.region.clone().unwrap_or_default(),
            self.backup_config.schedule_days.to_string(),
            self.backup_config.max_backups.to_string(),
        ];
        self.mode = AppMode::EditConfig;
    }

    pub(super) fn enter_config_field(&mut self) {
        if CONFIG_FIELD_TYPES[self.config_field_index] == ConfigFieldType::Boolean {
            // Toggle boolean immediately
            let new_val = self.config_field_values[self.config_field_index] != "true";
            self.config_field_values[self.config_field_index] =
                if new_val { "true".to_string() } else { "false".to_string() };
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
        let opt = if val.is_empty() { None } else { Some(val.clone()) };
        match idx {
            0 => self.sync_config.enabled = val == "true",
            1 => self.sync_config.turso_url = opt,
            2 => self.sync_config.turso_token = opt,
            3 => self.sync_config.sync_interval = val.parse().unwrap_or(10),
            4 => self.backup_config.enabled = val == "true",
            5 => self.backup_config.endpoint = opt,
            6 => self.backup_config.bucket = opt,
            7 => self.backup_config.prefix = if val.is_empty() { "dodo/".to_string() } else { val.clone() },
            8 => self.backup_config.access_key = opt,
            9 => self.backup_config.secret_key = opt,
            10 => self.backup_config.region = opt,
            11 => self.backup_config.schedule_days = val.parse().unwrap_or(7),
            12 => self.backup_config.max_backups = val.parse().unwrap_or(10),
            _ => {}
        }
    }

    pub(super) fn save_config(&mut self) -> Result<()> {
        let config = dodo::config::Config {
            sync: self.sync_config.clone(),
            backup: self.backup_config.clone(),
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
