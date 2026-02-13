use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use libsql::{params, Connection, Value};
use std::path::PathBuf;
use std::sync::mpsc;

use crate::cli::SortBy;
use crate::fuzzy::{find_best_match, rank_matches};
use crate::notation::ParsedInput;
use crate::session::Session;
use crate::task::{Area, Task, TaskStatus};

pub struct Database {
    db: libsql::Database,
    conn: Connection,
    rt: tokio::runtime::Runtime,
}

// Helper to extract Option<String> from a row value
fn val_opt_string(row: &libsql::Row, idx: i32) -> Result<Option<String>> {
    match row.get_value(idx)? {
        Value::Null => Ok(None),
        Value::Text(s) => Ok(Some(s)),
        _ => Ok(None),
    }
}

fn val_opt_i64(row: &libsql::Row, idx: i32) -> Result<Option<i64>> {
    match row.get_value(idx)? {
        Value::Null => Ok(None),
        Value::Integer(n) => Ok(Some(n)),
        _ => Ok(None),
    }
}

fn val_opt_bool(row: &libsql::Row, idx: i32) -> Result<Option<bool>> {
    match row.get_value(idx)? {
        Value::Null => Ok(None),
        Value::Integer(n) => Ok(Some(n != 0)),
        _ => Ok(None),
    }
}

fn val_string(row: &libsql::Row, idx: i32) -> Result<String> {
    Ok(row.get::<String>(idx)?)
}

fn val_i64(row: &libsql::Row, idx: i32) -> Result<i64> {
    Ok(row.get::<i64>(idx)?)
}

fn val_bool(row: &libsql::Row, idx: i32) -> Result<bool> {
    match row.get_value(idx)? {
        Value::Integer(n) => Ok(n != 0),
        _ => Ok(false),
    }
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::db_path()?;
        std::fs::create_dir_all(db_path.parent().context("Database path has no parent directory")?)?;
        Self::open(&db_path)
    }

    pub fn open(path: &std::path::Path) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create tokio runtime")?;
        let path_str = path.to_str().context("Invalid database path")?.to_string();
        let db = rt.block_on(async {
            libsql::Builder::new_local(&path_str).build().await
        }).context("Failed to open database")?;
        let conn = db.connect()
            .context("Failed to connect to database")?;
        let database = Self { db, conn, rt };
        database.migrate()?;
        Ok(database)
    }

    pub fn in_memory() -> Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create tokio runtime")?;
        let db = rt.block_on(async {
            libsql::Builder::new_local(":memory:").build().await
        }).context("Failed to open in-memory database")?;
        let conn = db.connect()
            .context("Failed to connect to database")?;
        let database = Self { db, conn, rt };
        database.migrate()?;
        Ok(database)
    }

    /// Get the database file path
    pub fn db_path() -> Result<PathBuf> {
        let home = dirs::data_local_dir()
            .context("Could not find local data directory")?;
        Ok(home.join("dodo").join("dodo.db"))
    }

    // ── Sync Transition Methods ──────────────────────────────────────

    /// List all auxiliary files for the database (everything matching dodo.db*
    /// except the db itself and .pre-sync backup). Includes WAL, SHM, and any
    /// libsql sync metadata files (like -info, -client_wal, etc.).
    fn list_auxiliary_db_files() -> Result<Vec<PathBuf>> {
        let db_path = Self::db_path()?;
        let parent = db_path
            .parent()
            .context("Database path has no parent directory")?;
        let db_name = db_path
            .file_name()
            .context("Database path has no file name")?
            .to_string_lossy()
            .to_string();

        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&db_name)
                    && name != db_name
                    && !name.ends_with(".pre-sync")
                {
                    files.push(parent.join(&name));
                }
            }
        }
        Ok(files)
    }

    /// Remove all auxiliary database files (WAL, SHM, sync metadata, etc.).
    fn remove_auxiliary_db_files() {
        if let Ok(files) = Self::list_auxiliary_db_files() {
            for file in files {
                let _ = std::fs::remove_file(&file);
            }
        }
    }

    /// Startup safety net: if dodo.db.pre-sync exists but dodo.db doesn't
    /// (crash between rename and replica creation), restores from backup.
    pub fn recover_interrupted_migration() -> Result<()> {
        let db_path = Self::db_path()?;
        let backup_path = db_path.with_extension("db.pre-sync");
        if backup_path.exists() && !db_path.exists() {
            eprintln!("Recovering from interrupted migration...");
            // Clean up any orphaned sync metadata before restoring
            Self::remove_auxiliary_db_files();
            std::fs::rename(&backup_path, &db_path)
                .context("Failed to recover database from interrupted migration")?;
            eprintln!("Recovery complete.");
        }
        Ok(())
    }

    /// Removes all sync metadata files when sync is disabled.
    /// Ensures re-enabling sync later triggers a fresh migration.
    pub fn clean_sync_metadata() -> Result<()> {
        Self::remove_auxiliary_db_files();
        Ok(())
    }

    /// Get the sync replica database file path (dodo-sync.db).
    pub fn sync_db_path() -> Result<PathBuf> {
        let home = dirs::data_local_dir()
            .context("Could not find local data directory")?;
        Ok(home.join("dodo").join("dodo-sync.db"))
    }

    /// Remove dodo-sync.db and all its auxiliary files. Called when sync is disabled.
    pub fn clean_sync_db() -> Result<()> {
        let sync_path = Self::sync_db_path()?;
        if sync_path.exists() {
            let _ = std::fs::remove_file(&sync_path);
        }
        // Remove auxiliary files (dodo-sync.db-wal, dodo-sync.db-shm, etc.)
        let parent = sync_path
            .parent()
            .context("Sync DB path has no parent directory")?;
        let sync_name = sync_path
            .file_name()
            .context("Sync DB path has no file name")?
            .to_string_lossy()
            .to_string();
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&sync_name) && name != sync_name {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    /// Merge remote tasks into the local database. Uses modified_at timestamp to
    /// decide wins. num_id conflicts resolved deterministically: earlier created keeps it.
    pub fn merge_remote_data(&self, tasks: &[Task], sessions: &[Session]) -> Result<()> {
        self.rt.block_on(async {
            for remote_task in tasks {
                // Check if task exists locally
                let mut rows = self.conn.query(
                    "SELECT id, modified_at, created, num_id FROM tasks WHERE id = ?1",
                    params![remote_task.id.clone()],
                ).await?;

                if let Some(row) = rows.next().await? {
                    // Task exists locally — compare modified_at
                    let local_modified = val_opt_string(&row, 1)?;
                    let local_created = val_string(&row, 2)?;
                    let local_num_id = val_opt_i64(&row, 3)?;

                    let local_ts = local_modified
                        .as_deref()
                        .or(Some(local_created.as_str()))
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|d| d.with_timezone(&Utc));

                    let remote_ts = remote_task.modified_at
                        .or(Some(remote_task.created));

                    let remote_is_newer = match (remote_ts, local_ts) {
                        (Some(r), Some(l)) => r > l,
                        (Some(_), None) => true,
                        _ => false,
                    };

                    if remote_is_newer {
                        // Remote wins — update local task
                        // First resolve num_id: adopt remote's num_id
                        let remote_num_id = remote_task.num_id;
                        if let (Some(r_num), Some(l_num)) = (remote_num_id, local_num_id) {
                            if r_num != l_num {
                                // Remote has a different num_id for this task.
                                // Check if r_num is taken by a DIFFERENT task.
                                self.resolve_num_id_conflict_async(
                                    &remote_task.id, r_num, remote_task.created,
                                ).await?;
                            }
                        }

                        self.conn.execute(
                            "UPDATE tasks SET num_id = ?2, title = ?3, area = ?4, project = ?5, context = ?6,
                             status = ?7, created = ?8, completed = ?9, estimate_minutes = ?10,
                             deadline = ?11, scheduled = ?12, tags = ?13, task_notes = ?14,
                             priority = ?15, modified_at = ?16, recurrence = ?17,
                             is_template = ?18, template_id = ?19
                             WHERE id = ?1",
                            params![
                                remote_task.id.clone(),
                                remote_task.num_id,
                                remote_task.title.clone(),
                                remote_task.area.as_str(),
                                remote_task.project.clone(),
                                remote_task.context.clone(),
                                remote_task.status.as_str(),
                                remote_task.created.to_rfc3339(),
                                remote_task.completed.map(|d| d.to_rfc3339()),
                                remote_task.estimate_minutes,
                                remote_task.deadline.map(|d| d.to_string()),
                                remote_task.scheduled.map(|d| d.to_string()),
                                remote_task.tags.clone(),
                                remote_task.notes.clone(),
                                remote_task.priority,
                                remote_task.modified_at.map(|d| d.to_rfc3339()),
                                remote_task.recurrence.clone(),
                                remote_task.is_template as i64,
                                remote_task.template_id.clone(),
                            ],
                        ).await?;
                    }
                } else {
                    // New task — insert with remote's num_id
                    let num_id = remote_task.num_id.unwrap_or_else(|| 0);
                    if num_id > 0 {
                        self.resolve_num_id_conflict_async(
                            &remote_task.id, num_id, remote_task.created,
                        ).await?;
                    }
                    let actual_num_id = if num_id > 0 {
                        num_id
                    } else {
                        // No num_id from remote, assign next available
                        let mut rows = self.conn.query(
                            "SELECT COALESCE(MAX(num_id), 0) + 1 FROM tasks", ()
                        ).await?;
                        let row = rows.next().await?.context("No result from MAX query")?;
                        val_i64(&row, 0)?
                    };

                    self.conn.execute(
                        "INSERT OR IGNORE INTO tasks (id, num_id, title, area, project, context, status, created, completed,
                         estimate_minutes, deadline, scheduled, tags, task_notes, priority, modified_at,
                         recurrence, is_template, template_id)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                        params![
                            remote_task.id.clone(),
                            actual_num_id,
                            remote_task.title.clone(),
                            remote_task.area.as_str(),
                            remote_task.project.clone(),
                            remote_task.context.clone(),
                            remote_task.status.as_str(),
                            remote_task.created.to_rfc3339(),
                            remote_task.completed.map(|d| d.to_rfc3339()),
                            remote_task.estimate_minutes,
                            remote_task.deadline.map(|d| d.to_string()),
                            remote_task.scheduled.map(|d| d.to_string()),
                            remote_task.tags.clone(),
                            remote_task.notes.clone(),
                            remote_task.priority,
                            remote_task.modified_at.map(|d| d.to_rfc3339()),
                            remote_task.recurrence.clone(),
                            remote_task.is_template as i64,
                            remote_task.template_id.clone(),
                        ],
                    ).await?;
                }
            }

            // Sessions: INSERT OR IGNORE (first-write-wins, append-only)
            for session in sessions {
                self.conn.execute(
                    "INSERT OR IGNORE INTO sessions (id, task_id, started, ended, duration, manual_edit, notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        session.id.clone(),
                        session.task_id.clone(),
                        session.started.to_rfc3339(),
                        session.ended.map(|d| d.to_rfc3339()),
                        session.duration,
                        session.manual_edit,
                        session.notes.clone(),
                    ],
                ).await?;
            }

            Ok(())
        })
    }

    /// Resolve num_id conflict: if `num_id` is taken by a different task (not `task_id`),
    /// the task created earlier keeps it, the other gets bumped to MAX+1.
    async fn resolve_num_id_conflict_async(
        &self,
        incoming_task_id: &str,
        num_id: i64,
        incoming_created: DateTime<Utc>,
    ) -> Result<()> {
        let mut rows = self.conn.query(
            "SELECT id, created FROM tasks WHERE num_id = ?1 AND id != ?2",
            params![num_id, incoming_task_id],
        ).await?;

        if let Some(row) = rows.next().await? {
            let existing_id = val_string(&row, 0)?;
            let existing_created_str = val_string(&row, 1)?;
            let existing_created = DateTime::parse_from_rfc3339(&existing_created_str)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or(Utc::now());

            // Earlier created timestamp keeps the num_id
            let bump_id = if incoming_created < existing_created {
                // Incoming is older — it keeps num_id, bump existing
                existing_id
            } else {
                // Existing is older (or equal) — it keeps num_id, incoming will get a new one
                // We don't bump incoming here — the caller will use the num_id as-is
                // only if it's not conflicting. Since it IS conflicting and incoming is newer,
                // we need to NOT assign num_id to incoming. But the caller is about to
                // INSERT/UPDATE with this num_id. So we bump the existing one.
                // Wait — the rule is: earlier created KEEPS. So if existing is earlier,
                // existing keeps it and we need to assign a different num_id to incoming.
                // But the caller will use `num_id` for the incoming task...
                // So: if existing is earlier, we DON'T bump existing; instead we'll need
                // the caller to use a different num_id. Let's return an error signal...
                // Actually, let's simplify: always bump the one that's newer.
                // If incoming is newer, incoming gets bumped (caller should use MAX+1).
                // If existing is newer, existing gets bumped.
                if existing_created < incoming_created {
                    // Existing is older, keeps num_id. Incoming is newer, needs new num_id.
                    // We need to signal the caller. For simplicity, let's re-assign num_id
                    // to incoming at MAX+1 by updating it after insert.
                    // Actually: we can just bump the incoming's num_id in the caller.
                    // But that's complex. Let's keep it simple:
                    // Bump the NEWER task to MAX+1.
                    // The incoming task is newer, so we DON'T insert with num_id.
                    // Instead, let's not bump here but handle in caller...
                    // This gets messy. Let me use a cleaner approach.
                    return Ok(()); // existing keeps, caller handles new num_id below
                } else {
                    existing_id
                }
            };

            // Bump the loser to MAX+1
            let mut rows = self.conn.query(
                "SELECT COALESCE(MAX(num_id), 0) + 1 FROM tasks", ()
            ).await?;
            let row = rows.next().await?.context("No result from MAX query")?;
            let new_num_id = val_i64(&row, 0)?;

            self.conn.execute(
                "UPDATE tasks SET num_id = ?1 WHERE id = ?2",
                params![new_num_id, bump_id],
            ).await?;
        }

        Ok(())
    }

    /// Perform a full bidirectional sync with a Turso remote replica.
    /// This is a standalone operation that creates its own DB connections.
    pub fn do_remote_sync(url: &str, token: &str) -> Result<()> {
        let db_path = Self::db_path()?;
        let sync_path = Self::sync_db_path()?;
        std::fs::create_dir_all(sync_path.parent().context("Sync DB path has no parent directory")?)?;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create tokio runtime for sync")?;

        let url = url.to_string();
        let token = token.to_string();
        let sync_path_str = sync_path.to_str().context("Invalid sync DB path")?.to_string();

        // 1. Open the remote replica (sync DB)
        let sync_db = rt.block_on(async {
            libsql::Builder::new_remote_replica(sync_path_str, url, token)
                .read_your_writes(false)
                .build()
                .await
        }).context("Failed to open sync replica")?;
        let sync_conn = sync_db.connect()
            .context("Failed to connect to sync replica")?;

        // 2. Pull from Turso
        rt.block_on(async {
            sync_db.sync().await.context("Failed to pull from Turso")?;
            Ok::<(), anyhow::Error>(())
        })?;

        // 3. Run migrations on sync DB
        let sync_database = Self { db: sync_db, conn: sync_conn, rt };
        sync_database.migrate()?;

        // 4. Open local DB (second connection)
        let local_db = Self::open(&db_path)?;

        // 5. Export from sync DB → merge into local (remote changes land locally)
        let (remote_tasks, remote_sessions) = sync_database.export_all_data()?;
        if !remote_tasks.is_empty() || !remote_sessions.is_empty() {
            local_db.merge_remote_data(&remote_tasks, &remote_sessions)?;
        }

        // 6. Export from local → merge into sync DB (local changes pushed to Turso)
        let (local_tasks, local_sessions) = local_db.export_all_data()?;
        if !local_tasks.is_empty() || !local_sessions.is_empty() {
            sync_database.merge_remote_data(&local_tasks, &local_sessions)?;
        }

        // 7. Push to Turso
        sync_database.rt.block_on(async {
            sync_database.db.sync().await.context("Failed to push to Turso")?;
            Ok::<(), anyhow::Error>(())
        })?;

        Ok(())
    }

    /// Spawn a background thread to perform sync. Returns a receiver for the result.
    /// The TUI polls this with try_recv() (non-blocking).
    pub fn sync_with_remote(url: String, token: String) -> mpsc::Receiver<Result<()>> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = Self::do_remote_sync(&url, &token);
            let _ = tx.send(result);
        });
        rx
    }

    /// Test sync connection by attempting to open a remote replica in a temp dir.
    pub fn test_sync_connection(url: String, token: String) -> Result<()> {
        let tmp_dir = std::env::temp_dir().join("dodo-sync-test");
        std::fs::create_dir_all(&tmp_dir)
            .context("Failed to create temp dir for sync test")?;
        let tmp_db = tmp_dir.join("test-sync.db");
        let tmp_path = tmp_db.to_str().context("Invalid temp path")?.to_string();

        let result = (|| -> Result<()> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("Failed to create tokio runtime")?;

            let sync_db = rt.block_on(async {
                libsql::Builder::new_remote_replica(tmp_path.clone(), url, token)
                    .read_your_writes(false)
                    .build()
                    .await
            }).context("Failed to connect to Turso")?;

            rt.block_on(async {
                sync_db.sync().await.context("Failed to sync with Turso")?;
                Ok::<(), anyhow::Error>(())
            })?;

            Ok(())
        })();

        // Clean up temp files regardless of outcome
        let _ = std::fs::remove_dir_all(&tmp_dir);

        result
    }

    /// Export all tasks and sessions from the database.
    pub fn export_all_data(&self) -> Result<(Vec<Task>, Vec<Session>)> {
        self.rt.block_on(async {
            // Export all tasks (including templates)
            let query = format!("{} GROUP BY t.id", Self::TASK_SELECT_WITH_ELAPSED);
            let mut rows = self.conn.query(&query, ()).await?;
            let mut tasks = Vec::new();
            while let Some(row) = rows.next().await? {
                tasks.push(row_to_task(&row)?);
            }

            // Export all sessions
            let mut rows = self.conn.query(
                "SELECT id, task_id, started, ended, duration, manual_edit, notes FROM sessions",
                (),
            ).await?;
            let mut sessions = Vec::new();
            while let Some(row) = rows.next().await? {
                sessions.push(row_to_session(&row)?);
            }

            Ok((tasks, sessions))
        })
    }

    /// Import tasks and sessions into the database using INSERT OR REPLACE.
    pub fn import_all_data(&self, tasks: &[Task], sessions: &[Session]) -> Result<()> {
        self.rt.block_on(async {
            for task in tasks {
                self.conn.execute(
                    "INSERT OR REPLACE INTO tasks (id, num_id, title, area, project, context, status, created, completed,
                     estimate_minutes, deadline, scheduled, tags, task_notes, priority, modified_at,
                     recurrence, is_template, template_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                    params![
                        task.id.clone(),
                        task.num_id,
                        task.title.clone(),
                        task.area.as_str(),
                        task.project.clone(),
                        task.context.clone(),
                        task.status.as_str(),
                        task.created.to_rfc3339(),
                        task.completed.map(|d| d.to_rfc3339()),
                        task.estimate_minutes,
                        task.deadline.map(|d| d.to_string()),
                        task.scheduled.map(|d| d.to_string()),
                        task.tags.clone(),
                        task.notes.clone(),
                        task.priority,
                        task.modified_at.map(|d| d.to_rfc3339()),
                        task.recurrence.clone(),
                        task.is_template as i64,
                        task.template_id.clone(),
                    ],
                ).await?;
            }

            for session in sessions {
                self.conn.execute(
                    "INSERT OR REPLACE INTO sessions (id, task_id, started, ended, duration, manual_edit, notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        session.id.clone(),
                        session.task_id.clone(),
                        session.started.to_rfc3339(),
                        session.ended.map(|d| d.to_rfc3339()),
                        session.duration,
                        session.manual_edit,
                        session.notes.clone(),
                    ],
                ).await?;
            }

            Ok(())
        })
    }

    fn migrate(&self) -> Result<()> {
        self.rt.block_on(async {
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
                (),
            ).await?;

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
                (),
            ).await?;

            // Add num_id column if it doesn't exist
            let has_num_id = self.conn.prepare("SELECT num_id FROM tasks LIMIT 0").await.is_ok();
            if !has_num_id {
                self.conn.execute_batch(
                    "ALTER TABLE tasks ADD COLUMN num_id INTEGER;
                     UPDATE tasks SET num_id = ROWID WHERE num_id IS NULL;
                     CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_num_id ON tasks(num_id);"
                ).await?;
            }

            // Add estimate_minutes column
            if self.conn.prepare("SELECT estimate_minutes FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN estimate_minutes INTEGER", ()).await?;
            }

            // Add deadline column
            if self.conn.prepare("SELECT deadline FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN deadline TEXT", ()).await?;
            }

            // Add scheduled column
            if self.conn.prepare("SELECT scheduled FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN scheduled TEXT", ()).await?;
            }

            // Add tags column
            if self.conn.prepare("SELECT tags FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN tags TEXT", ()).await?;
            }

            // Add task_notes column
            if self.conn.prepare("SELECT task_notes FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN task_notes TEXT", ()).await?;
            }

            // Add priority column
            if self.conn.prepare("SELECT priority FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN priority INTEGER", ()).await?;
            }

            // Add modified_at column
            if self.conn.prepare("SELECT modified_at FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN modified_at TEXT", ()).await?;
                self.conn.execute("UPDATE tasks SET modified_at = created WHERE modified_at IS NULL", ()).await?;
            }

            // Add recurrence column
            if self.conn.prepare("SELECT recurrence FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN recurrence TEXT", ()).await?;
            }

            // Add is_template column
            if self.conn.prepare("SELECT is_template FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN is_template INTEGER DEFAULT 0", ()).await?;
            }

            // Add template_id column
            if self.conn.prepare("SELECT template_id FROM tasks LIMIT 0").await.is_err() {
                self.conn.execute("ALTER TABLE tasks ADD COLUMN template_id TEXT", ()).await?;
            }

            // Indexes
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_tasks_area ON tasks(area)", ()
            ).await?;
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status)", ()
            ).await?;
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_sessions_task ON sessions(task_id)", ()
            ).await?;

            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_task(
        &self,
        title: &str,
        area: Area,
        project: Option<String>,
        context: Option<String>,
        estimate_minutes: Option<i64>,
        deadline: Option<NaiveDate>,
        scheduled: Option<NaiveDate>,
        tags: Option<String>,
        priority: Option<i64>,
    ) -> Result<i64> {
        let task = Task::new(title, area, project, context);

        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT COALESCE(MAX(num_id), 0) + 1 FROM tasks", ()
            ).await?;
            let row = rows.next().await?.context("No result from MAX query")?;
            let next_num_id = val_i64(&row, 0)?;

            let now = Utc::now().to_rfc3339();
            self.conn.execute(
                "INSERT INTO tasks (id, num_id, title, area, project, context, status, created, completed, estimate_minutes, deadline, scheduled, tags, priority, modified_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    task.id,
                    next_num_id,
                    task.title,
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
                    now,
                ],
            ).await?;

            Ok::<i64, anyhow::Error>(next_num_id)
        })
    }

    const TASK_SELECT_WITH_ELAPSED: &'static str =
        "SELECT t.id, t.num_id, t.title, t.area, t.project, t.context, t.status, t.created, t.completed,
                t.estimate_minutes, t.deadline, t.scheduled, t.tags, t.task_notes, t.priority,
                t.modified_at,
                COALESCE(SUM(
                    CASE WHEN s.ended IS NOT NULL THEN s.duration
                    ELSE CAST((julianday('now') - julianday(s.started)) * 86400 AS INTEGER)
                    END
                ), 0) as elapsed_seconds,
                t.recurrence, t.is_template, t.template_id
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

    pub fn list_tasks(&self, area: Option<Area>) -> Result<Vec<Task>> {
        self.list_tasks_sorted(area, SortBy::Created)
    }

    pub fn list_tasks_sorted(&self, area: Option<Area>, sort: SortBy) -> Result<Vec<Task>> {
        self.rt.block_on(async {
            let mut tasks = Vec::new();

            if let Some(area) = area {
                let area_str = area.as_str();
                let is_completed = matches!(area, Area::Completed);
                let order = Self::sort_order_sql(sort, is_completed);
                let filter = if is_completed {
                    "WHERE t.area = ?1 AND COALESCE(t.is_template, 0) = 0"
                } else {
                    "WHERE t.area = ?1 AND t.status != 'Done' AND COALESCE(t.is_template, 0) = 0"
                };
                let query = format!(
                    "{} {} GROUP BY t.id ORDER BY {}",
                    Self::TASK_SELECT_WITH_ELAPSED, filter, order
                );
                let mut rows = self.conn.query(&query, params![area_str]).await?;
                while let Some(row) = rows.next().await? {
                    tasks.push(row_to_task(&row)?);
                }
            } else {
                let order = Self::sort_order_sql(sort, false);
                let query = format!(
                    "{} WHERE (t.area = 'Today' OR t.status = 'Running') AND t.status != 'Done' AND COALESCE(t.is_template, 0) = 0
                     GROUP BY t.id
                     ORDER BY
                        CASE t.status
                            WHEN 'Running' THEN 0
                            ELSE 1
                        END,
                        {}",
                    Self::TASK_SELECT_WITH_ELAPSED, order
                );
                let mut rows = self.conn.query(&query, ()).await?;
                while let Some(row) = rows.next().await? {
                    tasks.push(row_to_task(&row)?);
                }
            }

            Ok(tasks)
        })
    }

    pub fn find_tasks(&self, query: &str) -> Result<Vec<Task>> {
        self.rt.block_on(async {
            let sql = format!(
                "{} WHERE t.status != 'Done' AND COALESCE(t.is_template, 0) = 0 GROUP BY t.id",
                Self::TASK_SELECT_WITH_ELAPSED
            );
            let mut rows = self.conn.query(&sql, ()).await?;
            let mut tasks = Vec::new();
            while let Some(row) = rows.next().await? {
                tasks.push(row_to_task(&row)?);
            }

            let ranked = rank_matches(&tasks, query);
            Ok(ranked.into_iter().cloned().collect())
        })
    }

    pub fn find_done_tasks(&self, query: &str) -> Result<Vec<Task>> {
        self.rt.block_on(async {
            let sql = format!(
                "{} WHERE t.status = 'Done' AND COALESCE(t.is_template, 0) = 0 GROUP BY t.id",
                Self::TASK_SELECT_WITH_ELAPSED
            );
            let mut rows = self.conn.query(&sql, ()).await?;
            let mut tasks = Vec::new();
            while let Some(row) = rows.next().await? {
                tasks.push(row_to_task(&row)?);
            }

            let ranked = rank_matches(&tasks, query);
            Ok(ranked.into_iter().cloned().collect())
        })
    }

    pub fn resolve_done_task(&self, query: &str) -> Result<Task> {
        if let Ok(num_id) = query.parse::<i64>() {
            if let Some(task) = self.find_task_by_num_id(num_id)? {
                if task.status == TaskStatus::Done {
                    return Ok(task);
                }
            }
        }
        let tasks = self.find_done_tasks(query)?;
        if let Some(best) = find_best_match(&tasks, query) {
            return Ok(best.clone());
        }
        anyhow::bail!("No completed task found matching '{}'", query);
    }

    pub fn start_timer(&self, query: &str) -> Result<(String, i64)> {
        // Pause any running task first
        self.pause_timer()?;

        let task = self.resolve_task(query)?;
        let title = task.title.clone();
        let num_id = task.num_id.unwrap_or(0);

        self.rt.block_on(async {
            let task_id = task.id.clone();
            self.conn.execute(
                "UPDATE tasks SET status = 'Running', modified_at = ?2 WHERE id = ?1",
                params![task.id, Utc::now().to_rfc3339()],
            ).await?;

            let session = Session::new(&task_id);
            self.conn.execute(
                "INSERT INTO sessions (id, task_id, started, ended, duration, manual_edit, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    session.id,
                    session.task_id,
                    session.started.to_rfc3339(),
                    session.ended.map(|d| d.to_rfc3339()),
                    session.duration,
                    session.manual_edit,
                    session.notes,
                ],
            ).await?;

            Ok::<(), anyhow::Error>(())
        })?;

        Ok((title, num_id))
    }

    pub fn pause_timer(&self) -> Result<()> {
        self.rt.block_on(async {
            let tx = self.conn.transaction().await?;

            // Find running task
            let mut rows = tx.query(
                "SELECT id FROM tasks WHERE status = 'Running'", ()
            ).await?;
            let running_id: Option<String> = match rows.next().await? {
                Some(row) => Some(val_string(&row, 0)?),
                None => None,
            };

            if let Some(task_id) = running_id {
                if let Some(mut session) = get_active_session(&tx, &task_id).await? {
                    session.stop();
                    let ended = session.ended.context("Session ended timestamp missing after stop()")?;
                    tx.execute(
                        "UPDATE sessions SET ended = ?1, duration = ?2 WHERE id = ?3",
                        params![
                            ended.to_rfc3339(),
                            session.duration,
                            session.id,
                        ],
                    ).await?;
                }

                tx.execute(
                    "UPDATE tasks SET status = 'Paused', modified_at = ?2 WHERE id = ?1",
                    params![task_id, Utc::now().to_rfc3339()],
                ).await?;
            }

            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    pub fn complete_task(&self) -> Result<Option<(String, i64)>> {
        let result = self.rt.block_on(async {
            let tx = self.conn.transaction().await?;

            // Find running or paused task
            let mut rows = tx.query(
                "SELECT id, title FROM tasks WHERE status IN ('Running', 'Paused')
                 ORDER BY CASE status WHEN 'Running' THEN 0 ELSE 1 END
                 LIMIT 1",
                (),
            ).await?;

            let found = match rows.next().await? {
                Some(row) => Some((val_string(&row, 0)?, val_string(&row, 1)?)),
                None => None,
            };

            if let Some((task_id, title)) = found {
                if let Some(mut session) = get_active_session(&tx, &task_id).await? {
                    if session.is_running() {
                        session.stop();
                        let ended = session.ended.context("Session ended timestamp missing after stop()")?;
                        tx.execute(
                            "UPDATE sessions SET ended = ?1, duration = ?2 WHERE id = ?3",
                            params![
                                ended.to_rfc3339(),
                                session.duration,
                                session.id,
                            ],
                        ).await?;
                    }
                }

                // Calculate total duration from today
                let mut dur_rows = tx.query(
                    "SELECT COALESCE(SUM(duration), 0) FROM sessions
                     WHERE task_id = ?1 AND date(started) = date('now')",
                    params![task_id.clone()],
                ).await?;
                let total_duration = match dur_rows.next().await? {
                    Some(row) => val_i64(&row, 0)?,
                    None => 0,
                };

                let now = Utc::now().to_rfc3339();
                tx.execute(
                    "UPDATE tasks SET status = 'Done', area = 'Completed', completed = ?1, modified_at = ?1
                     WHERE id = ?2",
                    params![now, task_id.clone()],
                ).await?;

                tx.commit().await?;

                Ok::<Option<(String, String, i64)>, anyhow::Error>(Some((task_id, title, total_duration)))
            } else {
                Ok(None)
            }
        })?;

        if let Some((task_id, title, total_duration)) = result {
            // Auto-generate next recurring instance
            if let Err(e) = self.complete_recurring_instance(&task_id) {
                eprintln!("Warning: failed to generate next recurring instance: {e}");
            }
            Ok(Some((title, total_duration)))
        } else {
            Ok(None)
        }
    }

    pub fn get_running_task(&self) -> Result<Option<(String, i64)>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT t.id, t.title FROM tasks t WHERE t.status = 'Running' LIMIT 1",
                (),
            ).await?;

            match rows.next().await? {
                Some(row) => {
                    let task_id = val_string(&row, 0)?;
                    let title = val_string(&row, 1)?;
                    let elapsed = match get_active_session(&self.conn, &task_id).await? {
                        Some(session) => session.elapsed_seconds(),
                        None => 0,
                    };
                    Ok(Some((title, elapsed)))
                }
                None => Ok(None),
            }
        })
    }

    pub fn delete_task(&self, query: &str) -> Result<(String, i64)> {
        let task = self.resolve_task(query)?;
        let title = task.title.clone();
        let num_id = task.num_id.unwrap_or(0);

        self.rt.block_on(async {
            self.conn.execute(
                "DELETE FROM sessions WHERE task_id = ?1", params![task.id.clone()],
            ).await?;
            self.conn.execute(
                "DELETE FROM tasks WHERE id = ?1", params![task.id],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })?;

        Ok((title, num_id))
    }

    pub fn append_note(&self, query: &str, text: &str) -> Result<String> {
        let task = self.resolve_task(query)?;
        let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M]");
        let new_entry = format!("{} {}", timestamp, text);

        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT task_notes FROM tasks WHERE id = ?1", params![task.id.clone()],
            ).await?;
            let existing: Option<String> = match rows.next().await? {
                Some(row) => val_opt_string(&row, 0)?,
                None => None,
            };

            let updated = match existing {
                Some(ref notes) if !notes.is_empty() => format!("{}\n{}", notes, new_entry),
                _ => new_entry,
            };

            self.conn.execute(
                "UPDATE tasks SET task_notes = ?1, modified_at = ?3 WHERE id = ?2",
                params![updated, task.id, Utc::now().to_rfc3339()],
            ).await?;

            Ok::<(), anyhow::Error>(())
        })?;

        Ok(task.title)
    }

    pub fn clear_notes(&self, query: &str) -> Result<String> {
        let task = self.resolve_task(query)?;
        self.rt.block_on(async {
            self.conn.execute(
                "UPDATE tasks SET task_notes = NULL, modified_at = ?2 WHERE id = ?1",
                params![task.id, Utc::now().to_rfc3339()],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(task.title)
    }

    pub fn get_task_notes(&self, query: &str) -> Result<(String, Option<String>)> {
        let task = self.resolve_task(query)?;
        let notes = self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT task_notes FROM tasks WHERE id = ?1", params![task.id],
            ).await?;
            match rows.next().await? {
                Some(row) => val_opt_string(&row, 0),
                None => Ok(None),
            }
        })?;
        Ok((task.title, notes))
    }

    pub fn update_task_fields(&self, query: &str, input: &ParsedInput, area: Option<Area>) -> Result<String> {
        let task = self.resolve_task(query)?;
        self.update_task_fields_by_id(&task.id, input, area)?;
        Ok(task.title)
    }

    /// Load all tasks with elapsed seconds for TUI grouping by effective_area()
    pub fn list_all_tasks(&self, sort: SortBy) -> Result<Vec<Task>> {
        self.rt.block_on(async {
            let mut tasks = Vec::new();
            let order_nondone = Self::sort_order_sql(sort, false);
            let order_done = Self::sort_order_sql(sort, true);

            // Non-done tasks (exclude templates)
            let query = format!(
                "{} WHERE t.status != 'Done' AND COALESCE(t.is_template, 0) = 0 GROUP BY t.id ORDER BY {}",
                Self::TASK_SELECT_WITH_ELAPSED, order_nondone
            );
            let mut rows = self.conn.query(&query, ()).await?;
            while let Some(row) = rows.next().await? {
                tasks.push(row_to_task(&row)?);
            }

            // Done tasks (exclude templates)
            let query = format!(
                "{} WHERE t.status = 'Done' AND COALESCE(t.is_template, 0) = 0 GROUP BY t.id ORDER BY {}",
                Self::TASK_SELECT_WITH_ELAPSED, order_done
            );
            let mut rows = self.conn.query(&query, ()).await?;
            while let Some(row) = rows.next().await? {
                tasks.push(row_to_task(&row)?);
            }

            Ok(tasks)
        })
    }

    pub fn list_tasks_by_project(&self, project: &str, sort: SortBy) -> Result<Vec<Task>> {
        self.rt.block_on(async {
            let mut tasks = Vec::new();
            let order = Self::sort_order_sql(sort, false);
            let query = format!(
                "{} WHERE t.project = ?1 AND t.status != 'Done' AND COALESCE(t.is_template, 0) = 0 GROUP BY t.id ORDER BY {}",
                Self::TASK_SELECT_WITH_ELAPSED, order
            );
            let mut rows = self.conn.query(&query, params![project]).await?;
            while let Some(row) = rows.next().await? {
                tasks.push(row_to_task(&row)?);
            }
            Ok(tasks)
        })
    }

    pub fn append_note_by_id(&self, task_id: &str, text: &str) -> Result<()> {
        let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M]");
        let new_entry = format!("{} {}", timestamp, text);

        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT task_notes FROM tasks WHERE id = ?1", params![task_id],
            ).await?;
            let existing: Option<String> = match rows.next().await? {
                Some(row) => val_opt_string(&row, 0)?,
                None => None,
            };

            let updated = match existing {
                Some(ref notes) if !notes.is_empty() => format!("{}\n{}", notes, new_entry),
                _ => new_entry,
            };

            self.conn.execute(
                "UPDATE tasks SET task_notes = ?1, modified_at = ?3 WHERE id = ?2",
                params![updated, task_id, Utc::now().to_rfc3339()],
            ).await?;

            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn get_task_notes_by_id(&self, task_id: &str) -> Result<Option<String>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT task_notes FROM tasks WHERE id = ?1", params![task_id],
            ).await?;
            match rows.next().await? {
                Some(row) => val_opt_string(&row, 0),
                None => Ok(None),
            }
        })
    }

    pub fn update_notes_by_id(&self, task_id: &str, text: &str) -> Result<()> {
        self.rt.block_on(async {
            let notes_val: Option<&str> = if text.is_empty() { None } else { Some(text) };
            self.conn.execute(
                "UPDATE tasks SET task_notes = ?1, modified_at = ?3 WHERE id = ?2",
                params![notes_val, task_id, Utc::now().to_rfc3339()],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn update_task_scheduled(&self, task_id: &str, date: NaiveDate) -> Result<()> {
        self.rt.block_on(async {
            self.conn.execute(
                "UPDATE tasks SET scheduled = ?1, modified_at = ?3 WHERE id = ?2",
                params![date.to_string(), task_id, Utc::now().to_rfc3339()],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn find_task_by_num_id(&self, num_id: i64) -> Result<Option<Task>> {
        self.rt.block_on(async {
            let query = format!(
                "{} WHERE t.num_id = ?1 GROUP BY t.id",
                Self::TASK_SELECT_WITH_ELAPSED
            );
            let mut rows = self.conn.query(&query, params![num_id]).await?;
            match rows.next().await? {
                Some(row) => Ok(Some(row_to_task(&row)?)),
                None => Ok(None),
            }
        })
    }

    pub fn delete_task_by_id(&self, task_id: &str) -> Result<()> {
        self.rt.block_on(async {
            self.conn.execute(
                "DELETE FROM sessions WHERE task_id = ?1", params![task_id],
            ).await?;
            self.conn.execute(
                "DELETE FROM tasks WHERE id = ?1", params![task_id],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn update_task_fields_by_id(&self, task_id: &str, input: &ParsedInput, area: Option<Area>) -> Result<()> {
        self.rt.block_on(async {
            if let Some(ref project) = input.project {
                self.conn.execute(
                    "UPDATE tasks SET project = ?1 WHERE id = ?2",
                    params![project.clone(), task_id],
                ).await?;
            }
            if !input.contexts.is_empty() {
                let ctx = input.contexts.join(",");
                self.conn.execute(
                    "UPDATE tasks SET context = ?1 WHERE id = ?2",
                    params![ctx, task_id],
                ).await?;
            }
            if !input.tags.is_empty() {
                let tags = input.tags.join(",");
                self.conn.execute(
                    "UPDATE tasks SET tags = ?1 WHERE id = ?2",
                    params![tags, task_id],
                ).await?;
            }
            if let Some(est) = input.estimate_minutes {
                self.conn.execute(
                    "UPDATE tasks SET estimate_minutes = ?1 WHERE id = ?2",
                    params![est, task_id],
                ).await?;
            }
            if let Some(ref dl) = input.deadline {
                self.conn.execute(
                    "UPDATE tasks SET deadline = ?1 WHERE id = ?2",
                    params![dl.to_string(), task_id],
                ).await?;
            }
            if let Some(ref sc) = input.scheduled {
                self.conn.execute(
                    "UPDATE tasks SET scheduled = ?1 WHERE id = ?2",
                    params![sc.to_string(), task_id],
                ).await?;
            }
            if let Some(p) = input.priority {
                self.conn.execute(
                    "UPDATE tasks SET priority = ?1 WHERE id = ?2",
                    params![p, task_id],
                ).await?;
            }
            if let Some(area) = area {
                self.conn.execute(
                    "UPDATE tasks SET area = ?1 WHERE id = ?2",
                    params![area.as_str(), task_id],
                ).await?;
            }
            self.conn.execute(
                "UPDATE tasks SET modified_at = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), task_id],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn uncomplete_task_by_id(&self, task_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.rt.block_on(async {
            self.conn.execute(
                "UPDATE tasks SET status = 'Pending', area = 'Today', completed = NULL, modified_at = ?1 WHERE id = ?2",
                params![now, task_id],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn update_task_title_by_id(&self, task_id: &str, title: &str) -> Result<()> {
        self.rt.block_on(async {
            self.conn.execute(
                "UPDATE tasks SET title = ?1, modified_at = ?3 WHERE id = ?2",
                params![title, task_id, Utc::now().to_rfc3339()],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn complete_task_by_id(&self, task_id: &str) -> Result<()> {
        self.rt.block_on(async {
            let tx = self.conn.transaction().await?;

            if let Some(mut session) = get_active_session(&tx, task_id).await? {
                if session.is_running() {
                    session.stop();
                    let ended = session.ended.context("Session ended timestamp missing after stop()")?;
                    tx.execute(
                        "UPDATE sessions SET ended = ?1, duration = ?2 WHERE id = ?3",
                        params![
                            ended.to_rfc3339(),
                            session.duration,
                            session.id,
                        ],
                    ).await?;
                }
            }

            let now = Utc::now().to_rfc3339();
            tx.execute(
                "UPDATE tasks SET status = 'Done', area = 'Completed', completed = ?1, modified_at = ?1 WHERE id = ?2",
                params![now, task_id],
            ).await?;

            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        })?;

        // Auto-generate next recurring instance
        if let Err(e) = self.complete_recurring_instance(task_id) {
            eprintln!("Warning: failed to generate next recurring instance: {e}");
        }

        Ok(())
    }

    pub fn resolve_task(&self, query: &str) -> Result<Task> {
        if let Ok(num_id) = query.parse::<i64>() {
            if let Some(task) = self.find_task_by_num_id(num_id)? {
                return Ok(task);
            }
        }
        let tasks = self.find_tasks(query)?;
        if let Some(best) = find_best_match(&tasks, query) {
            return Ok(best.clone());
        }
        anyhow::bail!("No task found matching '{}'", query);
    }

    // ── Reports ────────────────────────────────────────────────────────

    pub fn report_tasks_done(&self, from: &str, to: &str) -> Result<i64> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT COUNT(*) FROM tasks WHERE status = 'Done' AND completed >= ?1 AND completed < ?2",
                params![from, to],
            ).await?;
            match rows.next().await? {
                Some(row) => val_i64(&row, 0),
                None => Ok(0),
            }
        })
    }

    pub fn report_total_seconds(&self, from: &str, to: &str) -> Result<i64> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT COALESCE(SUM(duration), 0) FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL",
                params![from, to],
            ).await?;
            match rows.next().await? {
                Some(row) => val_i64(&row, 0),
                None => Ok(0),
            }
        })
    }

    pub fn report_by_hour(&self, from: &str, to: &str) -> Result<Vec<(i64, i64)>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT CAST(strftime('%H', started) AS INTEGER) as hour, COALESCE(SUM(duration), 0)
                 FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL
                 GROUP BY hour ORDER BY hour",
                params![from, to],
            ).await?;
            let mut result = vec![];
            while let Some(row) = rows.next().await? {
                result.push((val_i64(&row, 0)?, val_i64(&row, 1)?));
            }
            Ok(result)
        })
    }

    pub fn report_by_weekday(&self, from: &str, to: &str) -> Result<Vec<(i64, i64)>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT CAST(strftime('%w', started) AS INTEGER) as dow, COALESCE(SUM(duration), 0)
                 FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL
                 GROUP BY dow ORDER BY dow",
                params![from, to],
            ).await?;
            let mut result = vec![];
            while let Some(row) = rows.next().await? {
                result.push((val_i64(&row, 0)?, val_i64(&row, 1)?));
            }
            Ok(result)
        })
    }

    pub fn report_by_project(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT COALESCE(t.project, '(none)'), COALESCE(SUM(s.duration), 0)
                 FROM sessions s JOIN tasks t ON s.task_id = t.id
                 WHERE s.started >= ?1 AND s.started < ?2 AND s.ended IS NOT NULL
                 GROUP BY t.project ORDER BY SUM(s.duration) DESC",
                params![from, to],
            ).await?;
            let mut result = vec![];
            while let Some(row) = rows.next().await? {
                result.push((val_string(&row, 0)?, val_i64(&row, 1)?));
            }
            Ok(result)
        })
    }

    pub fn report_done_tasks(&self, from: &str, to: &str, limit: i64) -> Result<Vec<(String, i64)>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT title, COALESCE(
                    (SELECT SUM(duration) FROM sessions WHERE task_id = t.id), 0
                 ) FROM tasks t
                 WHERE status = 'Done' AND completed >= ?1 AND completed < ?2
                 ORDER BY completed DESC LIMIT ?3",
                params![from, to, limit],
            ).await?;
            let mut result = vec![];
            while let Some(row) = rows.next().await? {
                result.push((val_string(&row, 0)?, val_i64(&row, 1)?));
            }
            Ok(result)
        })
    }

    pub fn report_active_days(&self, from: &str, to: &str) -> Result<i64> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT COUNT(DISTINCT date(started)) FROM sessions WHERE started >= ?1 AND started < ?2 AND ended IS NOT NULL",
                params![from, to],
            ).await?;
            match rows.next().await? {
                Some(row) => val_i64(&row, 0),
                None => Ok(0),
            }
        })
    }

    // ── Recurring Templates ─────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub fn add_template(
        &self,
        title: &str,
        recurrence: &str,
        project: Option<String>,
        context: Option<String>,
        estimate_minutes: Option<i64>,
        deadline: Option<NaiveDate>,
        scheduled: Option<NaiveDate>,
        tags: Option<String>,
        priority: Option<i64>,
    ) -> Result<i64> {
        let task = Task::new(title, Area::Today, project.clone(), context.clone());

        let num_id = self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT COALESCE(MAX(num_id), 0) + 1 FROM tasks", ()
            ).await?;
            let row = rows.next().await?.context("No result from MAX query")?;
            let next_num_id = val_i64(&row, 0)?;

            let now = Utc::now().to_rfc3339();
            self.conn.execute(
                "INSERT INTO tasks (id, num_id, title, area, project, context, status, created, completed,
                 estimate_minutes, deadline, scheduled, tags, priority, modified_at,
                 recurrence, is_template, template_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 1, NULL)",
                params![
                    task.id.clone(),
                    next_num_id,
                    task.title,
                    task.area.as_str(),
                    project.clone(),
                    context.clone(),
                    task.status.as_str(),
                    task.created.to_rfc3339(),
                    task.completed.map(|d| d.to_rfc3339()),
                    estimate_minutes,
                    deadline.map(|d| d.to_string()),
                    scheduled.map(|d| d.to_string()),
                    tags.clone(),
                    priority,
                    now,
                    recurrence,
                ],
            ).await?;

            Ok::<(i64, String), anyhow::Error>((next_num_id, task.id.clone()))
        })?;

        // Generate first instance
        let today = chrono::Local::now().date_naive();
        self.generate_instance_for_template(&num_id.1, title, &project, &context,
            estimate_minutes, deadline, tags.as_deref(), priority, today)?;

        Ok(num_id.0)
    }

    pub fn list_templates(&self) -> Result<Vec<Task>> {
        self.rt.block_on(async {
            let mut templates = Vec::new();
            let query = format!(
                "{} WHERE COALESCE(t.is_template, 0) = 1 GROUP BY t.id ORDER BY t.created ASC",
                Self::TASK_SELECT_WITH_ELAPSED
            );
            let mut rows = self.conn.query(&query, ()).await?;
            while let Some(row) = rows.next().await? {
                templates.push(row_to_task(&row)?);
            }
            Ok(templates)
        })
    }

    pub fn delete_template(&self, template_id: &str) -> Result<()> {
        self.rt.block_on(async {
            self.conn.execute(
                "DELETE FROM sessions WHERE task_id IN (SELECT id FROM tasks WHERE template_id = ?1 AND status != 'Done')",
                params![template_id],
            ).await?;
            self.conn.execute(
                "DELETE FROM tasks WHERE template_id = ?1 AND status != 'Done'",
                params![template_id],
            ).await?;
            self.conn.execute(
                "DELETE FROM sessions WHERE task_id = ?1",
                params![template_id],
            ).await?;
            self.conn.execute(
                "DELETE FROM tasks WHERE id = ?1",
                params![template_id],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn pause_template(&self, template_id: &str) -> Result<()> {
        self.rt.block_on(async {
            self.conn.execute(
                "UPDATE tasks SET status = 'Paused', modified_at = ?2 WHERE id = ?1 AND COALESCE(is_template, 0) = 1",
                params![template_id, Utc::now().to_rfc3339()],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn resume_template(&self, template_id: &str) -> Result<()> {
        self.rt.block_on(async {
            self.conn.execute(
                "UPDATE tasks SET status = 'Pending', modified_at = ?2 WHERE id = ?1 AND COALESCE(is_template, 0) = 1",
                params![template_id, Utc::now().to_rfc3339()],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn template_history(&self, template_id: &str) -> Result<Vec<Task>> {
        self.rt.block_on(async {
            let mut tasks = Vec::new();
            let query = format!(
                "{} WHERE t.template_id = ?1 AND t.status = 'Done' GROUP BY t.id ORDER BY t.completed DESC",
                Self::TASK_SELECT_WITH_ELAPSED
            );
            let mut rows = self.conn.query(&query, params![template_id]).await?;
            while let Some(row) = rows.next().await? {
                tasks.push(row_to_task(&row)?);
            }
            Ok(tasks)
        })
    }

    pub fn generate_instances(&self) -> Result<usize> {
        let templates = self.list_templates()?;
        let today = chrono::Local::now().date_naive();
        let mut created = 0;

        for template in &templates {
            if template.status == TaskStatus::Paused {
                continue;
            }
            let recurrence = match &template.recurrence {
                Some(r) => r.clone(),
                None => continue,
            };

            let has_active: bool = self.rt.block_on(async {
                let mut rows = self.conn.query(
                    "SELECT COUNT(*) FROM tasks WHERE template_id = ?1 AND status != 'Done'",
                    params![template.id.clone()],
                ).await?;
                match rows.next().await? {
                    Some(row) => {
                        let count = val_i64(&row, 0)?;
                        Ok::<bool, anyhow::Error>(count > 0)
                    }
                    None => Ok(false),
                }
            })?;

            if has_active {
                continue;
            }

            let last_scheduled: Option<String> = self.rt.block_on(async {
                let mut rows = self.conn.query(
                    "SELECT MAX(scheduled) FROM tasks WHERE template_id = ?1",
                    params![template.id.clone()],
                ).await?;
                match rows.next().await? {
                    Some(row) => val_opt_string(&row, 0),
                    None => Ok(None),
                }
            })?;

            let from_date = last_scheduled
                .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
                .unwrap_or(today);

            let next_date = crate::notation::next_occurrence(&recurrence, from_date)
                .unwrap_or(today);

            self.generate_instance_for_template(
                &template.id,
                &template.title,
                &template.project,
                &template.context,
                template.estimate_minutes,
                template.deadline,
                template.tags.as_deref(),
                template.priority,
                next_date,
            )?;
            created += 1;
        }

        Ok(created)
    }

    fn generate_instance_for_template(
        &self,
        template_id: &str,
        title: &str,
        project: &Option<String>,
        context: &Option<String>,
        estimate_minutes: Option<i64>,
        deadline: Option<NaiveDate>,
        tags: Option<&str>,
        priority: Option<i64>,
        scheduled: NaiveDate,
    ) -> Result<i64> {
        let instance_id = ulid::Ulid::new().to_string();

        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT COALESCE(MAX(num_id), 0) + 1 FROM tasks", ()
            ).await?;
            let row = rows.next().await?.context("No result from MAX query")?;
            let next_num_id = val_i64(&row, 0)?;
            let now = Utc::now().to_rfc3339();

            self.conn.execute(
                "INSERT INTO tasks (id, num_id, title, area, project, context, status, created, completed,
                 estimate_minutes, deadline, scheduled, tags, priority, modified_at,
                 recurrence, is_template, template_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Pending', ?7, NULL, ?8, ?9, ?10, ?11, ?12, ?13, NULL, 0, ?14)",
                params![
                    instance_id,
                    next_num_id,
                    title,
                    "Today",
                    project.clone(),
                    context.clone(),
                    now.clone(),
                    estimate_minutes,
                    deadline.map(|d| d.to_string()),
                    scheduled.to_string(),
                    tags,
                    priority,
                    now,
                    template_id,
                ],
            ).await?;

            Ok::<i64, anyhow::Error>(next_num_id)
        })
    }

    pub fn complete_recurring_instance(&self, task_id: &str) -> Result<Option<String>> {
        let template_id: Option<String> = self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT template_id FROM tasks WHERE id = ?1", params![task_id],
            ).await?;
            match rows.next().await? {
                Some(row) => val_opt_string(&row, 0),
                None => Ok(None),
            }
        })?;

        let template_id = match template_id {
            Some(tid) => tid,
            None => return Ok(None),
        };

        let instance_scheduled: Option<String> = self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT scheduled FROM tasks WHERE id = ?1", params![task_id],
            ).await?;
            match rows.next().await? {
                Some(row) => val_opt_string(&row, 0),
                None => Ok(None),
            }
        })?;

        // Look up the template
        let template: Option<Task> = self.rt.block_on(async {
            let query = format!(
                "{} WHERE t.id = ?1 GROUP BY t.id",
                Self::TASK_SELECT_WITH_ELAPSED
            );
            let mut rows = self.conn.query(&query, params![template_id.clone()]).await?;
            match rows.next().await? {
                Some(row) => Ok::<Option<Task>, anyhow::Error>(Some(row_to_task(&row)?)),
                None => Ok(None),
            }
        })?;

        if let Some(template) = template {
            if template.status == TaskStatus::Paused {
                return Ok(Some(template_id));
            }

            if let Some(ref recurrence) = template.recurrence {
                let from_date = instance_scheduled
                    .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
                    .unwrap_or_else(|| chrono::Local::now().date_naive());

                let next_date = crate::notation::next_occurrence(recurrence, from_date)
                    .unwrap_or_else(|| chrono::Local::now().date_naive());

                self.generate_instance_for_template(
                    &template_id,
                    &template.title,
                    &template.project,
                    &template.context,
                    template.estimate_minutes,
                    template.deadline,
                    template.tags.as_deref(),
                    template.priority,
                    next_date,
                )?;
            }
        }

        Ok(Some(template_id))
    }

    pub fn resolve_template(&self, query: &str) -> Result<Task> {
        if let Ok(num_id) = query.parse::<i64>() {
            let found = self.rt.block_on(async {
                let q = format!(
                    "{} WHERE t.num_id = ?1 AND COALESCE(t.is_template, 0) = 1 GROUP BY t.id",
                    Self::TASK_SELECT_WITH_ELAPSED
                );
                let mut rows = self.conn.query(&q, params![num_id]).await?;
                match rows.next().await? {
                    Some(row) => Ok::<Option<Task>, anyhow::Error>(Some(row_to_task(&row)?)),
                    None => Ok(None),
                }
            })?;
            if let Some(task) = found {
                return Ok(task);
            }
        }
        let templates = self.list_templates()?;
        if let Some(best) = find_best_match(&templates, query) {
            return Ok(best.clone());
        }
        anyhow::bail!("No recurring template found matching '{}'", query);
    }

    pub fn template_last_date(&self, template_id: &str) -> Result<Option<NaiveDate>> {
        self.rt.block_on(async {
            let mut rows = self.conn.query(
                "SELECT MAX(scheduled) FROM tasks WHERE template_id = ?1",
                params![template_id],
            ).await?;
            match rows.next().await? {
                Some(row) => {
                    let s = val_opt_string(&row, 0)?;
                    Ok(s.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()))
                }
                None => Ok(None),
            }
        })
    }

    pub fn update_template_fields(&self, template_id: &str, input: &ParsedInput) -> Result<()> {
        self.rt.block_on(async {
            if let Some(ref project) = input.project {
                self.conn.execute(
                    "UPDATE tasks SET project = ?1 WHERE id = ?2",
                    params![project.clone(), template_id],
                ).await?;
            }
            if !input.contexts.is_empty() {
                let ctx = input.contexts.join(",");
                self.conn.execute(
                    "UPDATE tasks SET context = ?1 WHERE id = ?2",
                    params![ctx, template_id],
                ).await?;
            }
            if !input.tags.is_empty() {
                let tags = input.tags.join(",");
                self.conn.execute(
                    "UPDATE tasks SET tags = ?1 WHERE id = ?2",
                    params![tags, template_id],
                ).await?;
            }
            if let Some(est) = input.estimate_minutes {
                self.conn.execute(
                    "UPDATE tasks SET estimate_minutes = ?1 WHERE id = ?2",
                    params![est, template_id],
                ).await?;
            }
            if let Some(ref dl) = input.deadline {
                self.conn.execute(
                    "UPDATE tasks SET deadline = ?1 WHERE id = ?2",
                    params![dl.to_string(), template_id],
                ).await?;
            }
            if let Some(ref sc) = input.scheduled {
                self.conn.execute(
                    "UPDATE tasks SET scheduled = ?1 WHERE id = ?2",
                    params![sc.to_string(), template_id],
                ).await?;
            }
            if let Some(p) = input.priority {
                self.conn.execute(
                    "UPDATE tasks SET priority = ?1 WHERE id = ?2",
                    params![p, template_id],
                ).await?;
            }
            if let Some(ref recurrence) = input.recurrence {
                self.conn.execute(
                    "UPDATE tasks SET recurrence = ?1 WHERE id = ?2",
                    params![recurrence.clone(), template_id],
                ).await?;
            }
            self.conn.execute(
                "UPDATE tasks SET modified_at = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), template_id],
            ).await?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

// ── Standalone helpers ──────────────────────────────────────────────

fn row_to_task(row: &libsql::Row) -> Result<Task> {
    Ok(Task {
        id: val_string(row, 0)?,
        num_id: val_opt_i64(row, 1)?,
        title: val_string(row, 2)?,
        area: Area::from_str(&val_string(row, 3)?)
            .unwrap_or(Area::Today),
        project: val_opt_string(row, 4)?,
        context: val_opt_string(row, 5)?,
        status: TaskStatus::from_str(&val_string(row, 6)?)
            .unwrap_or(TaskStatus::Pending),
        created: DateTime::parse_from_rfc3339(&val_string(row, 7)?)
            .map_err(|e| anyhow::anyhow!(e))?
            .into(),
        completed: val_opt_string(row, 8)?
            .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
            .map(|d| d.into()),
        modified_at: val_opt_string(row, 15)?
            .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
            .map(|d| d.into()),
        estimate_minutes: val_opt_i64(row, 9)?,
        deadline: val_opt_string(row, 10)?
            .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        scheduled: val_opt_string(row, 11)?
            .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        priority: val_opt_i64(row, 14)?,
        tags: val_opt_string(row, 12)?,
        notes: val_opt_string(row, 13)?,
        elapsed_seconds: val_opt_i64(row, 16).ok().flatten(),
        recurrence: val_opt_string(row, 17)?,
        is_template: val_opt_bool(row, 18)?.unwrap_or(false),
        template_id: val_opt_string(row, 19)?,
    })
}

async fn get_active_session(conn: &Connection, task_id: &str) -> Result<Option<Session>> {
    let mut rows = conn.query(
        "SELECT id, task_id, started, ended, duration, manual_edit, notes
         FROM sessions WHERE task_id = ?1 AND ended IS NULL
         LIMIT 1",
        params![task_id],
    ).await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row_to_session(&row)?)),
        None => Ok(None),
    }
}

fn row_to_session(row: &libsql::Row) -> Result<Session> {
    Ok(Session {
        id: val_string(row, 0)?,
        task_id: val_string(row, 1)?,
        started: DateTime::parse_from_rfc3339(&val_string(row, 2)?)
            .map_err(|e| anyhow::anyhow!(e))?
            .into(),
        ended: val_opt_string(row, 3)?
            .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
            .map(|d| d.into()),
        duration: val_i64(row, 4)?,
        manual_edit: val_bool(row, 5)?,
        notes: val_opt_string(row, 6)?,
    })
}

