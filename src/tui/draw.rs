use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Bar, BarChart, BarGroup, Block, BorderType, Borders, Clear, Gauge, LineGauge, List,
        ListItem, ListState, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Tabs, Wrap,
    },
    Frame,
};

use chrono::Datelike;
use dodo::task::{Task, TaskStatus};

use super::constants::*;
use super::format::*;
use super::state::*;

pub(super) fn draw_ui(f: &mut Frame, app: &mut App) {
    // Always 3 lines for search — constant height avoids layout reflow / screen flicker
    // when activating or deactivating search mode.
    let search_height = if app.tab == TuiTab::Tasks { 3 } else { 0 };
    let view_selector_height = if app.tab == TuiTab::Tasks { 2 } else { 0 };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),                    // Header [0]
            Constraint::Length(1),                    // Tab bar [1]
            Constraint::Length(search_height),        // Search bar [2]
            Constraint::Length(view_selector_height), // View selector [3]
            Constraint::Min(0),                       // Content [4]
            Constraint::Length(1),                    // Footer [5]
        ])
        .split(f.area());

    // Header
    draw_header(f, app, outer[0]);

    // Tab bar
    let tab_index = match app.tab {
        TuiTab::Tasks => 0,
        TuiTab::Recurring => 1,
        TuiTab::Report => 2,
        TuiTab::Settings => 3,
    };
    let tab_names = [" Tasks ", " Recurring ", " Report ", " Settings "];
    let tab_keys = [" t ", " c ", " r ", " , "];
    let tab_titles: Vec<Line> = (0..4)
        .map(|i| {
            if i == tab_index {
                Line::from(vec![
                    Span::styled(
                        tab_names[i],
                        Style::default()
                            .fg(Color::Rgb(30, 30, 46))
                            .bg(FG_TEXT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        tab_keys[i],
                        Style::default().fg(FG_TEXT).bg(Color::Rgb(30, 30, 46)),
                    ),
                    Span::raw(" "),
                ])
            } else {
                Line::from(vec![
                    Span::styled(tab_names[i], Style::default().fg(FG_OVERLAY)),
                    Span::styled(tab_keys[i], Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
                    Span::raw(" "),
                ])
            }
        })
        .collect();
    let tabs = Tabs::new(tab_titles)
        .select(tab_index)
        .style(Style::default().fg(FG_OVERLAY))
        .divider(Span::styled(" | ", Style::default().fg(FG_OVERLAY)));
    f.render_widget(tabs, outer[1]);

    // Search bar (Tasks tab only)
    if app.tab == TuiTab::Tasks {
        draw_search_bar(f, app, outer[2]);
        draw_view_selector(f, app, outer[3]);
    }

    // Content
    match app.tab {
        TuiTab::Tasks => match app.tasks_view {
            TasksView::Panes => draw_tasks_panes(f, app, outer[4]),
            TasksView::Daily => draw_tasks_daily(f, app, outer[4]),
            TasksView::Weekly => draw_tasks_weekly(f, app, outer[4]),
            TasksView::Calendar => draw_tasks_calendar(f, app, outer[4]),
        },
        TuiTab::Recurring => draw_recurring_tab(f, app, outer[4]),
        TuiTab::Report => draw_report_tab(f, app, outer[4]),
        TuiTab::Settings => draw_backup_tab(f, app, outer[4]),
    }

    // Footer
    draw_footer(f, app, outer[5]);

    // Modal overlays
    match app.mode {
        AppMode::ConfirmDelete => draw_delete_modal(f, app),
        AppMode::RecConfirmDelete => draw_rec_delete_modal(f, app),
        AppMode::EditTask | AppMode::EditTaskField => draw_edit_modal(f, app),
        AppMode::EditElapsed => draw_elapsed_edit_modal(f, app),
        AppMode::NoteView => draw_note_view_modal(f, app),
        AppMode::AddTask => draw_add_bar(f, app),
        AppMode::RecAddTemplate => draw_rec_add_bar(f, app),
        AppMode::MoveTask => draw_move_bar(f, app),
        AppMode::EditConfig | AppMode::EditConfigField => draw_config_modal(f, app),
        AppMode::Help => draw_help_modal(f, app),
        AppMode::Normal | AppMode::Search => {}
    }
}

pub(super) fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.mode == AppMode::Search;
    let has_filter = !app.search_input.is_empty();

    // Always draw a bordered box so content never jumps position.
    // Only the border color changes: blue when focused, muted otherwise.
    let border_style = if is_focused {
        Style::default().fg(ACCENT_BLUE)
    } else {
        Style::default().fg(FG_OVERLAY)
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (content, style) = if is_focused {
        let text = format!("/ {}\u{2588}", app.search_input);
        (text, Style::default().fg(FG_TEXT))
    } else if has_filter {
        let text = format!("/ {}", app.search_input);
        (text, Style::default().fg(ACCENT_BLUE))
    } else {
        let text = String::from("/ search: +proj @ctx !! ^<3d =<1w ...");
        (text, Style::default().fg(FG_OVERLAY))
    };
    f.render_widget(Paragraph::new(content).style(style), inner);
}

pub(super) fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    // 3b: simplified running task display — solid colours, no 3-phase oscillation.
    let (running_info, timer_info, timer_style) = if let Some(ref info) = app.running_task {
        let title_str = format!(" \u{25B6} {} ", info.title);
        if let Some(est_min) = info.estimate_minutes {
            let remaining = est_min * 60 - info.elapsed_seconds;
            if remaining > 0 {
                let r = remaining.unsigned_abs();
                let timer = if r >= 3600 {
                    format!(" \u{23F1} {}h{:02}m left ", r / 3600, (r % 3600) / 60)
                } else {
                    format!(" \u{23F1} {}m left ", r / 60)
                };
                let pct = remaining as f64 / (est_min * 60) as f64;
                let style = if pct > 0.5 {
                    Style::default()
                        .fg(ACCENT_GREEN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(ACCENT_YELLOW)
                        .add_modifier(Modifier::BOLD)
                };
                (title_str, timer, style)
            } else {
                let over = (-remaining) as u64;
                let timer = if over >= 3600 {
                    format!(" +{}h{:02}m over ", over / 3600, (over % 3600) / 60)
                } else {
                    format!(" +{}m over ", over / 60)
                };
                // Pulse: alternate between two reds
                let style = if app.tick_count.is_multiple_of(2) {
                    Style::default().fg(ACCENT_RED).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(Color::Rgb(200, 80, 100))
                        .add_modifier(Modifier::BOLD)
                };
                (title_str, timer, style)
            }
        } else {
            let elapsed = info.elapsed_seconds as u64;
            let timer = if elapsed >= 3600 {
                format!(
                    " \u{23F1} {}h{:02}m ",
                    elapsed / 3600,
                    (elapsed % 3600) / 60
                )
            } else {
                format!(" \u{23F1} {}m ", elapsed / 60)
            };
            (
                title_str,
                timer,
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            )
        }
    } else {
        (String::new(), String::new(), Style::default())
    };

    // 3b: solid green for running task title (no 3-phase colour animation).
    let running_style = if app.running_task.is_some() {
        Style::default()
            .fg(ACCENT_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let header_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(FG_OVERLAY))
        .border_type(BorderType::Rounded);
    let inner = header_block.inner(area);
    f.render_widget(header_block, area);

    // Symbol legend: status icons + notation tokens + today's date on the right.
    let date_str = dodo::now_naive().format("%a %b %d").to_string();
    let dim = Style::default().fg(FG_OVERLAY);
    let legend_spans: Vec<Span> = vec![
        Span::raw("  "),
        Span::styled("\u{25CB}", Style::default().fg(FG_SUBTEXT)), // ○ pending
        Span::raw(" "),
        Span::styled("\u{25B6}", Style::default().fg(ACCENT_GREEN)), // ▶ running
        Span::raw(" "),
        Span::styled("\u{23F8}", Style::default().fg(ACCENT_YELLOW)), // ⏸ paused
        Span::raw(" "),
        Span::styled("\u{2713}", Style::default().fg(FG_OVERLAY)), // ✓ done
        Span::raw(" "),
        Span::styled("\u{21BB}", Style::default().fg(ACCENT_GREEN)), // ↻ recurring
        Span::raw("  "),
        Span::styled("+", Style::default().fg(ACCENT_MAUVE)), // +project
        Span::raw(" "),
        Span::styled("@", Style::default().fg(ACCENT_TEAL)), // @context
        Span::raw(" "),
        Span::styled("#", Style::default().fg(ACCENT_PEACH)), // #tag
        Span::raw(" "),
        Span::styled("~", Style::default().fg(FG_SUBTEXT)), // ~estimate
        Span::raw(" "),
        Span::styled("^", Style::default().fg(ACCENT_RED)), // ^deadline
        Span::raw(" "),
        Span::styled("=", Style::default().fg(ACCENT_BLUE)), // =scheduled
        Span::raw(" "),
        Span::styled("!", Style::default().fg(ACCENT_RED)), // !priority
        Span::raw("  "),
        Span::styled(date_str, dim),
        Span::raw(" "),
    ];
    let legend_width: u16 = legend_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(legend_width)])
        .split(inner);

    let mut left_spans = vec![
        Span::styled(
            " DODO ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(running_info, running_style),
        Span::styled(timer_info, timer_style),
    ];

    // 3c: compact sync indicator — `● synced`, `↻ syncing`, `⚠ sync err`; no key hints in header.
    match &app.sync_status {
        SyncStatus::Disabled => {}
        SyncStatus::Idle | SyncStatus::Synced(_) => {
            left_spans.push(Span::styled(
                " \u{25CF} ",
                Style::default().fg(ACCENT_GREEN),
            ));
            left_spans.push(Span::styled("synced", Style::default().fg(ACCENT_GREEN)));
        }
        SyncStatus::Syncing => {
            let icon = if app.tick_count.is_multiple_of(2) {
                "\u{21BB}"
            } else {
                "\u{21BA}"
            };
            left_spans.push(Span::styled(
                format!(" {} ", icon),
                Style::default().fg(ACCENT_YELLOW),
            ));
            left_spans.push(Span::styled("syncing", Style::default().fg(ACCENT_YELLOW)));
        }
        SyncStatus::Error(_) => {
            left_spans.push(Span::styled(" \u{26A0} ", Style::default().fg(ACCENT_RED)));
            left_spans.push(Span::styled("sync err", Style::default().fg(ACCENT_RED)));
        }
    }

    let left = Line::from(left_spans);
    f.render_widget(Paragraph::new(left), cols[0]);
    f.render_widget(Paragraph::new(Line::from(legend_spans)), cols[1]);
}

pub(super) fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys: Vec<(&str, &str)> = match app.tab {
        TuiTab::Tasks => match app.mode {
            AppMode::AddTask => vec![("Enter", "add"), ("Esc", "cancel")],
            AppMode::MoveTask => vec![("h/l", "select"), ("Enter", "move"), ("Esc", "cancel")],
            AppMode::Search => vec![("type", "filter"), ("Enter/Esc", "close")],
            _ => match app.tasks_view {
                TasksView::Panes => vec![
                    ("a", "add"),
                    ("s", "start"),
                    ("e", "elapsed"),
                    ("d", "done"),
                    ("n", "note"),
                    ("\u{21B5}", "edit"),
                    ("\u{232B}", "del"),
                    ("+/-", "day"),
                    ("o", "sort"),
                    ("v", "view"),
                    ("/", "find"),
                    ("?", "help"),
                    ("q", "quit"),
                ],
                TasksView::Daily => vec![
                    ("a", "add"),
                    ("s", "start"),
                    ("d", "done"),
                    ("e", "elapsed"),
                    ("H", "hide done"),
                    ("n", "note"),
                    ("+/-", "day"),
                    ("t", "today"),
                    ("v", "view"),
                    ("/", "find"),
                    ("?", "help"),
                    ("q", "quit"),
                ],
                TasksView::Weekly => vec![
                    ("a", "add"),
                    ("s", "start"),
                    ("d", "done"),
                    ("e", "elapsed"),
                    ("H", "hide done"),
                    ("+/-", "day"),
                    ("h/l", "day"),
                    ("[/]", "week"),
                    ("t", "today"),
                    ("v", "view"),
                    ("/", "find"),
                    ("?", "help"),
                    ("q", "quit"),
                ],
                TasksView::Calendar => match app.calendar_focus {
                    CalendarFocus::Grid => vec![
                        ("hjkl", "day"),
                        ("[/]", "month"),
                        ("t", "today"),
                        ("Tab", "list"),
                        ("v", "view"),
                        ("/", "find"),
                        ("?", "help"),
                        ("q", "quit"),
                    ],
                    CalendarFocus::TaskList => vec![
                        ("j/k", "task"),
                        ("s", "start"),
                        ("d", "done"),
                        ("e", "elapsed"),
                        ("H", "hide done"),
                        ("n", "note"),
                        ("\u{21B5}", "edit"),
                        ("Esc", "grid"),
                        ("v", "view"),
                        ("?", "help"),
                        ("q", "quit"),
                    ],
                },
            },
        },
        TuiTab::Recurring => match app.mode {
            AppMode::RecAddTemplate => vec![("Enter", "add"), ("Esc", "cancel")],
            _ => vec![
                ("a", "add"),
                ("e", "edit"),
                ("d", "del"),
                ("p", "pause"),
                ("R", "generate"),
                ("G", "last"),
                ("?", "help"),
                ("q", "quit"),
            ],
        },
        TuiTab::Report => vec![("h/l", "range"), ("?", "help"), ("q", "quit")],
        TuiTab::Settings => match app.mode {
            AppMode::EditConfig => vec![
                ("j/k \u{2193}\u{2191}", "navigate"),
                ("\u{21B5}", "edit"),
                ("t", "test"),
                ("Esc", "close"),
            ],
            AppMode::EditConfigField => vec![("\u{21B5}", "save"), ("Esc", "cancel")],
            _ => vec![
                ("j/k \u{2193}\u{2191}", "navigate"),
                ("u", "upload"),
                ("r", "restore"),
                ("d", "delete"),
                ("s", "sync"),
                ("e", "config"),
                ("?", "help"),
                ("q", "quit"),
            ],
        },
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

pub(super) fn draw_view_selector(f: &mut Frame, app: &App, area: Rect) {
    let views = [
        TasksView::Panes,
        TasksView::Daily,
        TasksView::Weekly,
        TasksView::Calendar,
    ];
    let mut left_spans: Vec<Span> = vec![Span::styled("  ", Style::default())];
    for (i, view) in views.iter().enumerate() {
        if i > 0 {
            left_spans.push(Span::styled(" | ", Style::default().fg(FG_OVERLAY)));
        }
        if *view == app.tasks_view {
            let label = if *view != TasksView::Panes && !app.show_done {
                format!("\u{25CF} {}\u{00AC}done  ", view.label())
            } else {
                format!("\u{25CF} {}  ", view.label())
            };
            left_spans.push(Span::styled(
                label,
                Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
            ));
        } else {
            left_spans.push(Span::styled(
                format!("  {}  ", view.label()),
                Style::default().fg(FG_OVERLAY),
            ));
        }
    }

    let right = if app.tasks_view != TasksView::Panes && !app.show_done {
        "H:show done  v:next  V:prev "
    } else {
        "H:hide done  v:next  V:prev "
    };
    // Bug 1d: use char count, not byte length, to handle multi-byte chars (e.g. ● = 3 bytes).
    let left_width: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (area.width as usize).saturating_sub(left_width + right.chars().count());
    left_spans.push(Span::raw(" ".repeat(pad)));
    left_spans.push(Span::styled(right, Style::default().fg(FG_OVERLAY)));

    f.render_widget(Paragraph::new(Line::from(left_spans)), area);
}

pub(super) fn draw_tasks_panes(f: &mut Frame, app: &mut App, area: Rect) {
    let today = dodo::today();
    let tomorrow = today + chrono::Duration::days(1);
    let week_end = today + chrono::Duration::days(7);

    let headers = [
        "LONG TERM".to_string(),
        format!(
            "THIS WEEK \u{2014} {}\u{2013}{}",
            tomorrow.format("%b%d"),
            week_end.format("%b%d")
        ),
        format!("TODAY \u{2014} {}", today.format("%b%d")),
        "DONE".to_string(),
    ];

    // Adaptive widths: active pane gets 40%, others share the remaining 60% equally (20% each).
    let active_pane = app.active_pane;
    let constraints: [Constraint; 4] = std::array::from_fn(|i| {
        if i == active_pane {
            Constraint::Percentage(40)
        } else {
            Constraint::Percentage(20)
        }
    });
    let pane_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    // Cache values derived from app before mutably borrowing individual panes.
    let frame_count = app.anim_frame();
    let sort_indices: [usize; 4] = std::array::from_fn(|i| app.panes[i].sort_index);
    let sort_asc: [bool; 4] = std::array::from_fn(|i| app.panes[i].sort_ascending);

    for i in 0..4 {
        let is_active = i == active_pane;
        let sl = sort_label(SORT_MODES[sort_indices[i]]);
        let arrow = if sort_asc[i] { "\u{2191}" } else { "\u{2193}" };
        let sort_display = format!("{}{}", sl, arrow);
        draw_pane(
            f,
            &mut app.panes[i],
            &headers[i],
            is_active,
            frame_count,
            &sort_display,
            pane_chunks[i],
        );
    }
}

pub(super) fn build_task_list_item(
    task: &Task,
    is_selected: bool,
    is_active: bool,
    frame_count: u64,
    width: u16,
    today: chrono::NaiveDate,
) -> ListItem<'static> {
    let is_running = task.status == TaskStatus::Running;
    let is_neon = is_running;
    let is_overdue = !is_running && task.status != TaskStatus::Done && is_task_overdue(task, today);
    let status_icon = match task.status {
        TaskStatus::Pending => "\u{25CB}",
        TaskStatus::Running => "\u{25B6}",
        TaskStatus::Paused => "\u{23F8}",
        TaskStatus::Done => "\u{2713}",
    };

    let num = task
        .num_id
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let notes_mark = match &task.notes {
        Some(n) if !n.is_empty() => " *",
        _ => "",
    };
    let recur_mark = if task.is_template || task.template_id.is_some() {
        Span::styled(
            "\u{21BB} ",
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::REVERSED),
        )
    } else {
        Span::raw("")
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
            Style::default().fg(ACCENT_RED).add_modifier(Modifier::BOLD),
            Style::default().fg(ACCENT_RED).add_modifier(Modifier::BOLD),
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

    // Marquee scroll for truncated titles
    let prefix_width = 7; // " NNN " (5) + "X " (2)
    let available_title_width = (width as usize).saturating_sub(prefix_width + 2); // 2 for border
    let full_title = format!("{}{}{}", recur_mark.content, task.title, notes_mark);
    let title_chars: Vec<char> = full_title.chars().collect();
    let display_title = if is_selected && is_active && !is_neon && available_title_width > 1 {
        // Marquee scroll: 3 second pause at start, brief pause at end, slower scrolling
        let scroll_len = title_chars.len().saturating_sub(available_title_width);
        if scroll_len > 0 {
            let pause_start: u64 = 180; // 3 seconds at 60fps
            let pause_end: u64 = 20;
            let total_cycle = pause_start + scroll_len as u64 + pause_end;
            let pos_in_cycle = (frame_count / 2) % total_cycle;
            let offset = if pos_in_cycle < pause_start {
                // Pause at start - show beginning of title
                0
            } else if pos_in_cycle < pause_start + scroll_len as u64 {
                // Scrolling phase
                (pos_in_cycle - pause_start) as usize
            } else {
                // Pause at end
                scroll_len
            };
            title_chars[offset..offset + available_title_width]
                .iter()
                .collect::<String>()
        } else {
            // Show full title if it fits
            full_title
        }
    } else if !is_selected && title_chars.len() > available_title_width && available_title_width > 1
    {
        // Truncated with ellipsis
        let mut s: String = title_chars[..available_title_width.saturating_sub(1)]
            .iter()
            .collect();
        s.push('\u{2026}');
        s
    } else {
        full_title
    };
    // Build line1 with styled recur_mark
    let line1 = Line::from(vec![
        Span::styled(format!(" {:>3} ", num), num_style),
        Span::styled(format!("{} ", status_icon), status_style),
        Span::styled(display_title, title_style),
    ]);

    let meta_spans = build_compact_meta(task, today);

    // Apply marquee to metadata row if needed.
    // Use char counts throughout — span content may contain multi-byte characters
    // (priority ■ is U+25A0 = 3 bytes; project/context names may be CJK/emoji).
    let display_meta = if is_selected && is_active && !is_neon && !meta_spans.is_empty() {
        let meta_char_width: u16 = meta_spans
            .iter()
            .map(|s| s.content.chars().count() as u16)
            .sum();
        let available_meta_width = width.saturating_sub(7);
        if available_meta_width > 1 {
            let scroll_len = meta_char_width.saturating_sub(available_meta_width);
            if scroll_len > 0 {
                let pause_frames: u64 = 20;
                let total_cycle = pause_frames + scroll_len as u64;
                let pos_in_cycle = (frame_count / 2) % total_cycle;
                let offset = if pos_in_cycle < scroll_len as u64 {
                    pos_in_cycle as usize
                } else {
                    scroll_len as usize
                };
                let mut result: Vec<Span> = Vec::new();
                let mut col: usize = 0; // char-column across all spans
                for span in meta_spans.iter() {
                    let span_chars: Vec<char> = span.content.chars().collect();
                    let span_len = span_chars.len();
                    if col + span_len <= offset {
                        col += span_len;
                        continue;
                    } else if col >= offset + available_meta_width as usize {
                        break;
                    }
                    let start = offset.saturating_sub(col);
                    let end = (offset + available_meta_width as usize - col).min(span_len);
                    if start < end {
                        let clipped: String = span_chars[start..end].iter().collect();
                        result.push(Span::styled(clipped, span.style));
                    }
                    col += span_len;
                }
                result
            } else {
                meta_spans
            }
        } else {
            meta_spans
        }
    } else {
        meta_spans
    };

    if is_neon {
        let neon_line1 = apply_neon(line1, frame_count, width);
        if display_meta.is_empty() {
            ListItem::new(vec![neon_line1])
        } else {
            // 4b: compute indent from prefix_width so it always aligns under the title.
            let mut line2_spans = vec![Span::raw(" ".repeat(prefix_width))];
            line2_spans.extend(display_meta);
            let line2 = Line::from(line2_spans);
            let neon_line2 = apply_neon(line2, frame_count, width);
            ListItem::new(vec![neon_line1, neon_line2])
        }
    } else if is_selected && is_active {
        // 14a: increased contrast for selected item (72/84/140 vs old 65/75/120).
        let bg = Color::Rgb(72, 84, 140);
        let item = if display_meta.is_empty() {
            ListItem::new(vec![line1])
        } else {
            let mut line2_spans = vec![Span::raw(" ".repeat(prefix_width))];
            line2_spans.extend(display_meta);
            let line2 = Line::from(line2_spans);
            ListItem::new(vec![line1, line2])
        };
        item.style(Style::default().bg(bg))
    } else {
        if display_meta.is_empty() {
            ListItem::new(vec![line1])
        } else {
            let mut line2_spans = vec![Span::raw(" ".repeat(prefix_width))];
            line2_spans.extend(display_meta);
            let line2 = Line::from(line2_spans);
            ListItem::new(vec![line1, line2])
        }
    }
}

// ── Daily View ──────────────────────────────────────────────────────

pub(super) fn draw_tasks_daily(f: &mut Frame, app: &mut App, area: Rect) {
    let today = dodo::today();

    let items: Vec<ListItem> = app
        .daily_entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| match entry {
            DailyEntry::Header {
                date,
                task_count,
                is_today,
            } => {
                let day_name = DAY_NAMES[date.weekday().num_days_from_sunday() as usize];
                let relative = if *is_today {
                    "Today"
                } else if *date == today + chrono::Duration::days(1) {
                    "Tomorrow"
                } else if *date == today - chrono::Duration::days(1) {
                    "Yesterday"
                } else {
                    ""
                };

                let date_str = date.format("%b %d").to_string();
                let label = if relative.is_empty() {
                    format!("{} \u{00B7} {}", date_str, day_name)
                } else {
                    format!("{} \u{00B7} {} \u{00B7} {}", date_str, relative, day_name)
                };
                let count_str = format!("  ({})", task_count);

                let style = if *is_today {
                    Style::default()
                        .fg(ACCENT_BLUE)
                        .add_modifier(Modifier::BOLD)
                } else if *date < today {
                    Style::default().fg(FG_OVERLAY)
                } else {
                    Style::default().fg(FG_SUBTEXT)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(label, style),
                    Span::styled(count_str, Style::default().fg(FG_OVERLAY)),
                ]))
            }
            DailyEntry::Task(task) => {
                let is_selected = idx == app.daily_cursor;
                build_task_list_item(task, is_selected, true, app.anim_frame(), area.width, today)
            }
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{258C} ");

    // Row-height-aware scroll: headers occupy 1 row, tasks occupy 2 rows.
    // Compute the offset that just keeps `cursor` visible in the viewport.
    let height = area.height as usize;
    let cursor = app.daily_cursor;

    // Helper: count rows consumed by entries[scroll..scroll+n] until we fill `height`.
    // Returns how many items fit starting from `start`.
    fn count_visible(entries: &[DailyEntry], start: usize, height: usize) -> usize {
        let mut rows = 0usize;
        let mut count = 0usize;
        for entry in entries.iter().skip(start) {
            let item_rows = if matches!(entry, DailyEntry::Task(_)) {
                2
            } else {
                1
            };
            if rows + item_rows > height {
                break;
            }
            rows += item_rows;
            count += 1;
        }
        count
    }

    // Scroll up: cursor above viewport — snap scroll to cursor.
    if cursor < app.daily_scroll {
        app.daily_scroll = cursor;
    }
    // Scroll down: cursor below viewport — advance scroll one step at a time.
    loop {
        let vis = count_visible(&app.daily_entries, app.daily_scroll, height);
        if vis == 0 || cursor < app.daily_scroll + vis {
            break;
        }
        app.daily_scroll += 1;
    }

    let mut list_state = ListState::default();
    *list_state.offset_mut() = app.daily_scroll;
    list_state.select(Some(cursor));
    f.render_stateful_widget(list, area, &mut list_state);

    // Scrollbar
    if app.daily_entries.len() > height {
        let mut scrollbar_state = ScrollbarState::new(app.daily_entries.len()).position(cursor);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(FG_OVERLAY));
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

// ── Weekly View ─────────────────────────────────────────────────────

pub(super) fn draw_tasks_weekly(f: &mut Frame, app: &App, area: Rect) {
    let today = dodo::today();

    // 9a: Show week date range above the tile grid.
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    let end_date = app.week_start_date + chrono::Duration::days(7);
    let week_label = format!(
        " Week of {} \u{2014} {}",
        app.week_start_date.format("%b %d"),
        end_date.format("%b %d")
    );
    f.render_widget(
        Paragraph::new(Span::styled(
            week_label,
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        )),
        outer[0],
    );

    let area = outer[1];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(rows[0]);
    let bot_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(rows[1]);

    let tiles: Vec<Rect> = top_cols.iter().chain(bot_cols.iter()).copied().collect();

    for i in 0..8 {
        let tile_date = app.week_start_date + chrono::Duration::days(i as i64);
        let is_today = tile_date == today;
        let is_active = i == app.weekly_active;
        draw_day_tile(
            f,
            &app.weekly_panes[i],
            tile_date,
            is_today,
            is_active,
            app.anim_frame(),
            tiles[i],
        );
    }
}

fn draw_day_tile(
    f: &mut Frame,
    pane: &PaneState,
    date: chrono::NaiveDate,
    is_today: bool,
    is_active: bool,
    frame_count: u64,
    area: Rect,
) {
    let day_name = DAY_NAMES[date.weekday().num_days_from_sunday() as usize];
    let header = format!("{} {}", day_name, date.day());

    let border_color = if is_active {
        ACCENT_BLUE
    } else if is_today {
        ACCENT_GREEN
    } else {
        FG_OVERLAY
    };

    let title_style = if is_active {
        Style::default()
            .fg(ACCENT_BLUE)
            .add_modifier(Modifier::BOLD)
    } else if is_today {
        Style::default()
            .fg(ACCENT_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(FG_SUBTEXT)
    };

    let block = Block::bordered()
        .title(Span::styled(format!(" {} ", header), title_style))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if pane.tasks.is_empty() {
        return;
    }

    let today_date = dodo::today();
    let selected_idx = pane.list_state.selected();

    let items: Vec<ListItem> = pane
        .tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| {
            let is_selected = is_active && selected_idx == Some(idx);
            build_task_list_item(
                task,
                is_selected,
                is_active,
                frame_count,
                inner.width,
                today_date,
            )
        })
        .collect();

    let list = List::new(items);
    let list = if is_active {
        list.highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("\u{258C} ")
    } else {
        list.highlight_symbol("  ")
    };

    f.render_stateful_widget(list, inner, &mut pane.list_state.clone());

    // Scrollbar
    let visible = inner.height as usize / 2;
    if pane.tasks.len() > visible && inner.height > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(pane.tasks.len()).position(pane.list_state.selected().unwrap_or(0));
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(FG_OVERLAY));
        f.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

// ── Calendar View ───────────────────────────────────────────────────

pub(super) fn draw_tasks_calendar(f: &mut Frame, app: &App, area: Rect) {
    let today = dodo::today();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Month/Year title
            Constraint::Length(1), // Day-of-week headers
            Constraint::Min(0),    // Grid
        ])
        .split(area);

    // Title line — centered title with right-aligned hints
    let month_names = [
        "",
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month_name = month_names[app.calendar_month as usize];
    let title = format!("\u{25C4} {} {} \u{25BA}", month_name, app.calendar_year);
    let hints = "[/]:month  t:today ";
    let hints_len = hints.len() as u16;
    let title_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(hints_len)])
        .split(layout[0]);

    // Center the month/year title above the weekday grid
    // The weekday grid spans the full width minus border padding
    let grid_width = layout[2].width;
    let title_width = title.len() as u16;
    let left_padding = (grid_width.saturating_sub(title_width)) / 2;

    let title_line = Line::from(vec![
        Span::raw(" ".repeat(left_padding as usize)),
        Span::styled(
            title,
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    // Bug 1b: Render to title_cols[0] (the title row), not layout[2] (the grid).
    f.render_widget(
        Paragraph::new(title_line).alignment(Alignment::Left),
        title_cols[0],
    );

    f.render_widget(
        Paragraph::new(Span::styled(hints, Style::default().fg(FG_OVERLAY))),
        title_cols[1],
    );

    // Day-of-week headers aligned with grid columns
    let dow_labels = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 7); 7])
        .split(layout[1]);
    for (i, label) in dow_labels.iter().enumerate() {
        f.render_widget(
            Paragraph::new(Span::styled(
                *label,
                Style::default().fg(FG_SUBTEXT).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            header_cols[i],
        );
    }

    // Compute calendar grid
    let first_of_month =
        chrono::NaiveDate::from_ymd_opt(app.calendar_year, app.calendar_month, 1).unwrap_or(today);
    let start_weekday = first_of_month.weekday().num_days_from_sunday(); // 0=Sun
    let days_in = days_in_month_cal(app.calendar_year, app.calendar_month);

    // Total cells: start_weekday + days_in, rounded up to multiple of 7
    let total_cells = start_weekday + days_in;
    let num_rows = total_cells.div_ceil(7);

    let grid_area = layout[2];
    if grid_area.height == 0 || num_rows == 0 {
        return;
    }

    let row_constraints: Vec<Constraint> = (0..num_rows)
        .map(|_| Constraint::Ratio(1, num_rows as u32))
        .collect();
    let grid_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(grid_area);

    let col_constraints = [Constraint::Ratio(1, 7); 7];

    for row in 0..num_rows as usize {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(grid_rows[row]);

        for col in 0..7 {
            let cell_idx = row * 7 + col;

            if (cell_idx as u32) < start_weekday || cell_idx as u32 >= start_weekday + days_in {
                // Empty cell (outside month)
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Rgb(40, 42, 54)));
                f.render_widget(block, cols[col]);
                continue;
            }

            let day_num = cell_idx as u32 - start_weekday + 1;
            let cell_date =
                chrono::NaiveDate::from_ymd_opt(app.calendar_year, app.calendar_month, day_num)
                    .unwrap_or(today);

            let is_cell_today = cell_date == today;
            let is_selected = cell_date == app.calendar_selected;
            let task_count = app
                .calendar_task_counts
                .get(&cell_date)
                .copied()
                .unwrap_or(0);

            let border_color = if is_selected && app.calendar_focus == CalendarFocus::TaskList {
                ACCENT_MAUVE
            } else if is_selected {
                ACCENT_BLUE
            } else if is_cell_today {
                ACCENT_GREEN
            } else {
                Color::Rgb(60, 62, 78)
            };

            let block = Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color));

            let inner = block.inner(cols[col]);
            f.render_widget(block, cols[col]);

            if inner.height == 0 || inner.width < 2 {
                continue;
            }

            // Line 1: date number
            let date_style = if is_cell_today {
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default()
                    .fg(ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_SUBTEXT)
            };
            let date_str = format!("{}", day_num);
            let count_str = if task_count > 0 {
                format!(" ({})", task_count)
            } else {
                String::new()
            };
            let date_line = Line::from(vec![
                Span::styled(date_str, date_style),
                Span::styled(count_str, Style::default().fg(FG_OVERLAY)),
            ]);
            let date_area = Rect::new(inner.x, inner.y, inner.width, 1);
            f.render_widget(Paragraph::new(date_line), date_area);

            // Lines 2+: task entries (shown in ALL cells, not just selected)
            let cell_tasks: Option<&Vec<Task>> = if is_selected {
                if app.calendar_tasks.is_empty() {
                    None
                } else {
                    Some(&app.calendar_tasks)
                }
            } else {
                app.calendar_tasks_by_date.get(&cell_date)
            };

            if let Some(tasks) = cell_tasks {
                let max_tasks = (inner.height as usize).saturating_sub(1);
                let has_more = tasks.len() > max_tasks;
                let show = if has_more {
                    max_tasks.saturating_sub(1)
                } else {
                    max_tasks
                };

                for (ti, task) in tasks.iter().take(show).enumerate() {
                    let y_offset = 1 + ti as u16;
                    if y_offset >= inner.height {
                        break;
                    }

                    let icon = match task.status {
                        TaskStatus::Running => "\u{25B6}",
                        TaskStatus::Paused => "\u{23F8}",
                        TaskStatus::Done => "\u{2713}",
                        TaskStatus::Pending => "\u{25CB}",
                    };

                    let max_title_len = (inner.width as usize).saturating_sub(2);
                    let title: String = task.title.chars().take(max_title_len).collect();

                    let is_task_selected = is_selected
                        && app.calendar_focus == CalendarFocus::TaskList
                        && ti == app.calendar_task_selected;

                    let style = if is_task_selected {
                        Style::default().fg(FG_TEXT).bg(Color::Rgb(65, 75, 120))
                    } else {
                        calendar_task_style(task, today)
                    };

                    let task_line = Line::from(Span::styled(format!("{}{}", icon, title), style));
                    let task_area = Rect::new(inner.x, inner.y + y_offset, inner.width, 1);
                    f.render_widget(Paragraph::new(task_line), task_area);
                }

                if has_more {
                    let remaining = tasks.len() - show;
                    let more_y = inner.y + 1 + show as u16;
                    if more_y < inner.y + inner.height {
                        let more_line = Line::from(Span::styled(
                            format!("+{} more", remaining),
                            Style::default().fg(FG_OVERLAY),
                        ));
                        let more_area = Rect::new(inner.x, more_y, inner.width, 1);
                        f.render_widget(Paragraph::new(more_line), more_area);
                    }
                }
            }
        }
    }
}

fn calendar_task_style(task: &Task, today: chrono::NaiveDate) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(ACCENT_GREEN),
        TaskStatus::Done => Style::default()
            .fg(ACCENT_TEAL)
            .add_modifier(Modifier::DIM | Modifier::CROSSED_OUT),
        TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
        TaskStatus::Pending => {
            if task.deadline.map(|d| d < today).unwrap_or(false) {
                Style::default().fg(ACCENT_RED)
            } else if task.priority.unwrap_or(0) >= 3 {
                Style::default().fg(ACCENT_RED)
            } else if task.priority.unwrap_or(0) >= 2 {
                Style::default().fg(ACCENT_YELLOW)
            } else {
                Style::default().fg(FG_SUBTEXT)
            }
        }
    }
}

fn days_in_month_cal(year: i32, month: u32) -> u32 {
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

pub(super) fn draw_recurring_tab(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .title(Span::styled(
            " RECURRING ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_BLUE));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.templates.is_empty() {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No recurring templates.",
                Style::default().fg(FG_SUBTEXT),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'a' to add one (e.g., standup *daily +work ~15m)",
                Style::default().fg(FG_OVERLAY),
            )),
        ])
        .alignment(Alignment::Center);
        f.render_widget(msg, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .templates
        .iter()
        .enumerate()
        .map(|(idx, template)| {
            let is_selected = idx == app.template_selected;
            let is_paused = template.status == TaskStatus::Paused;

            let icon = if is_paused { "\u{23F8}" } else { "\u{21BB}" };
            let icon_style = if is_paused {
                Style::default().fg(ACCENT_YELLOW)
            } else {
                Style::default().fg(ACCENT_GREEN)
            };

            let recurrence = template.recurrence.as_deref().unwrap_or("?");
            // 10a: show human-readable pattern instead of raw *pattern string.
            let recurrence_label = humanize_recurrence(recurrence);

            let last_date = app.db.template_last_date(&template.id).ok().flatten();
            let last_str = last_date
                .map(|d| d.format("%b %d").to_string())
                .unwrap_or_else(|| "-".into());
            let next_str = if is_paused {
                "(paused)".to_string()
            } else {
                last_date
                    .and_then(|d| dodo::notation::next_occurrence(recurrence, d))
                    .map(|d| d.format("%b %d").to_string())
                    .unwrap_or_else(|| "-".into())
            };

            let num = template
                .num_id
                .map(|n| n.to_string())
                .unwrap_or_else(|| "?".into());

            let title_style = if is_paused {
                Style::default().fg(FG_OVERLAY)
            } else {
                Style::default().fg(FG_TEXT)
            };

            // For recurring templates, only show estimate/project/context —
            // the scheduled date is always "in the past" (it's the base anchor)
            // and would show misleading red highlights.
            let meta = build_template_meta(template);

            let line1 = Line::from(vec![
                Span::styled(format!(" {:>3} ", num), Style::default().fg(FG_SUBTEXT)),
                Span::styled(format!("{} ", icon), icon_style),
                Span::styled(
                    format!("{:<16} ", recurrence_label),
                    Style::default().fg(ACCENT_PEACH),
                ),
                Span::styled(template.title.clone(), title_style),
            ]);

            let mut line2_spans = vec![Span::raw("                   ")];
            line2_spans.push(Span::styled(
                format!("last:{}", last_str),
                Style::default().fg(FG_SUBTEXT),
            ));
            line2_spans.push(Span::raw("  "));
            let next_style = if is_paused {
                Style::default().fg(ACCENT_YELLOW)
            } else {
                Style::default().fg(ACCENT_TEAL)
            };
            line2_spans.push(Span::styled(format!("next:{}", next_str), next_style));
            if !meta.is_empty() {
                line2_spans.push(Span::raw("  "));
                line2_spans.extend(meta);
            }
            let line2 = Line::from(line2_spans);

            let item = ListItem::new(vec![line1, line2]);
            if is_selected {
                // 14a: improved contrast for selected item.
                item.style(Style::default().bg(Color::Rgb(72, 84, 140)))
            } else {
                item
            }
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{258C} ");

    let mut list_state = ListState::default();
    list_state.select(Some(app.template_selected));
    f.render_stateful_widget(list, inner, &mut list_state);
}

pub(super) fn draw_rec_add_bar(f: &mut Frame, app: &App) {
    let area = f.area();
    // Bottom bar
    let bar_area = Rect::new(0, area.height.saturating_sub(3), area.width, 3);
    f.render_widget(Clear, bar_area);

    let block = Block::bordered()
        .title(Span::styled(
            " Add Recurring (title *pattern +proj ~est) ",
            Style::default().fg(ACCENT_BLUE),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_BLUE));

    let inner = block.inner(bar_area);
    f.render_widget(block, bar_area);

    let input_text = format!("{}\u{2588}", app.rec_add_input);
    f.render_widget(
        Paragraph::new(input_text).style(Style::default().fg(FG_TEXT)),
        inner,
    );
}

pub(super) fn draw_rec_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 20, f.area());
    f.render_widget(Clear, area);

    let title = app
        .templates
        .get(app.template_selected)
        .map(|t| t.title.as_str())
        .unwrap_or("?");

    let block = Block::bordered()
        .title(Span::styled(
            " Delete Recurring ",
            Style::default().fg(ACCENT_RED).add_modifier(Modifier::BOLD),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_RED));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Delete '{}'?", title),
            Style::default().fg(FG_TEXT),
        )),
        Line::from(Span::styled(
            "Active instance will also be deleted.",
            Style::default().fg(FG_SUBTEXT),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" y ", Style::default().fg(FG_TEXT).bg(ACCENT_RED)),
            Span::styled(" confirm  ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(" n ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
            Span::styled(" cancel", Style::default().fg(FG_SUBTEXT)),
        ]),
    ];
    f.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

pub(super) fn draw_report_tab(f: &mut Frame, app: &App, area: Rect) {
    let report = match &app.report {
        Some(r) => r,
        None => {
            let msg = Paragraph::new("Loading report...").style(Style::default().fg(FG_OVERLAY));
            f.render_widget(msg, area);
            return;
        }
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    // Two-line header: row 0 = range type selector, row 1 = period label + time nav
    let header_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(layout[0]);

    // Range selector (h/l to change type)
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
    let mut all_spans = vec![Span::styled("  Range: ", Style::default().fg(FG_OVERLAY))];
    for (i, s) in range_spans.into_iter().enumerate() {
        all_spans.push(s);
        if i < ranges.len() - 1 {
            all_spans.push(Span::styled("  ", Style::default()));
        }
    }
    all_spans.push(Span::styled("  h/l:type", Style::default().fg(FG_OVERLAY)));
    f.render_widget(Paragraph::new(Line::from(all_spans)), header_rows[0]);

    // Period label row: shows the actual date range and [/] nav hints
    let period_label = app.report_period_label();
    let nav_hint = if matches!(app.report_range, ReportRange::All) {
        String::new()
    } else if app.report_offset == 0 {
        "  [ J:prev period".to_string()
    } else {
        format!("  [ J:prev   ] K:next  ({}x back)", app.report_offset)
    };
    let period_line = Line::from(vec![
        Span::styled("  \u{25CF} ", Style::default().fg(ACCENT_BLUE)),
        Span::styled(
            period_label,
            Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(nav_hint, Style::default().fg(FG_OVERLAY)),
    ]);
    f.render_widget(Paragraph::new(period_line), header_rows[1]);

    // 3-row layout: summary cards, charts, lists
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),      // Row 1: compact summary
            Constraint::Percentage(45), // Row 2: bar charts
            Constraint::Min(0),         // Row 3: project + done lists
        ])
        .split(layout[1]);

    // ── Row 1: Compact Summary Cards ──────────────────────────────────
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

    let best_hour_str = report
        .by_hour
        .iter()
        .max_by_key(|(_, s)| *s)
        .map(|(h, _)| format!("{:02}:00", h))
        .unwrap_or_else(|| "-".to_string());
    let best_day_str = report
        .by_weekday
        .iter()
        .max_by_key(|(_, s)| *s)
        .map(|(d, _)| DAY_NAMES.get(*d as usize).unwrap_or(&"?").to_string())
        .unwrap_or_else(|| "-".to_string());

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

    let summary_inner = summary_block.inner(rows[0]);
    f.render_widget(summary_block, rows[0]);

    let summary_cols = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1)])
        .split(summary_inner);

    let streak_style = if report.streak >= 7 {
        Style::default()
            .fg(ACCENT_GREEN)
            .add_modifier(Modifier::BOLD)
    } else if report.streak >= 3 {
        Style::default().fg(ACCENT_YELLOW)
    } else {
        Style::default().fg(FG_SUBTEXT)
    };

    let summary_line1 = Line::from(vec![
        Span::styled(
            format!(" \u{2713}{} done", report.tasks_done),
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("\u{23F1} {}", format_dur(report.total_seconds)),
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{} active", report.active_days),
            Style::default().fg(ACCENT_YELLOW),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!(
                "{} day{}",
                report.streak,
                if report.streak == 1 { "" } else { "s" }
            ),
            streak_style,
        ),
    ]);
    let summary_line2 = Line::from(vec![
        Span::styled(
            format!(" avg/task: {}", format_dur(avg_per_task)),
            Style::default().fg(FG_SUBTEXT),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("avg/day: {}", format_dur(avg_per_day)),
            Style::default().fg(FG_SUBTEXT),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("best hour: {}", best_hour_str),
            Style::default().fg(FG_SUBTEXT),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("best day: {}", best_day_str),
            Style::default().fg(FG_SUBTEXT),
        ),
    ]);
    f.render_widget(
        Paragraph::new(vec![summary_line1, summary_line2]),
        summary_cols[0],
    );

    // Completion Gauge
    let done_ratio = if report.total_tasks > 0 {
        (report.tasks_done as f64 / report.total_tasks as f64).min(1.0)
    } else {
        0.0
    };
    let done_gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT_GREEN).bg(Color::Rgb(40, 42, 54)))
        .ratio(done_ratio)
        .label(format!("{}/{} done", report.tasks_done, report.total_tasks))
        .use_unicode(true);
    f.render_widget(done_gauge, summary_cols[1]);

    // ── Row 2: BarChart widgets ───────────────────────────────────────
    let chart_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    // Left: Weekly Activity BarChart
    let mut weekday_data = [0u64; 7];
    for (dow, secs) in &report.by_weekday {
        if (*dow as usize) < 7 {
            weekday_data[*dow as usize] = *secs as u64;
        }
    }
    let weekly_bars: Vec<Bar> = weekday_data
        .iter()
        .enumerate()
        .map(|(i, &secs)| {
            let mins = secs / 60;
            let label = if mins >= 60 {
                format!("{}h", mins / 60)
            } else if mins > 0 {
                format!("{}m", mins)
            } else {
                String::new()
            };
            Bar::default()
                .value(secs)
                .label(Line::from(DAY_NAMES[i]))
                .text_value(label)
                .style(Style::default().fg(ACCENT_TEAL))
        })
        .collect();

    let weekly_chart = BarChart::default()
        .block(
            Block::bordered()
                .title(Span::styled(
                    " Weekly Activity ",
                    Style::default()
                        .fg(ACCENT_TEAL)
                        .add_modifier(Modifier::BOLD),
                ))
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(FG_OVERLAY)),
        )
        .data(BarGroup::default().bars(&weekly_bars))
        .bar_width(3)
        .bar_gap(1)
        .bar_style(Style::default().fg(ACCENT_TEAL))
        .value_style(Style::default().fg(FG_SUBTEXT));
    f.render_widget(weekly_chart, chart_cols[0]);

    // Right: Hourly Distribution BarChart
    let mut hour_data = [0u64; 24];
    for (hour, secs) in &report.by_hour {
        if (*hour as usize) < 24 {
            hour_data[*hour as usize] = *secs as u64;
        }
    }
    let hourly_bars: Vec<Bar> = hour_data
        .iter()
        .enumerate()
        .map(|(i, &secs)| {
            let label = if i % 4 == 0 {
                format!("{}", i)
            } else {
                String::new()
            };
            Bar::default()
                .value(secs)
                .label(Line::from(label))
                .text_value(String::new())
                .style(Style::default().fg(ACCENT_PEACH))
        })
        .collect();

    let hourly_chart = BarChart::default()
        .block(
            Block::bordered()
                .title(Span::styled(
                    " Hours of Day ",
                    Style::default()
                        .fg(ACCENT_PEACH)
                        .add_modifier(Modifier::BOLD),
                ))
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(FG_OVERLAY)),
        )
        .data(BarGroup::default().bars(&hourly_bars))
        .bar_width(1)
        .bar_gap(0)
        .bar_style(Style::default().fg(ACCENT_PEACH))
        .value_style(Style::default().fg(FG_SUBTEXT));
    f.render_widget(hourly_chart, chart_cols[1]);

    // ── Row 3: Project list + Done tasks ──────────────────────────────
    let list_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(rows[2]);

    // Left: Time by Project with inline bars
    let mut proj_lines: Vec<Line> = vec![];
    for (project, secs) in &report.by_project {
        let pct = if report.total_seconds > 0 {
            (*secs as f64 / report.total_seconds as f64 * 100.0) as u64
        } else {
            0
        };
        let bar_width = 12usize;
        let filled = if report.total_seconds > 0 {
            (*secs as f64 / report.total_seconds as f64 * bar_width as f64) as usize
        } else {
            0
        };
        let bar_filled: String = "\u{2588}".repeat(filled);
        let bar_empty: String = "\u{2591}".repeat(bar_width.saturating_sub(filled));
        proj_lines.push(Line::from(vec![
            Span::styled(
                format!("  +{:<10}", project),
                Style::default().fg(ACCENT_MAUVE),
            ),
            Span::styled(bar_filled, Style::default().fg(ACCENT_MAUVE)),
            Span::styled(bar_empty, Style::default().fg(Color::Rgb(60, 62, 80))),
            Span::styled(
                format!(" {:>6}", format_dur(*secs)),
                Style::default().fg(FG_TEXT),
            ),
            Span::styled(format!(" {:>3}%", pct), Style::default().fg(FG_OVERLAY)),
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
    f.render_widget(proj, list_cols[0]);

    // Right: Done tasks
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
    f.render_widget(done, list_cols[1]);
}

// ── Pane Drawing ─────────────────────────────────────────────────────

/// Pastel rainbow hue from 0.0–1.0, returns soft RGB.
pub(super) fn pastel_from_hue(hue: f64) -> Color {
    let h = ((hue % 1.0) + 1.0) % 1.0;
    let (r, g, b) = match (h * 6.0) as u8 {
        0 => (1.0, h * 6.0, 0.0),
        1 => (2.0 - h * 6.0, 1.0, 0.0),
        2 => (0.0, 1.0, h * 6.0 - 2.0),
        3 => (0.0, 4.0 - h * 6.0, 1.0),
        4 => (h * 6.0 - 4.0, 0.0, 1.0),
        _ => (1.0, 0.0, 6.0 - h * 6.0),
    };
    // Blend toward white for pastel: base ~140, range ~115
    Color::Rgb(
        (140.0 + r * 115.0) as u8,
        (140.0 + g * 115.0) as u8,
        (140.0 + b * 115.0) as u8,
    )
}

/// Apply inverted pastel rainbow sweep: dark bold text on bright animated background.
pub(super) fn apply_neon(line: Line<'static>, frame_count: u64, width: u16) -> Line<'static> {
    let sigma = width as f64 * 0.25;
    let period = width as f64 + sigma * 4.0;
    let wave_center = (frame_count as f64 * 0.8) % period - sigma * 2.0;
    let hue_offset = frame_count as f64 * 0.008;

    let dark_fg = Color::Rgb(30, 30, 46);
    let text_style = Style::default().fg(dark_fg).add_modifier(Modifier::BOLD);

    let mut result: Vec<Span<'static>> = Vec::new();
    let mut x: f64 = 0.0;

    for span in line.spans {
        for ch in span.content.chars() {
            let d = x - wave_center;
            let intensity = (-0.5 * (d / sigma).powi(2)).exp();
            let hue = hue_offset + x / width as f64;
            let Color::Rgb(pr, pg, pb) = pastel_from_hue(hue) else {
                unreachable!()
            };
            let bg = Color::Rgb(
                (80.0 + intensity * (pr as f64 - 80.0)) as u8,
                (85.0 + intensity * (pg as f64 - 85.0)) as u8,
                (100.0 + intensity * (pb as f64 - 100.0)) as u8,
            );
            result.push(Span::styled(ch.to_string(), text_style.bg(bg)));
            x += 1.0;
        }
    }

    // Fill remaining row width with the glow
    while (x as u16) < width.saturating_sub(2) {
        let d = x - wave_center;
        let intensity = (-0.5 * (d / sigma).powi(2)).exp();
        let hue = hue_offset + x / width as f64;
        let Color::Rgb(pr, pg, pb) = pastel_from_hue(hue) else {
            unreachable!()
        };
        let bg = Color::Rgb(
            (80.0 + intensity * (pr as f64 - 80.0)) as u8,
            (85.0 + intensity * (pg as f64 - 85.0)) as u8,
            (100.0 + intensity * (pb as f64 - 100.0)) as u8,
        );
        result.push(Span::styled(" ", text_style.bg(bg)));
        x += 1.0;
    }

    Line::from(result)
}

fn build_sync_status_line(app: &App) -> Line<'static> {
    let url = app.sync_config.turso_url.clone().unwrap_or_default();
    match &app.sync_status {
        SyncStatus::Disabled => Line::from(vec![
            Span::styled("Sync: ", Style::default().fg(FG_SUBTEXT)),
            Span::styled("\u{25CB} not configured", Style::default().fg(FG_OVERLAY)),
        ]),
        SyncStatus::Idle => Line::from(vec![
            Span::styled("Sync: ", Style::default().fg(FG_SUBTEXT)),
            Span::styled("\u{25CF} ", Style::default().fg(ACCENT_GREEN)),
            Span::styled("enabled  ", Style::default().fg(ACCENT_GREEN)),
            Span::styled(url, Style::default().fg(ACCENT_TEAL)),
        ]),
        SyncStatus::Syncing => Line::from(vec![
            Span::styled("Sync: ", Style::default().fg(FG_SUBTEXT)),
            Span::styled("\u{21BB} syncing...", Style::default().fg(ACCENT_YELLOW)),
        ]),
        SyncStatus::Synced(instant) => {
            let elapsed = instant.elapsed().as_secs();
            let time_str = if elapsed < 60 {
                format!("{}s ago", elapsed)
            } else {
                format!("{}m ago", elapsed / 60)
            };
            Line::from(vec![
                Span::styled("Sync: ", Style::default().fg(FG_SUBTEXT)),
                Span::styled("\u{25CF} ", Style::default().fg(ACCENT_GREEN)),
                Span::styled("enabled  ", Style::default().fg(ACCENT_GREEN)),
                Span::styled(url, Style::default().fg(ACCENT_TEAL)),
                Span::styled(format!("  ({})", time_str), Style::default().fg(FG_SUBTEXT)),
            ])
        }
        SyncStatus::Error(msg) => Line::from(vec![
            Span::styled("Sync: ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(
                format!("\u{26A0} error: {}", msg),
                Style::default().fg(ACCENT_RED),
            ),
        ]),
    }
}

pub(super) fn draw_backup_tab(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FG_OVERLAY))
        .title(Span::styled(
            " Settings ",
            Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
        ))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !app.backup_config.is_ready() {
        // Unconfigured state: bordered setup block
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Sync status
                Constraint::Length(1), // Spacer
                Constraint::Min(0),    // Setup block
            ])
            .split(inner);

        f.render_widget(Paragraph::new(build_sync_status_line(app)), chunks[0]);

        let setup_block = Block::bordered()
            .title(Span::styled(
                " Setup ",
                Style::default()
                    .fg(ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT_YELLOW))
            .padding(Padding::horizontal(1));
        let setup_inner = setup_block.inner(chunks[2]);
        f.render_widget(setup_block, chunks[2]);

        let msg = vec![
            Line::from(""),
            Line::from(Span::styled(
                "S3-compatible backup is not configured yet.",
                Style::default().fg(FG_SUBTEXT),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "You need: endpoint, bucket, access key, and secret key.",
                Style::default().fg(FG_SUBTEXT),
            )),
            Line::from(Span::styled(
                "Prefix and region are optional (R2/MinIO don't need region).",
                Style::default().fg(FG_OVERLAY),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(FG_SUBTEXT)),
                Span::styled(" e ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
                Span::styled(" to open config editor", Style::default().fg(FG_SUBTEXT)),
            ]),
        ];
        f.render_widget(Paragraph::new(msg), setup_inner);
        return;
    }

    // Configured state: 4-chunk layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // [0] Sync status line
            Constraint::Length(5), // [1] Summary stats block
            Constraint::Length(1), // [2] Toast message
            Constraint::Min(0),    // [3] Backup list
        ])
        .split(inner);

    // [0] Sync status line
    f.render_widget(Paragraph::new(build_sync_status_line(app)), chunks[0]);

    // [1] Summary stats block
    let summary_block = Block::bordered()
        .title(Span::styled(
            " Summary ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FG_OVERLAY));
    let summary_inner = summary_block.inner(chunks[1]);
    f.render_widget(summary_block, chunks[1]);

    if !app.backup_entries.is_empty() {
        let total_size: i64 = app.backup_entries.iter().map(|e| e.size).sum();
        let endpoint = app.backup_config.endpoint.as_deref().unwrap_or("?");
        let bucket = app.backup_config.bucket.as_deref().unwrap_or("?");

        let latest_age = dodo::backup::format_age(&app.backup_entries[0].timestamp);
        let schedule = app.backup_config.schedule_days;
        let max = app.backup_config.max_backups;

        // Line 1: count + total size + endpoint/bucket
        let line1 = Line::from(vec![
            Span::styled(
                format!(
                    " {} backup{}",
                    app.backup_entries.len(),
                    if app.backup_entries.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
                Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  \u{2022}  ", Style::default().fg(FG_OVERLAY)),
            Span::styled(
                dodo::backup::format_size(total_size),
                Style::default().fg(FG_TEXT),
            ),
            Span::styled("          ", Style::default().fg(FG_OVERLAY)),
            Span::styled(
                format!("{}/{}", endpoint.trim_end_matches('/'), bucket),
                Style::default().fg(FG_OVERLAY),
            ),
        ]);

        // Line 2: latest + schedule + max
        let line2 = Line::from(vec![
            Span::styled(" Latest: ", Style::default().fg(FG_SUBTEXT)),
            Span::styled(&latest_age, Style::default().fg(ACCENT_TEAL)),
            Span::styled("  \u{2022}  ", Style::default().fg(FG_OVERLAY)),
            Span::styled(
                format!("Schedule: {}d", schedule),
                Style::default().fg(FG_SUBTEXT),
            ),
            Span::styled("  \u{2022}  ", Style::default().fg(FG_OVERLAY)),
            Span::styled(format!("Max: {}", max), Style::default().fg(FG_SUBTEXT)),
        ]);

        let summary_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(summary_inner);

        f.render_widget(Paragraph::new(line1), summary_chunks[0]);
        f.render_widget(Paragraph::new(line2), summary_chunks[1]);

        // Line 3: LineGauge showing days since last backup vs schedule
        let days_since = (chrono::Utc::now() - app.backup_entries[0].timestamp)
            .num_days()
            .max(0) as f64;
        let schedule_f = schedule as f64;
        let ratio = if schedule_f > 0.0 {
            (days_since / schedule_f).min(1.0)
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
        f.render_widget(gauge, summary_chunks[2]);
    } else {
        f.render_widget(
            Paragraph::new(Span::styled(
                " No backup data yet",
                Style::default().fg(FG_OVERLAY),
            )),
            summary_inner,
        );
    }

    // [2] Toast message
    if let Some(ref msg) = app.backup_status_msg {
        let color =
            if msg.starts_with("Error") || msg.contains("failed") || msg.starts_with("\u{2717}") {
                ACCENT_RED
            } else {
                ACCENT_GREEN
            };
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(" {}", msg),
                Style::default().fg(color),
            )),
            chunks[2],
        );
    }

    // [3] Backup list
    if app.backup_entries.is_empty() {
        let empty_msg = vec![
            Line::from(""),
            Line::from(Span::styled(
                "No backups yet",
                Style::default().fg(FG_SUBTEXT),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(FG_OVERLAY)),
                Span::styled(" u ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
                Span::styled(
                    " to create your first backup",
                    Style::default().fg(FG_OVERLAY),
                ),
            ]),
        ];
        f.render_widget(
            Paragraph::new(empty_msg).alignment(Alignment::Center),
            chunks[3],
        );
        return;
    }

    let items: Vec<ListItem> = app
        .backup_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let age = dodo::backup::format_age(&entry.timestamp);
            let size = dodo::backup::format_size(entry.size);
            let date_str = entry.timestamp.format("%b %d %H:%M").to_string();
            let is_selected = i == app.backup_selected;
            let num = format!("{:>3}", i + 1);

            // Line 1: NUM  ■  display_name
            let line1 = Line::from(vec![
                Span::styled(
                    if is_selected { "\u{258C}" } else { " " },
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(format!("{} ", num), Style::default().fg(FG_SUBTEXT)),
                Span::styled(
                    "\u{25A0} ",
                    Style::default().fg(if is_selected { ACCENT_BLUE } else { FG_OVERLAY }),
                ),
                Span::styled(
                    entry.display_name.clone(),
                    Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
                ),
            ]);

            // Line 2: age  •  size  •  date
            let line2 = Line::from(vec![
                Span::raw("      "),
                Span::styled(age, Style::default().fg(ACCENT_TEAL)),
                Span::styled("  \u{2022}  ", Style::default().fg(FG_OVERLAY)),
                Span::styled(size, Style::default().fg(FG_SUBTEXT)),
                Span::styled("  \u{2022}  ", Style::default().fg(FG_OVERLAY)),
                Span::styled(date_str, Style::default().fg(FG_OVERLAY)),
            ]);

            let item = ListItem::new(vec![line1, line2]);
            if is_selected {
                item.style(Style::default().bg(Color::Rgb(65, 75, 120)))
            } else {
                item
            }
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{258C} ");

    let mut list_state = ListState::default();
    list_state.select(Some(app.backup_selected));
    f.render_stateful_widget(list, chunks[3], &mut list_state);

    // Scrollbar when entries exceed visible area (2 lines per entry)
    let visible_approx = chunks[3].height as usize / 2;
    if app.backup_entries.len() > visible_approx && chunks[3].height > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(app.backup_entries.len()).position(app.backup_selected);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(FG_OVERLAY));
        f.render_stateful_widget(scrollbar, chunks[3], &mut scrollbar_state);
    }
}

pub(super) fn draw_pane(
    f: &mut Frame,
    pane: &mut PaneState,
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
    let (elapsed, estimate, done, total, on_time, overdue) = pane.stats();
    let stats_spans = build_pane_stats(elapsed, estimate, done, total, on_time, overdue);
    let right_text = format!("{} ", sort_label_str);
    let left_width = 1 + stats_spans
        .iter()
        .map(|s| s.content.chars().count())
        .sum::<usize>();
    let right_width = right_text.chars().count();
    let pad = (chunks[0].width as usize).saturating_sub(left_width + right_width);
    let mut stats_spans_full = vec![Span::raw(" ")];
    stats_spans_full.extend(stats_spans);
    stats_spans_full.push(Span::raw(" ".repeat(pad)));
    stats_spans_full.push(Span::styled(right_text, Style::default().fg(FG_OVERLAY)));
    let stats_line = Line::from(stats_spans_full);
    let stats_area = Rect::new(chunks[0].x, chunks[0].y, chunks[0].width, 1);
    f.render_widget(Paragraph::new(stats_line), stats_area);

    // LineGauge progress bar
    // For DONE pane: show net overrun (red) or underrun (green) relative to total estimate.
    // For other panes: show elapsed-vs-estimate utilisation.
    let is_done_pane = done > 0 && done == total;
    let (ratio, gauge_color) = if is_done_pane && estimate > 0 {
        let net = elapsed - estimate; // positive = over, negative = under
        if net >= 0 {
            let r = (net as f64 / estimate as f64).min(1.0);
            (r, ACCENT_RED)
        } else {
            let r = ((-net) as f64 / estimate as f64).min(1.0);
            (r, ACCENT_GREEN)
        }
    } else if estimate > 0 {
        let r = (elapsed as f64 / estimate as f64).min(1.0);
        let c = if r >= 1.0 {
            ACCENT_RED
        } else if r >= 0.75 {
            ACCENT_YELLOW
        } else {
            ACCENT_GREEN
        };
        (r, c)
    } else {
        (0.0, ACCENT_GREEN)
    };
    let gauge = LineGauge::default()
        .filled_style(Style::default().fg(gauge_color))
        .unfilled_style(Style::default().fg(Color::Rgb(40, 42, 54)))
        .ratio(ratio);
    let gauge_area = Rect::new(chunks[0].x, chunks[0].y + 1, chunks[0].width, 1);
    f.render_widget(gauge, gauge_area);

    // Task list area
    let list_area = chunks[1];
    let today = dodo::today();

    // 4d: Empty pane shows informative message.
    if pane.tasks.is_empty() {
        let msg = Line::from(vec![
            Span::styled("(empty) ", Style::default().fg(FG_OVERLAY)),
            Span::styled("a", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
            Span::styled(":add task", Style::default().fg(FG_OVERLAY)),
        ]);
        let center_y = list_area.height / 2;
        if center_y > 0 {
            let msg_area = Rect::new(list_area.x, list_area.y + center_y, list_area.width, 1);
            f.render_widget(Paragraph::new(msg).alignment(Alignment::Center), msg_area);
        }
        return;
    }

    // Bug 1e: delegate to build_task_list_item instead of duplicating logic here.
    let selected_idx = pane.list_state.selected();
    let items: Vec<ListItem> = pane
        .tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| {
            let is_selected = is_active && selected_idx == Some(idx);
            build_task_list_item(
                task,
                is_selected,
                is_active,
                frame_count,
                list_area.width,
                today,
            )
        })
        .collect();

    let list = List::new(items);
    let list = if is_active {
        list.highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("\u{258C} ")
    } else {
        list.highlight_symbol("  ")
    };

    // Zero scroll margin — cursor can sit at the very edge of the viewport.
    // Each task occupies 2 rows (title + meta), so halve height for item count.
    let visible_items = (list_area.height as usize) / 2;
    let margin = 0usize;
    let cursor = pane.list_state.selected().unwrap_or(0);
    let current_offset = *pane.list_state.offset_mut();
    let new_offset = if visible_items == 0 {
        0
    } else if cursor < current_offset + margin {
        cursor.saturating_sub(margin)
    } else if cursor + margin + 1 > current_offset + visible_items {
        (cursor + margin + 1).saturating_sub(visible_items)
    } else {
        current_offset
    };
    *pane.list_state.offset_mut() = new_offset;

    f.render_stateful_widget(list, list_area, &mut pane.list_state);

    // Scrollbar
    let visible_approx = list_area.height as usize / 2;
    if pane.tasks.len() > visible_approx && list_area.height > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(pane.tasks.len()).position(pane.list_state.selected().unwrap_or(0));
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(FG_OVERLAY));
        f.render_stateful_widget(scrollbar, list_area, &mut scrollbar_state);
    }
}

pub(super) fn is_task_overdue(task: &Task, today: chrono::NaiveDate) -> bool {
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

pub(super) fn build_pane_stats(
    elapsed: i64,
    estimate: i64,
    done: usize,
    total: usize,
    on_time: usize,
    overdue: usize,
) -> Vec<Span<'static>> {
    let muted = Style::default().fg(FG_SUBTEXT);
    let overlay = Style::default().fg(FG_OVERLAY);

    if total == 0 {
        return vec![Span::styled("(0)", overlay)];
    }

    let elapsed_str = format_dur_short(elapsed);

    // DONE pane: color-code on-time (green) / overrun (red) counts + net time delta
    if done > 0 && done == total {
        let net = elapsed - estimate; // positive = over budget, negative = under
        let net_str = if estimate == 0 {
            String::new()
        } else if net >= 0 {
            format!(" +{}", format_dur_short(net))
        } else {
            format!(" \u{2212}{}", format_dur_short(-net))
        };
        let net_style = if net > 0 {
            Style::default().fg(ACCENT_RED)
        } else {
            Style::default().fg(ACCENT_GREEN)
        };
        let mut spans = vec![
            Span::styled(on_time.to_string(), Style::default().fg(ACCENT_GREEN)),
            Span::styled("/", overlay),
            Span::styled(overdue.to_string(), Style::default().fg(ACCENT_RED)),
        ];
        if !net_str.is_empty() {
            spans.push(Span::styled(net_str, net_style));
        }
        spans
    } else if estimate > 0 {
        let pct = (elapsed as f64 / estimate as f64 * 100.0) as u64;
        let pct_style = if pct < 80 {
            Style::default().fg(ACCENT_GREEN)
        } else if pct < 100 {
            Style::default().fg(ACCENT_YELLOW)
        } else {
            Style::default().fg(ACCENT_RED)
        };
        vec![
            Span::styled(
                format!("{}/{}", elapsed_str, format_dur_short(estimate)),
                muted,
            ),
            Span::styled(format!(" | {}%", pct), pct_style),
            Span::styled(format!(" | {}/{}", done, total), muted),
        ]
    } else {
        vec![Span::styled(
            format!("{} | {}/{}", elapsed_str, done, total),
            muted,
        )]
    }
}

pub(super) fn task_num_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(ACCENT_GREEN),
        TaskStatus::Done => Style::default()
            .fg(FG_SUBTEXT)
            .add_modifier(Modifier::CROSSED_OUT),
        TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
        TaskStatus::Pending => Style::default().fg(FG_SUBTEXT),
    }
}

pub(super) fn task_title_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default()
            .fg(ACCENT_GREEN)
            .add_modifier(Modifier::BOLD),
        TaskStatus::Done => Style::default()
            .fg(FG_SUBTEXT)
            .add_modifier(Modifier::CROSSED_OUT),
        TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
        TaskStatus::Pending => Style::default().fg(FG_TEXT),
    }
}

/// Minimal meta for recurring templates: estimate + project + context only.
/// Omits scheduled/deadline dates (always-past anchor dates show misleading red).
pub(super) fn build_template_meta(task: &Task) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = vec![];
    let muted = Style::default().fg(FG_OVERLAY);

    if let Some(est) = task.estimate_minutes {
        spans.push(Span::styled(format!("~{}", format_est(est)), muted));
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
    spans
}

pub(super) fn build_compact_meta(task: &Task, today: chrono::NaiveDate) -> Vec<Span<'static>> {
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
                4 => Style::default().fg(ACCENT_RED).add_modifier(Modifier::BOLD),
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

    // Tags (4a: previously silently dropped)
    if let Some(ref t) = task.tags {
        for tag in t.split(',') {
            let tag = tag.trim();
            if !tag.is_empty() {
                if !spans.is_empty() {
                    spans.push(Span::styled(" ", muted));
                }
                spans.push(Span::styled(
                    format!("#{}", tag),
                    Style::default().fg(ACCENT_PEACH),
                ));
            }
        }
    }

    // Time display — show countdown/overrun for active tasks, delta for Done tasks.
    let elapsed = task.elapsed_seconds.unwrap_or(0);
    if task.status == TaskStatus::Done {
        // Show how much under (-) or over (+) estimate the task ran.
        if let Some(est) = task.estimate_minutes {
            if elapsed > 0 || est > 0 {
                if !spans.is_empty() {
                    spans.push(Span::styled(" ", muted));
                }
                let est_secs = est * 60;
                let delta = elapsed - est_secs;
                let (delta_str, delta_style) = if delta > 0 {
                    (
                        format!("+{}", format_dur_short(delta)),
                        Style::default().fg(ACCENT_RED),
                    )
                } else if delta < 0 {
                    (
                        format!("\u{2212}{}", format_dur_short(-delta)),
                        Style::default().fg(ACCENT_GREEN),
                    )
                } else {
                    ("\u{2713}".to_string(), Style::default().fg(ACCENT_TEAL))
                };
                spans.push(Span::styled(format!("({})", delta_str), delta_style));
            }
        }
    } else if elapsed > 0 {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
        }
        if let Some(est) = task.estimate_minutes {
            let remaining = est * 60 - elapsed;
            let (time_str, time_style) = if remaining > 0 {
                let r = remaining as u64;
                let time_str = if r >= 3600 {
                    format!("{}h{:02}m left", r / 3600, (r % 3600) / 60)
                } else {
                    format!("{}m left", r / 60)
                };
                let pct = remaining as f64 / (est * 60) as f64;
                let style = if pct > 0.5 {
                    Style::default().fg(ACCENT_GREEN)
                } else {
                    Style::default().fg(ACCENT_YELLOW)
                };
                (time_str, style)
            } else {
                let over = (-remaining) as u64;
                let time_str = if over >= 3600 {
                    format!("+{}h{:02}m over", over / 3600, (over % 3600) / 60)
                } else {
                    format!("+{}m over", over / 60)
                };
                (time_str, Style::default().fg(ACCENT_RED))
            };
            spans.push(Span::styled(format!("({})", time_str), time_style));
        } else {
            let elapsed_style = Style::default().fg(ACCENT_GREEN);
            spans.push(Span::styled(
                format!("({})", format_dur(elapsed)),
                elapsed_style,
            ));
        }
    }

    // Estimate: only render when there is no elapsed time (Bug 1a: avoid double-render).
    if elapsed == 0 {
        if let Some(est) = task.estimate_minutes {
            if !spans.is_empty() {
                spans.push(Span::styled(" ", muted));
            }
            spans.push(Span::styled(format!("~{}", format_est(est)), muted));
        }
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

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

pub(super) fn draw_shadow(f: &mut Frame, area: Rect) {
    if area.x + area.width < f.area().width && area.y + area.height < f.area().height {
        let shadow = Rect::new(area.x + 1, area.y + 1, area.width, area.height);
        let shadow_block = Block::default().style(Style::default().bg(Color::Rgb(20, 20, 30)));
        f.render_widget(shadow_block, shadow);
    }
}

pub(super) fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    if app.delete_task_is_recurring_instance {
        // Recurring instance: teal "Skip Occurrence" modal
        let block = Block::bordered()
            .title(Span::styled(
                " Skip Occurrence ",
                Style::default()
                    .fg(ACCENT_TEAL)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT_TEAL))
            .padding(Padding::horizontal(1));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let text = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Skip today's ", Style::default().fg(FG_TEXT)),
                Span::styled(
                    format!("\"{}\"", app.delete_task_title),
                    Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("?", Style::default().fg(FG_TEXT)),
            ]),
            Line::from(Span::styled(
                "  The series continues \u{2014} next occurrence will be generated.",
                Style::default().fg(FG_SUBTEXT),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(" Enter/y ", Style::default().fg(FG_TEXT).bg(ACCENT_TEAL)),
                Span::styled(" skip  ", Style::default().fg(FG_SUBTEXT)),
                Span::styled(" Esc/n ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
                Span::styled(" cancel  ", Style::default().fg(FG_SUBTEXT)),
                Span::styled(
                    "(d on Recurring tab = delete series)",
                    Style::default().fg(FG_OVERLAY),
                ),
            ]),
        ];

        f.render_widget(Paragraph::new(text), inner);
    } else {
        // Regular task: red "Delete Task" modal
        let block = Block::bordered()
            .title(Span::styled(
                " Delete Task ",
                Style::default().fg(ACCENT_RED).add_modifier(Modifier::BOLD),
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
                    Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("?", Style::default().fg(FG_TEXT)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(" Enter/y ", Style::default().fg(FG_TEXT).bg(ACCENT_RED)),
                Span::styled(" delete  ", Style::default().fg(FG_SUBTEXT)),
                Span::styled(" Esc/n ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
                Span::styled(" cancel", Style::default().fg(FG_SUBTEXT)),
            ]),
        ];

        f.render_widget(Paragraph::new(text), inner);
    }
}

fn build_multiline_input(input: &str, style: Style) -> Vec<Line<'static>> {
    let parts: Vec<&str> = input.split('\n').collect();
    let last = parts.len() - 1;
    parts
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let prefix = if i == 0 { "\u{276F} " } else { "  " };
            let suffix = if i == last { "\u{2588}" } else { "" };
            Line::from(Span::styled(format!("{}{}{}", prefix, line, suffix), style))
        })
        .collect()
}

pub(super) const EDIT_FIELD_HINTS: [&str; 9] = [
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

pub(super) fn draw_edit_modal(f: &mut Frame, app: &App) {
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
    } else if app.edit_is_template {
        " j/k \u{2193}\u{2191}:navigate  Enter:edit  Esc:close "
    } else {
        " j/k \u{2193}\u{2191}:navigate  Enter:edit  e:elapsed  Esc:close "
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
            let input_line_count = app.edit_field_input.split('\n').count();
            let input_area_height = (input_line_count as u16 + 2).max(4).min(14);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(input_area_height)])
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
            let mut input_lines = vec![
                Line::from(Span::styled(
                    format!("  {}", EDIT_FIELD_HINTS[8]),
                    Style::default().fg(FG_OVERLAY),
                )),
                Line::from(""),
            ];
            input_lines.extend(build_multiline_input(
                &app.edit_field_input,
                Style::default().fg(FG_TEXT),
            ));
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
                    format!(
                        "({} line{})",
                        line_count,
                        if line_count == 1 { "" } else { "s" }
                    )
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

pub(super) fn format_config_display_value(
    value: &str,
    field_type: ConfigFieldType,
) -> (String, Style) {
    match field_type {
        ConfigFieldType::Boolean => {
            if value == "true" {
                (
                    "\u{2611} enabled".to_string(),
                    Style::default().fg(ACCENT_GREEN),
                )
            } else {
                (
                    "\u{2610} disabled".to_string(),
                    Style::default().fg(FG_OVERLAY),
                )
            }
        }
        ConfigFieldType::Sensitive => {
            if value.is_empty() {
                ("(not set)".to_string(), Style::default().fg(FG_OVERLAY))
            } else {
                (
                    "\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}".to_string(),
                    Style::default().fg(FG_SUBTEXT),
                )
            }
        }
        ConfigFieldType::String | ConfigFieldType::Number => {
            if value.is_empty() {
                ("(not set)".to_string(), Style::default().fg(FG_OVERLAY))
            } else {
                (value.to_string(), Style::default().fg(FG_TEXT))
            }
        }
    }
}

pub(super) fn draw_config_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 75, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let title_text = if app.mode == AppMode::EditConfigField {
        format!(" Edit: {} ", CONFIG_FIELD_LABELS[app.config_field_index])
    } else {
        " Configuration ".to_string()
    };

    let help_text = if app.mode == AppMode::EditConfigField {
        " Enter:save  Esc:cancel "
    } else {
        " j/k \u{2193}\u{2191}:navigate  Enter:edit  t:test  Esc:close "
    };

    let block = Block::bordered()
        .title(Span::styled(
            title_text,
            Style::default()
                .fg(ACCENT_MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(help_text, Style::default().fg(FG_OVERLAY)))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_MAUVE))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.mode == AppMode::EditConfigField {
        // Show all fields dimmed above, input area at bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(4)])
            .split(inner);

        let mut lines: Vec<Line> = vec![];
        lines.push(Line::from(Span::styled(
            "\u{2500}\u{2500} Sync \u{2500}\u{2500}",
            Style::default().fg(FG_OVERLAY),
        )));
        for i in 0..CONFIG_FIELD_COUNT {
            if i == 4 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Backup \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            if i == 13 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Preferences \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            if i == 19 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Email \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            let (display, _) =
                format_config_display_value(&app.config_field_values[i], CONFIG_FIELD_TYPES[i]);
            let style = if i == app.config_field_index {
                Style::default()
                    .fg(ACCENT_MAUVE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_OVERLAY)
            };
            let indicator = if i == app.config_field_index {
                "\u{25B6} "
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                Span::styled(indicator, Style::default().fg(ACCENT_MAUVE)),
                Span::styled(format!("{:<16}", CONFIG_FIELD_LABELS[i]), style),
                Span::styled(display, Style::default().fg(FG_OVERLAY)),
            ]));
        }
        f.render_widget(Paragraph::new(lines), chunks[0]);

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(ACCENT_MAUVE))
            .border_type(BorderType::Rounded);
        let hint = CONFIG_FIELD_HINTS[app.config_field_index];
        let input_lines = vec![
            Line::from(Span::styled(
                format!("  {}", hint),
                Style::default().fg(FG_OVERLAY),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("\u{276F} {}\u{2588}", app.config_field_input),
                Style::default().fg(FG_TEXT),
            )),
        ];
        let input_widget = Paragraph::new(input_lines).block(input_block);
        f.render_widget(input_widget, chunks[1]);
    } else {
        // Field navigation view
        let has_test_result = app.config_test_result.is_some();
        let constraints = if has_test_result {
            vec![Constraint::Min(1), Constraint::Length(2)]
        } else {
            vec![Constraint::Min(1)]
        };
        let nav_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut lines: Vec<Line> = vec![];
        lines.push(Line::from(Span::styled(
            "\u{2500}\u{2500} Sync \u{2500}\u{2500}",
            Style::default().fg(FG_OVERLAY),
        )));
        for i in 0..CONFIG_FIELD_COUNT {
            if i == 4 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Backup \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            if i == 13 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Preferences \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            if i == 19 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Email \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            let is_selected = i == app.config_field_index;
            let (display, display_style) =
                format_config_display_value(&app.config_field_values[i], CONFIG_FIELD_TYPES[i]);
            let label_style = if is_selected {
                Style::default()
                    .fg(ACCENT_MAUVE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_SUBTEXT)
            };
            let val_style = if is_selected {
                display_style.add_modifier(Modifier::BOLD)
            } else {
                display_style
            };
            let indicator = if is_selected { "\u{25B6} " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(indicator, Style::default().fg(ACCENT_MAUVE)),
                Span::styled(format!("{:<16}", CONFIG_FIELD_LABELS[i]), label_style),
                Span::styled(display, val_style),
            ]));
            if is_selected {
                lines.push(Line::from(Span::styled(
                    format!("    {}", CONFIG_FIELD_HINTS[i]),
                    Style::default().fg(FG_OVERLAY),
                )));
            } else {
                lines.push(Line::from(""));
            }
        }
        f.render_widget(
            Paragraph::new(lines).scroll((app.config_scroll as u16, 0)),
            nav_chunks[0],
        );

        // Test result display
        if let Some(ref result) = app.config_test_result {
            let color = if result.starts_with('\u{2713}') {
                ACCENT_GREEN
            } else {
                ACCENT_RED
            };
            let result_block = Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(FG_OVERLAY))
                .border_type(BorderType::Rounded);
            let result_line = Line::from(Span::styled(
                format!("  {}", result),
                Style::default().fg(color),
            ));
            f.render_widget(
                Paragraph::new(result_line).block(result_block),
                nav_chunks[1],
            );
        }
    }
}

pub(super) fn draw_note_view_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 70, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let help_text = if app.note_editing {
        " Enter:save  Alt+Enter:newline  Esc:cancel "
    } else {
        " j/k \u{2193}\u{2191}:navigate  e:edit  a:add  d:delete  Esc:back "
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
        let input_line_count = app.edit_field_input.split('\n').count();
        let input_area_height = (input_line_count as u16 + 2).max(4).min(14);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(input_area_height)])
            .split(inner);

        let mut lines: Vec<Line> = Vec::new();
        for (i, note_entry) in app.note_lines.iter().enumerate() {
            let is_selected = i == app.note_selected;
            let base_style = if is_selected {
                Style::default()
                    .fg(FG_TEXT)
                    .bg(ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_SUBTEXT)
            };
            for (j, sub_line) in note_entry.split('\n').enumerate() {
                let prefix = if j == 0 { "  " } else { "      " };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", prefix, sub_line),
                    base_style,
                )));
            }
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), chunks[0]);

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(ACCENT_YELLOW))
            .border_type(BorderType::Rounded);
        let mut input_lines = vec![
            Line::from(Span::styled(
                "  Editing note. Alt+Enter for newline",
                Style::default().fg(FG_OVERLAY),
            )),
            Line::from(""),
        ];
        input_lines.extend(build_multiline_input(
            &app.edit_field_input,
            Style::default().fg(FG_TEXT),
        ));
        f.render_widget(Paragraph::new(input_lines).block(input_block), chunks[1]);
    } else {
        // List of note entries with selection highlight (multiline entries indented)
        let mut lines: Vec<Line> = Vec::new();
        for (i, note_entry) in app.note_lines.iter().enumerate() {
            let is_selected = i == app.note_selected;
            let style = if is_selected {
                Style::default().fg(FG_TEXT).bg(Color::Rgb(65, 75, 120))
            } else {
                Style::default().fg(FG_SUBTEXT)
            };
            for (j, sub_line) in note_entry.split('\n').enumerate() {
                let prefix = if j == 0 { "  " } else { "      " };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", prefix, sub_line),
                    style,
                )));
            }
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

pub(super) fn draw_elapsed_edit_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 25, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .title(Span::styled(
            " Edit Elapsed Time ",
            Style::default()
                .fg(ACCENT_TEAL)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " Enter:save  Esc:cancel ",
            Style::default().fg(FG_OVERLAY),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_TEAL))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let hint_line = Line::from(Span::styled(
        "  Duration, e.g.: 30m, 1h, 2h30m",
        Style::default().fg(FG_OVERLAY),
    ));
    let input_line = Line::from(Span::styled(
        format!("\u{276F} {}\u{2588}", app.elapsed_edit_input),
        Style::default().fg(FG_TEXT),
    ));
    let lines = vec![Line::from(""), hint_line, Line::from(""), input_line];
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_help_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 80, f.area());
    draw_shadow(f, area);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " j/k \u{2193}\u{2191}:scroll  Esc:close ",
            Style::default().fg(FG_OVERLAY),
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_BLUE))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let section_style = Style::default()
        .fg(ACCENT_BLUE)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(FG_SUBTEXT);
    let muted = Style::default().fg(FG_OVERLAY);

    let help_key = |k: &str, d: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {:<14}", k), key_style),
            Span::styled(d.to_string(), desc_style),
        ])
    };

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled("LEGEND", section_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status icons:  ", muted),
            Span::styled("\u{25CB}", Style::default().fg(FG_OVERLAY)),
            Span::styled(" pending  ", muted),
            Span::styled("\u{25B6}", Style::default().fg(ACCENT_GREEN)),
            Span::styled(" running  ", muted),
            Span::styled("\u{23F8}", Style::default().fg(ACCENT_YELLOW)),
            Span::styled(" paused  ", muted),
            Span::styled("\u{2713}", Style::default().fg(ACCENT_TEAL)),
            Span::styled(" done", muted),
        ]),
        Line::from(vec![
            Span::styled("  Notation:      ", muted),
            Span::styled("+proj ", Style::default().fg(ACCENT_MAUVE)),
            Span::styled("@ctx ", Style::default().fg(ACCENT_TEAL)),
            Span::styled("#tag ", Style::default().fg(ACCENT_PEACH)),
            Span::styled("~est ", muted),
            Span::styled("^deadline ", Style::default().fg(ACCENT_PEACH)),
            Span::styled("=scheduled ", Style::default().fg(ACCENT_TEAL)),
            Span::styled("! !! !!! !!!!", Style::default().fg(ACCENT_RED)),
        ]),
        Line::from(vec![
            Span::styled("  Duration:      ", muted),
            Span::styled("~30m  ~1h  ~1h30m  ~1d (8h)  ~1w (40h)", muted),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            muted,
        )),
        Line::from(""),
        Line::from(Span::styled("GLOBAL", section_style)),
        Line::from(""),
        help_key("Tab", "Next tab"),
        help_key("Shift+Tab", "Previous tab"),
        help_key("t", "Tasks tab"),
        help_key("c", "Recurring tab"),
        help_key("r", "Report tab"),
        help_key(",", "Settings tab"),
        help_key("y", "Sync now (when sync enabled)"),
        help_key("?", "Toggle this help"),
        help_key("q / Esc", "Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            muted,
        )),
        Line::from(""),
    ];

    match app.tab {
        TuiTab::Tasks => {
            lines.push(Line::from(Span::styled("TASKS", section_style)));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Navigation", desc_style)));
            lines.push(help_key("j / Down", "Next task"));
            lines.push(help_key("k / Up", "Previous task (search at top)"));
            lines.push(help_key("h / Left", "Previous pane"));
            lines.push(help_key("l / Right", "Next pane"));
            lines.push(help_key("G", "Jump to last task"));
            lines.push(help_key("gg", "Jump to first task"));
            lines.push(help_key("PgDn / PgUp", "Jump 10 tasks"));
            lines.push(help_key("[count]j/k", "Move by count (e.g., 5j)"));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Actions", desc_style)));
            lines.push(help_key("a", "Add new task"));
            lines.push(help_key("s", "Start/pause timer"));
            lines.push(help_key("d", "Toggle done/undone"));
            lines.push(help_key("n", "Open notes"));
            lines.push(help_key("Enter", "Edit task detail"));
            lines.push(help_key("Del / Bksp", "Delete task"));
            lines.push(help_key("< / >", "Quick-move task between panes"));
            lines.push(help_key("m", "Move task (pick target)"));
            lines.push(help_key("o", "Cycle sort mode"));
            lines.push(help_key(
                "v / V",
                "Next/prev view (Panes/Daily/Weekly/Calendar)",
            ));
            lines.push(help_key("t", "Jump to today (Daily/Weekly/Calendar)"));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Search", desc_style)));
            lines.push(help_key("/", "Open search bar"));
            lines.push(help_key("+proj", "Filter by project"));
            lines.push(help_key("@ctx", "Filter by context"));
            lines.push(help_key("!!", "Filter by priority >= 2"));
            lines.push(help_key("^<3d", "Deadline within 3 days"));
            lines.push(help_key("=<1w", "Scheduled within 1 week"));
            lines.push(help_key("keyword", "Filter by title"));
        }
        TuiTab::Recurring => {
            lines.push(Line::from(Span::styled("RECURRING", section_style)));
            lines.push(Line::from(""));
            lines.push(help_key("j/k \u{2193}\u{2191}", "Navigate templates"));
            lines.push(help_key("a", "Add template (*daily, *weekly, ...)"));
            lines.push(help_key("e / Enter", "Edit template"));
            lines.push(help_key("d / Del", "Delete template"));
            lines.push(help_key("p", "Pause/resume template"));
            lines.push(help_key("R", "Generate instances"));
        }
        TuiTab::Report => {
            lines.push(Line::from(Span::styled("REPORT", section_style)));
            lines.push(Line::from(""));
            lines.push(help_key("h / Left", "Previous range type"));
            lines.push(help_key("l / Right", "Next range type"));
            lines.push(help_key("[ / J", "Go back one period in time"));
            lines.push(help_key("] / K", "Go forward (toward today)"));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Range types: Day, Week, Month, Year, All",
                desc_style,
            )));
        }
        TuiTab::Settings => {
            lines.push(Line::from(Span::styled("SETTINGS", section_style)));
            lines.push(Line::from(""));
            lines.push(help_key("j/k \u{2193}\u{2191}", "Navigate backups"));
            lines.push(help_key("u", "Upload new backup"));
            lines.push(help_key("r", "Restore selected backup"));
            lines.push(help_key("d / Del", "Delete selected backup"));
            lines.push(help_key("s", "Trigger manual sync"));
            lines.push(help_key("e", "Edit config"));
        }
    }

    // Clamp scroll to content
    let content_height = lines.len() as u16;
    let visible_height = inner.height;
    let max_scroll = content_height.saturating_sub(visible_height) as usize;
    let scroll = app.help_scroll.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    f.render_widget(paragraph, inner);
}

pub(super) fn draw_add_bar(f: &mut Frame, app: &App) {
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

pub(super) fn draw_move_bar(f: &mut Frame, app: &App) {
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
