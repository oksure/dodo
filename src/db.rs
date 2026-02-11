use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

use crate::cli::{Area as CliArea, SortBy};
use crate::fuzzy::{find_best_match, rank_matches};
use crate::notation::ParsedInput;
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

        // Add estimate_minutes column
        if self.conn.prepare("SELECT estimate_minutes FROM tasks LIMIT 0").is_err() {
            self.conn.execute("ALTER TABLE tasks ADD COLUMN estimate_minutes INTEGER", [])?;
        }

        // Add deadline column
        if self.conn.prepare("SELECT deadline FROM tasks LIMIT 0").is_err() {
            self.conn.execute("ALTER TABLE tasks ADD COLUMN deadline TEXT", [])?;
        }

        // Add scheduled column
        if self.conn.prepare("SELECT scheduled FROM tasks LIMIT 0").is_err() {
            self.conn.execute("ALTER TABLE tasks ADD COLUMN scheduled TEXT", [])?;
        }

        // Add tags column
        if self.conn.prepare("SELECT tags FROM tasks LIMIT 0").is_err() {
            self.conn.execute("ALTER TABLE tasks ADD COLUMN tags TEXT", [])?;
        }

        // Add task_notes column (named to avoid conflict with sessions.notes)
        if self.conn.prepare("SELECT task_notes FROM tasks LIMIT 0").is_err() {
            self.conn.execute("ALTER TABLE tasks ADD COLUMN task_notes TEXT", [])?;
        }

        // Add priority column
        if self.conn.prepare("SELECT priority FROM tasks LIMIT 0").is_err() {
            self.conn.execute("ALTER TABLE tasks ADD COLUMN priority INTEGER", [])?;
        }

        // Add modified_at column
        if self.conn.prepare("SELECT modified_at FROM tasks LIMIT 0").is_err() {
            self.conn.execute("ALTER TABLE tasks ADD COLUMN modified_at TEXT", [])?;
            self.conn.execute("UPDATE tasks SET modified_at = created WHERE modified_at IS NULL", [])?;
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

    #[allow(clippy::too_many_arguments)]
    pub fn add_task(
        &self,
        title: &str,
        area: CliArea,
        project: Option<String>,
        context: Option<String>,
        estimate_minutes: Option<i64>,
        deadline: Option<NaiveDate>,
        scheduled: Option<NaiveDate>,
        tags: Option<String>,
        priority: Option<i64>,
    ) -> Result<i64> {
        let task = Task::new(title, Area::from(area), project, context);

        let next_num_id: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(num_id), 0) + 1 FROM tasks",
            [],
            |row| row.get(0),
        )?;

        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO tasks (id, num_id, title, area, project, context, status, created, completed, estimate_minutes, deadline, scheduled, tags, priority, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                estimate_minutes,
                deadline.map(|d| d.to_string()),
                scheduled.map(|d| d.to_string()),
                tags,
                priority,
                &now,
            ],
        )?;

        Ok(next_num_id)
    }

    const TASK_SELECT_WITH_ELAPSED: &'static str =
        "SELECT t.id, t.num_id, t.title, t.area, t.project, t.context, t.status, t.created, t.completed,
                t.estimate_minutes, t.deadline, t.scheduled, t.tags, t.task_notes, t.priority,
                t.modified_at,
                COALESCE(SUM(
                    CASE WHEN s.ended IS NOT NULL THEN s.duration
                    ELSE CAST((julianday('now') - julianday(s.started)) * 86400 AS INTEGER)
                    END
                ), 0) as elapsed_seconds
         FROM tasks t LEFT JOIN sessions s ON s.task_id = t.id";

    fn sort_order_sql(sort: SortBy, is_completed: bool) -> &'static str {
        match (sort, is_completed) {
            (SortBy::Created, true) => "t.created DESC",
            (SortBy::Created, false) => "t.created ASC",
            (SortBy::Modified, true) => "COALESCE(t.modified_at, t.created) DESC",
            (SortBy::Modified, false) => "COALESCE(t.modified_at, t.created) ASC",
            (SortBy::Area, _) => "CASE t.area WHEN 'LongTerm' THEN 0 WHEN 'ThisWeek' THEN 1 WHEN 'Today' THEN 2 WHEN 'Completed' THEN 3 END, t.created ASC",
            (SortBy::Title, _) => "t.title ASC",
        }
    }

    pub fn list_tasks(&self, area: Option<CliArea>) -> Result<Vec<Task>> {
        self.list_tasks_sorted(area, SortBy::Created)
    }

    pub fn list_tasks_sorted(&self, area: Option<CliArea>, sort: SortBy) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();

        if let Some(area) = area {
            let area_str = Area::from(area).as_str();
            let is_completed = matches!(area, CliArea::Completed);
            let order = Self::sort_order_sql(sort, is_completed);
            let filter = if is_completed {
                "WHERE t.area = ?1"
            } else {
                "WHERE t.area = ?1 AND t.status != 'Done'"
            };
            let query = format!(
                "{} {} GROUP BY t.id ORDER BY {}",
                Self::TASK_SELECT_WITH_ELAPSED, filter, order
            );
            let mut stmt = self.conn.prepare(&query)?;
            let mut rows = stmt.query(params![area_str])?;
            while let Some(row) = rows.next()? {
                tasks.push(self.row_to_task(row)?);
            }
        } else {
            let order = Self::sort_order_sql(sort, false);
            // Default: show Today + Running tasks
            let query = format!(
                "{} WHERE (t.area = 'Today' OR t.status = 'Running') AND t.status != 'Done'
                 GROUP BY t.id
                 ORDER BY
                    CASE t.status
                        WHEN 'Running' THEN 0
                        ELSE 1
                    END,
                    {}",
                Self::TASK_SELECT_WITH_ELAPSED, order
            );
            let mut stmt = self.conn.prepare(&query)?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                tasks.push(self.row_to_task(row)?);
            }
        }

        Ok(tasks)
    }

    pub fn find_tasks(&self, query: &str) -> Result<Vec<Task>> {
        let sql = format!(
            "{} WHERE t.status != 'Done' GROUP BY t.id",
            Self::TASK_SELECT_WITH_ELAPSED
        );
        let mut stmt = self.conn.prepare(&sql)?;

        let mut tasks = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            tasks.push(self.row_to_task(row)?);
        }

        // Rank by fuzzy relevance
        let ranked = rank_matches(&tasks, query);
        Ok(ranked.into_iter().cloned().collect())
    }

    pub fn start_timer(&self, query: &str) -> Result<(String, i64)> {
        // Pause any running task first
        self.pause_timer()?;

        let task = self.resolve_task(query)?;
        let title = task.title.clone();
        let num_id = task.num_id.unwrap_or(0);

        // Update task status
        self.conn.execute(
            "UPDATE tasks SET status = 'Running', modified_at = ?2 WHERE id = ?1",
            params![&task.id, Utc::now().to_rfc3339()],
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

        Ok((title, num_id))
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
                "UPDATE tasks SET status = 'Paused', modified_at = ?2 WHERE id = ?1",
                params![&task_id, Utc::now().to_rfc3339()],
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
            let now = Utc::now().to_rfc3339();
            tx.execute(
                "UPDATE tasks SET status = 'Done', area = 'Completed', completed = ?1, modified_at = ?1
                 WHERE id = ?2",
                params![&now, &task_id],
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

    pub fn delete_task(&self, query: &str) -> Result<(String, i64)> {
        let task = self.resolve_task(query)?;
        let title = task.title.clone();
        let num_id = task.num_id.unwrap_or(0);

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

        Ok((title, num_id))
    }

    pub fn append_note(&self, query: &str, text: &str) -> Result<String> {
        let task = self.resolve_task(query)?;
        let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M]");
        let new_entry = format!("{} {}", timestamp, text);

        let existing: Option<String> = self.conn.query_row(
            "SELECT task_notes FROM tasks WHERE id = ?1",
            params![&task.id],
            |row| row.get(0),
        )?;

        let updated = match existing {
            Some(ref notes) if !notes.is_empty() => format!("{}\n{}", notes, new_entry),
            _ => new_entry,
        };

        self.conn.execute(
            "UPDATE tasks SET task_notes = ?1, modified_at = ?3 WHERE id = ?2",
            params![&updated, &task.id, Utc::now().to_rfc3339()],
        )?;

        Ok(task.title)
    }

    pub fn clear_notes(&self, query: &str) -> Result<String> {
        let task = self.resolve_task(query)?;
        self.conn.execute(
            "UPDATE tasks SET task_notes = NULL, modified_at = ?2 WHERE id = ?1",
            params![&task.id, Utc::now().to_rfc3339()],
        )?;
        Ok(task.title)
    }

    pub fn get_task_notes(&self, query: &str) -> Result<(String, Option<String>)> {
        let task = self.resolve_task(query)?;
        let notes: Option<String> = self.conn.query_row(
            "SELECT task_notes FROM tasks WHERE id = ?1",
            params![&task.id],
            |row| row.get(0),
        )?;
        Ok((task.title, notes))
    }

    pub fn update_task_fields(&self, query: &str, input: &ParsedInput, area: Option<CliArea>) -> Result<String> {
        let task = self.resolve_task(query)?;

        if let Some(ref project) = input.project {
            self.conn.execute(
                "UPDATE tasks SET project = ?1 WHERE id = ?2",
                params![project, &task.id],
            )?;
        }

        if !input.contexts.is_empty() {
            let ctx = input.contexts.join(",");
            self.conn.execute(
                "UPDATE tasks SET context = ?1 WHERE id = ?2",
                params![ctx, &task.id],
            )?;
        }

        if !input.tags.is_empty() {
            let tags = input.tags.join(",");
            self.conn.execute(
                "UPDATE tasks SET tags = ?1 WHERE id = ?2",
                params![tags, &task.id],
            )?;
        }

        if let Some(est) = input.estimate_minutes {
            self.conn.execute(
                "UPDATE tasks SET estimate_minutes = ?1 WHERE id = ?2",
                params![est, &task.id],
            )?;
        }

        if let Some(ref dl) = input.deadline {
            self.conn.execute(
                "UPDATE tasks SET deadline = ?1 WHERE id = ?2",
                params![dl.to_string(), &task.id],
            )?;
        }

        if let Some(ref sc) = input.scheduled {
            self.conn.execute(
                "UPDATE tasks SET scheduled = ?1 WHERE id = ?2",
                params![sc.to_string(), &task.id],
            )?;
        }

        if let Some(p) = input.priority {
            self.conn.execute(
                "UPDATE tasks SET priority = ?1 WHERE id = ?2",
                params![p, &task.id],
            )?;
        }

        if let Some(area) = area {
            self.conn.execute(
                "UPDATE tasks SET area = ?1 WHERE id = ?2",
                params![Area::from(area).as_str(), &task.id],
            )?;
        }

        // Always update modified_at when any field changes
        self.conn.execute(
            "UPDATE tasks SET modified_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), &task.id],
        )?;

        Ok(task.title)
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
            modified_at: row.get::<_, Option<String>>(15)?
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|d| d.into()),
            estimate_minutes: row.get(9)?,
            deadline: row.get::<_, Option<String>>(10)?
                .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
            scheduled: row.get::<_, Option<String>>(11)?
                .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
            priority: row.get(14)?,
            tags: row.get(12)?,
            notes: row.get(13)?,
            elapsed_seconds: row.get(16).ok(),
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

    /// Load all tasks with elapsed seconds for TUI grouping by effective_area()
    pub fn list_all_tasks(&self, sort: SortBy) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        // Load non-done with ASC, done with DESC, combined
        let order_nondone = Self::sort_order_sql(sort, false);
        let order_done = Self::sort_order_sql(sort, true);

        // Non-done tasks
        let query = format!(
            "{} WHERE t.status != 'Done' GROUP BY t.id ORDER BY {}",
            Self::TASK_SELECT_WITH_ELAPSED, order_nondone
        );
        let mut stmt = self.conn.prepare(&query)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            tasks.push(self.row_to_task(row)?);
        }

        // Done tasks
        let query = format!(
            "{} WHERE t.status = 'Done' GROUP BY t.id ORDER BY {}",
            Self::TASK_SELECT_WITH_ELAPSED, order_done
        );
        let mut stmt = self.conn.prepare(&query)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            tasks.push(self.row_to_task(row)?);
        }

        Ok(tasks)
    }

    pub fn list_tasks_by_project(&self, project: &str, sort: SortBy) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        let order = Self::sort_order_sql(sort, false);
        let query = format!(
            "{} WHERE t.project = ?1 AND t.status != 'Done' GROUP BY t.id ORDER BY {}",
            Self::TASK_SELECT_WITH_ELAPSED, order
        );
        let mut stmt = self.conn.prepare(&query)?;
        let mut rows = stmt.query(params![project])?;
        while let Some(row) = rows.next()? {
            tasks.push(self.row_to_task(row)?);
        }
        Ok(tasks)
    }

    pub fn append_note_by_id(&self, task_id: &str, text: &str) -> Result<()> {
        let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M]");
        let new_entry = format!("{} {}", timestamp, text);

        let existing: Option<String> = self.conn.query_row(
            "SELECT task_notes FROM tasks WHERE id = ?1",
            params![task_id],
            |row| row.get(0),
        )?;

        let updated = match existing {
            Some(ref notes) if !notes.is_empty() => format!("{}\n{}", notes, new_entry),
            _ => new_entry,
        };

        self.conn.execute(
            "UPDATE tasks SET task_notes = ?1, modified_at = ?3 WHERE id = ?2",
            params![&updated, task_id, Utc::now().to_rfc3339()],
        )?;

        Ok(())
    }

    pub fn get_task_notes_by_id(&self, task_id: &str) -> Result<Option<String>> {
        let notes: Option<String> = self.conn.query_row(
            "SELECT task_notes FROM tasks WHERE id = ?1",
            params![task_id],
            |row| row.get(0),
        )?;
        Ok(notes)
    }

    pub fn update_task_scheduled(&self, task_id: &str, date: NaiveDate) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET scheduled = ?1, modified_at = ?3 WHERE id = ?2",
            params![date.to_string(), task_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn find_task_by_num_id(&self, num_id: i64) -> Result<Option<Task>> {
        let query = format!(
            "{} WHERE t.num_id = ?1 GROUP BY t.id",
            Self::TASK_SELECT_WITH_ELAPSED
        );
        let mut stmt = self.conn.prepare(&query)?;
        let mut rows = stmt.query(params![num_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_task(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn resolve_task(&self, query: &str) -> Result<Task> {
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

    /// Report: count completed tasks in date range
    pub fn report_tasks_done(&self, from: &str, to: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'Done' AND completed >= ?1 AND completed < ?2",
            params![from, to],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Report: total seconds worked in date range (from completed sessions)
    pub fn report_total_seconds(&self, from: &str, to: &str) -> Result<i64> {
        let total: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(duration), 0) FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL",
            params![from, to],
            |row| row.get(0),
        )?;
        Ok(total)
    }

    /// Report: sessions grouped by hour of day (0-23) -> total seconds per hour
    pub fn report_by_hour(&self, from: &str, to: &str) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT CAST(strftime('%H', started) AS INTEGER) as hour, COALESCE(SUM(duration), 0)
             FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL
             GROUP BY hour ORDER BY hour"
        )?;
        let mut rows = stmt.query(params![from, to])?;
        let mut result = vec![];
        while let Some(row) = rows.next()? {
            result.push((row.get(0)?, row.get(1)?));
        }
        Ok(result)
    }

    /// Report: sessions grouped by day of week (0=Sun..6=Sat) -> total seconds
    pub fn report_by_weekday(&self, from: &str, to: &str) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT CAST(strftime('%w', started) AS INTEGER) as dow, COALESCE(SUM(duration), 0)
             FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL
             GROUP BY dow ORDER BY dow"
        )?;
        let mut rows = stmt.query(params![from, to])?;
        let mut result = vec![];
        while let Some(row) = rows.next()? {
            result.push((row.get(0)?, row.get(1)?));
        }
        Ok(result)
    }

    /// Report: time by project in date range
    pub fn report_by_project(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(t.project, '(none)'), COALESCE(SUM(s.duration), 0)
             FROM sessions s JOIN tasks t ON s.task_id = t.id
             WHERE s.started >= ?1 AND s.started < ?2 AND s.ended IS NOT NULL
             GROUP BY t.project ORDER BY SUM(s.duration) DESC"
        )?;
        let mut rows = stmt.query(params![from, to])?;
        let mut result = vec![];
        while let Some(row) = rows.next()? {
            result.push((row.get(0)?, row.get(1)?));
        }
        Ok(result)
    }

    /// Report: recently completed tasks in date range
    pub fn report_done_tasks(&self, from: &str, to: &str, limit: i64) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT title, COALESCE(
                (SELECT SUM(duration) FROM sessions WHERE task_id = t.id), 0
             ) FROM tasks t
             WHERE status = 'Done' AND completed >= ?1 AND completed < ?2
             ORDER BY completed DESC LIMIT ?3"
        )?;
        let mut rows = stmt.query(params![from, to, limit])?;
        let mut result = vec![];
        while let Some(row) = rows.next()? {
            result.push((row.get(0)?, row.get(1)?));
        }
        Ok(result)
    }

    /// Report: total number of distinct days with work sessions
    pub fn report_active_days(&self, from: &str, to: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT date(started)) FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL",
            params![from, to],
            |row| row.get(0),
        )?;
        Ok(count)
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
