use anyhow::Result;
use chrono::Datelike;
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::Backend;
use ratatui::Terminal;
use std::io::Write;

use dodo::notation::prepare_task;
use dodo::task::TaskStatus;

use super::constants::*;
use super::draw::draw_ui;
use super::format::*;
use super::state::*;

fn play_bell() {
    print!("\x07");
    let _ = std::io::stdout().flush();
}

// Note: `let _ =` is used intentionally throughout the TUI event handlers for
// fire-and-forget DB operations. There is no user-visible error display mechanism
// in the TUI event loop, and these operations are best-effort (e.g., refreshing
// data, toggling timers). Failures are non-fatal and the TUI continues operating.
pub(super) fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let mut last_data_refresh = std::time::Instant::now();
    // Animations now use wall-clock time (app.anim_frame()), so actual render fps
    // no longer affects animation speed. We just need enough fps to look smooth:
    //   33ms ≈ 30fps — smooth enough for the rainbow sweep on running tasks
    //  100ms ≈ 10fps — barely-visible redraw for timer updates; keys still feel instant
    let poll_rate_fast = std::time::Duration::from_millis(33);
    let poll_rate_idle = std::time::Duration::from_millis(100);
    let data_refresh_rate = std::time::Duration::from_secs(1);

    loop {
        app.frame_count = app.frame_count.wrapping_add(1);
        terminal.draw(|f| draw_ui(f, app))?;

        // Use faster poll when a task is running (rainbow animation + timer countdown).
        let poll_rate = if app.running_task.is_some() {
            poll_rate_fast
        } else {
            poll_rate_idle
        };

        if crossterm::event::poll(poll_rate)? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.save_last_view();
                            let _ = app.save_config();
                            return Ok(());
                        }
                        KeyCode::Char('t') => {
                            if app.tab == TuiTab::Tasks {
                                // Already on Tasks tab: jump to today
                                match app.tasks_view {
                                    TasksView::Daily => app.daily_jump_to_today(),
                                    TasksView::Weekly => {
                                        app.week_start_date = chrono::Local::now().date_naive();
                                        app.weekly_active = 0;
                                        let _ = app.refresh_weekly();
                                    }
                                    TasksView::Calendar => {
                                        let today = chrono::Local::now().date_naive();
                                        app.calendar_selected = today;
                                        app.calendar_year = today.year();
                                        app.calendar_month = today.month();
                                        let _ = app.refresh_calendar();
                                    }
                                    TasksView::Panes => {} // no-op
                                }
                            } else {
                                app.tab = TuiTab::Tasks;
                            }
                            app.save_last_view();
                            let _ = app.save_config();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Char('c') => {
                            app.save_last_view();
                            let _ = app.save_config();
                            app.tab = TuiTab::Recurring;
                            let _ = app.refresh_templates();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Char('r') => {
                            app.save_last_view();
                            let _ = app.save_config();
                            app.tab = TuiTab::Report;
                            let _ = app.refresh_report();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Char(',') => {
                            app.save_last_view();
                            let _ = app.save_config();
                            app.tab = TuiTab::Settings;
                            app.refresh_backups();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Tab => {
                            match app.tab {
                                TuiTab::Tasks => {
                                    app.save_last_view();
                                    let _ = app.save_config();
                                    app.tab = TuiTab::Recurring;
                                    let _ = app.refresh_templates();
                                }
                                TuiTab::Recurring => {
                                    app.save_last_view();
                                    let _ = app.save_config();
                                    app.tab = TuiTab::Report;
                                    let _ = app.refresh_report();
                                }
                                TuiTab::Report => {
                                    app.save_last_view();
                                    let _ = app.save_config();
                                    app.tab = TuiTab::Settings;
                                    app.refresh_backups();
                                }
                                TuiTab::Settings => {
                                    app.save_last_view();
                                    let _ = app.save_config();
                                    app.tab = TuiTab::Tasks;
                                }
                            }
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::BackTab => {
                            match app.tab {
                                TuiTab::Tasks => {
                                    app.tab = TuiTab::Settings;
                                    app.refresh_backups();
                                }
                                TuiTab::Recurring => {
                                    app.tab = TuiTab::Tasks;
                                }
                                TuiTab::Report => {
                                    app.tab = TuiTab::Recurring;
                                    let _ = app.refresh_templates();
                                }
                                TuiTab::Settings => {
                                    app.tab = TuiTab::Report;
                                    let _ = app.refresh_report();
                                }
                            }
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Char('?') => {
                            app.help_scroll = 0;
                            app.mode = AppMode::Help;
                        }
                        KeyCode::Char('y') => {
                            if app.sync_enabled() {
                                app.trigger_sync();
                            }
                        }
                        _ => {
                            if app.tab == TuiTab::Tasks {
                                // Handle pending 'g' for gg (jump to first)
                                if app.pending_g {
                                    app.pending_g = false;
                                    if key.code == KeyCode::Char('g') {
                                        match app.tasks_view {
                                            TasksView::Panes => {
                                                app.panes[app.active_pane].jump_to_first()
                                            }
                                            TasksView::Daily => {
                                                // Jump to first Task entry
                                                for (i, entry) in
                                                    app.daily_entries.iter().enumerate()
                                                {
                                                    if matches!(entry, DailyEntry::Task(_)) {
                                                        app.daily_cursor = i;
                                                        break;
                                                    }
                                                }
                                            }
                                            TasksView::Weekly => {
                                                app.weekly_panes[app.weekly_active].jump_to_first()
                                            }
                                            TasksView::Calendar => {} // no-op for calendar
                                        }
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
                            } else if app.tab == TuiTab::Recurring {
                                if app.pending_g {
                                    app.pending_g = false;
                                    if key.code == KeyCode::Char('g') {
                                        // gg → jump to first template
                                        app.template_selected = 0;
                                    } else {
                                        // g followed by non-g → generate, then handle key
                                        let _ = app.db.generate_instances();
                                        let _ = app.refresh_templates();
                                        let _ = app.refresh_all();
                                        handle_recurring_key(app, key.code);
                                    }
                                } else {
                                    handle_recurring_key(app, key.code);
                                }
                            } else if app.tab == TuiTab::Settings {
                                handle_backup_key(app, key.code);
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
                        KeyCode::Char('u')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.add_input.clear();
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
                            // If input is empty, enter edit mode; otherwise save
                            if app.edit_field_input.is_empty() {
                                app.enter_edit_field();
                            } else {
                                let _ = app.save_edit_field();
                            }
                        }
                        KeyCode::Backspace => {
                            // Only allow backspace if we're in edit mode (field has input)
                            if !app.edit_field_input.is_empty() {
                                app.edit_field_input.pop();
                            } else {
                                app.enter_edit_field();
                            }
                        }
                        KeyCode::Char(c) => {
                            // Start typing to enter edit mode
                            app.edit_field_input.push(c);
                            app.mode = AppMode::EditTaskField;
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
                        KeyCode::Char('u')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.edit_field_input.clear();
                        }
                        KeyCode::Char(c) => {
                            app.edit_field_input.push(c);
                        }
                        _ => {}
                    },
                    AppMode::Search => match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Down => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Backspace => {
                            app.search_input.pop();
                            let _ = app.refresh_all();
                        }
                        KeyCode::Char('u')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.search_input.clear();
                            let _ = app.refresh_all();
                        }
                        KeyCode::Char(c) => {
                            app.search_input.push(c);
                            let _ = app.refresh_all();
                        }
                        _ => {}
                    },
                    AppMode::RecAddTemplate => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Enter => {
                            if !app.rec_add_input.is_empty() {
                                let prep = prepare_task(&app.rec_add_input);
                                if let Some(ref recurrence) = prep.recurrence {
                                    let _ = app.db.add_template(
                                        &prep.title,
                                        recurrence,
                                        prep.project,
                                        prep.context,
                                        prep.estimate_minutes,
                                        prep.deadline,
                                        prep.scheduled,
                                        prep.tags,
                                        prep.priority,
                                    );
                                    let _ = app.refresh_templates();
                                    let _ = app.refresh_all();
                                }
                            }
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Backspace => {
                            app.rec_add_input.pop();
                        }
                        KeyCode::Char('u')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.rec_add_input.clear();
                        }
                        KeyCode::Char(c) => {
                            app.rec_add_input.push(c);
                        }
                        _ => {}
                    },
                    AppMode::RecConfirmDelete => match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => {
                            if let Some(template) = app.templates.get(app.template_selected) {
                                let _ = app.db.delete_template(&template.id);
                                let _ = app.refresh_templates();
                                let _ = app.refresh_all();
                            }
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Char('n') | KeyCode::Esc => {
                            app.mode = AppMode::Normal;
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
                                KeyCode::Char('u')
                                    if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                                {
                                    app.edit_field_input.clear();
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
                    AppMode::EditConfig => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.config_field_index =
                                (app.config_field_index + 1) % CONFIG_FIELD_COUNT;
                            app.config_test_result = None;
                            app.auto_scroll_config(20); // approximate visible height
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.config_field_index = if app.config_field_index == 0 {
                                CONFIG_FIELD_COUNT - 1
                            } else {
                                app.config_field_index - 1
                            };
                            app.config_test_result = None;
                            app.auto_scroll_config(20); // approximate visible height
                        }
                        KeyCode::Enter => {
                            app.enter_config_field();
                        }
                        KeyCode::Char('t') => {
                            if app.config_field_index <= 3 {
                                // Sync fields (0-3): test Turso connection
                                let url = app.config_field_values[1].clone();
                                let token = app.config_field_values[2].clone();
                                if url.is_empty() || token.is_empty() {
                                    app.config_test_result =
                                        Some("\u{2717} URL and token are required".to_string());
                                    app.set_backup_status(
                                        "Error: Sync URL and token required".to_string(),
                                    );
                                } else {
                                    match dodo::db::Database::test_sync_connection(url, token) {
                                        Ok(()) => {
                                            app.config_test_result = Some(
                                                "\u{2713} Turso connection successful".to_string(),
                                            );
                                            app.set_backup_status(
                                                "\u{2713} Turso connection successful".to_string(),
                                            );
                                        }
                                        Err(e) => {
                                            app.config_test_result =
                                                Some(format!("\u{2717} {}", e));
                                            app.set_backup_status(format!(
                                                "Error: Sync test failed: {}",
                                                e
                                            ));
                                        }
                                    }
                                }
                            } else {
                                // Backup fields (4-12): test S3 connection
                                if !app.backup_config.is_ready() {
                                    app.config_test_result =
                                        Some("\u{2717} Backup config incomplete".to_string());
                                    app.set_backup_status(
                                        "Error: Backup config incomplete".to_string(),
                                    );
                                } else {
                                    match dodo::backup::test_connection(&app.backup_config) {
                                        Ok(msg) => {
                                            app.config_test_result =
                                                Some(format!("\u{2713} {}", msg));
                                            app.set_backup_status(format!("\u{2713} {}", msg));
                                        }
                                        Err(e) => {
                                            app.config_test_result =
                                                Some(format!("\u{2717} {}", e));
                                            app.set_backup_status(format!(
                                                "Error: Backup test failed: {}",
                                                e
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                    AppMode::EditConfigField => match key.code {
                        KeyCode::Esc => {
                            app.mode = AppMode::EditConfig;
                        }
                        KeyCode::Enter => {
                            app.save_config_field();
                        }
                        KeyCode::Backspace => {
                            app.config_field_input.pop();
                        }
                        KeyCode::Char('u')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.config_field_input.clear();
                        }
                        KeyCode::Char(c) => {
                            app.config_field_input.push(c);
                        }
                        _ => {}
                    },
                    AppMode::Help => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.help_scroll += 1;
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.help_scroll = app.help_scroll.saturating_sub(1);
                        }
                        KeyCode::PageDown => {
                            app.help_scroll += 10;
                        }
                        KeyCode::PageUp => {
                            app.help_scroll = app.help_scroll.saturating_sub(10);
                        }
                        _ => {}
                    },
                }
            }
        }

        if last_data_refresh.elapsed() >= data_refresh_rate {
            app.tick_count = app.tick_count.wrapping_add(1);

            // Poll for background sync completion
            app.check_sync_result();

            // Auto-dismiss toast messages
            if let Some(at) = app.backup_status_msg_at {
                let is_error = app
                    .backup_status_msg
                    .as_ref()
                    .map(|m| {
                        m.starts_with("Error") || m.contains("failed") || m.starts_with("\u{2717}")
                    })
                    .unwrap_or(false);
                let threshold = if is_error {
                    TOAST_ERROR_DURATION_SECS
                } else {
                    TOAST_DURATION_SECS
                };
                if at.elapsed().as_secs() >= threshold {
                    app.backup_status_msg = None;
                    app.backup_status_msg_at = None;
                }
            }

            // Periodic sync based on configured interval
            let sync_interval_ticks = app.sync_config.sync_interval as u64 * 60;
            if app.sync_enabled()
                && sync_interval_ticks > 0
                && app.tick_count.wrapping_sub(app.last_sync_tick) >= sync_interval_ticks
            {
                app.trigger_sync();
                app.last_sync_tick = app.tick_count;
            }

            match app.tab {
                TuiTab::Tasks => {
                    let _ = app.refresh_current_view();
                }
                TuiTab::Recurring => {
                    let _ = app.refresh_templates();
                }
                TuiTab::Report => {
                    let _ = app.refresh_report();
                }
                TuiTab::Settings => {
                    app.refresh_backups();
                }
            }

            // Periodic ding while a task is running
            if app.running_task.is_some()
                && app.preferences.sound_enabled
                && app.preferences.timer_sound_interval > 0
            {
                let interval_ticks = app.preferences.timer_sound_interval as u64 * 60;
                if app.tick_count.wrapping_sub(app.last_ding_tick) >= interval_ticks {
                    play_bell();
                    app.last_ding_tick = app.tick_count;
                }
            }

            last_data_refresh = std::time::Instant::now();
        }
    }
}

pub(super) fn handle_tasks_key(app: &mut App, code: KeyCode) {
    let count = app.count_prefix.take().unwrap_or(1);

    // Common keys across all views
    match code {
        KeyCode::Char('v') => {
            app.save_last_view();
            let _ = app.save_config();
            app.tasks_view = app.tasks_view.next();
            let _ = app.refresh_current_view();
            if app.tasks_view == TasksView::Daily {
                app.daily_jump_to_today();
            }
            return;
        }
        KeyCode::Char('V') => {
            app.save_last_view();
            let _ = app.save_config();
            app.tasks_view = app.tasks_view.prev();
            let _ = app.refresh_current_view();
            if app.tasks_view == TasksView::Daily {
                app.daily_jump_to_today();
            }
            return;
        }
        KeyCode::Char('s') => {
            let _ = app.toggle_selected();
            return;
        }
        KeyCode::Char('d') => {
            let was_done = app
                .current_selected_task()
                .map(|t| t.status == TaskStatus::Done)
                .unwrap_or(true);
            let _ = app.done();
            // Play bell on marking done (not on undone)
            if !was_done && app.preferences.sound_enabled {
                play_bell();
            }
            return;
        }
        KeyCode::Char('/') => {
            app.mode = AppMode::Search;
            return;
        }
        KeyCode::Char('n') => {
            app.open_note_quick();
            return;
        }
        KeyCode::Char('a') => {
            app.start_add_task();
            return;
        }
        KeyCode::Enter => {
            app.start_edit_task();
            return;
        }
        KeyCode::Backspace | KeyCode::Delete => {
            app.start_delete();
            return;
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.adjust_selected_date(1);
            return;
        }
        KeyCode::Char('-') => {
            app.adjust_selected_date(-1);
            return;
        }
        _ => {}
    }

    // View-specific navigation
    match app.tasks_view {
        TasksView::Panes => handle_panes_nav(app, code, count),
        TasksView::Daily => handle_daily_nav(app, code, count),
        TasksView::Weekly => handle_weekly_nav(app, code, count),
        TasksView::Calendar => handle_calendar_nav(app, code, count),
    }
}

fn handle_panes_nav(app: &mut App, code: KeyCode, count: usize) {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.panes[app.active_pane].jump(count);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let sel = app.panes[app.active_pane]
                .list_state
                .selected()
                .unwrap_or(0);
            if sel == 0 {
                app.mode = AppMode::Search;
            } else {
                app.panes[app.active_pane].jump_back(count);
            }
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
        KeyCode::Char('o') => app.cycle_sort(),
        KeyCode::Char('m') => {
            app.start_move_task();
        }
        KeyCode::Char('<') => {
            let _ = app.move_task_quick(-1);
        }
        KeyCode::Char('>') => {
            let _ = app.move_task_quick(1);
        }
        _ => {}
    }
}

pub(super) fn handle_daily_nav(app: &mut App, code: KeyCode, count: usize) {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            // Skip headers, move to next Task entry
            for _ in 0..count {
                let mut next = app.daily_cursor + 1;
                while next < app.daily_entries.len() {
                    if matches!(app.daily_entries[next], DailyEntry::Task(_)) {
                        break;
                    }
                    next += 1;
                }
                if next < app.daily_entries.len() {
                    app.daily_cursor = next;
                }
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            for _ in 0..count {
                if app.daily_cursor == 0 {
                    app.mode = AppMode::Search;
                    return;
                }
                let mut prev = app.daily_cursor.saturating_sub(1);
                while prev > 0 {
                    if matches!(app.daily_entries[prev], DailyEntry::Task(_)) {
                        break;
                    }
                    prev -= 1;
                }
                if matches!(app.daily_entries.get(prev), Some(DailyEntry::Task(_))) {
                    app.daily_cursor = prev;
                } else {
                    app.mode = AppMode::Search;
                    return;
                }
            }
        }
        KeyCode::Char('G') => {
            // Jump to last Task entry
            for i in (0..app.daily_entries.len()).rev() {
                if matches!(app.daily_entries[i], DailyEntry::Task(_)) {
                    app.daily_cursor = i;
                    break;
                }
            }
        }
        KeyCode::Char('g') => {
            app.pending_g = true;
        }
        KeyCode::PageDown => {
            for _ in 0..10 {
                let mut next = app.daily_cursor + 1;
                while next < app.daily_entries.len() {
                    if matches!(app.daily_entries[next], DailyEntry::Task(_)) {
                        break;
                    }
                    next += 1;
                }
                if next < app.daily_entries.len() {
                    app.daily_cursor = next;
                }
            }
        }
        KeyCode::PageUp => {
            for _ in 0..10 {
                if app.daily_cursor == 0 {
                    break;
                }
                let mut prev = app.daily_cursor.saturating_sub(1);
                while prev > 0 {
                    if matches!(app.daily_entries[prev], DailyEntry::Task(_)) {
                        break;
                    }
                    prev -= 1;
                }
                if matches!(app.daily_entries.get(prev), Some(DailyEntry::Task(_))) {
                    app.daily_cursor = prev;
                }
            }
        }
        KeyCode::Char('o') => app.cycle_sort(),
        KeyCode::Char('<') => {
            // Quick-move: change scheduled -1 day
            if let Some(DailyEntry::Task(ref t)) = app.daily_entries.get(app.daily_cursor) {
                let today = chrono::Local::now().date_naive();
                let current = t.scheduled.unwrap_or(today);
                let new_date = current - chrono::Duration::days(1);
                let _ = app.db.update_task_scheduled(&t.id, new_date);
                let _ = app.refresh_daily();
            }
        }
        KeyCode::Char('>') => {
            // Quick-move: change scheduled +1 day
            if let Some(DailyEntry::Task(ref t)) = app.daily_entries.get(app.daily_cursor) {
                let today = chrono::Local::now().date_naive();
                let current = t.scheduled.unwrap_or(today);
                let new_date = current + chrono::Duration::days(1);
                let _ = app.db.update_task_scheduled(&t.id, new_date);
                let _ = app.refresh_daily();
            }
        }
        KeyCode::Char('z') => {
            // zz: center cursor (vim-style)
            // Only for Daily view
            if app.tasks_view == TasksView::Daily {
                // Center the cursor (simple centering)
                app.daily_scroll = app.daily_cursor.saturating_sub(3);
            }
        }
        _ => {}
    }
}

fn handle_weekly_nav(app: &mut App, code: KeyCode, count: usize) {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.weekly_panes[app.weekly_active].jump(count);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let sel = app.weekly_panes[app.weekly_active]
                .list_state
                .selected()
                .unwrap_or(0);
            if sel == 0 {
                app.mode = AppMode::Search;
            } else {
                app.weekly_panes[app.weekly_active].jump_back(count);
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if app.weekly_active > 0 {
                app.weekly_active -= 1;
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if app.weekly_active < 7 {
                app.weekly_active += 1;
            }
        }
        KeyCode::Char('[') => {
            // Previous week
            let week_start_day = match app.preferences.week_start {
                dodo::config::WeekStart::Sunday => chrono::Weekday::Sun,
                dodo::config::WeekStart::Monday => chrono::Weekday::Mon,
            };
            app.week_start_date = app.week_start_date - chrono::Duration::days(7);
            // Align to week start day
            while app.week_start_date.weekday() != week_start_day {
                app.week_start_date = app.week_start_date - chrono::Duration::days(1);
            }
            let _ = app.refresh_weekly();
        }
        KeyCode::Char(']') => {
            // Next week
            let week_start_day = match app.preferences.week_start {
                dodo::config::WeekStart::Sunday => chrono::Weekday::Sun,
                dodo::config::WeekStart::Monday => chrono::Weekday::Mon,
            };
            app.week_start_date = app.week_start_date + chrono::Duration::days(7);
            // Align to week start day
            while app.week_start_date.weekday() != week_start_day {
                app.week_start_date = app.week_start_date - chrono::Duration::days(1);
            }
            let _ = app.refresh_weekly();
        }
        KeyCode::Char('o') => {
            let pane = &mut app.weekly_panes[app.weekly_active];
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
        KeyCode::Char('<') => {
            // Quick-move task to previous day
            if let Some(task) = app.weekly_panes[app.weekly_active].selected_task() {
                let today = chrono::Local::now().date_naive();
                let current = task.scheduled.unwrap_or(today);
                let new_date = current - chrono::Duration::days(1);
                let _ = app.db.update_task_scheduled(&task.id, new_date);
                let _ = app.refresh_weekly();
            }
        }
        KeyCode::Char('>') => {
            // Quick-move task to next day
            if let Some(task) = app.weekly_panes[app.weekly_active].selected_task() {
                let today = chrono::Local::now().date_naive();
                let current = task.scheduled.unwrap_or(today);
                let new_date = current + chrono::Duration::days(1);
                let _ = app.db.update_task_scheduled(&task.id, new_date);
                let _ = app.refresh_weekly();
            }
        }
        KeyCode::Char('m') => {
            app.start_move_task();
        }
        KeyCode::Char('G') => {
            app.weekly_panes[app.weekly_active].jump_to_last();
        }
        KeyCode::Char('g') => {
            app.pending_g = true;
        }
        _ => {}
    }
}

fn handle_calendar_nav(app: &mut App, code: KeyCode, _count: usize) {
    match app.calendar_focus {
        CalendarFocus::Grid => match code {
            KeyCode::Char('h') | KeyCode::Left => {
                app.calendar_selected = app.calendar_selected - chrono::Duration::days(1);
                app.calendar_year = app.calendar_selected.year();
                app.calendar_month = app.calendar_selected.month();
                let _ = app.refresh_calendar();
            }
            KeyCode::Char('l') | KeyCode::Right => {
                app.calendar_selected = app.calendar_selected + chrono::Duration::days(1);
                app.calendar_year = app.calendar_selected.year();
                app.calendar_month = app.calendar_selected.month();
                let _ = app.refresh_calendar();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.calendar_selected = app.calendar_selected + chrono::Duration::days(7);
                app.calendar_year = app.calendar_selected.year();
                app.calendar_month = app.calendar_selected.month();
                let _ = app.refresh_calendar();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.calendar_selected = app.calendar_selected - chrono::Duration::days(7);
                app.calendar_year = app.calendar_selected.year();
                app.calendar_month = app.calendar_selected.month();
                let _ = app.refresh_calendar();
            }
            KeyCode::Char('[') => {
                // Previous month
                if app.calendar_month == 1 {
                    app.calendar_month = 12;
                    app.calendar_year -= 1;
                } else {
                    app.calendar_month -= 1;
                }
                // Clamp selected day
                let max_day = days_in_month(app.calendar_year, app.calendar_month);
                let day = app.calendar_selected.day().min(max_day);
                if let Some(d) =
                    chrono::NaiveDate::from_ymd_opt(app.calendar_year, app.calendar_month, day)
                {
                    app.calendar_selected = d;
                }
                let _ = app.refresh_calendar();
            }
            KeyCode::Char(']') => {
                // Next month
                if app.calendar_month == 12 {
                    app.calendar_month = 1;
                    app.calendar_year += 1;
                } else {
                    app.calendar_month += 1;
                }
                let max_day = days_in_month(app.calendar_year, app.calendar_month);
                let day = app.calendar_selected.day().min(max_day);
                if let Some(d) =
                    chrono::NaiveDate::from_ymd_opt(app.calendar_year, app.calendar_month, day)
                {
                    app.calendar_selected = d;
                }
                let _ = app.refresh_calendar();
            }
            KeyCode::Tab => {
                if !app.calendar_tasks.is_empty() {
                    app.calendar_focus = CalendarFocus::TaskList;
                    app.calendar_task_selected = 0;
                }
            }
            _ => {}
        },
        CalendarFocus::TaskList => match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.calendar_tasks.is_empty()
                    && app.calendar_task_selected < app.calendar_tasks.len() - 1
                {
                    app.calendar_task_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.calendar_task_selected > 0 {
                    app.calendar_task_selected -= 1;
                }
            }
            KeyCode::Tab | KeyCode::Esc => {
                app.calendar_focus = CalendarFocus::Grid;
            }
            _ => {}
        },
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

pub(super) fn handle_recurring_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.templates.is_empty() && app.template_selected < app.templates.len() - 1 {
                app.template_selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.template_selected > 0 {
                app.template_selected -= 1;
            }
        }
        KeyCode::Char('a') => {
            app.rec_add_input.clear();
            app.mode = AppMode::RecAddTemplate;
        }
        KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
            if !app.templates.is_empty() {
                app.mode = AppMode::RecConfirmDelete;
            }
        }
        KeyCode::Char('p') => {
            if let Some(template) = app.templates.get(app.template_selected) {
                if template.status == TaskStatus::Paused {
                    let _ = app.db.resume_template(&template.id);
                } else {
                    let _ = app.db.pause_template(&template.id);
                }
                let _ = app.refresh_templates();
            }
        }
        KeyCode::Char('G') => {
            if !app.templates.is_empty() {
                app.template_selected = app.templates.len() - 1;
            }
        }
        KeyCode::Char('g') => {
            app.pending_g = true;
        }
        KeyCode::Char('e') | KeyCode::Enter => {
            // Open edit modal for the selected template
            if let Some(template) = app.templates.get(app.template_selected) {
                app.edit_task_id = Some(template.id.clone());
                app.edit_field_index = 0;
                app.edit_field_values = [
                    template.title.clone(),
                    template.project.clone().unwrap_or_default(),
                    template.context.clone().unwrap_or_default(),
                    template.tags.clone().unwrap_or_default(),
                    template
                        .estimate_minutes
                        .map(|m| format_estimate_tui(m))
                        .unwrap_or_default(),
                    template.deadline.map(|d| d.to_string()).unwrap_or_default(),
                    template
                        .scheduled
                        .map(|d| d.to_string())
                        .unwrap_or_default(),
                    template
                        .priority
                        .map(|p| "!".repeat(p.clamp(1, 4) as usize))
                        .unwrap_or_default(),
                    template.recurrence.clone().unwrap_or_default(),
                ];
                app.edit_field_input.clear();
                app.mode = AppMode::EditTask;
            }
        }
        _ => {}
    }
}

pub(super) fn handle_backup_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.backup_entries.is_empty() {
                app.backup_selected = (app.backup_selected + 1) % app.backup_entries.len();
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if !app.backup_entries.is_empty() {
                app.backup_selected = if app.backup_selected == 0 {
                    app.backup_entries.len() - 1
                } else {
                    app.backup_selected - 1
                };
            }
        }
        KeyCode::Char('u') => {
            if app.backup_config.is_ready() {
                match dodo::backup::create_backup(&app.backup_config) {
                    Ok(key) => {
                        app.set_backup_status(format!("\u{2713} Uploaded: {}", key));
                        app.refresh_backups();
                    }
                    Err(e) => {
                        app.set_backup_status(format!("Error: Upload failed: {}", e));
                    }
                }
            } else {
                app.set_backup_status("Error: Backup not configured".to_string());
            }
        }
        KeyCode::Char('r') => {
            if let Some(entry) = app.backup_entries.get(app.backup_selected) {
                let key = entry.key.clone();
                let name = entry.display_name.clone();
                match dodo::backup::restore_backup(&app.backup_config, &key) {
                    Ok(()) => {
                        app.set_backup_status(format!("\u{2713} Restored: {}", name));
                        let _ = app.refresh_all();
                    }
                    Err(e) => {
                        app.set_backup_status(format!("Error: Restore failed: {}", e));
                    }
                }
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(entry) = app.backup_entries.get(app.backup_selected) {
                let key = entry.key.clone();
                let name = entry.display_name.clone();
                match dodo::backup::delete_backup(&app.backup_config, &key) {
                    Ok(()) => {
                        app.set_backup_status(format!("\u{2713} Deleted: {}", name));
                        app.refresh_backups();
                    }
                    Err(e) => {
                        app.set_backup_status(format!("Error: Delete failed: {}", e));
                    }
                }
            }
        }
        KeyCode::Char('e') => {
            app.start_edit_config();
        }
        KeyCode::Char('s') => {
            if app.sync_enabled() {
                app.trigger_sync();
                app.set_backup_status("\u{21BB} Sync started...".to_string());
            } else {
                app.set_backup_status("Error: Sync not configured".to_string());
            }
        }
        _ => {}
    }
}
