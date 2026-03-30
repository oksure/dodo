use dodo::fuzzy::{find_best_match, rank_matches};
use dodo::task::{Area, Task};

fn make_task(title: &str) -> Task {
    Task::new(title, Area::Today, None, None)
}

#[test]
fn exact_match_wins() {
    let tasks = vec![
        make_task("Write unit tests"),
        make_task("Write quarterly report"),
        make_task("Review writing style guide"),
    ];
    let best = find_best_match(&tasks, "Write unit tests").unwrap();
    assert_eq!(best.title, "Write unit tests");
}

#[test]
fn prefix_match_over_substring() {
    let tasks = vec![make_task("Overwrite config"), make_task("Write unit tests")];
    // "Write" is a prefix of "Write unit tests" but a substring of "Overwrite config"
    let best = find_best_match(&tasks, "Write").unwrap();
    assert_eq!(best.title, "Write unit tests");
}

#[test]
fn substring_match_finds_middle() {
    let tasks = vec![
        make_task("Fix production bug"),
        make_task("Write quarterly report"),
    ];
    let best = find_best_match(&tasks, "report").unwrap();
    assert_eq!(best.title, "Write quarterly report");
}

#[test]
fn word_start_match() {
    let tasks = vec![
        make_task("Draft blog post"),
        make_task("Update blogroll page"),
    ];
    // "blog" starts the word "blog" in "Draft blog post" (word-start: 60)
    // "blog" starts the word "blogroll" in "Update blogroll page" (word-start: 60)
    // Both score 60 — but "Draft blog post" also contains "blog" as substring (50)
    // Actually both contain "blog" as substring too. Let's test differently.
    let best = find_best_match(&tasks, "post").unwrap();
    assert_eq!(best.title, "Draft blog post");
}

#[test]
fn case_insensitive() {
    let tasks = vec![make_task("Fix Production Bug")];
    let best = find_best_match(&tasks, "fix production bug").unwrap();
    assert_eq!(best.title, "Fix Production Bug");
}

#[test]
fn empty_tasks_returns_none() {
    let tasks: Vec<Task> = vec![];
    assert!(find_best_match(&tasks, "anything").is_none());
}

#[test]
fn rank_matches_orders_by_relevance() {
    let tasks = vec![
        make_task("Review writing style guide"),
        make_task("Overwrite config"),
        make_task("Write unit tests"),
    ];
    let ranked = rank_matches(&tasks, "write");
    // "Write unit tests" = prefix (75)
    // "Overwrite config" = substring (50)
    // "Review writing style guide" = word-contains "writing" contains "write" (40)
    assert_eq!(ranked[0].title, "Write unit tests");
    assert_eq!(ranked[1].title, "Overwrite config");
    assert_eq!(ranked[2].title, "Review writing style guide");
}

#[test]
fn rank_matches_exact_first() {
    let tasks = vec![make_task("Write report"), make_task("report")];
    let ranked = rank_matches(&tasks, "report");
    // "report" = exact (100), "Write report" = substring (50)
    assert_eq!(ranked[0].title, "report");
    assert_eq!(ranked[1].title, "Write report");
}
