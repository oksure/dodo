use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
    Frame, Terminal,
};
use std::io;

use dodo::cli::SortBy;
use dodo::db::Database;
use dodo::task::{Area, Task, TaskStatus};

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

    /// Compute stats for this pane: (elapsed_seconds, estimate_seconds, done_count, total_count)
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

    /// Returns (from_rfc3339, to_rfc3339) for the date range
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

struct App<'a> {
    panes: [PaneState; 4],
    active_pane: usize,
    sort_index: usize,
    running_task: Option<String>,
    db: &'a Database,
    mode: AppMode,
    note_task_id: Option<String>,
    note_task_title: String,
    note_content: String,
    note_input: String,
    tab: TuiTab,
    report_range: ReportRange,
    report: Option<ReportData>,
    tick_count: u64,
}

impl<'a> App<'a> {
    fn new(db: &'a Database) -> Self {
        Self {
            panes: [PaneState::new(), PaneState::new(), PaneState::new(), PaneState::new()],
            active_pane: 2, // Start on TODAY
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

        // Group tasks by effective area
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
        let tasks_done = self.db.report_tasks_done(&from, &to)?;
        let total_seconds = self.db.report_total_seconds(&from, &to)?;
        let active_days = self.db.report_active_days(&from, &to)?;
        let by_hour = self.db.report_by_hour(&from, &to)?;
        let by_weekday = self.db.report_by_weekday(&from, &to)?;
        let by_project = self.db.report_by_project(&from, &to)?;
        let done_tasks = self.db.report_done_tasks(&from, &to, 20)?;

        self.report = Some(ReportData {
            tasks_done,
            total_seconds,
            active_days,
            by_hour,
            by_weekday,
            by_project,
            done_tasks,
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
        self.db.complete_task()?;
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
}

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
                                    KeyCode::Char('j') | KeyCode::Down => app.panes[app.active_pane].next(),
                                    KeyCode::Char('k') | KeyCode::Up => app.panes[app.active_pane].previous(),
                                    KeyCode::Char('h') | KeyCode::Left => app.move_pane_left(),
                                    KeyCode::Char('l') | KeyCode::Right => app.move_pane_right(),
                                    KeyCode::Char('s') => { let _ = app.toggle_selected(); }
                                    KeyCode::Char('d') => { let _ = app.done(); }
                                    KeyCode::Char('o') => app.cycle_sort(),
                                    KeyCode::Char('r') => { let _ = app.refresh_all(); }
                                    KeyCode::Char('n') => { app.open_note_view(); }
                                    _ => {}
                                }
                            } else {
                                // Report tab keys
                                match key.code {
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        app.report_range = app.report_range.next();
                                        let _ = app.refresh_report();
                                    }
                                    KeyCode::Char('h') | KeyCode::Left => {
                                        app.report_range = app.report_range.prev();
                                        let _ = app.refresh_report();
                                    }
                                    KeyCode::Char('r') => { let _ = app.refresh_report(); }
                                    _ => {}
                                }
                            }
                        }
                    },
                    AppMode::NoteView => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => { app.mode = AppMode::Normal; }
                        KeyCode::Char('e') => { app.enter_note_edit(); }
                        _ => {}
                    },
                    AppMode::NoteEdit => match key.code {
                        KeyCode::Esc => { app.mode = AppMode::NoteView; }
                        KeyCode::Enter => { let _ = app.save_note(); }
                        KeyCode::Backspace => { app.note_input.pop(); }
                        KeyCode::Char(c) => { app.note_input.push(c); }
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
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Length(1), Constraint::Min(0)])
        .split(f.area());

    // Header
    let header = build_header(app);
    f.render_widget(header, outer[0]);

    // Tab bar
    let tab_titles: Vec<Line> = vec![
        Line::from(" Tasks "),
        Line::from(" Report "),
    ];
    let tab_index = if app.tab == TuiTab::Tasks { 0 } else { 1 };
    let tabs = Tabs::new(tab_titles)
        .select(tab_index)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .divider(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
    f.render_widget(tabs, outer[1]);

    match app.tab {
        TuiTab::Tasks => draw_tasks_tab(f, app, outer[2]),
        TuiTab::Report => draw_report_tab(f, app, outer[2]),
    }

    // Note modal overlay
    if app.mode == AppMode::NoteView || app.mode == AppMode::NoteEdit {
        draw_note_modal(f, app);
    }
}

fn draw_tasks_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
        f.render_stateful_widget(pane_widget, pane_chunks[i], &mut app.panes[i].list_state.clone());
    }
}

fn draw_report_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let report = match &app.report {
        Some(r) => r,
        None => {
            let msg = Paragraph::new("Loading report...").style(Style::default().fg(Color::DarkGray));
            f.render_widget(msg, area);
            return;
        }
    };

    // Range selector at top
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    // Range tabs
    let ranges = [ReportRange::Day, ReportRange::Week, ReportRange::Month, ReportRange::Year, ReportRange::All];
    let range_spans: Vec<Span> = ranges.iter().map(|r| {
        if *r == app.report_range {
            Span::styled(
                format!(" {} ", r.label()),
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                format!(" {} ", r.label()),
                Style::default().fg(Color::Gray),
            )
        }
    }).collect();
    let mut all_spans = vec![Span::styled("  Range: ", Style::default().fg(Color::DarkGray))];
    for (i, s) in range_spans.into_iter().enumerate() {
        all_spans.push(s);
        if i < ranges.len() - 1 {
            all_spans.push(Span::styled("  ", Style::default()));
        }
    }
    all_spans.push(Span::styled("    (h/l to change)", Style::default().fg(Color::DarkGray)));
    f.render_widget(Paragraph::new(Line::from(all_spans)), layout[0]);

    // Report body: two columns
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[1]);

    // Left column: summary stats + productivity
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
            Span::styled("  Tasks done:    ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", report.tasks_done), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Total time:    ", Style::default().fg(Color::Gray)),
            Span::styled(format_dur(report.total_seconds), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Active days:   ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{}", report.active_days), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("  Avg/task:      ", Style::default().fg(Color::Gray)),
            Span::styled(format_dur(avg_per_task), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Avg/day:       ", Style::default().fg(Color::Gray)),
            Span::styled(format_dur(avg_per_day), Style::default().fg(Color::White)),
        ]),
    ];

    let summary = Paragraph::new(summary_lines).block(
        Block::default()
            .title(Span::styled(" Summary ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(summary, left_layout[0]);

    // Productivity: most productive hour + day
    let mut prod_lines: Vec<Line> = vec![];

    // Most productive hour
    if let Some((hour, secs)) = report.by_hour.iter().max_by_key(|(_h, s)| *s) {
        prod_lines.push(Line::from(vec![
            Span::styled("  Best hour:     ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{:02}:00", hour), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  ({})", format_dur(*secs)), Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Most productive day of week
    if let Some((dow, secs)) = report.by_weekday.iter().max_by_key(|(_d, s)| *s) {
        let day_name = DAY_NAMES.get(*dow as usize).unwrap_or(&"?");
        prod_lines.push(Line::from(vec![
            Span::styled("  Best day:      ", Style::default().fg(Color::Gray)),
            Span::styled(day_name.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  ({})", format_dur(*secs)), Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Hour distribution - mini bar chart
    if !report.by_hour.is_empty() {
        prod_lines.push(Line::from(""));
        prod_lines.push(Line::from(Span::styled("  Hours worked:", Style::default().fg(Color::Gray))));
        let max_secs = report.by_hour.iter().map(|(_, s)| *s).max().unwrap_or(1).max(1);
        for (hour, secs) in &report.by_hour {
            let bar_width = (*secs as f64 / max_secs as f64 * 20.0) as usize;
            let bar: String = "\u{2588}".repeat(bar_width);
            prod_lines.push(Line::from(vec![
                Span::styled(format!("  {:02}:00 ", hour), Style::default().fg(Color::DarkGray)),
                Span::styled(bar, Style::default().fg(Color::Cyan)),
                Span::styled(format!(" {}", format_dur(*secs)), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let prod = Paragraph::new(prod_lines).block(
        Block::default()
            .title(Span::styled(" Productivity ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(prod, left_layout[1]);

    // Right column: time by project + done tasks
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
            Span::styled(format!("  +{:<14}", project), Style::default().fg(Color::Magenta)),
            Span::styled(format!("{:>8}", format_dur(*secs)), Style::default().fg(Color::White)),
            Span::styled(format!("  {:>3}%", pct), Style::default().fg(Color::DarkGray)),
        ]));
    }
    if proj_lines.is_empty() {
        proj_lines.push(Line::from(Span::styled("  (no data)", Style::default().fg(Color::DarkGray))));
    }

    let proj = Paragraph::new(proj_lines).block(
        Block::default()
            .title(Span::styled(" Time by Project ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(proj, right_layout[0]);

    // Done tasks
    let mut done_lines: Vec<Line> = vec![];
    for (title, secs) in &report.done_tasks {
        done_lines.push(Line::from(vec![
            Span::styled("  \u{2713} ", Style::default().fg(Color::Green)),
            Span::styled(title.clone(), Style::default().fg(Color::White)),
            Span::styled(format!("  ({})", format_dur(*secs)), Style::default().fg(Color::DarkGray)),
        ]));
    }
    if done_lines.is_empty() {
        done_lines.push(Line::from(Span::styled("  (no completed tasks)", Style::default().fg(Color::DarkGray))));
    }

    let done = Paragraph::new(done_lines).block(
        Block::default()
            .title(Span::styled(" Completed Tasks ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(done, right_layout[1]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
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

fn draw_note_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, f.area());
    f.render_widget(ratatui::widgets::Clear, area);

    let title = format!(" Notes: {} ", app.note_task_title);
    let help_text = if app.mode == AppMode::NoteEdit {
        " Enter:save  Esc:cancel "
    } else {
        " e:edit  Esc/q:close "
    };

    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(help_text, Style::default().fg(Color::DarkGray)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

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
            .style(Style::default().fg(Color::Gray))
            .wrap(ratatui::widgets::Wrap { trim: false });
        f.render_widget(notes_widget, chunks[0]);

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray));
        let input_text = format!(">{}", app.note_input);
        let input_widget = Paragraph::new(input_text)
            .style(Style::default().fg(Color::White))
            .block(input_block);
        f.render_widget(input_widget, chunks[1]);
    } else {
        let content = if app.note_content.is_empty() {
            "(no notes)".to_string()
        } else {
            app.note_content.clone()
        };
        let notes_widget = Paragraph::new(content)
            .style(Style::default().fg(Color::Gray))
            .wrap(ratatui::widgets::Wrap { trim: false });
        f.render_widget(notes_widget, inner);
    }
}

fn build_header(app: &App<'_>) -> Paragraph<'static> {
    let sort_label = match app.current_sort() {
        SortBy::Created => "created",
        SortBy::Modified => "modified",
        SortBy::Title => "title",
        SortBy::Area => "area",
    };

    let running_info = if let Some(ref task) = app.running_task {
        format!(" \u{25B6} {} ", task)
    } else {
        String::new()
    };

    // Animate the running task color: cycle between green shades
    let running_style = if app.running_task.is_some() {
        let phase = app.tick_count % 3;
        match phase {
            0 => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            1 => Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        }
    } else {
        Style::default()
    };

    let help = if app.tab == TuiTab::Tasks {
        format!("  sort:{}  1/2:tab h/l:pane j/k:nav s:start/stop d:done n:note o:sort q:quit ", sort_label)
    } else {
        " 1/2:tab h/l:range r:refresh q:quit ".to_string()
    };

    let text = Line::from(vec![
        Span::styled(
            " DODO ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(running_info, running_style),
        Span::styled(help, Style::default().fg(Color::DarkGray)),
    ]);

    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}

fn build_pane(pane: &PaneState, label: &str, is_active: bool, tick_count: u64) -> List<'static> {
    let border_style = if is_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title_style = if is_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let items: Vec<ListItem> = pane
        .tasks
        .iter()
        .map(|task| {
            let is_running = task.status == TaskStatus::Running;

            let status_icon = match task.status {
                TaskStatus::Pending => " ",
                TaskStatus::Running => "\u{25B6}",
                TaskStatus::Paused => "\u{23F8}",
                TaskStatus::Done => "\u{2713}",
            };

            let num = task.num_id.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
            let notes_mark = match &task.notes {
                Some(n) if !n.is_empty() => " *",
                _ => "",
            };

            // Animated style for running task
            let (num_style, title_style) = if is_running {
                let phase = tick_count % 3;
                let color = match phase {
                    0 => Color::Green,
                    1 => Color::LightGreen,
                    _ => Color::Cyan,
                };
                (
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )
            } else {
                (task_num_style(task), task_title_style(task))
            };

            let line1 = Line::from(vec![
                Span::styled(format!("{:>3} {} ", num, status_icon), num_style),
                Span::styled(format!("{}{}", task.title, notes_mark), title_style),
            ]);

            let meta_spans = build_compact_meta(task);
            if meta_spans.is_empty() {
                ListItem::new(vec![line1])
            } else {
                let mut line2_spans = vec![Span::raw("      ")];
                line2_spans.extend(meta_spans);
                let line2 = Line::from(line2_spans);
                ListItem::new(vec![line1, line2])
            }
        })
        .collect();

    // Build title with stats
    let (elapsed, estimate, done, total) = pane.stats();
    let stats = build_pane_stats(elapsed, estimate, done, total);
    let block_title = format!(" {} {} ", label, stats);

    List::new(items)
        .block(
            Block::default()
                .title(Span::styled(block_title, title_style))
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
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
        format!("({}/{} | {}% | {}/{} done)", elapsed_str, estimate_str, pct, done, total)
    } else {
        format!("({} | {}/{})", elapsed_str, done, total)
    }
}

fn task_num_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(Color::Green),
        TaskStatus::Done => Style::default().fg(Color::DarkGray),
        TaskStatus::Paused => Style::default().fg(Color::Yellow),
        TaskStatus::Pending => Style::default().fg(Color::Gray),
    }
}

fn task_title_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        TaskStatus::Done => Style::default().fg(Color::DarkGray),
        TaskStatus::Paused => Style::default().fg(Color::Yellow),
        TaskStatus::Pending => Style::default().fg(Color::White),
    }
}

fn build_compact_meta(task: &Task) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = vec![];
    let gray = Style::default().fg(Color::DarkGray);

    if let Some(p) = task.priority {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        let pri_style = match p {
            4 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            3 => Style::default().fg(Color::LightRed),
            2 => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Gray),
        };
        spans.push(Span::styled("!".repeat(p.clamp(1, 4) as usize), pri_style));
    }
    if let Some(ref p) = task.project {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        spans.push(Span::styled(format!("+{}", p), Style::default().fg(Color::Magenta)));
    }
    if let Some(est) = task.estimate_minutes {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        spans.push(Span::styled(format!("~{}", format_est(est)), gray));
    }

    let elapsed = task.elapsed_seconds.unwrap_or(0);
    if elapsed > 0 {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        // Color elapsed based on estimate progress
        let elapsed_style = match task.estimate_minutes {
            Some(est) if elapsed > est * 60 => Style::default().fg(Color::Red), // over estimate
            Some(est) if elapsed > est * 45 => Style::default().fg(Color::Yellow), // >75%
            _ => Style::default().fg(Color::Green),
        };
        spans.push(Span::styled(format!("({})", format_dur(elapsed)), elapsed_style));
    }

    let today = chrono::Local::now().date_naive();
    let seven_days = today + chrono::Duration::days(7);

    if let Some(ref dl) = task.deadline {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        let dl_style = if *dl < today {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if *dl <= seven_days {
            Style::default().fg(Color::Yellow)
        } else {
            gray
        };
        spans.push(Span::styled(format!("^{}", dl.format("%b%d")), dl_style));
    }
    if let Some(ref sc) = task.scheduled {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        spans.push(Span::styled(format!("={}", sc.format("%b%d")), Style::default().fg(Color::Cyan)));
    }

    spans
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

/// Short format for pane stats (no seconds)
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
