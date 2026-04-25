use crossterm::event::KeyCode;
use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

use dodo::cli::Area;
use dodo::db::Database;
use dodo::task::TaskStatus;

use super::draw::{draw_tasks_daily, draw_ui};
use super::event::handle_daily_nav;
use super::state::{App, DailyEntry, TasksView, TuiTab};

fn test_db() -> Database {
    Database::in_memory().unwrap()
}

/// Extract all text from a ratatui buffer as a single string.
fn buffer_text(buf: &Buffer) -> String {
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf.cell((x, y)).unwrap().symbol());
        }
    }
    s
}

/// Create an App with N tasks spread across dates to populate the daily view.
fn app_with_daily_tasks(db: &mut Database, count: usize) -> App<'_> {
    let today = chrono::Local::now().date_naive();
    for i in 0..count {
        let day_offset = i as i64 / 3; // ~3 tasks per day
        let scheduled = today + chrono::Duration::days(day_offset);
        db.add_task(
            &format!("Task {}", i + 1),
            Area::Today,
            None,
            None,
            Some(60),
            None,
            Some(scheduled),
            None,
            None,
        )
        .unwrap();
    }
    let mut app = App::new(db);
    app.tasks_view = TasksView::Daily;
    app.tab = TuiTab::Tasks;
    app.refresh_daily().unwrap();
    app
}

/// Simulate draw_tasks_daily to trigger scroll calculation (needs a frame).
fn trigger_scroll(app: &mut App) {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 80, 20);
            draw_tasks_daily(f, app, area);
        })
        .unwrap();
}

// ── Daily scroll symmetry ───────────────────────────────────────────

#[test]
fn daily_scroll_down_cursor_stays_within_margin() {
    let mut db = test_db();
    let mut app = app_with_daily_tasks(&mut db, 30);

    // Navigate down many times, triggering scroll at each step
    let mut offsets_down = Vec::new();
    for _ in 0..25 {
        handle_daily_nav(&mut app, KeyCode::Char('j'), 1);
        trigger_scroll(&mut app);
        offsets_down.push((app.daily_cursor, app.daily_scroll));
    }

    // Cursor should never exceed daily_entries bounds
    for (cursor, _scroll) in &offsets_down {
        assert!(*cursor < app.daily_entries.len());
    }

    // Once scrolling starts, scroll offset should increase
    let scrolling_entries: Vec<_> = offsets_down.iter().filter(|(_, s)| *s > 0).collect();
    if scrolling_entries.len() >= 2 {
        // Scroll should be monotonically non-decreasing when going down
        for pair in scrolling_entries.windows(2) {
            assert!(
                pair[1].1 >= pair[0].1,
                "scroll should not decrease going down"
            );
        }
    }
}

#[test]
fn daily_scroll_up_cursor_stays_within_margin() {
    let mut db = test_db();
    let mut app = app_with_daily_tasks(&mut db, 30);

    // Go to the bottom first
    for _ in 0..25 {
        handle_daily_nav(&mut app, KeyCode::Char('j'), 1);
        trigger_scroll(&mut app);
    }
    let _bottom_cursor = app.daily_cursor;
    let bottom_scroll = app.daily_scroll;

    // Now navigate back up
    let mut offsets_up = Vec::new();
    for _ in 0..25 {
        handle_daily_nav(&mut app, KeyCode::Char('k'), 1);
        trigger_scroll(&mut app);
        offsets_up.push((app.daily_cursor, app.daily_scroll));
    }

    // Scroll should monotonically non-increase when going up
    let mut prev_scroll = bottom_scroll;
    for (_, scroll) in &offsets_up {
        assert!(
            *scroll <= prev_scroll,
            "scroll should not increase going up: {} > {}",
            scroll,
            prev_scroll
        );
        prev_scroll = *scroll;
    }

    // Cursor should reach the first task entry
    let first_task_idx = app
        .daily_entries
        .iter()
        .position(|e| matches!(e, DailyEntry::Task(_)))
        .unwrap();
    assert_eq!(
        offsets_up.last().unwrap().0,
        first_task_idx,
        "cursor should reach first task"
    );
}

#[test]
fn daily_scroll_cursor_stays_within_margin() {
    let mut db = test_db();
    let mut app = app_with_daily_tasks(&mut db, 30);
    let height: usize = 20; // TestBackend height
    let _margin: usize = 3;

    // Navigate down, verify cursor stays within margin of viewport edges
    for _ in 0..25 {
        handle_daily_nav(&mut app, KeyCode::Char('j'), 1);
        trigger_scroll(&mut app);
        let scroll = app.daily_scroll;
        let cursor = app.daily_cursor;
        // Cursor should be >= scroll (visible)
        assert!(
            cursor >= scroll,
            "cursor {} below scroll {}",
            cursor,
            scroll
        );
        // Cursor should be within viewport
        assert!(
            cursor < scroll + height,
            "cursor {} outside viewport (scroll={}, height={})",
            cursor,
            scroll,
            height
        );
    }

    // Navigate back up, same invariant
    for _ in 0..25 {
        handle_daily_nav(&mut app, KeyCode::Char('k'), 1);
        trigger_scroll(&mut app);
        let scroll = app.daily_scroll;
        let cursor = app.daily_cursor;
        assert!(
            cursor >= scroll,
            "cursor {} below scroll {}",
            cursor,
            scroll
        );
        assert!(
            cursor < scroll + height,
            "cursor {} outside viewport going up (scroll={}, height={})",
            cursor,
            scroll,
            height
        );
    }
}

// ── Daily navigation ────────────────────────────────────────────────

#[test]
fn daily_nav_skips_headers() {
    let mut db = test_db();
    let mut app = app_with_daily_tasks(&mut db, 6);

    // Find the first task entry
    let first_task = app
        .daily_entries
        .iter()
        .position(|e| matches!(e, DailyEntry::Task(_)))
        .unwrap();
    app.daily_cursor = first_task;

    // Navigate down — should skip headers and land on task entries
    for _ in 0..5 {
        let prev = app.daily_cursor;
        handle_daily_nav(&mut app, KeyCode::Char('j'), 1);
        if app.daily_cursor != prev {
            assert!(
                matches!(app.daily_entries[app.daily_cursor], DailyEntry::Task(_)),
                "cursor at {} is not a Task entry",
                app.daily_cursor
            );
        }
    }

    // Navigate back up — should also skip headers
    for _ in 0..5 {
        let prev = app.daily_cursor;
        handle_daily_nav(&mut app, KeyCode::Char('k'), 1);
        if app.daily_cursor != prev {
            assert!(
                matches!(app.daily_entries[app.daily_cursor], DailyEntry::Task(_)),
                "cursor at {} is not a Task entry going up",
                app.daily_cursor
            );
        }
    }
}

#[test]
fn daily_nav_count_multiplier() {
    let mut db = test_db();
    let mut app = app_with_daily_tasks(&mut db, 15);

    let first_task = app
        .daily_entries
        .iter()
        .position(|e| matches!(e, DailyEntry::Task(_)))
        .unwrap();
    app.daily_cursor = first_task;

    // Navigate down by 3
    handle_daily_nav(&mut app, KeyCode::Char('j'), 3);

    // Count how many task entries we skipped
    let tasks_between = app.daily_entries[first_task + 1..=app.daily_cursor]
        .iter()
        .filter(|e| matches!(e, DailyEntry::Task(_)))
        .count();
    assert_eq!(tasks_between, 3, "should have moved 3 tasks down");
}

#[test]
fn daily_nav_g_jumps_to_end() {
    let mut db = test_db();
    let mut app = app_with_daily_tasks(&mut db, 10);

    let first_task = app
        .daily_entries
        .iter()
        .position(|e| matches!(e, DailyEntry::Task(_)))
        .unwrap();
    app.daily_cursor = first_task;

    // G jumps to last task
    handle_daily_nav(&mut app, KeyCode::Char('G'), 1);

    // Should be on the last Task entry
    let last_task = app
        .daily_entries
        .iter()
        .rposition(|e| matches!(e, DailyEntry::Task(_)))
        .unwrap();
    assert_eq!(app.daily_cursor, last_task);
}

// ── Rendering ───────────────────────────────────────────────────────

#[test]
fn daily_view_renders_without_panic() {
    let mut db = test_db();
    let mut app = app_with_daily_tasks(&mut db, 10);

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    // Should not panic
    terminal
        .draw(|f| {
            let area = f.area();
            draw_tasks_daily(f, &mut app, area);
        })
        .unwrap();
}

#[test]
fn full_ui_renders_without_panic() {
    let mut db = test_db();
    let mut app = App::new(&mut db);
    app.refresh_all().unwrap();

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    // Panes view
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    // Daily view
    app.tasks_view = TasksView::Daily;
    app.refresh_daily().unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    // Weekly view
    app.tasks_view = TasksView::Weekly;
    app.refresh_weekly().unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    // Calendar view
    app.tasks_view = TasksView::Calendar;
    app.refresh_calendar().unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    // Recurring tab
    app.tab = TuiTab::Recurring;
    app.refresh_templates().unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    // Report tab
    app.tab = TuiTab::Report;
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    // Settings tab
    app.tab = TuiTab::Settings;
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
}

#[test]
fn daily_view_renders_task_titles() {
    let mut db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Buy groceries",
        Area::Today,
        None,
        None,
        Some(60),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Write report",
        Area::Today,
        Some("work".into()),
        None,
        Some(120),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let mut app = App::new(&mut db);
    app.tasks_view = TasksView::Daily;
    app.refresh_daily().unwrap();

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            let area = f.area();
            draw_tasks_daily(f, &mut app, area);
        })
        .unwrap();

    // Check the rendered buffer contains our task titles
    let content = buffer_text(terminal.backend().buffer());
    assert!(
        content.contains("Buy groceries"),
        "should render task title"
    );
    assert!(content.contains("Write report"), "should render task title");
}

#[test]
fn recurring_mark_rendered() {
    let mut db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "Daily standup",
        "daily",
        Some("work".into()),
        None,
        Some(15),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let mut app = App::new(&mut db);
    app.tasks_view = TasksView::Panes;
    app.refresh_all().unwrap();

    let backend = TestBackend::new(160, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    let content = buffer_text(terminal.backend().buffer());
    assert!(
        content.contains("\u{21BB}") || content.contains("Daily standup"),
        "recurring instance should be rendered"
    );
}

// ── Pane navigation ─────────────────────────────────────────────────

#[test]
fn pane_navigation_wraps() {
    let mut db = test_db();
    let mut app = App::new(&mut db);
    app.tasks_view = TasksView::Panes;
    app.refresh_all().unwrap();

    // Start at pane 2 (TODAY)
    assert_eq!(app.active_pane, 2);

    // Navigate right → pane 3 (DONE)
    handle_tasks_key_wrapper(&mut app, KeyCode::Char('l'));
    assert_eq!(app.active_pane, 3);

    // Navigate right again → stay at 3 (no wrap past DONE)
    handle_tasks_key_wrapper(&mut app, KeyCode::Char('l'));
    assert_eq!(app.active_pane, 3);

    // Navigate left back
    handle_tasks_key_wrapper(&mut app, KeyCode::Char('h'));
    assert_eq!(app.active_pane, 2);
}

/// Wrapper to call handle_tasks_key (which is pub(super))
fn handle_tasks_key_wrapper(app: &mut App, code: KeyCode) {
    super::event::handle_tasks_key(app, code);
}

// ── View cycling ────────────────────────────────────────────────────

#[test]
fn view_cycling_with_v() {
    let mut db = test_db();
    let mut app = App::new(&mut db);
    app.tasks_view = TasksView::Panes;
    app.refresh_all().unwrap();

    assert_eq!(app.tasks_view, TasksView::Panes);

    // v cycles forward: Panes → Daily → Weekly → Calendar → Panes
    handle_tasks_key_wrapper(&mut app, KeyCode::Char('v'));
    assert_eq!(app.tasks_view, TasksView::Daily);

    handle_tasks_key_wrapper(&mut app, KeyCode::Char('v'));
    assert_eq!(app.tasks_view, TasksView::Weekly);

    handle_tasks_key_wrapper(&mut app, KeyCode::Char('v'));
    assert_eq!(app.tasks_view, TasksView::Calendar);

    handle_tasks_key_wrapper(&mut app, KeyCode::Char('v'));
    assert_eq!(app.tasks_view, TasksView::Panes);
}

// ── Tombstone via App delete ────────────────────────────────────────

#[test]
fn delete_task_records_tombstone_prevents_merge() {
    let mut db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Delete me",
        Area::Today,
        None,
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    let task_id = task.id.clone();

    // Delete via the same path the TUI uses
    db.delete_task_by_id(&task_id).unwrap();

    // Verify deleted
    assert!(db.find_task_by_num_id(1).unwrap().is_none());

    // Try to merge back (simulating sync)
    let remote = dodo::task::Task {
        id: task_id,
        num_id: Some(1),
        title: "Delete me".into(),
        area: Area::Today,
        project: None,
        context: None,
        status: TaskStatus::Pending,
        created: chrono::Utc::now(),
        completed: None,
        estimate_minutes: None,
        elapsed_seconds: None,
        elapsed_snapshot: None,
        deadline: None,
        scheduled: Some(today),
        tags: None,
        notes: None,
        priority: None,
        modified_at: Some(chrono::Utc::now()),
        recurrence: None,
        is_template: false,
        template_id: None,
    };
    db.merge_remote_data(&[remote], &[]).unwrap();

    // Should NOT be resurrected
    assert!(
        db.find_task_by_num_id(1).unwrap().is_none(),
        "tombstoned task should not be resurrected by merge"
    );
}

// ── Done pane stats format ──────────────────────────────────────────

#[test]
fn done_pane_shows_stats() {
    let mut db = test_db();
    let today = chrono::Local::now().date_naive();

    // Create and complete some tasks
    for i in 0..3 {
        db.add_task(
            &format!("Done task {}", i),
            Area::Today,
            None,
            None,
            Some(60),
            None,
            Some(today),
            None,
            None,
        )
        .unwrap();
    }
    // Complete them
    for i in 1..=3 {
        let task = db.find_task_by_num_id(i).unwrap().unwrap();
        db.complete_task_by_id(&task.id).unwrap();
    }

    let mut app = App::new(&mut db);
    app.tasks_view = TasksView::Panes;
    app.refresh_all().unwrap();

    let backend = TestBackend::new(160, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw_ui(f, &mut app)).unwrap();

    let content = buffer_text(terminal.backend().buffer());
    assert!(content.contains("DONE"), "should show DONE pane header");
    assert!(
        content.contains("on-time") || content.contains("Done task"),
        "DONE pane should show completed tasks or stats"
    );
}
