use dodo::cli::SortBy;
use dodo::task::{format_estimate, Task};

pub(super) fn format_estimate_tui(minutes: i64) -> String {
    format_estimate(minutes)
}

// parse_filter_days moved to dodo::notation

pub(super) fn sort_tasks(a: &Task, b: &Task, sort: SortBy, ascending: bool) -> std::cmp::Ordering {
    let ord = match sort {
        SortBy::Created | SortBy::Area => a.created.cmp(&b.created),
        SortBy::Modified => {
            let a_mod = a.modified_at.unwrap_or(a.created);
            let b_mod = b.modified_at.unwrap_or(b.created);
            a_mod.cmp(&b_mod)
        }
        SortBy::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
    };
    if ascending { ord } else { ord.reverse() }
}

pub(super) fn sort_label(sort: SortBy) -> &'static str {
    match sort {
        SortBy::Created => "created",
        SortBy::Modified => "modified",
        SortBy::Title => "title",
        SortBy::Area => "area",
    }
}

pub(super) fn format_dur(seconds: i64) -> String {
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

pub(super) fn format_dur_short(seconds: i64) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{}h{}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

/// 10a: Convert raw recurrence patterns to human-readable descriptions.
pub(super) fn humanize_recurrence(pattern: &str) -> String {
    let p = pattern.trim_start_matches('*');
    // Day-of-week list: mon,wed,fri
    if p.contains(',') || ["mon", "tue", "wed", "thu", "fri", "sat", "sun"].contains(&p)
    {
        let days: Vec<String> = p
            .split(',')
            .map(|d| {
                let d = d.trim();
                match d {
                    "mon" => "Mon",
                    "tue" => "Tue",
                    "wed" => "Wed",
                    "thu" => "Thu",
                    "fri" => "Fri",
                    "sat" => "Sat",
                    "sun" => "Sun",
                    _ => d,
                }
                .to_string()
            })
            .collect();
        return days.join(", ");
    }
    // Day-of-month: day15
    if let Some(rest) = p.strip_prefix("day") {
        if let Ok(n) = rest.parse::<u32>() {
            return format!("Day {} monthly", n);
        }
    }
    // Named
    match p {
        "daily" => return "Every day".to_string(),
        "weekly" => return "Every week".to_string(),
        "monthly" => return "Every month".to_string(),
        _ => {}
    }
    // Numeric: Nd or Nw or Nm
    if let Some(rest) = p.strip_suffix('d') {
        if let Ok(n) = rest.parse::<u32>() {
            return if n == 1 { "Every day".to_string() } else { format!("Every {} days", n) };
        }
    }
    if let Some(rest) = p.strip_suffix('w') {
        if let Ok(n) = rest.parse::<u32>() {
            return if n == 1 { "Every week".to_string() } else { format!("Every {} weeks", n) };
        }
    }
    if let Some(rest) = p.strip_suffix('m') {
        if let Ok(n) = rest.parse::<u32>() {
            return if n == 1 { "Every month".to_string() } else { format!("Every {} months", n) };
        }
    }
    // Fallback: return raw pattern
    pattern.to_string()
}

pub(super) fn format_est(minutes: i64) -> String {
    format_estimate(minutes)
}
