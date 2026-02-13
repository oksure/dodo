use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::Backend;
use ratatui::Terminal;

use dodo::notation::parse_notation;
use dodo::task::TaskStatus;

use super::constants::*;
use super::draw::draw_ui;
use super::format::*;
use super::state::*;

// Note: `let _ =` is used intentionally throughout the TUI event handlers for
// fire-and-forget DB operations. There is no user-visible error display mechanism
// in the TUI event loop, and these operations are best-effort (e.g., refreshing
// data, toggling timers). Failures are non-fatal and the TUI continues operating.
pub(super) fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
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
                        KeyCode::Char('c') => {
                            app.tab = TuiTab::Recurring;
                            let _ = app.refresh_templates();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Char('r') => {
                            app.tab = TuiTab::Report;
                            let _ = app.refresh_report();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Char('b') if app.tab != TuiTab::Tasks => {
                            app.tab = TuiTab::Backup;
                            app.refresh_backups();
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::Tab => {
                            match app.tab {
                                TuiTab::Tasks => {
                                    app.tab = TuiTab::Recurring;
                                    let _ = app.refresh_templates();
                                }
                                TuiTab::Recurring => {
                                    app.tab = TuiTab::Report;
                                    let _ = app.refresh_report();
                                }
                                TuiTab::Report => {
                                    app.tab = TuiTab::Backup;
                                    app.refresh_backups();
                                }
                                TuiTab::Backup => {
                                    app.tab = TuiTab::Tasks;
                                }
                            }
                            app.count_prefix = None;
                            app.pending_g = false;
                        }
                        KeyCode::BackTab => {
                            match app.tab {
                                TuiTab::Tasks => {
                                    app.tab = TuiTab::Backup;
                                    app.refresh_backups();
                                }
                                TuiTab::Recurring => {
                                    app.tab = TuiTab::Tasks;
                                }
                                TuiTab::Report => {
                                    app.tab = TuiTab::Recurring;
                                    let _ = app.refresh_templates();
                                }
                                TuiTab::Backup => {
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
                            } else if app.tab == TuiTab::Recurring {
                                handle_recurring_key(app, key.code);
                            } else if app.tab == TuiTab::Backup {
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
                                let parsed = parse_notation(&app.rec_add_input);
                                if let Some(ref recurrence) = parsed.recurrence {
                                    let title = if parsed.title.is_empty() {
                                        app.rec_add_input.clone()
                                    } else {
                                        parsed.title.clone()
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

                                    let _ = app.db.add_template(
                                        &title,
                                        recurrence,
                                        parsed.project,
                                        context,
                                        estimate,
                                        parsed.deadline,
                                        scheduled,
                                        tags,
                                        parsed.priority,
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
                            app.config_field_index = (app.config_field_index + 1) % CONFIG_FIELD_COUNT;
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.config_field_index = if app.config_field_index == 0 {
                                CONFIG_FIELD_COUNT - 1
                            } else {
                                app.config_field_index - 1
                            };
                        }
                        KeyCode::Enter => {
                            app.enter_config_field();
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
            match app.tab {
                TuiTab::Tasks => { let _ = app.refresh_all(); }
                TuiTab::Recurring => { let _ = app.refresh_templates(); }
                TuiTab::Report => { let _ = app.refresh_report(); }
                TuiTab::Backup => { app.refresh_backups(); }
            }
            last_data_refresh = std::time::Instant::now();
        }
    }
}

pub(super) fn handle_tasks_key(app: &mut App, code: KeyCode) {
    let count = app.count_prefix.take().unwrap_or(1);
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.panes[app.active_pane].jump(count);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let sel = app.panes[app.active_pane].list_state.selected().unwrap_or(0);
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
        KeyCode::Char('s') => {
            let _ = app.toggle_selected();
        }
        KeyCode::Char('d') => {
            let _ = app.done();
        }
        KeyCode::Char('o') => app.cycle_sort(),
        KeyCode::Char('/') => {
            app.mode = AppMode::Search;
        }
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
        KeyCode::Char('g') => {
            let _ = app.db.generate_instances();
            let _ = app.refresh_templates();
            let _ = app.refresh_all();
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
                    template.estimate_minutes.map(|m| format_estimate_tui(m)).unwrap_or_default(),
                    template.deadline.map(|d| d.to_string()).unwrap_or_default(),
                    template.scheduled.map(|d| d.to_string()).unwrap_or_default(),
                    template.priority.map(|p| "!".repeat(p.clamp(1, 4) as usize)).unwrap_or_default(),
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
                        app.backup_status_msg = Some(format!("Uploaded: {}", key));
                        app.refresh_backups();
                    }
                    Err(e) => {
                        app.backup_status_msg = Some(format!("Upload failed: {}", e));
                    }
                }
            } else {
                app.backup_status_msg = Some("Backup not configured".to_string());
            }
        }
        KeyCode::Char('r') => {
            if let Some(entry) = app.backup_entries.get(app.backup_selected) {
                let key = entry.key.clone();
                match dodo::backup::restore_backup(&app.backup_config, &key) {
                    Ok(()) => {
                        app.backup_status_msg =
                            Some(format!("Restored: {}", entry.display_name));
                        let _ = app.refresh_all();
                    }
                    Err(e) => {
                        app.backup_status_msg = Some(format!("Restore failed: {}", e));
                    }
                }
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(entry) = app.backup_entries.get(app.backup_selected) {
                let key = entry.key.clone();
                match dodo::backup::delete_backup(&app.backup_config, &key) {
                    Ok(()) => {
                        app.backup_status_msg =
                            Some(format!("Deleted: {}", entry.display_name));
                        app.refresh_backups();
                    }
                    Err(e) => {
                        app.backup_status_msg = Some(format!("Delete failed: {}", e));
                    }
                }
            }
        }
        KeyCode::Char('e') => {
            app.start_edit_config();
        }
        _ => {}
    }
}
