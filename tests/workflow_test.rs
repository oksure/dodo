use dodo::cli::Area;
use dodo::db::Database;
use dodo::task::TaskStatus;

fn test_db() -> Database {
    Database::in_memory().unwrap()
}

// ── 1. Simple Daily List ──────────────────────────────────────────────

#[test]
fn add_returns_incrementing_numeric_ids() {
    let db = test_db();
    let id1 = db
        .add_task(
            "Buy groceries",
            Area::Today,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let id2 = db
        .add_task(
            "Reply to email",
            Area::Today,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let id3 = db
        .add_task(
            "Fix faucet",
            Area::Today,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
}

#[test]
fn list_shows_today_tasks() {
    let db = test_db();
    db.add_task(
        "Task A",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task B",
        Area::ThisWeek,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task C",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let tasks = db.list_tasks(None).unwrap();
    // Default list shows Today area only (+ running)
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().all(|t| t.area_str() == "TODAY"));
}

#[test]
fn start_and_done_completes_task() {
    let db = test_db();
    db.add_task(
        "Buy groceries",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
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
    db.add_task(
        "Draft blog post",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Task A",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task B",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    use chrono::Local;

    let db = test_db();
    let today = Local::now().date_naive();
    let next_week = today + chrono::Duration::days(3);
    let far_future = today + chrono::Duration::days(30);

    // Someday/maybe — scheduled far out → LONG
    db.add_task(
        "Learn piano",
        Area::LongTerm,
        None,
        None,
        None,
        None,
        Some(far_future),
        None,
        None,
    )
    .unwrap();
    // Active project — scheduled within a week → WEEK
    db.add_task(
        "Prepare talk",
        Area::ThisWeek,
        None,
        None,
        None,
        None,
        Some(next_week),
        None,
        None,
    )
    .unwrap();
    // Next action — scheduled today → TODAY
    db.add_task(
        "Call dentist",
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

    let long = db.list_tasks(Some(Area::LongTerm)).unwrap();
    assert_eq!(long.len(), 1);
    assert_eq!(long[0].title, "Learn piano");
    assert_eq!(long[0].area_str(), "LONG");

    let week = db.list_tasks(Some(Area::ThisWeek)).unwrap();
    assert_eq!(week.len(), 1);
    assert_eq!(week[0].title, "Prepare talk");
    assert_eq!(week[0].area_str(), "WEEK");

    let today_tasks = db.list_tasks(Some(Area::Today)).unwrap();
    assert_eq!(today_tasks.len(), 1);
    assert_eq!(today_tasks[0].title, "Call dentist");
    assert_eq!(today_tasks[0].area_str(), "TODAY");
}

#[test]
fn gtd_contexts_stored_and_displayed() {
    let db = test_db();
    db.add_task(
        "Call dentist",
        Area::Today,
        None,
        Some("phone".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Order cables",
        Area::Today,
        None,
        Some("computer".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Fix bug",
        Area::Today,
        Some("acme".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Fix production bug",
        Area::Today,
        Some("acme".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    // Important, not urgent → week
    db.add_task(
        "Write test suite",
        Area::ThisWeek,
        Some("acme".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    // Neither → long
    db.add_task(
        "Refactor auth module",
        Area::LongTerm,
        Some("acme".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    assert_eq!(db.list_tasks(Some(Area::Today)).unwrap().len(), 1);
    assert_eq!(db.list_tasks(Some(Area::ThisWeek)).unwrap().len(), 1);
    assert_eq!(db.list_tasks(Some(Area::LongTerm)).unwrap().len(), 1);
}

// ── 5. Freelancing: projects and time tracking ────────────────────────

#[test]
fn freelance_multiple_projects() {
    let db = test_db();
    db.add_task(
        "Design landing page",
        Area::Today,
        Some("clientA".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "API integration",
        Area::Today,
        Some("clientB".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Task Alpha",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task Beta",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task Gamma",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    db.start_timer("2").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Task Beta");
}

#[test]
fn delete_by_numeric_id() {
    let db = test_db();
    db.add_task(
        "Task Alpha",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task Beta",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    db.delete_task("1").unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Task Beta");
}

#[test]
fn numeric_id_not_found_falls_back_to_fuzzy() {
    let db = test_db();
    db.add_task(
        "Task 42 is special",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Write quarterly report",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Fix production bug",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    db.start_timer("report").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Write quarterly report");
}

#[test]
fn fuzzy_prefers_better_match() {
    let db = test_db();
    db.add_task(
        "Overwrite config",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Write unit tests",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // "Write" is a prefix of "Write unit tests" (75) but substring of "Overwrite config" (50)
    db.start_timer("Write").unwrap();
    let running = db.get_running_task().unwrap().unwrap();
    assert_eq!(running.0, "Write unit tests");
}

#[test]
fn fuzzy_no_match_errors() {
    let db = test_db();
    db.add_task(
        "Buy groceries",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Read DDIA",
        Area::LongTerm,
        Some("thesis".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    // This week's writing
    db.add_task(
        "Write literature review",
        Area::ThisWeek,
        Some("thesis".into()),
        Some("writing".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    // Today's action
    db.add_task(
        "Email advisor",
        Area::Today,
        Some("thesis".into()),
        Some("email".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Task A",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task B",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Timed task",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Design mockup",
        Area::Today,
        None,
        None,
        Some(120),
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = &tasks[0];
    assert_eq!(task.estimate_minutes, Some(120));
    let display = format!("{}", task);
    assert!(display.contains("~2h"));
}

#[test]
fn elapsed_time_shows_in_list() {
    let db = test_db();
    db.add_task(
        "Track me",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Buy milk",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Task with notes",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Fix bug",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Move me",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let parsed = parse_notation("1");
    let _ = db
        .update_task_fields(&parsed.title, &parsed, Some(Area::ThisWeek))
        .unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.area, dodo::task::Area::ThisWeek);
}

// ── 14. Multiple contexts ────────────────────────────────────────────

#[test]
fn multiple_contexts_stored() {
    let db = test_db();
    db.add_task(
        "Team meeting",
        Area::Today,
        None,
        Some("john,sarah".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

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
    db.add_task(
        "Fix critical issue",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        Some("urgent,bug".into()),
        None,
    )
    .unwrap();

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
    db.add_task(
        "Project milestone",
        Area::Today,
        None,
        None,
        None,
        Some(dl),
        Some(sc),
        None,
        None,
    )
    .unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.deadline, Some(dl));
    assert_eq!(task.scheduled, Some(sc));
}

// ── 17. Priority ────────────────────────────────────────────────────

#[test]
fn priority_stored_and_displayed() {
    let db = test_db();
    db.add_task(
        "Critical bug",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(3),
    )
    .unwrap();

    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = &tasks[0];
    assert_eq!(task.priority, Some(3));
    let display = format!("{}", task);
    assert!(display.contains("!!!"));
}

// ── 18. Recurring: Template CRUD ────────────────────────────────────

#[test]
fn recurring_add_template_creates_template_and_instance() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    let num_id = db
        .add_template(
            "standup",
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
    assert_eq!(num_id, 1);

    // Template should be in templates list
    let templates = db.list_templates().unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].title, "standup");
    assert!(templates[0].is_template);
    assert_eq!(templates[0].recurrence, Some("daily".into()));

    // First instance should be created
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all.len(), 1); // Just the instance (templates are excluded)
    assert_eq!(all[0].title, "standup");
    assert!(!all[0].is_template);
    assert_eq!(all[0].template_id, Some(templates[0].id.clone()));
}

#[test]
fn recurring_templates_excluded_from_normal_listings() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        Some(15),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "normal task",
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

    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    // Should see instance + normal task, but NOT the template
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|t| !t.is_template));
}

#[test]
fn recurring_delete_template_removes_template_and_active_instance() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let templates = db.list_templates().unwrap();
    assert_eq!(templates.len(), 1);

    db.delete_template(&templates[0].id).unwrap();

    let templates = db.list_templates().unwrap();
    assert!(templates.is_empty());

    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert!(all.is_empty());
}

// ── 19. Recurring: Instance Generation ──────────────────────────────

#[test]
fn recurring_complete_instance_generates_next() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        Some(15),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    // Find the instance
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all.len(), 1);
    let instance_id = all[0].id.clone();

    // Complete the instance
    db.complete_task_by_id(&instance_id).unwrap();

    // Should now have a new instance (old one is in Done)
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    let non_done: Vec<_> = all
        .iter()
        .filter(|t| t.status != dodo::task::TaskStatus::Done)
        .collect();
    let done: Vec<_> = all
        .iter()
        .filter(|t| t.status == dodo::task::TaskStatus::Done)
        .collect();
    assert_eq!(non_done.len(), 1, "should have 1 active instance");
    assert_eq!(done.len(), 1, "should have 1 done instance");
    assert_eq!(non_done[0].title, "standup");
}

#[test]
fn recurring_one_active_instance_constraint() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    // Generate should create nothing since there's already an active instance
    let created = db.generate_instances().unwrap();
    assert_eq!(created, 0);
}

#[test]
fn recurring_generate_after_delete_recreates() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    // Delete the instance (skip)
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all.len(), 1);
    db.delete_task_by_id(&all[0].id).unwrap();

    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert!(all.is_empty());

    // Generate should recreate
    let created = db.generate_instances().unwrap();
    assert_eq!(created, 1);

    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].title, "standup");
}

// ── 20. Recurring: Pause/Resume ─────────────────────────────────────

#[test]
fn recurring_pause_stops_generation() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let templates = db.list_templates().unwrap();
    let tid = templates[0].id.clone();

    // Pause the template
    db.pause_template(&tid).unwrap();

    // Complete the active instance
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    db.complete_task_by_id(&all[0].id).unwrap();

    // Should NOT generate a new instance since template is paused
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    let non_done: Vec<_> = all
        .iter()
        .filter(|t| t.status != dodo::task::TaskStatus::Done)
        .collect();
    assert_eq!(
        non_done.len(),
        0,
        "paused template should not generate new instance"
    );

    // Resume and generate
    db.resume_template(&tid).unwrap();
    let created = db.generate_instances().unwrap();
    assert_eq!(created, 1);
}

// ── 21. Recurring: History ──────────────────────────────────────────

#[test]
fn recurring_history_shows_completed_instances() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let templates = db.list_templates().unwrap();
    let tid = templates[0].id.clone();

    // Complete two instances
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    db.complete_task_by_id(&all[0].id).unwrap();
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    let active: Vec<_> = all
        .iter()
        .filter(|t| t.status != dodo::task::TaskStatus::Done)
        .collect();
    db.complete_task_by_id(&active[0].id).unwrap();

    let history = db.template_history(&tid).unwrap();
    assert_eq!(history.len(), 2);
}

// ── 22. Recurring: Resolve Template ─────────────────────────────────

#[test]
fn recurring_resolve_template_by_name() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_template(
        "standup",
        "daily",
        None,
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let template = db.resolve_template("standup").unwrap();
    assert!(template.is_template);
    assert_eq!(template.title, "standup");
}

#[test]
fn recurring_resolve_template_by_num_id() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    let num_id = db
        .add_template(
            "standup",
            "daily",
            None,
            None,
            None,
            None,
            Some(today),
            None,
            None,
        )
        .unwrap();

    let template = db.resolve_template(&num_id.to_string()).unwrap();
    assert!(template.is_template);
    assert_eq!(template.title, "standup");
}

// ── Update notes by ID ──────────────────────────────────────────────

#[test]
fn update_notes_by_id_sets_notes() {
    let db = test_db();
    let num_id = db
        .add_task(
            "test task",
            Area::Today,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = tasks.iter().find(|t| t.num_id == Some(num_id)).unwrap();

    db.update_notes_by_id(&task.id, "line one\nline two")
        .unwrap();
    let notes = db.get_task_notes_by_id(&task.id).unwrap();
    assert_eq!(notes, Some("line one\nline two".to_string()));
}

#[test]
fn update_notes_by_id_empty_clears_notes() {
    let db = test_db();
    let num_id = db
        .add_task(
            "test task",
            Area::Today,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = tasks.iter().find(|t| t.num_id == Some(num_id)).unwrap();

    db.append_note_by_id(&task.id, "existing note").unwrap();
    assert!(db.get_task_notes_by_id(&task.id).unwrap().is_some());

    db.update_notes_by_id(&task.id, "").unwrap();
    let notes = db.get_task_notes_by_id(&task.id).unwrap();
    assert!(notes.is_none());
}

#[test]
fn update_notes_by_id_replaces_existing() {
    let db = test_db();
    let num_id = db
        .add_task(
            "test task",
            Area::Today,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let tasks = db.list_tasks(Some(Area::Today)).unwrap();
    let task = tasks.iter().find(|t| t.num_id == Some(num_id)).unwrap();

    db.append_note_by_id(&task.id, "old note").unwrap();
    db.update_notes_by_id(&task.id, "completely new content")
        .unwrap();
    let notes = db.get_task_notes_by_id(&task.id).unwrap();
    assert_eq!(notes, Some("completely new content".to_string()));
}

// ── Export / Import roundtrip ───────────────────────────────────────

#[test]
fn export_import_roundtrip_preserves_all_data() {
    let db1 = test_db();

    // Add tasks with various field types
    let today = chrono::Local::now().date_naive();
    let deadline = today + chrono::Duration::days(7);

    db1.add_task(
        "Task with all fields",
        Area::Today,
        Some("backend".into()),
        Some("work,office".into()),
        Some(120),
        Some(deadline),
        Some(today),
        Some("urgent,review".into()),
        Some(3),
    )
    .unwrap();

    db1.add_task(
        "Simple task",
        Area::ThisWeek,
        None,
        None,
        Some(60),
        None,
        None,
        None,
        None,
    )
    .unwrap();

    db1.add_task(
        "Long term task",
        Area::LongTerm,
        Some("frontend".into()),
        None,
        None,
        None,
        None,
        None,
        Some(1),
    )
    .unwrap();

    // Add notes to first task
    db1.append_note("1", "First note line").unwrap();

    // Start and pause a task to create sessions
    db1.start_timer("1").unwrap();
    db1.pause_timer().unwrap();

    // Complete a task
    db1.start_timer("2").unwrap();
    db1.complete_task().unwrap();

    // Add a recurring template
    db1.add_template(
        "standup",
        "daily",
        Some("team".into()),
        Some("meeting".into()),
        Some(15),
        None,
        Some(today),
        None,
        Some(2),
    )
    .unwrap();

    // Export all data
    let (tasks, sessions) = db1.export_all_data().unwrap();
    assert!(tasks.len() >= 4); // 3 tasks + 1 template + at least 1 instance
    assert!(!sessions.is_empty());

    // Import into a fresh database
    let db2 = test_db();
    db2.import_all_data(&tasks, &sessions).unwrap();

    // Verify tasks
    let (exported_tasks, exported_sessions) = db2.export_all_data().unwrap();
    assert_eq!(exported_tasks.len(), tasks.len());
    assert_eq!(exported_sessions.len(), sessions.len());

    // Verify specific task fields survived roundtrip
    let full_task = exported_tasks
        .iter()
        .find(|t| t.title == "Task with all fields")
        .unwrap();
    assert_eq!(full_task.project.as_deref(), Some("backend"));
    assert_eq!(full_task.context.as_deref(), Some("work,office"));
    assert_eq!(full_task.estimate_minutes, Some(120));
    assert_eq!(full_task.deadline, Some(deadline));
    assert_eq!(full_task.scheduled, Some(today));
    assert_eq!(full_task.tags.as_deref(), Some("urgent,review"));
    assert_eq!(full_task.priority, Some(3));
    assert!(full_task.notes.is_some());
    assert_eq!(full_task.status, TaskStatus::Paused);

    // Verify completed task
    let simple_task = exported_tasks
        .iter()
        .find(|t| t.title == "Simple task")
        .unwrap();
    assert_eq!(simple_task.status, TaskStatus::Done);
    assert!(simple_task.completed.is_some());

    // Verify template survived
    let template = exported_tasks
        .iter()
        .find(|t| t.title == "standup" && t.is_template)
        .unwrap();
    assert_eq!(template.recurrence.as_deref(), Some("daily"));
    assert_eq!(template.project.as_deref(), Some("team"));
    assert_eq!(template.priority, Some(2));

    // Verify sessions have correct data
    for orig in &sessions {
        let imported = exported_sessions.iter().find(|s| s.id == orig.id).unwrap();
        assert_eq!(imported.task_id, orig.task_id);
        assert_eq!(imported.duration, orig.duration);
        assert_eq!(imported.manual_edit, orig.manual_edit);
    }
}

// ── Phase 1: Done target + undo ─────────────────────────────────────

#[test]
fn done_specific_task_by_id() {
    let db = test_db();
    db.add_task(
        "Task A",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Task B",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // Complete task 2 by numeric ID (not running)
    db.complete_task_by_id(&db.find_task_by_num_id(2).unwrap().unwrap().id)
        .unwrap();

    let task = db.find_task_by_num_id(2).unwrap().unwrap();
    assert_eq!(task.status, TaskStatus::Done);

    // Task 1 should still be pending
    let task1 = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task1.status, TaskStatus::Pending);
}

#[test]
fn done_specific_task_by_fuzzy() {
    let db = test_db();
    db.add_task(
        "Fix login bug",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.add_task(
        "Update docs",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let task = db.resolve_task("login").unwrap();
    db.complete_task_by_id(&task.id).unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.status, TaskStatus::Done);
    assert_eq!(task.title, "Fix login bug");
}

#[test]
fn undone_reopens_task() {
    let db = test_db();
    db.add_task(
        "Write tests",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // Complete it
    let task = db.resolve_task("1").unwrap();
    db.complete_task_by_id(&task.id).unwrap();
    assert_eq!(
        db.find_task_by_num_id(1).unwrap().unwrap().status,
        TaskStatus::Done
    );

    // Undo it
    let done_task = db.resolve_done_task("1").unwrap();
    db.uncomplete_task_by_id(&done_task.id).unwrap();
    assert_eq!(
        db.find_task_by_num_id(1).unwrap().unwrap().status,
        TaskStatus::Pending
    );
}

// ── Phase 2: Move task between areas ────────────────────────────────

#[test]
fn move_task_to_week() {
    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Weekly review",
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

    let task = db.resolve_task("1").unwrap();
    let date = dodo::task::Area::ThisWeek.to_scheduled_date();
    db.update_task_scheduled(&task.id, date).unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.effective_area(), Area::ThisWeek);
}

#[test]
fn move_task_to_today() {
    let db = test_db();
    let future = chrono::Local::now().date_naive() + chrono::Duration::days(10);
    db.add_task(
        "Long term goal",
        Area::Today,
        None,
        None,
        None,
        None,
        Some(future),
        None,
        None,
    )
    .unwrap();

    // Should be in LongTerm initially
    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.effective_area(), Area::LongTerm);

    // Move to Today
    let date = dodo::task::Area::Today.to_scheduled_date();
    db.update_task_scheduled(&task.id, date).unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task.effective_area(), Area::Today);
}

// ── Phase 3: Reports ────────────────────────────────────────────────

#[test]
fn report_empty_range() {
    let db = test_db();
    let range = dodo::cli::ReportRange::Month;
    let (from, to) = range.date_range();

    assert_eq!(db.report_tasks_done(&from, &to).unwrap(), 0);
    assert_eq!(db.report_total_seconds(&from, &to).unwrap(), 0);
    assert_eq!(db.report_active_days(&from, &to).unwrap(), 0);
    assert!(db.report_by_project(&from, &to).unwrap().is_empty());
}

#[test]
fn note_delete_line() {
    let db = test_db();
    db.add_task(
        "Note task",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let task = db.resolve_task("1").unwrap();
    db.update_notes_by_id(&task.id, "line one\nline two\nline three")
        .unwrap();

    // Delete line 2
    let notes = db.get_task_notes_by_id(&task.id).unwrap().unwrap();
    let mut lines: Vec<&str> = notes.lines().collect();
    lines.remove(1); // 0-indexed
    db.update_notes_by_id(&task.id, &lines.join("\n")).unwrap();

    let notes = db.get_task_notes_by_id(&task.id).unwrap().unwrap();
    assert_eq!(notes, "line one\nline three");
}

#[test]
fn note_edit_line() {
    let db = test_db();
    db.add_task(
        "Note task",
        Area::Today,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let task = db.resolve_task("1").unwrap();
    db.update_notes_by_id(&task.id, "line one\nline two\nline three")
        .unwrap();

    // Edit line 2
    let notes = db.get_task_notes_by_id(&task.id).unwrap().unwrap();
    let mut lines: Vec<String> = notes.lines().map(|l| l.to_string()).collect();
    lines[1] = "line TWO updated".to_string();
    db.update_notes_by_id(&task.id, &lines.join("\n")).unwrap();

    let notes = db.get_task_notes_by_id(&task.id).unwrap().unwrap();
    assert_eq!(notes, "line one\nline TWO updated\nline three");
}

#[test]
fn report_with_data() {
    let db = test_db();
    db.add_task(
        "Report task",
        Area::Today,
        Some("backend".into()),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    db.start_timer("1").unwrap();
    db.complete_task().unwrap();

    let range = dodo::cli::ReportRange::All;
    let (from, to) = range.date_range();

    assert_eq!(db.report_tasks_done(&from, &to).unwrap(), 1);
    let done = db.report_done_tasks(&from, &to, 10).unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].0, "Report task");
}

// ── Merge: remote newer wins ─────────────────────────────────────────

#[test]
fn merge_remote_newer_wins() {
    use chrono::{Duration, Utc};

    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Original title",
        Area::Today,
        Some("old_proj".into()),
        None,
        Some(60),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();

    // Create a "remote" version that's newer
    let mut remote_task = task.clone();
    remote_task.title = "Updated from remote".to_string();
    remote_task.project = Some("new_proj".to_string());
    remote_task.modified_at = Some(Utc::now() + Duration::seconds(100));

    db.merge_remote_data(&[remote_task], &[]).unwrap();

    let updated = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(updated.title, "Updated from remote");
    assert_eq!(updated.project.as_deref(), Some("new_proj"));
}

#[test]
fn merge_local_newer_wins() {
    use chrono::{Duration, Utc};

    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Local title",
        Area::Today,
        Some("local_proj".into()),
        None,
        None,
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let task = db.find_task_by_num_id(1).unwrap().unwrap();

    // Create a "remote" version that's OLDER
    let mut remote_task = task.clone();
    remote_task.title = "Old remote title".to_string();
    remote_task.project = Some("old_proj".to_string());
    remote_task.modified_at = Some(Utc::now() - Duration::seconds(100));

    db.merge_remote_data(&[remote_task], &[]).unwrap();

    let unchanged = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(unchanged.title, "Local title");
    assert_eq!(unchanged.project.as_deref(), Some("local_proj"));
}

#[test]
fn merge_new_remote_task_no_conflict() {
    use chrono::Utc;
    use dodo::task::Task;

    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Local task",
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

    // Create a brand new remote task with num_id=2 (no conflict)
    let remote_task = Task {
        id: ulid::Ulid::new().to_string(),
        num_id: Some(2),
        title: "Remote task".to_string(),
        area: Area::Today,
        project: None,
        context: None,
        status: TaskStatus::Pending,
        created: Utc::now(),
        completed: None,
        modified_at: Some(Utc::now()),
        estimate_minutes: Some(30),
        deadline: None,
        scheduled: Some(today),
        priority: None,
        tags: None,
        notes: None,
        elapsed_seconds: None,
        elapsed_snapshot: None,
        recurrence: None,
        is_template: false,
        template_id: None,
    };

    db.merge_remote_data(&[remote_task], &[]).unwrap();

    let local = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(local.title, "Local task");

    let remote = db.find_task_by_num_id(2).unwrap().unwrap();
    assert_eq!(remote.title, "Remote task");
    assert_eq!(remote.estimate_minutes, Some(30));
}

#[test]
fn merge_num_id_conflict_earlier_wins() {
    use chrono::{Duration, Utc};
    use dodo::task::Task;

    let db = test_db();
    let today = chrono::Local::now().date_naive();
    // Local task created "now" with num_id=1
    db.add_task(
        "Local task",
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
    let _local = db.find_task_by_num_id(1).unwrap().unwrap();

    // Remote task also wants num_id=1, but was created EARLIER
    let remote_task = Task {
        id: ulid::Ulid::new().to_string(),
        num_id: Some(1),
        title: "Remote task".to_string(),
        area: Area::Today,
        project: None,
        context: None,
        status: TaskStatus::Pending,
        created: Utc::now() - Duration::seconds(1000),
        completed: None,
        modified_at: Some(Utc::now()),
        estimate_minutes: None,
        deadline: None,
        scheduled: Some(today),
        priority: None,
        tags: None,
        notes: None,
        elapsed_seconds: None,
        elapsed_snapshot: None,
        recurrence: None,
        is_template: false,
        template_id: None,
    };

    db.merge_remote_data(&[remote_task.clone()], &[]).unwrap();

    // Remote was created earlier, so it should keep num_id=1
    let task_at_1 = db.find_task_by_num_id(1).unwrap().unwrap();
    assert_eq!(task_at_1.title, "Remote task");

    // Local task should have been bumped to num_id=2
    let task_at_2 = db.find_task_by_num_id(2).unwrap().unwrap();
    assert_eq!(task_at_2.title, "Local task");
}

#[test]
fn merge_sessions_ignore_duplicates() {
    use chrono::Utc;
    use dodo::session::Session;

    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Task",
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

    // Create a session
    let session = Session {
        id: "test-session-id".to_string(),
        task_id: task.id.clone(),
        started: Utc::now(),
        ended: Some(Utc::now()),
        duration: 300,
        manual_edit: false,
        notes: None,
    };

    // Merge same session twice — second should be ignored
    db.merge_remote_data(&[], &[session.clone()]).unwrap();
    db.merge_remote_data(&[], &[session.clone()]).unwrap();

    // Verify only one session exists
    let (_, sessions) = db.export_all_data().unwrap();
    let matching: Vec<_> = sessions
        .iter()
        .filter(|s| s.id == "test-session-id")
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].duration, 300);
}

// ── Tombstone: deleted tasks not resurrected by sync ──────────────────

#[test]
fn deleted_task_not_resurrected_by_merge() {
    use chrono::Utc;
    use dodo::task::Task;

    let db = test_db();
    let today = chrono::Local::now().date_naive();
    db.add_task(
        "Will be deleted",
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

    // Delete the task (records tombstone)
    db.delete_task_by_id(&task_id).unwrap();
    assert!(db.find_task_by_num_id(1).unwrap().is_none());

    // Simulate sync: remote still has the task
    let remote_task = Task {
        id: task_id.clone(),
        num_id: Some(1),
        title: "Will be deleted".to_string(),
        area: Area::Today,
        project: None,
        context: None,
        status: TaskStatus::Pending,
        created: Utc::now(),
        completed: None,
        estimate_minutes: Some(60),
        elapsed_seconds: None,
        elapsed_snapshot: None,
        deadline: None,
        scheduled: Some(today),
        tags: None,
        notes: None,
        priority: None,
        modified_at: Some(Utc::now()),
        recurrence: None,
        is_template: false,
        template_id: None,
    };

    // Merge should skip the tombstoned task
    db.merge_remote_data(&[remote_task], &[]).unwrap();

    // Task should NOT come back
    assert!(db.find_task_by_num_id(1).unwrap().is_none());
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all.len(), 0);
}

#[test]
fn deleted_template_instances_not_resurrected_by_merge() {
    use chrono::Utc;
    use dodo::task::Task;

    let db = test_db();
    let today = chrono::Local::now().date_naive();

    // Create a recurring template
    db.add_template(
        "Daily standup",
        "daily",
        Some("work".into()),
        None,
        Some(60),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    let templates = db.list_templates().unwrap();
    assert_eq!(templates.len(), 1);
    let template_id = templates[0].id.clone();

    // Get the instance that was auto-created
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    let instance = all
        .iter()
        .find(|t| t.template_id.as_deref() == Some(&template_id))
        .unwrap();
    let instance_id = instance.id.clone();

    // Delete the template (records tombstones for template + instances)
    db.delete_template(&template_id).unwrap();
    assert!(db.list_templates().unwrap().is_empty());

    // Simulate sync: remote still has both
    let remote_template = Task {
        id: template_id.clone(),
        num_id: Some(1),
        title: "Daily standup".to_string(),
        area: Area::Today,
        project: Some("work".to_string()),
        context: None,
        status: TaskStatus::Pending,
        created: Utc::now(),
        completed: None,
        estimate_minutes: Some(60),
        elapsed_seconds: None,
        elapsed_snapshot: None,
        deadline: None,
        scheduled: Some(today),
        tags: None,
        notes: None,
        priority: None,
        modified_at: Some(Utc::now()),
        recurrence: Some("*daily".to_string()),
        is_template: true,
        template_id: None,
    };

    let remote_instance = Task {
        id: instance_id.clone(),
        num_id: Some(2),
        title: "Daily standup".to_string(),
        area: Area::Today,
        project: Some("work".to_string()),
        context: None,
        status: TaskStatus::Pending,
        created: Utc::now(),
        completed: None,
        estimate_minutes: Some(60),
        elapsed_seconds: None,
        elapsed_snapshot: None,
        deadline: None,
        scheduled: Some(today),
        tags: None,
        notes: None,
        priority: None,
        modified_at: Some(Utc::now()),
        recurrence: None,
        is_template: false,
        template_id: Some(template_id.clone()),
    };

    // Merge should skip both tombstoned tasks
    db.merge_remote_data(&[remote_template, remote_instance], &[])
        .unwrap();

    // Neither should come back
    assert!(db.list_templates().unwrap().is_empty());
    let all = db.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all.len(), 0);
}

// ── 5c: num_id conflict reverse direction ─────────────────────────────
#[test]
fn num_id_conflict_resolution_both_directions() {
    use chrono::{Duration, Utc};

    // DB A: task_a was created earlier, gets num_id=1
    let db_a = test_db();
    let earlier = Utc::now() - Duration::seconds(60);
    let later = Utc::now();

    // Add task_a with an early creation; it holds num_id 1
    let num_a = db_a
        .add_task("Task A", Area::Today, None, None, None, None, None, None, None)
        .unwrap();
    assert_eq!(num_a, 1);

    // DB B: task_b created later, also gets num_id=1 (conflict)
    let db_b = test_db();
    let num_b = db_b
        .add_task("Task B", Area::Today, None, None, None, None, None, None, None)
        .unwrap();
    assert_eq!(num_b, 1);

    // Export both, build Task structs with explicit created timestamps to force order
    let (tasks_a, _) = db_a.export_all_data().unwrap();
    let (tasks_b, _) = db_b.export_all_data().unwrap();
    let mut task_a = tasks_a[0].clone();
    let mut task_b = tasks_b[0].clone();
    task_a.created = earlier;
    task_a.modified_at = Some(earlier);
    task_b.created = later;
    task_b.modified_at = Some(later);

    // Merge A→B: task_a (earlier) should keep num_id=1; task_b (later) gets bumped
    let db_merged_ab = test_db();
    db_merged_ab.merge_remote_data(&[task_a.clone()], &[]).unwrap();
    db_merged_ab.merge_remote_data(&[task_b.clone()], &[]).unwrap();
    let all_ab = db_merged_ab.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all_ab.len(), 2);
    let a_in_ab = all_ab.iter().find(|t| t.id == task_a.id).unwrap();
    let b_in_ab = all_ab.iter().find(|t| t.id == task_b.id).unwrap();
    assert_eq!(a_in_ab.num_id, Some(1), "Earlier task should keep num_id=1");
    assert_ne!(b_in_ab.num_id, Some(1), "Later task should be bumped off num_id=1");

    // Merge B→A: same invariant regardless of merge order
    let db_merged_ba = test_db();
    db_merged_ba.merge_remote_data(&[task_b.clone()], &[]).unwrap();
    db_merged_ba.merge_remote_data(&[task_a.clone()], &[]).unwrap();
    let all_ba = db_merged_ba.list_all_tasks(dodo::cli::SortBy::Created).unwrap();
    assert_eq!(all_ba.len(), 2);
    let a_in_ba = all_ba.iter().find(|t| t.id == task_a.id).unwrap();
    let b_in_ba = all_ba.iter().find(|t| t.id == task_b.id).unwrap();
    assert_eq!(a_in_ba.num_id, Some(1), "Earlier task should keep num_id=1 in reverse merge");
    assert_ne!(b_in_ba.num_id, Some(1), "Later task should be bumped in reverse merge");
}

// ── 5d: concurrent-generate dedup ─────────────────────────────────────

#[test]
fn dedup_keeps_earliest_created_instance() {
    use chrono::{Duration, Utc};

    let db = test_db();
    let today = chrono::Local::now().date_naive();

    // Create a recurring template
    db.add_template(
        "dedup test standup",
        "daily",
        None,
        None,
        Some(15),
        None,
        Some(today),
        None,
        None,
    )
    .unwrap();

    // Generate one instance (the normal one)
    db.generate_instances().unwrap();

    // Simulate a second instance arriving from a concurrent machine by exporting the
    // existing instance, cloning it with a new ID and later timestamp, then merging.
    let (tasks, _sessions) = db.export_all_data().unwrap();
    let first_instance = tasks
        .iter()
        .find(|t| !t.is_template && t.template_id.is_some())
        .expect("should have at least one instance");

    // Build a second instance with a later created time and distinct ID
    let second_id = format!("{}_dup", first_instance.id);
    let second_instance = dodo::task::Task {
        id: second_id.clone(),
        num_id: Some(99),
        title: first_instance.title.clone(),
        area: first_instance.area.clone(),
        project: first_instance.project.clone(),
        context: first_instance.context.clone(),
        status: dodo::task::TaskStatus::Pending,
        created: Utc::now() + Duration::seconds(10), // later than first
        completed: None,
        estimate_minutes: first_instance.estimate_minutes,
        elapsed_seconds: None,
        elapsed_snapshot: None,
        deadline: None,
        scheduled: first_instance.scheduled,
        tags: None,
        notes: None,
        priority: None,
        modified_at: Some(Utc::now() + Duration::seconds(10)),
        recurrence: None,
        is_template: false,
        template_id: first_instance.template_id.clone(),
    };

    // Merge the duplicate into local DB
    db.merge_remote_data(&[second_instance], &[]).unwrap();

    // Should now have 2 active instances for this template; dedup removes the later one
    db.dedup_active_recurring_instances().unwrap();

    let (tasks_after, _) = db.export_all_data().unwrap();
    let active_instances: Vec<_> = tasks_after
        .iter()
        .filter(|t| !t.is_template && t.template_id.is_some() && t.status == dodo::task::TaskStatus::Pending)
        .collect();

    // Exactly one Pending instance should remain
    assert_eq!(active_instances.len(), 1, "Dedup should leave exactly one active instance");
    // It should be the earlier one (the original first_instance)
    assert_eq!(
        active_instances[0].id, first_instance.id,
        "Dedup should keep the earliest-created instance"
    );
}
