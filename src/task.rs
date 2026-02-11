use chrono::{DateTime, Utc};
use std::fmt;

#[derive(Clone, Debug)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub area: Area,
    pub project: Option<String>,
    pub context: Option<String>,
    pub status: TaskStatus,
    pub created: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
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
            title: title.to_string(),
            area,
            project,
            context,
            status: TaskStatus::Pending,
            created: Utc::now(),
            completed: None,
        }
    }

    pub fn display_tags(&self) -> String {
        let mut tags = vec![];
        if let Some(ref p) = self.project {
            tags.push(format!("+{}", p));
        }
        if let Some(ref c) = self.context {
            tags.push(format!("@{}", c));
        }
        if tags.is_empty() {
            String::new()
        } else {
            format!(" {}", tags.join(" "))
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
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_icon = match self.status {
            TaskStatus::Pending => " ",
            TaskStatus::Running => "▶",
            TaskStatus::Paused => "⏸",
            TaskStatus::Done => "✓",
        };
        write!(
            f,
            "[{}] {}{}{}",
            status_icon,
            self.title,
            self.display_tags(),
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
