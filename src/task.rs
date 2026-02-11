use chrono::{DateTime, NaiveDate, Utc};
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
    pub estimate_minutes: Option<i64>,
    pub deadline: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
    pub tags: Option<String>,
    pub notes: Option<String>,
    pub elapsed_seconds: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Area {
    LongTerm,
    ThisWeek,
    Today,
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
            estimate_minutes: None,
            deadline: None,
            scheduled: None,
            tags: None,
            notes: None,
            elapsed_seconds: None,
        }
    }

    pub fn area_str(&self) -> &'static str {
        match self.area {
            Area::LongTerm => "LONG",
            Area::ThisWeek => "WEEK",
            Area::Today => "TODAY",
            Area::Completed => "DONE",
        }
    }

    pub fn display_metadata(&self) -> String {
        let mut parts = vec![];
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
            parts.push(format!("${}", dl.format("%b%d")));
        }
        if let Some(ref sc) = self.scheduled {
            parts.push(format!("^{}", sc.format("%b%d")));
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
    if hours > 0 {
        format!("{}h{}m", hours, mins)
    } else {
        format!("{}m", mins)
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
        write!(
            f,
            "[{}] [{}] {} {}{}{}{}",
            num_prefix,
            status_icon,
            self.area_str(),
            self.title,
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
