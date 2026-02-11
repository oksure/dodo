use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub task_id: String,
    pub started: DateTime<Utc>,
    pub ended: Option<DateTime<Utc>>,
    pub duration: i64,
    pub manual_edit: bool,
    pub notes: Option<String>,
}

impl Session {
    pub fn new(task_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: ulid::Ulid::new().to_string(),
            task_id: task_id.to_string(),
            started: now,
            ended: None,
            duration: 0,
            manual_edit: false,
            notes: None,
        }
    }

    pub fn elapsed_seconds(&self) -> i64 {
        let end = self.ended.unwrap_or_else(Utc::now);
        (end - self.started).num_seconds()
    }

    pub fn stop(&mut self) {
        self.ended = Some(Utc::now());
        self.duration = self.elapsed_seconds();
    }

    pub fn is_running(&self) -> bool {
        self.ended.is_none()
    }
}
