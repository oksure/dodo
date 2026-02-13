# Dodo CLI

A keyboard-first todo + time tracker CLI in Rust.

## Build & Run

```bash
cargo build
cargo run -- <command>
cargo test
```

## Project Structure

- `src/lib.rs` — library crate root, re-exports all modules
- `src/main.rs` — binary entry point with `cmd_*` handler functions per command, output formatting
- `src/cli.rs` — clap command/argument definitions; re-exports `Area` from `task.rs`
- `src/db.rs` — SQLite database (libsql), migrations, all queries, session lifecycle
- `src/task.rs` — `Task` struct, `Area` enum (single source of truth, with `ValueEnum`), `TaskStatus` enum, `Display` impl
- `src/session.rs` — `Session` struct with `elapsed_seconds()`, `stop()`, `is_running()`
- `src/fuzzy.rs` — fuzzy matching with scored ranking (`score()`, `find_best_match()`, `rank_matches()`)
- `src/notation.rs` — inline notation parser (`parse_notation()`, `parse_duration()`, `parse_date()`)
- `src/config.rs` — config file parsing (`~/.config/dodo/config.toml`) for sync and backup settings
- `src/backup.rs` — S3-compatible backup operations (create, list, restore, delete, prune, age check)
- `src/tui/` — ratatui terminal UI (binary-only, not in lib), split into modules:
  - `mod.rs` — `run_tui()` entry point, terminal setup/teardown, initial/final sync
  - `constants.rs` — color palette, sort modes, field labels/hints/types
  - `format.rs` — `format_dur()`, `format_est()`, `sort_tasks()`, `sort_label()`, `parse_filter_days()`
  - `state.rs` — `PaneState`, `AppMode`, `TuiTab`, `ReportRange`, `SyncStatus`, `App` struct + all impl methods
  - `event.rs` — `run_app()` event loop, `handle_tasks_key()`, `handle_recurring_key()`, `handle_backup_key()`, periodic sync
  - `draw.rs` — all `draw_*` rendering functions, styling helpers, animation, sync indicator
- `tests/fuzzy_test.rs` — 8 unit tests for fuzzy scoring logic
- `tests/notation_test.rs` — 61 unit tests for notation/duration/date/priority/recurrence parsing
- `tests/config_test.rs` — 22 unit tests for config parsing, defaults, is_ready checks, serialization roundtrip
- `tests/workflow_test.rs` — 43 integration tests covering real-world workflows
- `USAGE.md` — real-world use cases with GTD, Pomodoro, Eisenhower frameworks

## Key Patterns

- **Lib/bin split**: `src/lib.rs` exposes `backup`, `cli`, `config`, `db`, `fuzzy`, `notation`, `session`, `task` as public modules. `src/main.rs` is the binary that also owns `tui/` (since it depends on ratatui/crossterm). Integration tests import via `dodo::`.
- **main.rs command dispatch**: `main()` is a clean dispatch table calling `cmd_add()`, `cmd_list()`, `cmd_start()`, `cmd_done()`, `cmd_status()`, `cmd_remove()`, `cmd_edit()`, `cmd_note()`, `cmd_recurring()`, `cmd_sync()`, `cmd_backup()`. The `DONE_DISPLAY_LIMIT` constant controls how many done tasks `cmd_list` shows.
- **Area enum**: Single definition in `task.rs` with `#[derive(ValueEnum)]` for clap integration. `cli.rs` re-exports it via `pub use crate::task::Area`. No more duplicate enum or `CliArea` alias.
- **TUI module structure**: `src/tui/` uses `pub(super)` visibility for all internal items. Only `run_tui()` in `mod.rs` is `pub`. Dependency graph (no cycles): `mod.rs → state, event`; `event → state, draw`; `draw → state, constants, format`; `state → constants, format`.
- **Inline notation**: `notation.rs::parse_notation()` extracts 7 token types from input: `+project`, `@context` (multiple), `#tag` (multiple), `~duration`, `^deadline`, `=scheduled`, `!`–`!!!!` priority. Remaining text becomes the title. Single-value tokens use last-wins; multi-value tokens collect all. Tokens must be preceded by whitespace (e.g., `email@test` is not parsed).
- **Duration parsing**: `~30m`, `~1h`, `~1h30m`, `~1d` (480m = 8h workday), `~1w` (2400m = 5×8h). Units are composable: `~2d4h`.
- **Date parsing**: Named (`today`/`tdy`, `tomorrow`/`tmr`, `yesterday`/`ytd`), day names (`mon`–`sun` → next occurrence), relative (`3d`, `2w`, `1m`, `-3d`), MMDD (`0115`), YYYYMMDD (`20250502`), ISO (`2025-05-02`).
- **Numeric task IDs**: Tasks have an auto-incrementing `num_id` (integer) in addition to a ULID string `id`. Commands like `start`, `remove`, `edit`, `note` accept either a numeric ID or a fuzzy text query. Resolution logic is in `db.rs::resolve_task()`.
- **Fuzzy matching**: `fuzzy.rs::score()` ranks matches: exact (100) > prefix (75) > word-start (60) > substring (50) > word-contains (40). `find_best_match()` picks the top result; `rank_matches()` sorts all results by relevance. `find_tasks()` loads all non-done tasks and returns them ranked.
- **Task resolution**: `resolve_task(query)` tries `parse::<i64>()` first for numeric ID lookup, then falls back to fuzzy-ranked search via `find_tasks()` + `find_best_match()`.
- **update_task_fields delegation**: `update_task_fields(query, ...)` resolves the query then delegates to `update_task_fields_by_id(id, ...)`, which is the single implementation for all 9 field updates.
- **Session lifecycle**: `Session` methods (`elapsed_seconds`, `stop`, `is_running`) are used by `pause_timer`, `complete_task`, and `get_running_task` in `db.rs`. Sessions are loaded from DB via `row_to_session()` / `get_active_session()`.
- **Elapsed time**: `list_tasks()` and `find_tasks()` use a LEFT JOIN on sessions to compute total elapsed seconds per task, including live running sessions via `julianday('now')`.
- **Default command**: Running `dodo` with no subcommand launches the TUI. `dodo help` / `dodo h` shows CLI help.
- **Date-based area grouping**: `Task::effective_area()` computes area from the `scheduled` date only (deadline is informational, not used for pane placement): ≤today=Today, ≤7days=ThisWeek, >7days=LongTerm, no scheduled date=Today, Done=Completed. `area_str()` delegates to `effective_area()`.
- **Default task values**: New tasks default to 1h estimate (`or(Some(60))`) and `scheduled = today` if no dates specified.
- **Start/stop toggle**: `dodo s` with no args pauses the running task. No separate `pause` command. In TUI, `s` toggles: if task is Running, pauses it; otherwise starts it.
- **CLI grouped list**: `dodo ls` with no area shows all four groups (TODAY, THIS WEEK, LONG TERM, DONE) with section headers and counts. DONE is limited to 5 tasks. Specifying an area (`dodo ls today`) shows just that area. `--project` flag filters by project.
- **Display format**: Tasks render as `[num_id] [status_icon] AREA title [*] !priority +project @context #tag ~estimate ^deadline =scheduled (elapsed/estimate) [running]`. The `*` appears after the title if the task has notes.
- **Sorting**: `SortBy` enum in `cli.rs` (`Created`, `Modified`, `Area`, `Title`). Per-pane `sort_ascending` flag. TUI `o` key cycles: `created↑ → created↓ → modified↑ → modified↓ → title↑ → title↓`. DONE pane defaults to `modified↓` (newest done first). Pane header shows sort label with ↑/↓ arrow right-aligned.
- **Modified tracking**: `modified_at` column on tasks, updated by all mutation methods (`start_timer`, `pause_timer`, `complete_task`, `update_task_fields`, `append_note`, `clear_notes`). Used for `--sort modified` ordering.
- **TUI header legend**: Right-aligned symbol legend on the DODO header line showing status icons (`○ ▶ ⏸ ✓`) and notation symbols (`+proj @ctx ~est ^dead =sched !pri`) with matching colors.
- **TUI search bar**: Bordered box between tab bar and panes, activated with `/`. Live-filters tasks across all panes as you type. Supports `+project` (project filter), `@context` (context filter), and plain text (title substring match), all AND-ed. `Enter`/`Esc` exits search mode (filter stays). `AppMode::Search` handles input.
- **TUI done/undone follows cursor**: Pressing `d` to mark done/undone moves the cursor to the task's new pane (e.g., TODAY→DONE or DONE→TODAY).
- **TUI pane layout**: Tasks tab has four vertical panes (LONG TERM, THIS WEEK, TODAY, DONE) with `h`/`l` pane navigation, `j`/`k` task navigation. Pane headers show elapsed/estimate/percentage/done stats. Report tab has DAY/WEEK/MONTH/YEAR/ALL range selector (default: Month). Switch tabs with `t`/`c`/`r`/`b`/`Tab`.
- **TUI colors**: Running tasks animate with pastel rainbow sweep (continuous left→right). Priority colored by level (Red for !!!!). Projects in Magenta. Elapsed colored by estimate progress (Green→Yellow→Red). Deadlines Red if overdue, Yellow if upcoming.
- **TUI note modal**: `n` key opens NoteView if task has notes (j/k navigate, `e` edit, `d` delete, `a` append), or goes straight to append input if no notes. `Alt+Enter` inserts newlines within a note entry. `AppMode` enum (in `state.rs`): Normal, AddTask, MoveTask, ConfirmDelete, EditTask, EditTaskField, NoteView, Search, RecAddTemplate, RecConfirmDelete, EditConfig, EditConfigField. NoteView supports inline editing with `note_editing` flag.
- **TUI responsiveness**: Event loop (in `event.rs`) polls at 16ms (~60fps) for instant key response, data refresh on 1-second timer. Tick counter drives running task animation.
- **TUI fire-and-forget pattern**: `let _ =` is used intentionally in TUI event handlers for best-effort DB operations. No error display mechanism exists in the event loop; failures are non-fatal.
- **Report queries**: `db.rs` has 7 report methods: `report_tasks_done`, `report_total_seconds`, `report_by_hour`, `report_by_weekday`, `report_by_project`, `report_done_tasks`, `report_active_days`. All take date range strings.
- **DB migrations**: Schema changes use check-then-alter pattern in `db.rs::migrate()`. New columns are added with `ALTER TABLE` guarded by a `SELECT` probe.
- **Complete prefers running**: `complete_task()` uses `ORDER BY` to prefer Running tasks over Paused ones when multiple are active.
- **Quote-free input**: Add, Start, Remove, Edit, Note commands use `Vec<String>` with `trailing_var_arg` so users can type `dodo a fix login bug +backend ~2h !!!` without quotes.
- **Notation precedence**: Inline notation tokens override CLI flags (e.g., `+backend` in text overrides `--project`).
- **Recurring tasks**: Template + instance model. Templates (`is_template=1`) live in the Recurring tab and generate instances that appear in normal panes. Recurrence patterns: `*daily`, `*3d`, `*weekly`, `*2w`, `*monthly`, `*3m`, `*mon,wed,fri`, `*day15`. One active instance per template. Completing an instance auto-generates the next. Deleting an instance = skip; `g` (generate) recreates. Paused templates stop generating. CLI: `dodo rec` (list/add/edit/delete/pause/resume/generate/history).
- **TUI four-tab layout**: Tab 1 (Tasks, `t`): four vertical panes. Tab 2 (Recurring, `c`): template list with pause/generate/edit. Tab 3 (Report, `r`): productivity stats. Tab 4 (Backup, `b`): S3 backup list with upload/restore/delete/sync. `Tab` cycles through all four. `b` key works from any tab.
- **Recurrence notation**: `parse_recurrence()` validates patterns. `next_occurrence()` computes the next date from a pattern + reference date. Day-of-month clamps to last day (e.g., day31 in Feb → Feb 28).
- **Instance indicator**: Recurring instances show `↻` after the title in task panes.
- **Database engine**: Uses `libsql` (SQLite-compatible fork supporting Turso embedded replicas). `Database` struct stores `libsql::Database` handle + `Connection` + `tokio::runtime::Runtime`. Local-only via `Database::new()` / `Builder::new_local()`. Turso sync via `Database::new_with_sync(url, token)` / `Builder::new_remote_replica()` with 60s auto-sync interval and `read_your_writes(true)`. `db.sync()` triggers on-demand sync. All DB methods use an async bridge: each method wraps async libsql calls in `self.rt.block_on()`. Row value extraction uses helper functions (`val_string`, `val_i64`, `val_bool`, `val_opt_string`, `val_opt_i64`, `val_opt_bool`) since libsql uses a `Value` enum for nullable fields.
- **Config file**: `~/.config/dodo/config.toml` with `[sync]` and `[backup]` sections. Parsed via `config.rs` with serde. Env var fallbacks: `DODO_TURSO_TOKEN`, `DODO_S3_ACCESS_KEY`, `DODO_S3_SECRET_KEY` (only used when config field is `None`). `SyncConfig::is_ready()` and `BackupConfig::is_ready()` check all required fields are present and enabled. `SyncConfig.sync_interval` (default 10 minutes) controls periodic sync frequency in TUI.
- **S3 backup**: `backup.rs` provides `create_backup` (gzip compress + upload), `list_backups` (newest first), `restore_backup` (download + decompress + safety `.pre-restore` copy), `delete_backup`, `check_backup_age` (startup overdue warning). Auto-prunes old backups beyond `max_backups` limit. Uses `new_runtime()` helper to deduplicate tokio runtime creation. CLI: `dodo backup` (create), `dodo backup list`, `dodo backup restore [latest]`, `dodo backup delete <name>`.
- **Turso sync**: Wired up via `Database::new_with_sync()` when `SyncConfig::is_ready()`. Uses `Builder::new_remote_replica()` with 60s auto-sync interval. `main.rs` loads config before DB init and branches on sync readiness. `dodo sync status/enable/disable` commands. Sync config stored in `[sync]` section of config file. Enable flow is interactive (prompts for URL/token).
- **Sync transitions**: Seamless local↔sync mode switching via `db.rs` transition layer. `needs_sync_transition()` detects local-only DBs missing sync metadata. `migrate_to_sync()` exports all data, renames old DB as `.pre-sync` backup, creates fresh remote replica, imports data, syncs. Rollback on failure restores the backup file. `clean_sync_metadata()` removes `client_wal` when sync is disabled (ensures re-enable triggers fresh migration). `recover_interrupted_migration()` runs at startup to restore from `.pre-sync` backup if crash interrupted migration. `export_all_data()`/`import_all_data()` handle bulk data transfer using `INSERT OR REPLACE`.
- **TUI sync status**: `SyncStatus` enum (`Disabled`, `Idle`, `Syncing`, `Synced(Instant)`, `Error(String)`) in `state.rs`. Header shows sync indicator: `● synced y:sync` (green) when connected, `↻ syncing` (yellow) during sync, `⚠ sync err y:retry` (red) on failure, nothing when disabled. Syncs on TUI launch, periodically (configurable interval, default 10min), and on quit. Final sync result printed to stderr after terminal teardown. `y` key triggers manual sync globally; `s` key triggers sync in Backup tab.
- **TUI Backup tab**: Shows config instructions when not configured, or backup list with name/age/size. `SyncStatus`-aware sync line shows URL and time since last sync. When backup is unconfigured, also shows sync config instructions. Keys: `j/k` navigate, `u` upload, `r` restore, `d` delete, `s` sync, `e` config. Status messages shown for operation results.
- **Startup backup check**: On every CLI invocation, if backup is configured and overdue (>= `schedule_days` since last backup), prints a reminder to stderr.

## Database

- libsql (SQLite-compatible) stored at `~/.local/share/dodo/dodo.db`
- `Database::new_with_sync(url, token)` for Turso embedded replica mode
- `Database::in_memory()` available for tests
- Tables: `tasks` (with `num_id INTEGER UNIQUE`, `estimate_minutes`, `deadline`, `scheduled`, `tags`, `task_notes`, `priority`, `modified_at`, `recurrence`, `is_template`, `template_id`), `sessions`

## Testing

- `cargo test` runs all 160 tests
- `src/backup.rs` (inline) — 25 unit tests for format_size, format_age, parse_backup_timestamp
- `tests/config_test.rs` — 22 unit tests for config parsing, TOML deserialization, defaults, is_ready checks, serialize/deserialize roundtrip
- `tests/fuzzy_test.rs` — 8 unit tests for fuzzy scoring (exact, prefix, substring, word-level, ranking)
- `tests/notation_test.rs` — 61 unit tests for notation parsing (duration, dates, token extraction, title cleanup, edge cases, recurrence patterns, next occurrence computation)
- `tests/workflow_test.rs` — 44 integration tests using `Database::in_memory()`, covering: simple daily list, Pomodoro start/pause/resume, GTD four horizons with contexts and projects, Eisenhower quadrants, freelance multi-project time tracking, numeric ID selection, fuzzy matching integration, academic multi-area workflow, session lifecycle, estimates, elapsed time, notes, edit command, multiple contexts, tags, deadlines, priority, recurring template CRUD, instance generation, pause/resume, history, update_notes_by_id, export/import roundtrip

## Conventions

- No `unwrap()` in production paths — use `anyhow::Result`, `?`, and `.context()`. Exception: `is_ready()`-guarded config field access (commented with safety invariant) and known-valid date constructions in TUI
- No silent `let _ =` in library code — use `if let Err(e) = ... { eprintln!("Warning: ...") }`. Exception: TUI event handlers (fire-and-forget, documented)
- Keep CLI output minimal and scannable
- Prefer editing existing files over creating new ones
- Every function should be actively used — no dead code kept for "future use"
- Tests mirror real-world use cases documented in USAGE.md
