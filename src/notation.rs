use chrono::{Datelike, Local, Months, NaiveDate, Weekday};

#[derive(Debug, Default, PartialEq)]
pub struct ParsedInput {
    pub title: String,
    pub project: Option<String>,
    pub contexts: Vec<String>,
    pub tags: Vec<String>,
    pub estimate_minutes: Option<i64>,
    pub deadline: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
    pub priority: Option<i64>,
    pub recurrence: Option<String>,
}

impl ParsedInput {
    pub fn has_updates(&self) -> bool {
        self.project.is_some()
            || !self.contexts.is_empty()
            || !self.tags.is_empty()
            || self.estimate_minutes.is_some()
            || self.deadline.is_some()
            || self.scheduled.is_some()
            || self.priority.is_some()
            || self.recurrence.is_some()
    }
}

pub fn parse_notation(input: &str) -> ParsedInput {
    let mut result = ParsedInput::default();
    let mut title_parts: Vec<&str> = Vec::new();

    for token in input.split_whitespace() {
        let first_byte = token.as_bytes()[0];

        // Priority tokens: !, !!, !!!, !!!!
        if token.len() <= 4 && token.bytes().all(|b| b == b'!') {
            result.priority = Some(token.len() as i64);
            continue;
        }

        // Recurrence tokens: *daily, *3d, *mon,wed,fri, *day15
        if first_byte == b'*' && token.len() >= 2 {
            let pattern = &token[1..];
            if parse_recurrence(pattern).is_some() {
                result.recurrence = Some(pattern.to_lowercase());
                continue;
            }
        }

        // All notation symbols are ASCII single-byte characters.
        // Skip tokens that don't start with a known symbol or are too short.
        if token.len() < 2
            || !matches!(first_byte, b'+' | b'@' | b'#' | b'~' | b'^' | b'=')
        {
            title_parts.push(token);
            continue;
        }

        // Safe to slice at byte 1 since the prefix is ASCII
        let prefix = first_byte;
        let value = &token[1..];

        match prefix {
            b'+' if is_word_token(value) => {
                result.project = Some(value.to_string());
            }
            b'@' if is_word_token(value) => {
                result.contexts.push(value.to_string());
            }
            b'#' if is_word_token(value) => {
                result.tags.push(value.to_string());
            }
            b'~' => {
                if let Some(mins) = parse_duration(value) {
                    result.estimate_minutes = Some(mins);
                } else {
                    title_parts.push(token);
                }
            }
            b'^' => {
                if let Some(date) = parse_date(value) {
                    result.deadline = Some(date);
                } else {
                    title_parts.push(token);
                }
            }
            b'=' => {
                if let Some(date) = parse_date(value) {
                    result.scheduled = Some(date);
                } else {
                    title_parts.push(token);
                }
            }
            _ => {
                title_parts.push(token);
            }
        }
    }

    result.title = title_parts.join(" ");
    result
}

fn is_word_token(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

pub fn parse_duration(s: &str) -> Option<i64> {
    let s = s.to_lowercase();
    let mut total: i64 = 0;
    let mut current_num = String::new();
    let mut found_unit = false;

    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else {
            let n: i64 = current_num.parse().ok()?;
            current_num.clear();
            match c {
                'w' => { total += n * 2400; found_unit = true; }
                'd' => { total += n * 480; found_unit = true; }
                'h' => { total += n * 60; found_unit = true; }
                'm' => { total += n; found_unit = true; }
                _ => return None,
            }
        }
    }

    if !current_num.is_empty() {
        // Trailing number with no unit — treat as minutes
        if found_unit {
            return None; // e.g. "1h30" is ambiguous
        }
        let n: i64 = current_num.parse().ok()?;
        total += n; // bare number = minutes
    }

    if total > 0 { Some(total) } else { None }
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let today = Local::now().date_naive();
    let s_lower = s.to_lowercase();

    // Named dates
    match s_lower.as_str() {
        "today" | "tdy" => return Some(today),
        "tomorrow" | "tmr" => return today.succ_opt(),
        "yesterday" | "ytd" => return today.pred_opt(),
        _ => {}
    }

    // Day names → next occurrence
    if let Some(target_wd) = parse_weekday(&s_lower) {
        let current_wd = today.weekday();
        let days_ahead = (target_wd.num_days_from_monday() as i64
            - current_wd.num_days_from_monday() as i64
            + 7)
            % 7;
        let days_ahead = if days_ahead == 0 { 7 } else { days_ahead };
        return today.checked_add_signed(chrono::Duration::days(days_ahead));
    }

    // ISO date: YYYY-MM-DD
    if let Ok(date) = NaiveDate::parse_from_str(&s_lower, "%Y-%m-%d") {
        return Some(date);
    }

    // YYYYMMDD (8 digits)
    if s_lower.len() == 8 && s_lower.bytes().all(|b| b.is_ascii_digit()) {
        let year: i32 = s_lower[0..4].parse().ok()?;
        let month: u32 = s_lower[4..6].parse().ok()?;
        let day: u32 = s_lower[6..8].parse().ok()?;
        return NaiveDate::from_ymd_opt(year, month, day);
    }

    // MMDD (4 digits)
    if s_lower.len() == 4 && s_lower.bytes().all(|b| b.is_ascii_digit()) {
        let month: u32 = s_lower[0..2].parse().ok()?;
        let day: u32 = s_lower[2..4].parse().ok()?;
        let year = today.year();
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            if date < today {
                return NaiveDate::from_ymd_opt(year + 1, month, day);
            }
            return Some(date);
        }
    }

    // Relative: 1d, 2w, 3d, -3d
    if let Some(date) = parse_relative_date(&s_lower, today) {
        return Some(date);
    }

    None
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    match s {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Validate a recurrence pattern string. Returns the normalized pattern if valid.
pub fn parse_recurrence(s: &str) -> Option<String> {
    let s = s.to_lowercase();

    // Named aliases
    match s.as_str() {
        "daily" | "1d" => return Some("daily".into()),
        "weekly" | "1w" => return Some("weekly".into()),
        "monthly" | "1m" => return Some("monthly".into()),
        _ => {}
    }

    // Interval patterns: 3d, 2w, 3m
    if s.len() >= 2 {
        let unit = s.as_bytes()[s.len() - 1];
        let num_str = &s[..s.len() - 1];
        if matches!(unit, b'd' | b'w' | b'm') {
            if let Ok(n) = num_str.parse::<u32>() {
                if n > 0 {
                    return Some(s.clone());
                }
            }
        }
    }

    // Day-of-month: day1..day31
    if let Some(rest) = s.strip_prefix("day") {
        if let Ok(day) = rest.parse::<u32>() {
            if (1..=31).contains(&day) {
                return Some(s.clone());
            }
        }
    }

    // Day-of-week list: mon,wed,fri
    let parts: Vec<&str> = s.split(',').collect();
    if !parts.is_empty() && parts.iter().all(|p| parse_weekday_short(p).is_some()) {
        return Some(s.clone());
    }

    None
}

/// Compute the next occurrence date from a recurrence pattern and a reference date.
pub fn next_occurrence(pattern: &str, from: NaiveDate) -> Option<NaiveDate> {
    let p = pattern.to_lowercase();

    match p.as_str() {
        "daily" | "1d" => return from.succ_opt(),
        "weekly" | "1w" => return from.checked_add_signed(chrono::Duration::days(7)),
        "monthly" | "1m" => return from.checked_add_months(Months::new(1)),
        _ => {}
    }

    // Interval: Nd, Nw, Nm
    if p.len() >= 2 {
        let unit = p.as_bytes()[p.len() - 1];
        let num_str = &p[..p.len() - 1];
        if let Ok(n) = num_str.parse::<u32>() {
            match unit {
                b'd' => return from.checked_add_signed(chrono::Duration::days(n as i64)),
                b'w' => return from.checked_add_signed(chrono::Duration::days(n as i64 * 7)),
                b'm' => return from.checked_add_months(Months::new(n)),
                _ => {}
            }
        }
    }

    // Day-of-month: dayN
    if let Some(rest) = p.strip_prefix("day") {
        if let Ok(target_day) = rest.parse::<u32>() {
            // Find next month's target day (or current month if day hasn't passed)
            let mut year = from.year();
            let mut month = from.month();
            let from_day = from.day();

            if from_day >= target_day {
                // Move to next month
                month += 1;
                if month > 12 {
                    month = 1;
                    year += 1;
                }
            }

            // Clamp day to the actual last day of the month
            let last_day = last_day_of_month(year, month);
            let day = target_day.min(last_day);
            return NaiveDate::from_ymd_opt(year, month, day);
        }
    }

    // Day-of-week list: mon,wed,fri
    let parts: Vec<&str> = p.split(',').collect();
    let weekdays: Vec<Weekday> = parts.iter().filter_map(|s| parse_weekday_short(s)).collect();
    if !weekdays.is_empty() {
        let current_wd = from.weekday();
        // Find the next matching weekday strictly after `from`
        let mut best_offset = 8u32; // impossibly high
        for &wd in &weekdays {
            let offset = (wd.num_days_from_monday() as i32
                - current_wd.num_days_from_monday() as i32
                + 7)
                % 7;
            let offset = if offset == 0 { 7 } else { offset as u32 };
            if offset < best_offset {
                best_offset = offset;
            }
        }
        return from.checked_add_signed(chrono::Duration::days(best_offset as i64));
    }

    None
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    // The first day of the next month minus 1 day
    let (y, m) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(y, m, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

fn parse_weekday_short(s: &str) -> Option<Weekday> {
    match s {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_relative_date(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let (negative, rest) = if let Some(stripped) = s.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, s)
    };

    if rest.len() < 2 {
        return None;
    }

    let unit = rest.as_bytes()[rest.len() - 1];
    let num_str = &rest[..rest.len() - 1];
    let n: i64 = num_str.parse().ok()?;

    match unit {
        b'd' => {
            let days = if negative { -n } else { n };
            today.checked_add_signed(chrono::Duration::days(days))
        }
        b'w' => {
            let days = if negative { -n * 7 } else { n * 7 };
            today.checked_add_signed(chrono::Duration::days(days))
        }
        b'm' => {
            let months = Months::new(n as u32);
            if negative {
                today.checked_sub_months(months)
            } else {
                today.checked_add_months(months)
            }
        }
        _ => None,
    }
}
