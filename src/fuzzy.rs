// Simplified fuzzy matching - will use basic substring for MVP
// TODO: Add nucleo_matcher when we have time to debug lifetimes

use crate::task::Task;

/// Find best matching task using simple substring matching
pub fn find_best_match(tasks: &[Task], query: &str) -> Option<&Task> {
    if tasks.is_empty() {
        return None;
    }

    let query_lower = query.to_lowercase();
    
    // Exact substring match first
    for task in tasks {
        if task.title.to_lowercase().contains(&query_lower) {
            return Some(task);
        }
    }
    
    // Prefix matching
    for task in tasks {
        if task.title.to_lowercase().starts_with(&query_lower) {
            return Some(task);
        }
    }
    
    // Word-by-word matching
    for task in tasks {
        let words: Vec<&str> = task.title.to_lowercase().split_whitespace().collect();
        for word in words {
            if word.contains(&query_lower) {
                return Some(task);
            }
        }
    }
    
    tasks.first() // Fallback: return first task
}

/// Score all tasks and return sorted by relevance
pub fn rank_matches(tasks: &[Task], _query: &str) -> Vec<&Task> {
    tasks.iter().collect()
}
