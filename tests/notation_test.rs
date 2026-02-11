use chrono::{Datelike, Local, NaiveDate};
use dodo::notation::{parse_date, parse_duration, parse_notation};

// ── Duration parsing ──────────────────────────────────────────────────

#[test]
fn duration_minutes() {
    assert_eq!(parse_duration("30m"), Some(30));
}

#[test]
fn duration_hours() {
    assert_eq!(parse_duration("1h"), Some(60));
    assert_eq!(parse_duration("2h"), Some(120));
}

#[test]
fn duration_hours_minutes() {
    assert_eq!(parse_duration("1h30m"), Some(90));
    assert_eq!(parse_duration("2h15m"), Some(135));
}

#[test]
fn duration_days() {
    assert_eq!(parse_duration("1d"), Some(480));
    assert_eq!(parse_duration("2d"), Some(960));
}

#[test]
fn duration_weeks() {
    assert_eq!(parse_duration("1w"), Some(2400));
}

#[test]
fn duration_composite() {
    assert_eq!(parse_duration("2d4h"), Some(1200));
    assert_eq!(parse_duration("1w2d"), Some(3360));
}

// ── Date parsing ──────────────────────────────────────────────────────

#[test]
fn date_today() {
    let today = Local::now().date_naive();
    assert_eq!(parse_date("today"), Some(today));
    assert_eq!(parse_date("tdy"), Some(today));
}

#[test]
fn date_tomorrow() {
    let tomorrow = Local::now().date_naive().succ_opt().unwrap();
    assert_eq!(parse_date("tomorrow"), Some(tomorrow));
    assert_eq!(parse_date("tmr"), Some(tomorrow));
}

#[test]
fn date_yesterday() {
    let yesterday = Local::now().date_naive().pred_opt().unwrap();
    assert_eq!(parse_date("yesterday"), Some(yesterday));
    assert_eq!(parse_date("ytd"), Some(yesterday));
}

#[test]
fn date_iso() {
    assert_eq!(
        parse_date("2025-05-02"),
        Some(NaiveDate::from_ymd_opt(2025, 5, 2).unwrap())
    );
}

#[test]
fn date_relative_days() {
    let today = Local::now().date_naive();
    assert_eq!(
        parse_date("3d"),
        today.checked_add_signed(chrono::Duration::days(3))
    );
}

#[test]
fn date_relative_weeks() {
    let today = Local::now().date_naive();
    assert_eq!(
        parse_date("2w"),
        today.checked_add_signed(chrono::Duration::days(14))
    );
}

#[test]
fn date_relative_negative() {
    let today = Local::now().date_naive();
    assert_eq!(
        parse_date("-3d"),
        today.checked_add_signed(chrono::Duration::days(-3))
    );
}

#[test]
fn date_month_day() {
    let today = Local::now().date_naive();
    let year = today.year();
    // Pick a date in the future
    let future_month = if today.month() < 12 { today.month() + 1 } else { 1 };
    let future_year = if future_month == 1 { year + 1 } else { year };
    let input = format!("{}/15", future_month);
    assert_eq!(
        parse_date(&input),
        NaiveDate::from_ymd_opt(future_year, future_month, 15)
    );
}

#[test]
fn date_day_name() {
    // Any day name should return a date in the next 7 days
    let today = Local::now().date_naive();
    let result = parse_date("mon").unwrap();
    let diff = (result - today).num_days();
    assert!(diff >= 1 && diff <= 7);
}

// ── Notation parsing ──────────────────────────────────────────────────

#[test]
fn single_project() {
    let p = parse_notation("Fix bug +backend");
    assert_eq!(p.title, "Fix bug");
    assert_eq!(p.project, Some("backend".to_string()));
}

#[test]
fn single_context() {
    let p = parse_notation("Call dentist @phone");
    assert_eq!(p.title, "Call dentist");
    assert_eq!(p.contexts, vec!["phone"]);
}

#[test]
fn single_tag() {
    let p = parse_notation("Fix login #urgent");
    assert_eq!(p.title, "Fix login");
    assert_eq!(p.tags, vec!["urgent"]);
}

#[test]
fn multiple_contexts() {
    let p = parse_notation("Team standup @john @sarah");
    assert_eq!(p.title, "Team standup");
    assert_eq!(p.contexts, vec!["john", "sarah"]);
}

#[test]
fn multiple_tags() {
    let p = parse_notation("Fix crash #urgent #bug");
    assert_eq!(p.title, "Fix crash");
    assert_eq!(p.tags, vec!["urgent", "bug"]);
}

#[test]
fn estimate_token() {
    let p = parse_notation("Design page ~2h");
    assert_eq!(p.title, "Design page");
    assert_eq!(p.estimate_minutes, Some(120));
}

#[test]
fn deadline_token() {
    let p = parse_notation("Submit report $tmr");
    assert_eq!(p.title, "Submit report");
    let tomorrow = Local::now().date_naive().succ_opt().unwrap();
    assert_eq!(p.deadline, Some(tomorrow));
}

#[test]
fn scheduled_token() {
    let p = parse_notation("Start project ^tmr");
    assert_eq!(p.title, "Start project");
    let tomorrow = Local::now().date_naive().succ_opt().unwrap();
    assert_eq!(p.scheduled, Some(tomorrow));
}

#[test]
fn all_six_symbols() {
    let p = parse_notation("Fix login bug +backend @john @sarah #urgent #p1 ~2h $tmr ^tdy");
    assert_eq!(p.title, "Fix login bug");
    assert_eq!(p.project, Some("backend".to_string()));
    assert_eq!(p.contexts, vec!["john", "sarah"]);
    assert_eq!(p.tags, vec!["urgent", "p1"]);
    assert_eq!(p.estimate_minutes, Some(120));
    assert!(p.deadline.is_some());
    assert!(p.scheduled.is_some());
}

#[test]
fn title_cleanup() {
    let p = parse_notation("+proj  Fix   bug  ~1h");
    assert_eq!(p.title, "Fix bug");
    assert_eq!(p.project, Some("proj".to_string()));
    assert_eq!(p.estimate_minutes, Some(60));
}

#[test]
fn no_tokens() {
    let p = parse_notation("Just a plain title");
    assert_eq!(p.title, "Just a plain title");
    assert_eq!(p.project, None);
    assert!(p.contexts.is_empty());
    assert!(p.tags.is_empty());
    assert_eq!(p.estimate_minutes, None);
    assert_eq!(p.deadline, None);
    assert_eq!(p.scheduled, None);
}

#[test]
fn only_tokens() {
    let p = parse_notation("+backend @john ~30m");
    assert_eq!(p.title, "");
    assert_eq!(p.project, Some("backend".to_string()));
    assert_eq!(p.contexts, vec!["john"]);
    assert_eq!(p.estimate_minutes, Some(30));
}

#[test]
fn email_not_parsed_as_context() {
    // email@test should not be parsed because @ is mid-word
    let p = parse_notation("Send email@test.com a message");
    assert_eq!(p.title, "Send email@test.com a message");
    assert!(p.contexts.is_empty());
}

#[test]
fn project_last_wins() {
    let p = parse_notation("Task +first +second");
    assert_eq!(p.project, Some("second".to_string()));
}

#[test]
fn estimate_last_wins() {
    let p = parse_notation("Task ~30m ~1h");
    assert_eq!(p.estimate_minutes, Some(60));
}

#[test]
fn unicode_title_no_panic() {
    let p = parse_notation("강의자료 도서관 요청");
    assert_eq!(p.title, "강의자료 도서관 요청");
    assert!(p.project.is_none());
    assert!(p.contexts.is_empty());
}

#[test]
fn unicode_with_notation() {
    let p = parse_notation("강의자료 준비 +학교 ~2h");
    assert_eq!(p.title, "강의자료 준비");
    assert_eq!(p.project, Some("학교".to_string()));
    assert_eq!(p.estimate_minutes, Some(120));
}

#[test]
fn single_unicode_char() {
    let p = parse_notation("가");
    assert_eq!(p.title, "가");
}
