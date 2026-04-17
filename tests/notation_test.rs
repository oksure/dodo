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
fn date_mmdd() {
    let today = Local::now().date_naive();
    let year = today.year();
    // Pick a date in the future
    let future_month = if today.month() < 12 {
        today.month() + 1
    } else {
        1
    };
    let future_year = if future_month == 1 { year + 1 } else { year };
    let input = format!("{:02}15", future_month);
    assert_eq!(
        parse_date(&input),
        NaiveDate::from_ymd_opt(future_year, future_month, 15)
    );
}

#[test]
fn date_yyyymmdd() {
    assert_eq!(
        parse_date("20250502"),
        Some(NaiveDate::from_ymd_opt(2025, 5, 2).unwrap())
    );
}

#[test]
fn date_relative_months() {
    let today = Local::now().date_naive();
    let result = parse_date("1m").unwrap();
    let expected = today.checked_add_months(chrono::Months::new(1)).unwrap();
    assert_eq!(result, expected);
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
    let p = parse_notation("Submit report ^tmr");
    assert_eq!(p.title, "Submit report");
    let tomorrow = Local::now().date_naive().succ_opt().unwrap();
    assert_eq!(p.deadline, Some(tomorrow));
}

#[test]
fn scheduled_token() {
    let p = parse_notation("Start project =tmr");
    assert_eq!(p.title, "Start project");
    let tomorrow = Local::now().date_naive().succ_opt().unwrap();
    assert_eq!(p.scheduled, Some(tomorrow));
}

#[test]
fn all_symbols() {
    let p = parse_notation("Fix login bug +backend @john @sarah #urgent #p1 ~2h ^tmr =tdy !!!");
    assert_eq!(p.title, "Fix login bug");
    assert_eq!(p.project, Some("backend".to_string()));
    assert_eq!(p.contexts, vec!["john", "sarah"]);
    assert_eq!(p.tags, vec!["urgent", "p1"]);
    assert_eq!(p.estimate_minutes, Some(120));
    assert!(p.deadline.is_some());
    assert!(p.scheduled.is_some());
    assert_eq!(p.priority, Some(3));
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
    assert_eq!(p.priority, None);
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

// ── Priority parsing ────────────────────────────────────────────────

#[test]
fn priority_single_bang() {
    let p = parse_notation("Low priority task !");
    assert_eq!(p.title, "Low priority task");
    assert_eq!(p.priority, Some(1));
}

#[test]
fn priority_double_bang() {
    let p = parse_notation("Medium task !!");
    assert_eq!(p.title, "Medium task");
    assert_eq!(p.priority, Some(2));
}

#[test]
fn priority_triple_bang() {
    let p = parse_notation("High priority !!!");
    assert_eq!(p.title, "High priority");
    assert_eq!(p.priority, Some(3));
}

#[test]
fn priority_max_bang() {
    let p = parse_notation("Critical !!!!");
    assert_eq!(p.title, "Critical");
    assert_eq!(p.priority, Some(4));
}

#[test]
fn priority_last_wins() {
    let p = parse_notation("Task ! !!!");
    assert_eq!(p.priority, Some(3));
}

#[test]
fn five_bangs_not_priority() {
    let p = parse_notation("Task !!!!!");
    assert_eq!(p.title, "Task !!!!!");
    assert_eq!(p.priority, None);
}

// ── Recurrence notation ────────────────────────────────────────────

use dodo::notation::{next_occurrence, parse_recurrence};

#[test]
fn recurrence_daily() {
    assert_eq!(parse_recurrence("daily"), Some("daily".into()));
    assert_eq!(parse_recurrence("1d"), Some("daily".into()));
}

#[test]
fn recurrence_weekly() {
    assert_eq!(parse_recurrence("weekly"), Some("weekly".into()));
    assert_eq!(parse_recurrence("1w"), Some("weekly".into()));
}

#[test]
fn recurrence_monthly() {
    assert_eq!(parse_recurrence("monthly"), Some("monthly".into()));
    assert_eq!(parse_recurrence("1m"), Some("monthly".into()));
}

#[test]
fn recurrence_interval_days() {
    assert_eq!(parse_recurrence("3d"), Some("3d".into()));
    assert_eq!(parse_recurrence("14d"), Some("14d".into()));
}

#[test]
fn recurrence_interval_weeks() {
    assert_eq!(parse_recurrence("2w"), Some("2w".into()));
}

#[test]
fn recurrence_interval_months() {
    assert_eq!(parse_recurrence("3m"), Some("3m".into()));
}

#[test]
fn recurrence_day_of_month() {
    assert_eq!(parse_recurrence("day15"), Some("day15".into()));
    assert_eq!(parse_recurrence("day1"), Some("day1".into()));
    assert_eq!(parse_recurrence("day31"), Some("day31".into()));
}

#[test]
fn recurrence_weekday_list() {
    assert_eq!(parse_recurrence("mon,wed,fri"), Some("mon,wed,fri".into()));
    assert_eq!(parse_recurrence("tue,thu"), Some("tue,thu".into()));
}

#[test]
fn recurrence_invalid() {
    assert_eq!(parse_recurrence("foo"), None);
    assert_eq!(parse_recurrence("0d"), None);
    assert_eq!(parse_recurrence("day0"), None);
    assert_eq!(parse_recurrence("day32"), None);
}

#[test]
fn recurrence_token_in_notation() {
    let p = parse_notation("standup *daily +work ~15m");
    assert_eq!(p.title, "standup");
    assert_eq!(p.recurrence, Some("daily".into()));
    assert_eq!(p.project, Some("work".into()));
    assert_eq!(p.estimate_minutes, Some(15));
}

#[test]
fn recurrence_weekday_in_notation() {
    let p = parse_notation("code review *mon,wed,fri +backend");
    assert_eq!(p.title, "code review");
    assert_eq!(p.recurrence, Some("mon,wed,fri".into()));
    assert_eq!(p.project, Some("backend".into()));
}

#[test]
fn recurrence_day_of_month_in_notation() {
    let p = parse_notation("pay rent *day15 !!");
    assert_eq!(p.title, "pay rent");
    assert_eq!(p.recurrence, Some("day15".into()));
    assert_eq!(p.priority, Some(2));
}

// ── Next occurrence computation ────────────────────────────────────

#[test]
fn next_occurrence_daily() {
    let from = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
    assert_eq!(
        next_occurrence("daily", from),
        Some(NaiveDate::from_ymd_opt(2026, 2, 11).unwrap())
    );
}

#[test]
fn next_occurrence_3d() {
    let from = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
    assert_eq!(
        next_occurrence("3d", from),
        Some(NaiveDate::from_ymd_opt(2026, 2, 13).unwrap())
    );
}

#[test]
fn next_occurrence_weekly() {
    let from = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
    assert_eq!(
        next_occurrence("weekly", from),
        Some(NaiveDate::from_ymd_opt(2026, 2, 17).unwrap())
    );
}

#[test]
fn next_occurrence_monthly() {
    let from = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    assert_eq!(
        next_occurrence("monthly", from),
        Some(NaiveDate::from_ymd_opt(2026, 2, 15).unwrap())
    );
}

#[test]
fn next_occurrence_day_of_month() {
    // From day 10, day15 should give 15th of same month
    let from = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
    assert_eq!(
        next_occurrence("day15", from),
        Some(NaiveDate::from_ymd_opt(2026, 2, 15).unwrap())
    );

    // From day 20, day15 should give 15th of next month
    let from = NaiveDate::from_ymd_opt(2026, 2, 20).unwrap();
    assert_eq!(
        next_occurrence("day15", from),
        Some(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap())
    );
}

#[test]
fn next_occurrence_day31_in_february() {
    // Semantics changed: day31 now SKIPS months that don't have day 31 (Feb → March 31)
    // Previously clamped to Feb 28 but that caused an infinite same-date loop.
    let from = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
    assert_eq!(
        next_occurrence("day31", from),
        Some(NaiveDate::from_ymd_opt(2026, 3, 31).unwrap())
    );
}

#[test]
fn next_occurrence_day31_skip_months() {
    // From Feb 28, day31 should jump to March 31 (not get stuck)
    let from = NaiveDate::from_ymd_opt(2026, 2, 28).unwrap();
    assert_eq!(
        next_occurrence("day31", from),
        Some(NaiveDate::from_ymd_opt(2026, 3, 31).unwrap())
    );

    // From April 30, day31 should jump to May 31 (April has only 30 days)
    let from = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
    assert_eq!(
        next_occurrence("day31", from),
        Some(NaiveDate::from_ymd_opt(2026, 5, 31).unwrap())
    );

    // Non-leap Feb: day29 should skip to March 29
    let from = NaiveDate::from_ymd_opt(2027, 2, 28).unwrap();
    assert_eq!(
        next_occurrence("day29", from),
        Some(NaiveDate::from_ymd_opt(2027, 3, 29).unwrap())
    );
}

#[test]
fn next_occurrence_day15_unchanged() {
    // day15 still works normally when day hasn't passed
    let from = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
    assert_eq!(
        next_occurrence("day15", from),
        Some(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap())
    );
}

#[test]
fn next_occurrence_weekday_list() {
    // Feb 10, 2026 is Tuesday. mon,wed,fri → next is Wed Feb 11
    let from = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
    let next = next_occurrence("mon,wed,fri", from);
    assert_eq!(next, Some(NaiveDate::from_ymd_opt(2026, 2, 11).unwrap()));
}

#[test]
fn next_occurrence_weekday_wraps() {
    // Feb 13, 2026 is Friday. mon,wed,fri → next is Mon Feb 16
    let from = NaiveDate::from_ymd_opt(2026, 2, 13).unwrap();
    let next = next_occurrence("mon,wed,fri", from);
    assert_eq!(next, Some(NaiveDate::from_ymd_opt(2026, 2, 16).unwrap()));
}

// ── 5a: day31 year boundary ───────────────────────────────────────────

#[test]
fn next_occurrence_day31_year_boundary() {
    let from = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
    assert_eq!(
        next_occurrence("day31", from),
        Some(NaiveDate::from_ymd_opt(2027, 1, 31).unwrap())
    );
}

// ── 5b: day29 from Feb29 leap year ───────────────────────────────────

#[test]
fn next_occurrence_day29_from_leap_feb29() {
    let from = NaiveDate::from_ymd_opt(2028, 2, 29).unwrap();
    assert_eq!(
        next_occurrence("day29", from),
        Some(NaiveDate::from_ymd_opt(2028, 3, 29).unwrap())
    );
}
