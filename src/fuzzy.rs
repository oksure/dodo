use crate::task::Task;

/// Score a task title against a query. Higher = better match.
fn score(title: &str, query: &str) -> u32 {
    let title_lower = title.to_lowercase();
    let query_lower = query.to_lowercase();

    if title_lower == query_lower {
        return 100; // Exact match
    }
    if title_lower.starts_with(&query_lower) {
        return 75; // Prefix match
    }
    if title_lower.contains(&query_lower) {
        return 50; // Substring match
    }
    // Word-level matching: any word starts with query
    for word in title_lower.split_whitespace() {
        if word.starts_with(&query_lower) {
            return 60;
        }
    }
    // Word-level matching: any word contains query
    for word in title_lower.split_whitespace() {
        if word.contains(&query_lower) {
            return 40;
        }
    }
    0
}

/// Find best matching task using multi-pass fuzzy matching.
/// Priority: exact match > prefix > word-start > substring > word-contains > fallback.
pub fn find_best_match<'a>(tasks: &'a [Task], query: &str) -> Option<&'a Task> {
    if tasks.is_empty() {
        return None;
    }
    tasks.iter().max_by_key(|t| score(&t.title, query))
}

/// Score all tasks and return sorted by relevance (best first).
pub fn rank_matches<'a>(tasks: &'a [Task], query: &str) -> Vec<&'a Task> {
    let mut scored: Vec<_> = tasks.iter().map(|t| (t, score(&t.title, query))).collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().map(|(t, _)| t).collect()
}
