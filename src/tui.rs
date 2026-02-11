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
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

use crate::db::Database;
use crate::task::{Area, Task};

pub enum FocusArea {
    LongTerm,
    ThisWeek,
    Today,
}

pub fn run_tui(db: &Database) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(db);
    app.refresh_tasks()?;

    // Run the loop
    let res = run_app(&mut terminal, &mut app, db);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

struct App<'a> {
    focus: FocusArea,
    tasks: Vec<Task>,
    list_state: ratatui::widgets::ListState,
    running_task: Option<String>,
    db: &'a Database,
}

impl<'a> App<'a> {
    fn new(db: &'a Database) -> Self {
        let mut list_state = ratatui::widgets::ListState::default();
        list_state.select(Some(0));
        
        Self {
            focus: FocusArea::Today,
            tasks: Vec::new(),
            list_state,
            running_task: None,
            db,
        }
    }

    fn refresh_tasks(&mut self) -> Result<()> {
        self.tasks = match self.focus {
            FocusArea::Today => self.db.list_tasks(Some(crate::cli::Area::Today))?,
            FocusArea::ThisWeek => self.db.list_tasks(Some(crate::cli::Area::ThisWeek))?,
            FocusArea::LongTerm => self.db.list_tasks(Some(crate::cli::Area::LongTerm))?,
        };
        
        // Update running task
        if let Ok(Some((title, _))) = self.db.get_running_task() {
            self.running_task = Some(title);
        } else {
            self.running_task = None;
        }
        
        Ok(())
    }

    fn next_task(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.tasks.len().saturating_sub(1) {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous_task(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.tasks.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn switch_focus(&mut self, focus: FocusArea) {
        self.focus = focus;
        self.list_state.select(Some(0));
        let _ = self.refresh_tasks();
    }

    fn start_selected(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            if let Some(task) = self.tasks.get(i) {
                // Stop any running task first
                let _ = self.db.pause_timer();
                // Start this one
                let _ = self.db.start_timer(&task.title);
                self.refresh_tasks()?;
            }
        }
        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        self.db.pause_timer()?;
        self.refresh_tasks()?;
        Ok(())
    }

    fn done(&mut self) -> Result<()> {
        self.db.complete_task()?;
        self.refresh_tasks()?;
        Ok(())
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App, _db: &Database) -> Result<()> {
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
                    KeyCode::Char('j') | KeyCode::Down => app.next_task(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous_task(),
                    KeyCode::Char('s') => { let _ = app.start_selected(); }
                    KeyCode::Char('p') => { let _ = app.pause(); }
                    KeyCode::Char('d') => { let _ = app.done(); }
                    KeyCode::Char('l') => app.switch_focus(FocusArea::LongTerm),
                    KeyCode::Char('w') => app.switch_focus(FocusArea::ThisWeek),
                    KeyCode::Char('t') => app.switch_focus(FocusArea::Today),
                    KeyCode::Char('r') => { let _ = app.refresh_tasks(); }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            // Periodic refresh (for timer updates)
            last_tick = std::time::Instant::now();
        }
    }
}

fn draw_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.area());

    // Header
    let header = build_header(app);
    f.render_widget(header, chunks[0]);

    // Main area split
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(20)])
        .split(chunks[1]);

    // Sidebar
    let sidebar = build_sidebar(app);
    f.render_widget(sidebar, main_chunks[0]);

    // Task list
    let task_list = build_task_list(app);
    f.render_stateful_widget(task_list, main_chunks[1], &mut app.list_state.clone());
}

fn build_header(app: &App) -> Paragraph {
    let focus_name = match app.focus {
        FocusArea::LongTerm => "LONG TERM",
        FocusArea::ThisWeek => "THIS WEEK",
        FocusArea::Today => "TODAY",
    };

    let running_info = if let Some(ref task) = app.running_task {
        format!(" ▶ {} ", task)
    } else {
        String::new()
    };

    let text = Line::from(vec![
        Span::styled(
            format!(" DODO | {} ", focus_name),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(running_info, Style::default().fg(Color::Green)),
    ]);

    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}

fn build_sidebar(app: &App) -> Paragraph {
    let long_active = matches!(app.focus, FocusArea::LongTerm);
    let week_active = matches!(app.focus, FocusArea::ThisWeek);
    let today_active = matches!(app.focus, FocusArea::Today);

    let long_style = if long_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let week_style = if week_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let today_style = if today_active {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let text = vec![
        Line::from(vec![Span::styled("  LONG  ", long_style)]),
        Line::from(vec![Span::styled("  WEEK  ", week_style)]),
        Line::from(vec![Span::styled("> TODAY <", today_style),]),
        Line::from(""),
        Line::from(vec![Span::styled("j/k: nav", Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled("s: start", Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled("p: pause", Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled("d: done", Style::default().fg(Color::DarkGray))]),
        Line::from(vec![Span::styled("q: quit", Style::default().fg(Color::DarkGray))]),
    ];

    Paragraph::new(text).block(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}

fn build_task_list(app: &App) -> List {
    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let style = if task.status == crate::task::TaskStatus::Running {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            
            let prefix = if Some(i) == app.list_state.selected() {
                "> "
            } else {
                "  "
            };
            
            ListItem::new(format!("{}{}", prefix, task)).style(style)
        })
        .collect();

    List::new(items).block(
        Block::default()
            .title(format!(" Tasks ({}) ", app.tasks.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}
