use chrono::{Datelike, Local, NaiveDate, Weekday};

#[derive(Debug, Default, PartialEq)]
pub struct ParsedInput {
    pub title: String,
    pub project: Option<String>,
    pub contexts: Vec<String>,
    pub tags: Vec<String>,
    pub estimate_minutes: Option<i64>,
    pub deadline: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
}

impl ParsedInput {
    pub fn has_updates(&self) -> bool {
        self.project.is_some()
            || !self.contexts.is_empty()
            || !self.tags.is_empty()
            || self.estimate_minutes.is_some()
            || self.deadline.is_some()
            || self.scheduled.is_some()
    }
}

pub fn parse_notation(input: &str) -> ParsedInput {
    let mut result = ParsedInput::default();
    let mut title_parts: Vec<&str> = Vec::new();

    for token in input.split_whitespace() {
        let first_byte = token.as_bytes()[0];

        // All notation symbols are ASCII single-byte characters.
        // Skip tokens that don't start with a known symbol or are too short.
        if token.len() < 2
            || !matches!(first_byte, b'+' | b'@' | b'#' | b'~' | b'$' | b'^')
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
            b'$' => {
                if let Some(date) = parse_date(value) {
                    result.deadline = Some(date);
                } else {
                    title_parts.push(token);
                }
            }
            b'^' => {
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

    // M/D or MM/DD
    if s_lower.contains('/') {
        let parts: Vec<&str> = s_lower.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(month), Ok(day)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                let year = today.year();
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                    // Use next year if the date has passed
                    if date < today {
                        return NaiveDate::from_ymd_opt(year + 1, month, day);
                    }
                    return Some(date);
                }
            }
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

    let days = match unit {
        b'd' => n,
        b'w' => n * 7,
        _ => return None,
    };

    let days = if negative { -days } else { days };
    today.checked_add_signed(chrono::Duration::days(days))
}
