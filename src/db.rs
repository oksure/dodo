use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

use crate::cli::Area as CliArea;
use crate::fuzzy::{find_best_match, rank_matches};
use crate::session::Session;
use crate::task::{Area, Task, TaskStatus};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::db_path()?;
        std::fs::create_dir_all(db_path.parent().unwrap())?;
        Self::open(&db_path)
    }

    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)
            .context("Failed to open database")?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .context("Failed to open in-memory database")?;
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

        // Add num_id column if it doesn't exist
        let has_num_id: bool = self.conn
            .prepare("SELECT num_id FROM tasks LIMIT 0")
            .is_ok();
        if !has_num_id {
            self.conn.execute_batch(
                "ALTER TABLE tasks ADD COLUMN num_id INTEGER;
                 UPDATE tasks SET num_id = ROWID WHERE num_id IS NULL;
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_num_id ON tasks(num_id);"
            )?;
        }

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
    ) -> Result<i64> {
        let task = Task::new(title, Area::from(area), project, context);

        let next_num_id: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(num_id), 0) + 1 FROM tasks",
            [],
            |row| row.get(0),
        )?;

        self.conn.execute(
            "INSERT INTO tasks (id, num_id, title, area, project, context, status, created, completed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &task.id,
                next_num_id,
                &task.title,
                task.area.as_str(),
                task.project,
                task.context,
                task.status.as_str(),
                task.created.to_rfc3339(),
                task.completed.map(|d| d.to_rfc3339()),
            ],
        )?;

        Ok(next_num_id)
    }

    pub fn list_tasks(&self, area: Option<CliArea>) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();

        if let Some(area) = area {
            let area_str = Area::from(area).as_str();
            let mut stmt = self.conn.prepare(
                "SELECT id, num_id, title, area, project, context, status, created, completed
                 FROM tasks WHERE area = ?1 AND status != 'Done'
                 ORDER BY created DESC"
            )?;
            let mut rows = stmt.query(params![area_str])?;
            while let Some(row) = rows.next()? {
                tasks.push(self.row_to_task(row)?);
            }
        } else {
            // Default: show Today + Running tasks
            let mut stmt = self.conn.prepare(
                "SELECT id, num_id, title, area, project, context, status, created, completed
                 FROM tasks WHERE (area = 'Today' OR status = 'Running') AND status != 'Done'
                 ORDER BY
                    CASE status
                        WHEN 'Running' THEN 0
                        ELSE 1
                    END,
                    created DESC"
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                tasks.push(self.row_to_task(row)?);
            }
        }

        Ok(tasks)
    }

    pub fn find_tasks(&self, query: &str) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, num_id, title, area, project, context, status, created, completed
             FROM tasks WHERE status != 'Done'"
        )?;

        let mut tasks = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            tasks.push(self.row_to_task(&row)?);
        }

        // Rank by fuzzy relevance
        let ranked = rank_matches(&tasks, query);
        Ok(ranked.into_iter().cloned().collect())
    }

    pub fn start_timer(&self, query: &str) -> Result<()> {
        // Pause any running task first
        self.pause_timer()?;

        let task = self.resolve_task(query)?;
        
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
            // Load the active session and stop it
            if let Some(mut session) = Self::get_active_session(&tx, &task_id)? {
                session.stop();
                tx.execute(
                    "UPDATE sessions SET ended = ?1, duration = ?2
                     WHERE id = ?3",
                    params![
                        session.ended.unwrap().to_rfc3339(),
                        session.duration,
                        &session.id,
                    ],
                )?;
            }

            // Update task status
            tx.execute(
                "UPDATE tasks SET status = 'Paused' WHERE id = ?1",
                params![&task_id],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn complete_task(&self) -> Result<Option<(String, i64)>> {
        let tx = self.conn.unchecked_transaction()?;

        // Find running or paused task
        let result: Option<(String, String)> = tx.query_row(
            "SELECT id, title FROM tasks WHERE status IN ('Running', 'Paused')
             ORDER BY CASE status WHEN 'Running' THEN 0 ELSE 1 END
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional()?;

        if let Some((task_id, title)) = result {
            // Close active session if running
            if let Some(mut session) = Self::get_active_session(&tx, &task_id)? {
                if session.is_running() {
                    session.stop();
                    tx.execute(
                        "UPDATE sessions SET ended = ?1, duration = ?2
                         WHERE id = ?3",
                        params![
                            session.ended.unwrap().to_rfc3339(),
                            session.duration,
                            &session.id,
                        ],
                    )?;
                }
            }

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
            let elapsed = match Self::get_active_session(&self.conn, &task_id)? {
                Some(session) => session.elapsed_seconds(),
                None => 0,
            };
            Ok(Some((title, elapsed)))
        } else {
            Ok(None)
        }
    }

    pub fn delete_task(&self, query: &str) -> Result<()> {
        let task = self.resolve_task(query)?;
        
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
            num_id: row.get(1)?,
            title: row.get(2)?,
            area: Area::from_str(&row.get::<_, String>(3)?)
                .unwrap_or(Area::Today),
            project: row.get(4)?,
            context: row.get(5)?,
            status: TaskStatus::from_str(&row.get::<_, String>(6)?)
                .unwrap_or(TaskStatus::Pending),
            created: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                .map_err(|e| anyhow::anyhow!(e))?
                .into(),
            completed: row.get::<_, Option<String>>(8)?
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|d| d.into()),
        })
    }

    fn row_to_session(row: &rusqlite::Row) -> Result<Session> {
        Ok(Session {
            id: row.get(0)?,
            task_id: row.get(1)?,
            started: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                .map_err(|e| anyhow::anyhow!(e))?
                .into(),
            ended: row.get::<_, Option<String>>(3)?
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|d| d.into()),
            duration: row.get(4)?,
            manual_edit: row.get(5)?,
            notes: row.get(6)?,
        })
    }

    fn get_active_session(conn: &Connection, task_id: &str) -> Result<Option<Session>> {
        let mut stmt = conn.prepare(
            "SELECT id, task_id, started, ended, duration, manual_edit, notes
             FROM sessions WHERE task_id = ?1 AND ended IS NULL
             LIMIT 1"
        )?;
        let mut rows = stmt.query(params![task_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_session(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn find_task_by_num_id(&self, num_id: i64) -> Result<Option<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, num_id, title, area, project, context, status, created, completed
             FROM tasks WHERE num_id = ?1"
        )?;
        let mut rows = stmt.query(params![num_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_task(row)?))
        } else {
            Ok(None)
        }
    }

    fn resolve_task(&self, query: &str) -> Result<Task> {
        // Try numeric ID first
        if let Ok(num_id) = query.parse::<i64>() {
            if let Some(task) = self.find_task_by_num_id(num_id)? {
                return Ok(task);
            }
        }
        // Fall back to fuzzy matching across all non-done tasks
        let tasks = self.find_tasks(query)?;
        if let Some(best) = find_best_match(&tasks, query) {
            return Ok(best.clone());
        }
        anyhow::bail!("No task found matching '{}'", query);
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
