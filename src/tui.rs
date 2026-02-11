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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;

use dodo::cli::{Area as CliArea, SortBy};
use dodo::db::Database;
use dodo::task::{Task, TaskStatus};

const PANE_LABELS: [&str; 4] = ["LONG TERM", "THIS WEEK", "TODAY", "DONE"];
const PANE_AREAS: [CliArea; 4] = [CliArea::LongTerm, CliArea::ThisWeek, CliArea::Today, CliArea::Completed];
const SORT_MODES: [SortBy; 3] = [SortBy::Created, SortBy::Modified, SortBy::Title];

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
}

struct App<'a> {
    panes: [PaneState; 4],
    active_pane: usize,
    sort_index: usize,
    running_task: Option<String>,
    db: &'a Database,
}

impl<'a> App<'a> {
    fn new(db: &'a Database) -> Self {
        Self {
            panes: [PaneState::new(), PaneState::new(), PaneState::new(), PaneState::new()],
            active_pane: 2, // Start on TODAY
            sort_index: 0,
            running_task: None,
            db,
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
        for (i, area) in PANE_AREAS.iter().enumerate() {
            self.panes[i].tasks = self.db.list_tasks_sorted(Some(*area), sort)?;
            // Clamp selection
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

    fn start_selected(&mut self) -> Result<()> {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            let num_id = task.num_id.map(|n| n.to_string()).unwrap_or_default();
            if !num_id.is_empty() {
                let _ = self.db.start_timer(&num_id);
                self.refresh_all()?;
            }
        }
        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        self.db.pause_timer()?;
        self.refresh_all()?;
        Ok(())
    }

    fn done(&mut self) -> Result<()> {
        self.db.complete_task()?;
        self.refresh_all()?;
        Ok(())
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = std::time::Duration::from_millis(250);

    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| std::time::Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('j') | KeyCode::Down => app.panes[app.active_pane].next(),
                    KeyCode::Char('k') | KeyCode::Up => app.panes[app.active_pane].previous(),
                    KeyCode::Char('h') | KeyCode::Left => app.move_pane_left(),
                    KeyCode::Char('l') | KeyCode::Right => app.move_pane_right(),
                    KeyCode::Char('s') => { let _ = app.start_selected(); }
                    KeyCode::Char('p') => { let _ = app.pause(); }
                    KeyCode::Char('d') => { let _ = app.done(); }
                    KeyCode::Char('o') => app.cycle_sort(),
                    KeyCode::Char('r') => { let _ = app.refresh_all(); }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = std::time::Instant::now();
        }
    }
}

fn draw_ui(f: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.area());

    // Header
    let header = build_header(app);
    f.render_widget(header, outer[0]);

    // Four panes
    let pane_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(outer[1]);

    for i in 0..4 {
        let is_active = i == app.active_pane;
        let pane_widget = build_pane(&app.panes[i], PANE_LABELS[i], is_active);
        f.render_stateful_widget(pane_widget, pane_chunks[i], &mut app.panes[i].list_state.clone());
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
        format!(" ▶ {} ", task)
    } else {
        String::new()
    };

    let text = Line::from(vec![
        Span::styled(
            " DODO ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(running_info, Style::default().fg(Color::Green)),
        Span::styled(
            format!("  sort:{} ", sort_label),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            " h/l:pane j/k:nav s:start p:pause d:done o:sort r:refresh q:quit ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}

fn build_pane(pane: &PaneState, label: &str, is_active: bool) -> List<'static> {
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
            let status_icon = match task.status {
                TaskStatus::Pending => " ",
                TaskStatus::Running => "▶",
                TaskStatus::Paused => "⏸",
                TaskStatus::Done => "✓",
            };

            let num = task.num_id.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
            let notes_mark = match &task.notes {
                Some(n) if !n.is_empty() => " *",
                _ => "",
            };

            // Line 1: num_id status_icon title [notes_mark]
            let line1 = Line::from(vec![
                Span::styled(
                    format!("{:>3} {} ", num, status_icon),
                    task_num_style(task),
                ),
                Span::styled(
                    format!("{}{}", task.title, notes_mark),
                    task_title_style(task),
                ),
            ]);

            // Line 2: metadata (only if any present)
            let meta = build_compact_meta(task);
            if meta.is_empty() {
                ListItem::new(vec![line1])
            } else {
                let line2 = Line::from(vec![
                    Span::raw("      "),
                    Span::styled(meta, Style::default().fg(Color::DarkGray)),
                ]);
                ListItem::new(vec![line1, line2])
            }
        })
        .collect();

    let block_title = format!(" {} ({}) ", label, pane.tasks.len());

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

fn task_num_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(Color::Green),
        TaskStatus::Done => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Gray),
    }
}

fn task_title_style(task: &Task) -> Style {
    match task.status {
        TaskStatus::Running => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        TaskStatus::Done => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn build_compact_meta(task: &Task) -> String {
    let mut parts = vec![];

    if let Some(p) = task.priority {
        parts.push("!".repeat(p.clamp(1, 4) as usize));
    }
    if let Some(ref p) = task.project {
        parts.push(format!("+{}", p));
    }
    if let Some(est) = task.estimate_minutes {
        parts.push(format!("~{}", format_est(est)));
    }

    let elapsed = task.elapsed_seconds.unwrap_or(0);
    if elapsed > 0 {
        parts.push(format!("({})", format_dur(elapsed)));
    }

    if let Some(ref dl) = task.deadline {
        parts.push(format!("^{}", dl.format("%b%d")));
    }
    if let Some(ref sc) = task.scheduled {
        parts.push(format!("={}", sc.format("%b%d")));
    }

    parts.join(" ")
}

fn format_dur(seconds: i64) -> String {
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
