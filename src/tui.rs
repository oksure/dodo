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

use dodo::cli::SortBy;
use dodo::db::Database;
use dodo::task::{Area, Task, TaskStatus};

const PANE_LABELS: [&str; 4] = ["LONG TERM", "THIS WEEK", "TODAY", "DONE"];
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

#[derive(PartialEq)]
enum AppMode {
    Normal,
    NoteView,
    NoteEdit,
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

    fn toggle_selected(&mut self) -> Result<()> {
        if let Some(task) = self.panes[self.active_pane].selected_task() {
            if task.status == TaskStatus::Running {
                // Pause running task
                self.db.pause_timer()?;
            } else {
                let num_id = task.num_id.map(|n| n.to_string()).unwrap_or_default();
                if !num_id.is_empty() {
                    // Set scheduled to today so it appears in TODAY pane
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
                // Refresh note content
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

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut last_data_refresh = std::time::Instant::now();
    let poll_rate = std::time::Duration::from_millis(16); // ~60fps for responsive input
    let data_refresh_rate = std::time::Duration::from_secs(1); // refresh data every 1s

    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if crossterm::event::poll(poll_rate)? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
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

        // Refresh data periodically (every 1s) for elapsed time updates
        if last_data_refresh.elapsed() >= data_refresh_rate {
            let _ = app.refresh_all();
            last_data_refresh = std::time::Instant::now();
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

    // Note modal overlay
    if app.mode == AppMode::NoteView || app.mode == AppMode::NoteEdit {
        draw_note_modal(f, app);
    }
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

    // Clear the area behind the modal
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
        // Split inner area: existing notes + input line
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(3)])
            .split(inner);

        // Existing notes
        let content = if app.note_content.is_empty() {
            "(no notes yet)".to_string()
        } else {
            app.note_content.clone()
        };
        let notes_widget = Paragraph::new(content)
            .style(Style::default().fg(Color::Gray))
            .wrap(ratatui::widgets::Wrap { trim: false });
        f.render_widget(notes_widget, chunks[0]);

        // Input line
        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray));
        let input_text = format!(">{}", app.note_input);
        let input_widget = Paragraph::new(input_text)
            .style(Style::default().fg(Color::White))
            .block(input_block);
        f.render_widget(input_widget, chunks[1]);
    } else {
        // NoteView: just show notes
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
            " h/l:pane j/k:nav s:start/stop d:done n:note o:sort r:refresh q:quit ",
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

            // Line 2: metadata spans with date colors
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

fn build_compact_meta(task: &Task) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = vec![];
    let gray = Style::default().fg(Color::DarkGray);

    if let Some(p) = task.priority {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        spans.push(Span::styled("!".repeat(p.clamp(1, 4) as usize), gray));
    }
    if let Some(ref p) = task.project {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        spans.push(Span::styled(format!("+{}", p), gray));
    }
    if let Some(est) = task.estimate_minutes {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        spans.push(Span::styled(format!("~{}", format_est(est)), gray));
    }

    let elapsed = task.elapsed_seconds.unwrap_or(0);
    if elapsed > 0 {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        spans.push(Span::styled(format!("({})", format_dur(elapsed)), gray));
    }

    let today = chrono::Local::now().date_naive();
    let seven_days = today + chrono::Duration::days(7);

    if let Some(ref dl) = task.deadline {
        if !spans.is_empty() { spans.push(Span::styled(" ", gray)); }
        let dl_style = if *dl < today {
            Style::default().fg(Color::Red) // overdue
        } else if *dl <= seven_days {
            Style::default().fg(Color::Yellow) // upcoming
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
