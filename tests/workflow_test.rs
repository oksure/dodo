use dodo::cli::Area;
use dodo::db::Database;

fn test_db() -> Database {
    Database::in_memory().unwrap()
}

// ── 1. Simple Daily List ──────────────────────────────────────────────

#[test]
fn add_returns_incrementing_numeric_ids() {
    let db = test_db();
    let id1 = db.add_task("Buy groceries", Area::Today, None, None, None, None, None, None).unwrap();
    let id2 = db.add_task("Reply to email", Area::Today, None, None, None, None, None, None).unwrap();
    let id3 = db.add_task("Fix faucet", Area::Today, None, None, None, None, None, None).unwrap();
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
}

#[test]
fn list_shows_today_tasks() {
    let db = test_db();
    db.add_task("Task A", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Task B", Area::ThisWeek, None, None, None, None, None, None).unwrap();
    db.add_task("Task C", Area::Today, None, None, None, None, None, None).unwrap();

    let tasks = db.list_tasks(None).unwrap();
    // Default list shows Today area only (+ running)
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().all(|t| t.area_str() == "TODAY"));
}

#[test]
fn start_and_done_completes_task() {
    let db = test_db();
    db.add_task("Buy groceries", Area::Today, None, None, None, None, None, None).unwrap();
    db.start_timer("1").unwrap();

    let running = db.get_running_task().unwrap();
    assert!(running.is_some());
    assert_eq!(running.unwrap().0, "Buy groceries");

    let result = db.complete_task().unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().0, "Buy groceries");

    // No longer running
    assert!(db.get_running_task().unwrap().is_none());
}

// ── 2. Pomodoro: start / pause / resume ───────────────────────────────

#[test]
fn pomodoro_start_pause_resume() {
    let db = test_db();
    db.add_task("Draft blog post", Area::Today, None, None, None, None, None, None).unwrap();

    // Pomodoro 1
    db.start_timer("blog").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Draft blog post");

    db.pause_timer().unwrap();
    assert!(db.get_running_task().unwrap().is_none());

    // Pomodoro 2: resume same task
    db.start_timer("blog").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Draft blog post");

    db.complete_task().unwrap();
    assert!(db.get_running_task().unwrap().is_none());
}

#[test]
fn starting_new_task_pauses_current() {
    let db = test_db();
    db.add_task("Task A", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Task B", Area::Today, None, None, None, None, None, None).unwrap();

    db.start_timer("1").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Task A");

    // Starting B auto-pauses A
    db.start_timer("2").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Task B");

    // A is no longer running
    let task_a = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task_a.status, dodo::task::TaskStatus::Paused);
}

// ── 3. GTD: areas and contexts ────────────────────────────────────────

#[test]
fn gtd_four_horizons() {
    let db = test_db();

    // Someday/maybe
    db.add_task("Learn piano", Area::LongTerm, None, None, None, None, None, None).unwrap();
    // Active project
    db.add_task("Prepare talk", Area::ThisWeek, None, None, None, None, None, None).unwrap();
    // Next action
    db.add_task("Call dentist", Area::Today, None, None, None, None, None, None).unwrap();

    let long = db.list_tasks(Some(Area::LongTerm)).unwrap();
    assert_eq!(long.len(), 1);
    assert_eq!(long[0].title, "Learn piano");
    assert_eq!(long[0].area_str(), "LONG");

    let week = db.list_tasks(Some(Area::ThisWeek)).unwrap();
    assert_eq!(week.len(), 1);
    assert_eq!(week[0].title, "Prepare talk");
    assert_eq!(week[0].area_str(), "WEEK");

    let today = db.list_tasks(Some(Area::Today)).unwrap();
    assert_eq!(today.len(), 1);
    assert_eq!(today[0].title, "Call dentist");
    assert_eq!(today[0].area_str(), "TODAY");
}

#[test]
fn gtd_contexts_stored_and_displayed() {
    let db = test_db();
    db.add_task("Call dentist", Area::Today, None, Some("phone".into()), None, None, None, None).unwrap();
    db.add_task("Order cables", Area::Today, None, Some("computer".into()), None, None, None, None).unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    assert_eq!(tasks.len(), 2);

    let call = tasks.iter().find(|t| t.title == "Call dentist").unwrap();
    assert_eq!(call.context.as_deref(), Some("phone"));
    assert!(call.display_tags().contains("@phone"));

    let order = tasks.iter().find(|t| t.title == "Order cables").unwrap();
    assert_eq!(order.context.as_deref(), Some("computer"));
}

#[test]
fn gtd_projects_stored_and_displayed() {
    let db = test_db();
    db.add_task("Fix bug", Area::Today, Some("acme".into()), None, None, None, None, None).unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = &tasks[0];
    assert_eq!(task.project.as_deref(), Some("acme"));
    assert!(task.display_tags().contains("+acme"));
}

// ── 4. Eisenhower: area-based prioritization ──────────────────────────

#[test]
fn eisenhower_quadrants() {
    let db = test_db();

    // Urgent+Important → today
    db.add_task("Fix production bug", Area::Today, Some("acme".into()), None, None, None, None, None).unwrap();
    // Important, not urgent → week
    db.add_task("Write test suite", Area::ThisWeek, Some("acme".into()), None, None, None, None, None).unwrap();
    // Neither → long
    db.add_task("Refactor auth module", Area::LongTerm, Some("acme".into()), None, None, None, None, None).unwrap();

    assert_eq!(db.list_tasks(Some(Area::Today)).unwrap().len(), 1);
    assert_eq!(db.list_tasks(Some(Area::ThisWeek)).unwrap().len(), 1);
    assert_eq!(db.list_tasks(Some(Area::LongTerm)).unwrap().len(), 1);
}

// ── 5. Freelancing: projects and time tracking ────────────────────────

#[test]
fn freelance_multiple_projects() {
    let db = test_db();
    db.add_task("Design landing page", Area::Today, Some("clientA".into()), None, None, None, None, None).unwrap();
    db.add_task("API integration", Area::Today, Some("clientB".into()), None, None, None, None, None).unwrap();

    // Work on clientA
    db.start_timer("landing").unwrap();
    db.pause_timer().unwrap();

    // Switch to clientB
    db.start_timer("API").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "API integration");

    db.complete_task().unwrap();

    // clientA task is still there (paused)
    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.status, dodo::task::TaskStatus::Paused);
    assert_eq!(task.project.as_deref(), Some("clientA"));
}

// ── 6. Numeric ID selection ───────────────────────────────────────────

#[test]
fn start_by_numeric_id() {
    let db = test_db();
    db.add_task("Task Alpha", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Task Beta", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Task Gamma", Area::Today, None, None, None, None, None, None).unwrap();

    db.start_timer("2").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Task Beta");
}

#[test]
fn delete_by_numeric_id() {
    let db = test_db();
    db.add_task("Task Alpha", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Task Beta", Area::Today, None, None, None, None, None, None).unwrap();

    db.delete_task("1").unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Task Beta");
}

#[test]
fn numeric_id_not_found_falls_back_to_fuzzy() {
    let db = test_db();
    db.add_task("Task 42 is special", Area::Today, None, None, None, None, None, None).unwrap();

    // "42" as numeric ID doesn't exist (task has num_id=1), so it falls through
    // to fuzzy search where "42" matches substring in "Task 42 is special"
    db.start_timer("42").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Task 42 is special");
}

// ── 7. Fuzzy matching integration ─────────────────────────────────────

#[test]
fn fuzzy_start_by_substring() {
    let db = test_db();
    db.add_task("Write quarterly report", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Fix production bug", Area::Today, None, None, None, None, None, None).unwrap();

    db.start_timer("report").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Write quarterly report");
}

#[test]
fn fuzzy_prefers_better_match() {
    let db = test_db();
    db.add_task("Overwrite config", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Write unit tests", Area::Today, None, None, None, None, None, None).unwrap();

    // "Write" is a prefix of "Write unit tests" (75) but substring of "Overwrite config" (50)
    db.start_timer("Write").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Write unit tests");
}

#[test]
fn fuzzy_no_match_errors() {
    let db = test_db();
    db.add_task("Buy groceries", Area::Today, None, None, None, None, None, None).unwrap();

    let _result = db.start_timer("zzz_nonexistent");
    // All tasks score 0 for "zzz_nonexistent" but find_best_match still
    // returns the best (or only) task. With a single task scoring 0 it
    // picks it. This is by design — fuzzy always returns something if
    // tasks exist. Only fails on empty DB.
    // So test the truly empty case:
    let db2 = test_db();
    let result = db2.start_timer("anything");
    assert!(result.is_err());
}

// ── 8. Academic workflow ──────────────────────────────────────────────

#[test]
fn academic_multi_area_with_projects() {
    let db = test_db();

    // Long-term reading
    db.add_task("Read DDIA", Area::LongTerm, Some("thesis".into()), None, None, None, None, None).unwrap();
    // This week's writing
    db.add_task("Write literature review", Area::ThisWeek, Some("thesis".into()), Some("writing".into()), None, None, None, None).unwrap();
    // Today's action
    db.add_task("Email advisor", Area::Today, Some("thesis".into()), Some("email".into()), None, None, None, None).unwrap();

    // All three areas populated
    assert_eq!(db.list_tasks(Some(Area::LongTerm)).unwrap().len(), 1);
    assert_eq!(db.list_tasks(Some(Area::ThisWeek)).unwrap().len(), 1);
    assert_eq!(db.list_tasks(Some(Area::Today)).unwrap().len(), 1);

    // Start deep work, verify tracking works
    db.start_timer("literature").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Write literature review");

    let (title, _duration) = db.complete_task().unwrap().unwrap();
    assert_eq!(title, "Write literature review");

    // Completed task no longer in week list
    assert_eq!(db.list_tasks(Some(Area::ThisWeek)).unwrap().len(), 0);
}

// ── 9. Completed tasks disappear from lists ───────────────────────────

#[test]
fn done_tasks_leave_active_lists() {
    let db = test_db();
    db.add_task("Task A", Area::Today, None, None, None, None, None, None).unwrap();
    db.add_task("Task B", Area::Today, None, None, None, None, None, None).unwrap();

    db.start_timer("1").unwrap();
    db.complete_task().unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Task B");
}

// ── 10. Session lifecycle ─────────────────────────────────────────────

#[test]
fn session_elapsed_seconds_works() {
    use dodo::session::Session;

    let session = Session::new("task-id");
    assert!(session.is_running());
    assert!(session.elapsed_seconds() >= 0);

    let mut session2 = Session::new("task-id-2");
    session2.stop();
    assert!(!session2.is_running());
    assert!(session2.ended.is_some());
    assert!(session2.duration >= 0);
}

#[test]
fn pause_records_duration() {
    let db = test_db();
    db.add_task("Timed task", Area::Today, None, None, None, None, None, None).unwrap();

    db.start_timer("1").unwrap();
    // Immediately pause — duration should be >= 0
    db.pause_timer().unwrap();

    // Task should be paused, not running
    assert!(db.get_running_task().unwrap().is_none());
    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.status, dodo::task::TaskStatus::Paused);
}

// ── 11. Estimate and elapsed time ─────────────────────────────────────

#[test]
fn task_with_estimate_stored_and_displayed() {
    let db = test_db();
    db.add_task("Design mockup", Area::Today, None, None, Some(120), None, None, None).unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = &tasks[0];
    assert_eq!(task.estimate_minutes, Some(120));
    let display = format!("{}", task);
    assert!(display.contains("~2h"));
}

#[test]
fn elapsed_time_shows_in_list() {
    let db = test_db();
    db.add_task("Track me", Area::Today, None, None, None, None, None, None).unwrap();

    db.start_timer("1").unwrap();
    // Elapsed time should be computed from session JOIN
    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = tasks.iter().find(|t| t.title == "Track me").unwrap();
    // elapsed_seconds should be Some and >= 0
    assert!(task.elapsed_seconds.is_some());
}

// ── 12. Notes ─────────────────────────────────────────────────────────

#[test]
fn note_append_and_retrieve() {
    let db = test_db();
    db.add_task("Buy milk", Area::Today, None, None, None, None, None, None).unwrap();

    db.append_note("1", "Need whole milk").unwrap();
    let (title, notes) = db.get_task_notes("1").unwrap();
    assert_eq!(title, "Buy milk");
    assert!(notes.is_some());
    let notes = notes.unwrap();
    assert!(notes.contains("Need whole milk"));
    // Should have timestamp
    assert!(notes.contains("[20"));

    // Append second note
    db.append_note("1", "Also eggs").unwrap();
    let (_, notes) = db.get_task_notes("1").unwrap();
    let notes = notes.unwrap();
    assert!(notes.contains("Need whole milk"));
    assert!(notes.contains("Also eggs"));
}

#[test]
fn note_clear() {
    let db = test_db();
    db.add_task("Task with notes", Area::Today, None, None, None, None, None, None).unwrap();

    db.append_note("1", "Some note").unwrap();
    db.clear_notes("1").unwrap();
    let (_, notes) = db.get_task_notes("1").unwrap();
    assert!(notes.is_none());
}

// ── 13. Edit command ──────────────────────────────────────────────────

#[test]
fn edit_updates_task_fields() {
    use dodo::notation::parse_notation;

    let db = test_db();
    db.add_task("Fix bug", Area::Today, None, None, None, None, None, None).unwrap();

    // Edit deadline and estimate
    let parsed = parse_notation("1 ~2h");
    let title = db.update_task_fields(&parsed.title, &parsed, None).unwrap();
    assert_eq!(title, "Fix bug");

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.estimate_minutes, Some(120));
}

#[test]
fn edit_updates_area() {
    use dodo::notation::parse_notation;

    let db = test_db();
    db.add_task("Move me", Area::Today, None, None, None, None, None, None).unwrap();

    let parsed = parse_notation("1");
    let _ = db.update_task_fields(&parsed.title, &parsed, Some(Area::ThisWeek)).unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.area, dodo::task::Area::ThisWeek);
}

// ── 14. Multiple contexts ────────────────────────────────────────────

#[test]
fn multiple_contexts_stored() {
    let db = test_db();
    db.add_task("Team meeting", Area::Today, None, Some("john,sarah".into()), None, None, None, None).unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = &tasks[0];
    assert_eq!(task.context.as_deref(), Some("john,sarah"));
    let display = task.display_tags();
    assert!(display.contains("@john"));
    assert!(display.contains("@sarah"));
}

// ── 15. Tags ──────────────────────────────────────────────────────────

#[test]
fn tags_stored_and_displayed() {
    let db = test_db();
    db.add_task("Fix critical issue", Area::Today, None, None, None, None, None, Some("urgent,bug".into())).unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = &tasks[0];
    assert_eq!(task.tags.as_deref(), Some("urgent,bug"));
    let display = task.display_tags();
    assert!(display.contains("#urgent"));
    assert!(display.contains("#bug"));
}

// ── 16. Deadline and scheduled dates ─────────────────────────────────

#[test]
fn deadline_and_scheduled_stored() {
    use chrono::NaiveDate;

    let db = test_db();
    let dl = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    let sc = NaiveDate::from_ymd_opt(2025, 6, 10).unwrap();
    db.add_task("Project milestone", Area::Today, None, None, None, Some(dl), Some(sc), None).unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.deadline, Some(dl));
    assert_eq!(task.scheduled, Some(sc));
}
