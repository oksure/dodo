use chrono::{DateTime, Local, NaiveDate, Utc};
use clap::ValueEnum;
use std::fmt;

#[derive(Clone, Debug)]
pub struct Task {
    pub id: String,
    pub num_id: Option<i64>,
    pub title: String,
    pub area: Area,
    pub project: Option<String>,
    pub context: Option<String>,
    pub status: TaskStatus,
    pub created: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub estimate_minutes: Option<i64>,
    pub deadline: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
    pub priority: Option<i64>,
    pub tags: Option<String>,
    pub notes: Option<String>,
    pub elapsed_seconds: Option<i64>,
    pub recurrence: Option<String>,
    pub is_template: bool,
    pub template_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum Area {
    #[value(name = "long")]
    LongTerm,
    #[value(name = "week")]
    ThisWeek,
    #[value(name = "today")]
    Today,
    #[value(name = "done")]
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Done,
}

impl Task {
    pub fn new(title: &str, area: Area, project: Option<String>, context: Option<String>) -> Self {
        Self {
            id: ulid::Ulid::new().to_string(),
            num_id: None,
            title: title.to_string(),
            area,
            project,
            context,
            status: TaskStatus::Pending,
            created: Utc::now(),
            completed: None,
            modified_at: None,
            estimate_minutes: None,
            deadline: None,
            scheduled: None,
            priority: None,
            tags: None,
            notes: None,
            elapsed_seconds: None,
            recurrence: None,
            is_template: false,
            template_id: None,
        }
    }

    pub fn area_str(&self) -> &'static str {
        match self.effective_area() {
            Area::LongTerm => "LONG",
            Area::ThisWeek => "WEEK",
            Area::Today => "TODAY",
            Area::Completed => "DONE",
        }
    }

    /// Compute area from scheduled date only. Done tasks → Completed.
    /// No scheduled date → Today (default).
    pub fn effective_area(&self) -> Area {
        if self.status == TaskStatus::Done {
            return Area::Completed;
        }

        let today = Local::now().date_naive();
        let seven_days = today + chrono::Duration::days(7);

        match self.scheduled {
            Some(date) if date <= today => Area::Today,
            Some(date) if date <= seven_days => Area::ThisWeek,
            Some(_) => Area::LongTerm,
            None => Area::Today,
        }
    }

    pub fn display_metadata(&self) -> String {
        let mut parts = vec![];
        if let Some(p) = self.priority {
            let bangs = "!".repeat(p.clamp(1, 4) as usize);
            parts.push(bangs);
        }
        if let Some(ref p) = self.project {
            parts.push(format!("+{}", p));
        }
        if let Some(ref c) = self.context {
            for ctx in c.split(',') {
                let ctx = ctx.trim();
                if !ctx.is_empty() {
                    parts.push(format!("@{}", ctx));
                }
            }
        }
        if let Some(ref t) = self.tags {
            for tag in t.split(',') {
                let tag = tag.trim();
                if !tag.is_empty() {
                    parts.push(format!("#{}", tag));
                }
            }
        }
        if let Some(est) = self.estimate_minutes {
            parts.push(format!("~{}", format_estimate(est)));
        }
        if let Some(ref dl) = self.deadline {
            parts.push(format!("^{}", dl.format("%b%d")));
        }
        if let Some(ref sc) = self.scheduled {
            parts.push(format!("={}", sc.format("%b%d")));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(" {}", parts.join(" "))
        }
    }

    pub fn display_time(&self) -> String {
        let elapsed = self.elapsed_seconds.unwrap_or(0);
        let estimate = self.estimate_minutes;

        if elapsed == 0 && estimate.is_none() {
            return String::new();
        }

        let elapsed_str = format_duration_short(elapsed);

        match estimate {
            Some(est) if elapsed > 0 => {
                format!(" ({}/{})", elapsed_str, format_estimate(est))
            }
            Some(est) => {
                format!(" (0m/{})", format_estimate(est))
            }
            None if elapsed > 0 => {
                format!(" ({})", elapsed_str)
            }
            _ => String::new(),
        }
    }

    // Keep backward compat for tests that reference display_tags
    pub fn display_tags(&self) -> String {
        self.display_metadata()
    }
}

fn format_duration_short(seconds: i64) -> String {
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

fn format_estimate(minutes: i64) -> String {
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

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_icon = match self.status {
            TaskStatus::Pending => " ",
            TaskStatus::Running => "▶",
            TaskStatus::Paused => "⏸",
            TaskStatus::Done => "✓",
        };
        let num_prefix = match self.num_id {
            Some(n) => format!("{}", n),
            None => "?".to_string(),
        };
        let notes_indicator = match &self.notes {
            Some(n) if !n.is_empty() => " *",
            _ => "",
        };
        write!(
            f,
            "[{}] [{}] {} {}{}{}{}{}",
            num_prefix,
            status_icon,
            self.area_str(),
            self.title,
            notes_indicator,
            self.display_metadata(),
            self.display_time(),
            if self.status == TaskStatus::Running {
                " [running]"
            } else {
                ""
            }
        )
    }
}

impl Area {
    /// Map an area to a scheduled date for move operations.
    /// Used by both CLI `move` command and TUI move actions.
    pub fn to_scheduled_date(self) -> NaiveDate {
        let today = Local::now().date_naive();
        match self {
            Area::Today => today,
            Area::ThisWeek => today + chrono::Duration::days(1),
            Area::LongTerm => today + chrono::Duration::days(8),
            Area::Completed => today, // fallback, shouldn't be used for moves
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "long" | "longterm" => Some(Area::LongTerm),
            "week" | "thisweek" => Some(Area::ThisWeek),
            "today" => Some(Area::Today),
            "done" | "completed" => Some(Area::Completed),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Area::LongTerm => "LongTerm",
            Area::ThisWeek => "ThisWeek",
            Area::Today => "Today",
            Area::Completed => "Completed",
        }
    }
}

impl From<Area> for String {
    fn from(area: Area) -> String {
        area.as_str().to_string()
    }
}

impl TaskStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(TaskStatus::Pending),
            "running" => Some(TaskStatus::Running),
            "paused" => Some(TaskStatus::Paused),
            "done" | "completed" => Some(TaskStatus::Done),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "Pending",
            TaskStatus::Running => "Running",
            TaskStatus::Paused => "Paused",
            TaskStatus::Done => "Done",
        }
    }
}
