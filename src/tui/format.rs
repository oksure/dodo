use dodo::cli::SortBy;
use dodo::task::Task;

pub(super) fn format_estimate_tui(minutes: i64) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 && mins > 0 {
        format!("{}h{}m", hours, mins)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        format!("{}m", mins)
    }
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

pub(super) fn format_est(minutes: i64) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 && mins > 0 {
        format!("{}h{}m", hours, mins)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        format!("{}m", mins)
    }
}
