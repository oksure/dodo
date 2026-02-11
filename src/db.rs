use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

use crate::cli::Area as CliArea;
use crate::session::Session;
use crate::task::{Area, Task, TaskStatus};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::db_path()?;
        std::fs::create_dir_all(db_path.parent().unwrap())?;
        
        let conn = Connection::open(&db_path)
            .context("Failed to open database")?;
        
        let db = Self { conn };
        db.migrate()?;
        
        Ok(db)
    }

    fn db_path() -> Result<PathBuf> {
        let home = dirs::data_local_dir()
            .context("Could not find local data directory")?;
        Ok(home.join("dodo").join("dodo.db"))
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                area TEXT NOT NULL,
                project TEXT,
                context TEXT,
                status TEXT NOT NULL,
                created TEXT NOT NULL,
                completed TEXT
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                started TEXT NOT NULL,
                ended TEXT,
                duration INTEGER DEFAULT 0,
                manual_edit BOOLEAN DEFAULT FALSE,
                notes TEXT,
                FOREIGN KEY (task_id) REFERENCES tasks(id)
            )",
            [],
        )?;

        // Index for fast queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_area ON tasks(area)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_task ON sessions(task_id)",
            [],
        )?;

        Ok(())
    }

    pub fn add_task(
        &self,
        title: &str,
        area: CliArea,
        project: Option<String>,
        context: Option<String>,
    ) -> Result<Option<String>> {
        let task = Task::new(title, Area::from(area), project, context);
        let id = task.id.clone();
        
        self.conn.execute(
            "INSERT INTO tasks (id, title, area, project, context, status, created, completed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &task.id,
                &task.title,
                task.area.as_str(),
                task.project,
                task.context,
                task.status.as_str(),
                task.created.to_rfc3339(),
                task.completed.map(|d| d.to_rfc3339()),
            ],
        )?;
        
        Ok(Some(id))
    }

    pub fn list_tasks(&self, area: Option<CliArea>) -> Result<Vec<Task>> {
        let mut stmt = if let Some(area) = area {
            let area_str = Area::from(area).as_str();
            self.conn.prepare(
                "SELECT id, title, area, project, context, status, created, completed
                 FROM tasks WHERE area = ?1 AND status != 'Done'
                 ORDER BY created DESC"
            )?
            .query(params![area_str])?
        } else {
            // Default: show Today + Running tasks
            self.conn.prepare(
                "SELECT id, title, area, project, context, status, created, completed
                 FROM tasks WHERE (area = 'Today' OR status = 'Running') AND status != 'Done'
                 ORDER BY 
                    CASE status 
                        WHEN 'Running' THEN 0 
                        ELSE 1 
                    END,
                    created DESC"
            )?
            .query([])?
        };

        let mut tasks = Vec::new();
        while let Some(row) = stmt.next()? {
            tasks.push(self.row_to_task(&row)?);
        }
        
        Ok(tasks)
    }

    pub fn find_tasks(&self, query: &str) -> Result<Vec<Task>> {
        // Simple substring search for now - fuzzy comes later
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT id, title, area, project, context, status, created, completed
             FROM tasks WHERE title LIKE ?1 AND status != 'Done'
             ORDER BY created DESC"
        )?;
        
        let mut tasks = Vec::new();
        let mut rows = stmt.query(params![&pattern])?;
        while let Some(row) = rows.next()? {
            tasks.push(self.row_to_task(&row)?);
        }
        
        Ok(tasks)
    }

    pub fn start_timer(&self, query: &str) -> Result<()> {
        // Pause any running task first
        self.pause_timer()?;
        
        // Find task by fuzzy match
        let tasks = self.find_tasks(query)?;
        if tasks.is_empty() {
            anyhow::bail!("No task found matching '{}'", query);
        }
        
        // For now, take the first match (TODO: show fuzzy picker)
        let task = &tasks[0];
        
        // Update task status
        self.conn.execute(
            "UPDATE tasks SET status = 'Running' WHERE id = ?1",
            params![&task.id],
        )?;
        
        // Create session
        let session = Session::new(&task.id);
        self.conn.execute(
            "INSERT INTO sessions (id, task_id, started, ended, duration, manual_edit, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &session.id,
                &session.task_id,
                session.started.to_rfc3339(),
                session.ended.map(|d| d.to_rfc3339()),
                session.duration,
                session.manual_edit,
                session.notes,
            ],
        )?;
        
        Ok(())
    }

    pub fn pause_timer(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        
        // Find running task
        let running_id: Option<String> = tx.query_row(
            "SELECT id FROM tasks WHERE status = 'Running'",
            [],
            |row| row.get(0),
        ).optional()?;
        
        if let Some(task_id) = running_id {
            // Update task status
            tx.execute(
                "UPDATE tasks SET status = 'Paused' WHERE id = ?1",
                params![&task_id],
            )?;
            
            // Close open session
            tx.execute(
                "UPDATE sessions SET ended = ?1, duration = 
                 (julianday(?1) - julianday(started)) * 86400
                 WHERE task_id = ?2 AND ended IS NULL",
                params![Utc::now().to_rfc3339(), &task_id],
            )?;
        }
        
        tx.commit()?;
        Ok(())
    }

    pub fn complete_task(&self) -> Result<Option<(String, i64)>> {
        let tx = self.conn.unchecked_transaction()?;
        
        // Find running or paused task
        let result: Option<(String, String)> = tx.query_row(
            "SELECT id, title FROM tasks WHERE status IN ('Running', 'Paused')",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional()?;
        
        if let Some((task_id, title)) = result {
            // Close session if running
            tx.execute(
                "UPDATE sessions SET ended = ?1, duration = 
                 (julianday(?1) - julianday(started)) * 86400
                 WHERE task_id = ?2 AND ended IS NULL",
                params![Utc::now().to_rfc3339(), &task_id],
            )?;
            
            // Calculate total duration from today
            let total_duration: i64 = tx.query_row(
                "SELECT COALESCE(SUM(duration), 0) FROM sessions 
                 WHERE task_id = ?1 AND date(started) = date('now')",
                params![&task_id],
                |row| row.get(0),
            )?;
            
            // Mark as done
            tx.execute(
                "UPDATE tasks SET status = 'Done', area = 'Completed', completed = ?1
                 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), &task_id],
            )?;
            
            tx.commit()?;
            Ok(Some((title, total_duration)))
        } else {
            Ok(None)
        }
    }

    pub fn get_running_task(&self) -> Result<Option<(String, i64)>> {
        let result: Option<(String, String)> = self.conn.query_row(
            "SELECT t.id, t.title FROM tasks t
             WHERE t.status = 'Running'
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional()?;
        
        if let Some((task_id, title)) = result {
            // Calculate elapsed from session
            let elapsed: i64 = self.conn.query_row(
                "SELECT COALESCE(
                    (julianday(COALESCE(ended, ?1)) - julianday(started)) * 86400,
                    0
                ) FROM sessions 
                 WHERE task_id = ?2 AND ended IS NULL",
                params![Utc::now().to_rfc3339(), &task_id],
                |row| row.get(0),
            )?;
            
            Ok(Some((title, elapsed)))
        } else {
            Ok(None)
        }
    }

    pub fn delete_task(&self, query: &str) -> Result<()> {
        let tasks = self.find_tasks(query)?;
        if tasks.is_empty() {
            anyhow::bail!("No task found matching '{}'", query);
        }
        
        let task = &tasks[0];
        
        // Delete sessions first (foreign key constraint)
        self.conn.execute(
            "DELETE FROM sessions WHERE task_id = ?1",
            params![&task.id],
        )?;
        
        // Delete task
        self.conn.execute(
            "DELETE FROM tasks WHERE id = ?1",
            params![&task.id],
        )?;
        
        Ok(())
    }

    fn row_to_task(&self, row: &rusqlite::Row) -> Result<Task> {
        Ok(Task {
            id: row.get(0)?,
            title: row.get(1)?,
            area: Area::from_str(&row.get::<_, String>(2)?)
                .unwrap_or(Area::Today),
            project: row.get(3)?,
            context: row.get(4)?,
            status: TaskStatus::from_str(&row.get::<_, String>(5)?)
                .unwrap_or(TaskStatus::Pending),
            created: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                .map_err(|e| anyhow::anyhow!(e))?
                .into(),
            completed: row.get::<_, Option<String>>(7)?
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|d| d.into()),
        })
    }
}

impl From<CliArea> for Area {
    fn from(area: CliArea) -> Self {
        match area {
            CliArea::LongTerm => Area::LongTerm,
            CliArea::ThisWeek => Area::ThisWeek,
            CliArea::Today => Area::Today,
            CliArea::Completed => Area::Completed,
        }
    }
}
