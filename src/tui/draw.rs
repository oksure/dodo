use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, LineGauge, List, ListItem, ListState, Padding,
        Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Sparkline, Tabs, Wrap,
    },
    Frame,
};

use dodo::task::{Task, TaskStatus};

use super::constants::*;
use super::format::*;
use super::state::*;

pub(super) fn draw_ui(f: &mut Frame, app: &App) {
    let search_height = if app.tab == TuiTab::Tasks { 3 } else { 0 };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),            // Header [0]
            Constraint::Length(1),            // Tab bar [1]
            Constraint::Length(search_height), // Search bar [2]
            Constraint::Min(0),              // Content [3]
            Constraint::Length(1),            // Footer [4]
        ])
        .split(f.area());

    // Header
    draw_header(f, app, outer[0]);

    // Tab bar
    let tab_titles: Vec<Line> = vec![
        Line::from(vec![
            Span::raw(" Tasks "),
            Span::styled(" t ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" Recurring "),
            Span::styled(" c ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" Report "),
            Span::styled(" r ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" Backup "),
            Span::styled(" b ", Style::default().fg(FG_TEXT).bg(BG_SURFACE)),
            Span::raw(" "),
        ]),
    ];
    let tab_index = match app.tab {
        TuiTab::Tasks => 0,
        TuiTab::Recurring => 1,
        TuiTab::Report => 2,
        TuiTab::Backup => 3,
    };
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

    // Search bar (Tasks tab only)
    if app.tab == TuiTab::Tasks {
        draw_search_bar(f, app, outer[2]);
    }

    // Content
    match app.tab {
        TuiTab::Tasks => draw_tasks_tab(f, app, outer[3]),
        TuiTab::Recurring => draw_recurring_tab(f, app, outer[3]),
        TuiTab::Report => draw_report_tab(f, app, outer[3]),
        TuiTab::Backup => draw_backup_tab(f, app, outer[3]),
    }

    // Footer
    draw_footer(f, app, outer[4]);

    // Modal overlays
    match app.mode {
        AppMode::ConfirmDelete => draw_delete_modal(f, app),
        AppMode::RecConfirmDelete => draw_rec_delete_modal(f, app),
        AppMode::EditTask | AppMode::EditTaskField => draw_edit_modal(f, app),
        AppMode::NoteView => draw_note_view_modal(f, app),
        AppMode::AddTask => draw_add_bar(f, app),
        AppMode::RecAddTemplate => draw_rec_add_bar(f, app),
        AppMode::MoveTask => draw_move_bar(f, app),
        AppMode::EditConfig | AppMode::EditConfigField => draw_config_modal(f, app),
        AppMode::Normal | AppMode::Search => {}
    }
}

pub(super) fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.mode == AppMode::Search;
    let has_filter = !app.search_input.is_empty();

    let border_color = if is_focused { ACCENT_BLUE } else { FG_OVERLAY };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if is_focused {
        let input_text = format!("/ {}\u{2588}", app.search_input);
        f.render_widget(
            Paragraph::new(input_text).style(Style::default().fg(FG_TEXT)),
            inner,
        );
    } else if has_filter {
        let filter_text = format!("/ {}", app.search_input);
        f.render_widget(
            Paragraph::new(filter_text).style(Style::default().fg(ACCENT_BLUE)),
            inner,
        );
    } else {
        let hint = "/ +proj @ctx !! ^<3d =<1w keyword";
        f.render_widget(
            Paragraph::new(hint).style(Style::default().fg(FG_OVERLAY)),
            inner,
        );
    }
}

pub(super) fn draw_header(f: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys: Vec<(&str, &str)> = match app.tab {
        TuiTab::Tasks => match app.mode {
            AppMode::AddTask => vec![
                ("Enter", "add"),
                ("Esc", "cancel"),
            ],
            AppMode::MoveTask => vec![
                ("h/l", "select"),
                ("Enter", "move"),
                ("Esc", "cancel"),
            ],
            AppMode::Search => vec![
                ("type", "filter"),
                ("Enter/Esc", "done"),
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
                ("/", "search"),
                ("q", "quit"),
            ],
        },
        TuiTab::Recurring => match app.mode {
            AppMode::RecAddTemplate => vec![
                ("Enter", "add"),
                ("Esc", "cancel"),
            ],
            _ => vec![
                ("a", "add"),
                ("e", "edit"),
                ("d", "del"),
                ("p", "pause"),
                ("g", "generate"),
                ("q", "quit"),
            ],
        },
        TuiTab::Report => vec![
            ("h/l", "range"),
            ("q", "quit"),
        ],
        TuiTab::Backup => match app.mode {
            AppMode::EditConfig => vec![
                ("j/k", "navigate"),
                ("\u{21B5}", "edit"),
                ("Esc", "close"),
            ],
            AppMode::EditConfigField => vec![
                ("\u{21B5}", "save"),
                ("Esc", "cancel"),
            ],
            _ => vec![
                ("j/k", "navigate"),
                ("u", "upload"),
                ("r", "restore"),
                ("d", "delete"),
                ("e", "config"),
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

pub(super) fn draw_tasks_tab(f: &mut Frame, app: &App, area: Rect) {
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
        let arrow = if app.panes[i].sort_ascending { "\u{2191}" } else { "\u{2193}" };
        let sort_display = format!("{}{}", sl, arrow);
        draw_pane(f, &app.panes[i], &headers[i], is_active, app.frame_count, &sort_display, pane_chunks[i]);
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

            let meta = build_compact_meta(template, chrono::Local::now().date_naive());

            let line1 = Line::from(vec![
                Span::styled(format!(" {:>3} ", num), Style::default().fg(FG_SUBTEXT)),
                Span::styled(format!("{} ", icon), icon_style),
                Span::styled(
                    format!("{:<8} ", recurrence),
                    Style::default().fg(ACCENT_PEACH),
                ),
                Span::styled(template.title.clone(), title_style),
            ]);

            let mut line2_spans = vec![Span::raw("                ")];
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
            Style::default()
                .fg(ACCENT_RED)
                .add_modifier(Modifier::BOLD),
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
    f.render_widget(
        Paragraph::new(text).alignment(Alignment::Center),
        inner,
    );
}

pub(super) fn draw_report_tab(f: &mut Frame, app: &App, area: Rect) {
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

/// Apply pastel rainbow sweep effect: a glow spot moves continuously left→right.
pub(super) fn apply_neon(line: Line<'static>, frame_count: u64, width: u16) -> Line<'static> {
    let sigma = width as f64 * 0.25;
    let period = width as f64 + sigma * 4.0;
    let wave_center = (frame_count as f64 * 0.8) % period - sigma * 2.0;
    let hue_offset = frame_count as f64 * 0.008;

    let mut result: Vec<Span<'static>> = Vec::new();
    let mut x: f64 = 0.0;

    for span in line.spans {
        let base_style = span.style;
        for ch in span.content.chars() {
            let d = x - wave_center;
            let intensity = (-0.5 * (d / sigma).powi(2)).exp();
            let hue = hue_offset + x / width as f64;
            let Color::Rgb(pr, pg, pb) = pastel_from_hue(hue) else { unreachable!() };
            let bg = Color::Rgb(
                (30.0 + intensity * (pr as f64 - 30.0)) as u8,
                (30.0 + intensity * (pg as f64 - 30.0)) as u8,
                (35.0 + intensity * (pb as f64 - 35.0)) as u8,
            );
            result.push(Span::styled(ch.to_string(), base_style.bg(bg)));
            x += 1.0;
        }
    }

    // Fill remaining row width with the glow
    while (x as u16) < width.saturating_sub(2) {
        let d = x - wave_center;
        let intensity = (-0.5 * (d / sigma).powi(2)).exp();
        let hue = hue_offset + x / width as f64;
        let Color::Rgb(pr, pg, pb) = pastel_from_hue(hue) else { unreachable!() };
        let bg = Color::Rgb(
            (30.0 + intensity * (pr as f64 - 30.0)) as u8,
            (30.0 + intensity * (pg as f64 - 30.0)) as u8,
            (35.0 + intensity * (pb as f64 - 35.0)) as u8,
        );
        result.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1.0;
    }

    Line::from(result)
}

pub(super) fn draw_backup_tab(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FG_OVERLAY))
        .title(Span::styled(
            " Backup ",
            Style::default().fg(FG_TEXT).add_modifier(Modifier::BOLD),
        ))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !app.backup_config.is_ready() {
        let mut msg = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Backup not configured",
                Style::default().fg(ACCENT_YELLOW).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Add a [backup] section to ~/.config/dodo/config.toml:",
                Style::default().fg(FG_SUBTEXT),
            )),
            Line::from(""),
            Line::from(Span::styled("  [backup]", Style::default().fg(FG_TEXT))),
            Line::from(Span::styled("  enabled = true", Style::default().fg(FG_TEXT))),
            Line::from(Span::styled(
                "  endpoint = \"https://s3.example.com\"",
                Style::default().fg(FG_TEXT),
            )),
            Line::from(Span::styled(
                "  bucket = \"my-bucket\"",
                Style::default().fg(FG_TEXT),
            )),
            Line::from(Span::styled(
                "  access_key = \"...\"",
                Style::default().fg(FG_TEXT),
            )),
            Line::from(Span::styled(
                "  secret_key = \"...\"",
                Style::default().fg(FG_TEXT),
            )),
            Line::from(""),
        ];
        // Sync status in unconfigured backup view
        if app.sync_config.is_ready() {
            let url = app.sync_config.turso_url.as_deref().unwrap_or("");
            msg.push(Line::from(vec![
                Span::styled("Turso sync: ", Style::default().fg(FG_SUBTEXT)),
                Span::styled("\u{25CF} ", Style::default().fg(ACCENT_GREEN)),
                Span::styled("enabled  ", Style::default().fg(ACCENT_GREEN)),
                Span::styled(url, Style::default().fg(ACCENT_TEAL)),
            ]));
        } else {
            msg.push(Line::from(vec![
                Span::styled("Turso sync: ", Style::default().fg(FG_SUBTEXT)),
                Span::styled("\u{25CB} not configured", Style::default().fg(FG_OVERLAY)),
            ]));
            msg.push(Line::from(Span::styled(
                "Add a [sync] section to enable:",
                Style::default().fg(FG_SUBTEXT),
            )));
            msg.push(Line::from(""));
            msg.push(Line::from(Span::styled("  [sync]", Style::default().fg(FG_TEXT))));
            msg.push(Line::from(Span::styled("  enabled = true", Style::default().fg(FG_TEXT))));
            msg.push(Line::from(Span::styled(
                "  turso_url = \"libsql://mydb.turso.io\"",
                Style::default().fg(FG_TEXT),
            )));
            msg.push(Line::from(Span::styled(
                "  turso_token = \"your-token\"",
                Style::default().fg(FG_TEXT),
            )));
        }
        f.render_widget(Paragraph::new(msg), inner);
        return;
    }

    // Split into sync status + status message + list area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Sync status
            Constraint::Length(2), // Status message
            Constraint::Min(0),   // Backup list
        ])
        .split(inner);

    // Sync status line
    let sync_line = if app.sync_config.is_ready() {
        let url = app.sync_config.turso_url.as_deref().unwrap_or("");
        Line::from(vec![
            Span::styled("Sync: ", Style::default().fg(FG_SUBTEXT)),
            Span::styled("\u{25CF} ", Style::default().fg(ACCENT_GREEN)),
            Span::styled("enabled  ", Style::default().fg(ACCENT_GREEN)),
            Span::styled(url, Style::default().fg(ACCENT_TEAL)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Sync: ", Style::default().fg(FG_SUBTEXT)),
            Span::styled("\u{25CB} not configured", Style::default().fg(FG_OVERLAY)),
        ])
    };
    f.render_widget(Paragraph::new(sync_line), chunks[0]);

    // Status message
    if let Some(ref msg) = app.backup_status_msg {
        let color = if msg.starts_with("Error") || msg.contains("failed") {
            ACCENT_RED
        } else {
            ACCENT_GREEN
        };
        f.render_widget(
            Paragraph::new(Span::styled(msg.as_str(), Style::default().fg(color))),
            chunks[1],
        );
    }

    // Backup list
    if app.backup_entries.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "No backups found. Press 'u' to create one.",
                Style::default().fg(FG_SUBTEXT),
            )),
            chunks[2],
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
            let is_selected = i == app.backup_selected;

            let style = if is_selected {
                Style::default().fg(FG_TEXT).bg(BG_SURFACE)
            } else {
                Style::default().fg(FG_SUBTEXT)
            };

            let line = Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    Style::default().fg(ACCENT_BLUE),
                ),
                Span::styled(&entry.display_name, style.add_modifier(Modifier::BOLD)),
                Span::styled("  ", style),
                Span::styled(age, Style::default().fg(ACCENT_TEAL)),
                Span::styled("  ", style),
                Span::styled(size, Style::default().fg(FG_OVERLAY)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, chunks[2]);
}

pub(super) fn draw_pane(
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
    let left_width = left_text.chars().count();
    let right_width = right_text.chars().count();
    let pad = (chunks[0].width as usize).saturating_sub(left_width + right_width);
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
            let recur_mark = if task.template_id.is_some() { " \u{21BB}" } else { "" };

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
                Span::styled(format!("{}{}{}", task.title, recur_mark, notes_mark), title_style),
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

pub(super) fn build_pane_stats(elapsed: i64, estimate: i64, done: usize, total: usize) -> String {
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

pub(super) fn task_num_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(ACCENT_GREEN),
        TaskStatus::Done => Style::default().fg(FG_SUBTEXT),
        TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
        TaskStatus::Pending => Style::default().fg(FG_SUBTEXT),
    }
}

pub(super) fn task_title_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default()
            .fg(ACCENT_GREEN)
            .add_modifier(Modifier::BOLD),
        TaskStatus::Done => Style::default().fg(FG_SUBTEXT),
        TaskStatus::Paused => Style::default().fg(ACCENT_YELLOW),
        TaskStatus::Pending => Style::default().fg(FG_TEXT),
    }
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

pub(super) fn format_config_display_value(value: &str, field_type: ConfigFieldType) -> (String, Style) {
    match field_type {
        ConfigFieldType::Boolean => {
            if value == "true" {
                ("\u{2611} enabled".to_string(), Style::default().fg(ACCENT_GREEN))
            } else {
                ("\u{2610} disabled".to_string(), Style::default().fg(FG_OVERLAY))
            }
        }
        ConfigFieldType::Sensitive => {
            if value.is_empty() {
                ("(not set)".to_string(), Style::default().fg(FG_OVERLAY))
            } else {
                ("\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}".to_string(), Style::default().fg(FG_SUBTEXT))
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
        " j/k:navigate  Enter:edit  Esc:close "
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
            if i == 3 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Backup \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            let (display, _) = format_config_display_value(
                &app.config_field_values[i],
                CONFIG_FIELD_TYPES[i],
            );
            let style = if i == app.config_field_index {
                Style::default().fg(ACCENT_MAUVE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_OVERLAY)
            };
            let indicator = if i == app.config_field_index { "\u{25B6} " } else { "  " };
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
        let mut lines: Vec<Line> = vec![];
        lines.push(Line::from(Span::styled(
            "\u{2500}\u{2500} Sync \u{2500}\u{2500}",
            Style::default().fg(FG_OVERLAY),
        )));
        for i in 0..CONFIG_FIELD_COUNT {
            if i == 3 {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "\u{2500}\u{2500} Backup \u{2500}\u{2500}",
                    Style::default().fg(FG_OVERLAY),
                )));
            }
            let is_selected = i == app.config_field_index;
            let (display, display_style) = format_config_display_value(
                &app.config_field_values[i],
                CONFIG_FIELD_TYPES[i],
            );
            let label_style = if is_selected {
                Style::default().fg(ACCENT_MAUVE).add_modifier(Modifier::BOLD)
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
        f.render_widget(Paragraph::new(lines), inner);
    }
}

pub(super) fn draw_note_view_modal(f: &mut Frame, app: &App) {
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
