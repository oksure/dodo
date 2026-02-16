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
- `src/cli.rs` — clap command/argument definitions; re-exports `Area` from `task.rs`; `ReportRange` enum (shared between CLI and TUI)
- `src/db.rs` — SQLite database (libsql), migrations, all queries, session lifecycle
- `src/task.rs` — `Task` struct, `Area` enum (single source of truth, with `ValueEnum`, `to_scheduled_date()`), `TaskStatus` enum, `Display` impl
- `src/session.rs` — `Session` struct with `elapsed_seconds()`, `stop()`, `is_running()`
- `src/fuzzy.rs` — fuzzy matching with scored ranking (`score()`, `find_best_match()`, `rank_matches()`)
- `src/notation.rs` — inline notation parser (`parse_notation()`, `parse_duration()`, `parse_date()`, `prepare_task()`, `parse_filter_days()`)
- `src/config.rs` — config file parsing (`~/.config/dodo/config.toml`) for sync, backup, preferences, and email settings (`WeekStart` enum, `PreferencesConfig` with `sound_enabled`, `timer_sound_interval`, `default_view`, `last_view`, `default_estimate`; `EmailConfig` with `enabled`, `api_key`, `from`, `to`, `digest_time`)
- `src/backup.rs` — S3-compatible backup operations (create, list, restore, delete, prune, age check)
- `src/email.rs` — Resend API email digest (`send_digest()`, `send_test()`, `print_cron_entry()`)
- `src/tui/` — ratatui terminal UI (binary-only, not in lib), split into modules:
  - `mod.rs` — `run_tui()` entry point, terminal setup/teardown, initial background sync
  - `constants.rs` — color palette, sort modes, field labels/hints/types
  - `format.rs` — `format_dur()`, `format_est()`, `sort_tasks()`, `sort_label()`, `parse_filter_days()`
  - `state.rs` — `RunningTaskInfo`, `PaneState`, `AppMode`, `TuiTab`, `TasksView`, `DailyEntry`, `CalendarFocus`, `ReportRange`, `SyncStatus`, `App` struct + all impl methods
  - `event.rs` — `run_app()` event loop, `handle_tasks_key()`, `handle_daily_nav()`, `handle_recurring_key()`, `handle_backup_key()`, periodic sync
  - `draw.rs` — all `draw_*` rendering functions, styling helpers, animation, sync indicator
  - `tests.rs` — 14 TUI tests using ratatui `TestBackend` (scroll, navigation, rendering, view cycling, pane nav, tombstone)
- `tests/fuzzy_test.rs` — 8 unit tests for fuzzy scoring logic
- `tests/notation_test.rs` — 61 unit tests for notation/duration/date/priority/recurrence parsing
- `tests/config_test.rs` — 47 unit tests for config parsing, defaults, is_ready checks, serialization roundtrip, PreferencesConfig/WeekStart/sound/view/estimate, EmailConfig/is_ready/roundtrip
- `tests/workflow_test.rs` — 58 integration tests covering real-world workflows
- `USAGE.md` — real-world use cases with GTD, Pomodoro, Eisenhower frameworks

## Key Patterns

- **Lib/bin split**: `src/lib.rs` exposes `backup`, `cli`, `config`, `db`, `email`, `fuzzy`, `notation`, `session`, `task` as public modules. `src/main.rs` is the binary that also owns `tui/` (since it depends on ratatui/crossterm). Integration tests import via `dodo::`.
- **main.rs command dispatch**: `main()` is a clean dispatch table calling `cmd_add()`, `cmd_list()`, `cmd_start()`, `cmd_done()`, `cmd_status()`, `cmd_remove()`, `cmd_move()`, `cmd_edit()`, `cmd_note()`, `cmd_recurring()`, `cmd_config()`, `cmd_report()`, `cmd_sync()`, `cmd_backup()`, `cmd_email()`. The `DONE_DISPLAY_LIMIT` constant controls how many done tasks `cmd_list` shows.
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
- **Done target + undo**: `dodo d` completes running task (default). `dodo d <query>` completes specific task by ID/fuzzy. `dodo d --undo <query>` reopens a completed task. `resolve_done_task()` in db.rs handles finding completed tasks.
- **Move command**: `dodo mv --to <area> <query>` moves a task to TODAY/THIS WEEK/LONG TERM by adjusting its scheduled date. Uses `Area::to_scheduled_date()` shared helper (also used by TUI).
- **Report command**: `dodo rp [day|week|month|year|all]` shows productivity reports. `ReportRange` enum lives in cli.rs (shared with TUI). Calls all 7 `db.report_*()` methods.
- **Config command**: `dodo cfg show` prints current config as TOML, `dodo cfg path` prints config file path.
- **Sync now**: `dodo sync now` triggers an immediate sync (in addition to existing status/enable/disable).
- **List filters**: `dodo ls !! ^<3d =<1w --desc` supports priority minimum, deadline/scheduled range filters, and descending sort. `parse_filter_days()` moved to notation.rs (shared with TUI).
- **Note line ops**: `dodo note --delete-line N <task>` removes a note line, `dodo note --edit-line N <task>` replaces one. `dodo note --show` numbers lines for reference.
- **Shared task defaults**: `prepare_task()` in notation.rs centralizes notation parsing + defaults (1h estimate, scheduled=today). Used by `cmd_add`, `cmd_recurring Add`, TUI `confirm_add_task`, TUI `RecAddTemplate`.
- **Start/stop toggle**: `dodo s` with no args pauses the running task. No separate `pause` command. In TUI, `s` toggles: if task is Running, pauses it; otherwise starts it.
- **CLI grouped list**: `dodo ls` with no area shows all four groups (TODAY, THIS WEEK, LONG TERM, DONE) with section headers and counts. DONE is limited to 5 tasks. Specifying an area (`dodo ls today`) shows just that area. `--project` flag filters by project.
- **Display format**: Tasks render as `[num_id] [status_icon] AREA title [*] !priority +project @context #tag ~estimate ^deadline =scheduled (elapsed/estimate) [running]`. The `*` appears after the title if the task has notes.
- **Sorting**: `SortBy` enum in `cli.rs` (`Created`, `Modified`, `Area`, `Title`). Per-pane `sort_ascending` flag. TUI `o` key cycles: `created↑ → created↓ → modified↑ → modified↓ → title↑ → title↓`. DONE pane defaults to `modified↓` (newest done first). Pane header shows sort label with ↑/↓ arrow right-aligned.
- **Modified tracking**: `modified_at` column on tasks, updated by all mutation methods (`start_timer`, `pause_timer`, `complete_task`, `update_task_fields`, `append_note`, `clear_notes`). Used for `--sort modified` ordering.
- **TUI header legend**: Right-aligned symbol legend on the DODO header line showing status icons (`○ ▶ ⏸ ✓`) and notation symbols (`+proj @ctx ~est ^dead =sched !pri`) with matching colors. Running task shows countdown timer (`⏱ 23m left` green/yellow) or overage (`+5m over` red pulsing) when estimate exists, plain elapsed otherwise.
- **TUI search bar**: Bordered box between tab bar and panes, activated with `/`. Live-filters tasks across all panes as you type. Supports `+project` (project filter), `@context` (context filter), and plain text (title substring match), all AND-ed. `Enter`/`Esc` exits search mode (filter stays). `AppMode::Search` handles input.
- **TUI done/undone follows cursor**: Pressing `d` to mark done/undone moves the cursor to the task's new pane (e.g., TODAY→DONE or DONE→TODAY).
- **TUI pane layout**: Tasks tab has four views (Panes, Daily, Weekly, Calendar) cycled with `v`/`V`. Panes view: four vertical panes (LONG TERM, THIS WEEK, TODAY, DONE) with `h`/`l` pane navigation, `j`/`k` task navigation. Pane headers show elapsed/estimate/percentage/done stats. Report tab has DAY/WEEK/MONTH/YEAR/ALL range selector (default: Month). Switch tabs with `t`/`c`/`r`/`,`/`Tab`.
- **TUI colors**: Running tasks animate with pastel rainbow sweep (continuous left→right). Priority colored by level (Red for !!!!). Projects in Magenta. Elapsed colored by estimate progress (Green→Yellow→Red). Deadlines Red if overdue, Yellow if upcoming. Done tasks render with strikethrough (`CROSSED_OUT` modifier) in all views including calendar.
- **TUI note modal**: `n` key opens NoteView if task has notes (j/k navigate, `e` edit, `d` delete, `a` append), or goes straight to append input if no notes. `Alt+Enter` inserts newlines within a note entry. Multiline input shows visible line breaks while typing via `build_multiline_input()` helper. Notes are grouped by timestamp using `split_note_entries()` — continuation lines (without `[YYYY-MM-DD` prefix) are joined with their parent entry and rendered indented. Edit/delete operates on whole entries. `AppMode` enum (in `state.rs`): Normal, AddTask, MoveTask, ConfirmDelete, EditTask, EditTaskField, NoteView, Search, RecAddTemplate, RecConfirmDelete, EditConfig, EditConfigField. NoteView supports inline editing with `note_editing` flag.
- **TUI sound**: Terminal BEL (`\x07`) for audio feedback — plays on task completion (`d` key) and periodically while a task is running (every `timer_sound_interval` minutes). Controlled by `preferences.sound_enabled` (default true). Zero dependencies.
- **Marquee scrolling**: Long task titles show marquee animation when selected (3-second pause at start, then scroll through full title). Works in all views (Panes, Daily, Weekly, Calendar).
- **View persistence**: TUI remembers and restores the last opened view across sessions using `preferences.last_view` config field.
- **Recurring instance indicator**: Recurring instances show `↻` mark at the **beginning** of the title with inverted green styling.
- **Timer display improvements**: Shows countdown/overage based on total elapsed time from all sessions (not just current). Header shows `(23m left)` in green/yellow or `(+5m over)` in red.
- **DONE pane stats**: Shows on-time vs overdue counts (`7 on-time, 2 overdue`) instead of percentage.
- **View selector**: Pipe separators (`|`) between view labels with clearer active indicator (●).
- **TUI countdown timer**: `RunningTaskInfo` struct in `state.rs` holds `title`, `elapsed_seconds`, `estimate_minutes`. Header shows remaining time or overage. `get_running_task()` returns `(String, i64, Option<i64>)` — title, elapsed, estimate.
- **TUI responsiveness**: Event loop (in `event.rs`) polls at 16ms (~60fps) for instant key response, data refresh on 1-second timer. Tick counter drives running task animation.
- **TUI fire-and-forget pattern**: `let _ =` is used intentionally in TUI event handlers for best-effort DB operations. No error display mechanism exists in the event loop; failures are non-fatal.
- **Report queries**: `db.rs` has 7 report methods: `report_tasks_done`, `report_total_seconds`, `report_by_hour`, `report_by_weekday`, `report_by_project`, `report_done_tasks`, `report_active_days`. All take date range strings.
- **DB migrations**: Schema changes use check-then-alter pattern in `db.rs::migrate()`. New columns are added with `ALTER TABLE` guarded by a `SELECT` probe.
- **Complete prefers running**: `complete_task()` uses `ORDER BY` to prefer Running tasks over Paused ones when multiple are active.
- **Quote-free input**: Add, Start, Remove, Edit, Note commands use `Vec<String>` with `trailing_var_arg` so users can type `dodo a fix login bug +backend ~2h !!!` without quotes.
- **Notation precedence**: Inline notation tokens override CLI flags (e.g., `+backend` in text overrides `--project`).
- **Recurring tasks**: Template + instance model. Templates (`is_template=1`) live in the Recurring tab and generate instances that appear in normal panes. Recurrence patterns: `*daily`, `*3d`, `*weekly`, `*2w`, `*monthly`, `*3m`, `*mon,wed,fri`, `*day15`. One active instance per template. Completing an instance auto-generates the next. Deleting an instance = skip; `g` (generate) recreates. Paused templates stop generating. CLI: `dodo rec` (list/add/edit/delete/pause/resume/generate/history).
- **TUI four-tab layout**: Tab 1 (Tasks, `t`): four views cycled with `v`/`V` (Panes → Daily → Weekly → Calendar). Tab 2 (Recurring, `c`): template list with pause/generate/edit. Tab 3 (Report, `r`): productivity stats. Tab 4 (Settings, `,`): S3 backup list with upload/restore/delete/sync + config editor with Week Start preference. `Tab` cycles through all four.
- **Tasks views**: Panes (4-column: LONG TERM/THIS WEEK/TODAY/DONE), Daily (scrollable date-grouped list), Weekly (2×4 tile grid for 8 days), Calendar (full-screen month grid with centered month/year title and task cells). `v`/`V` cycles views (switching to Daily auto-focuses today's first task). `t` key jumps to today in Daily/Weekly/Calendar views.
- **Recurrence notation**: `parse_recurrence()` validates patterns. `next_occurrence()` computes the next date from a pattern + reference date. Day-of-month clamps to last day (e.g., day31 in Feb → Feb 28).
- **Instance indicator**: Recurring instances show `↻` at the **beginning** of the title in task panes (styled with inverted green).
- **Database engine**: Uses `libsql` (SQLite-compatible fork supporting Turso embedded replicas). `Database` struct stores `libsql::Database` handle + `Connection` + `tokio::runtime::Runtime`. Main DB (`dodo.db`) is always `Builder::new_local()` — zero network latency. Sync uses a separate replica (`dodo-sync.db`) opened with `Builder::new_remote_replica()` + `read_your_writes(false)` only inside background sync threads. All DB methods use an async bridge: each method wraps async libsql calls in `self.rt.block_on()`. Row value extraction uses helper functions (`val_string`, `val_i64`, `val_bool`, `val_opt_string`, `val_opt_i64`, `val_opt_bool`) since libsql uses a `Value` enum for nullable fields.
- **Config file**: `~/.config/dodo/config.toml` with `[sync]`, `[backup]`, `[preferences]`, and `[email]` sections. Parsed via `config.rs` with serde. Env var fallbacks: `DODO_TURSO_TOKEN`, `DODO_S3_ACCESS_KEY`, `DODO_S3_SECRET_KEY`, `DODO_RESEND_API_KEY` (only used when config field is `None`). `SyncConfig::is_ready()`, `BackupConfig::is_ready()`, and `EmailConfig::is_ready()` check all required fields are present and enabled. `SyncConfig.sync_interval` (default 10 minutes) controls periodic sync frequency in TUI. `PreferencesConfig` fields: `week_start` (sunday/monday), `sound_enabled` (default true), `timer_sound_interval` (default 10 min), `default_view` (panes/daily/weekly/calendar), `last_view` (panes/daily/weekly/calendar), `default_estimate` (default 60 min). `EmailConfig` fields: `enabled` (default false), `api_key`, `from`, `to`, `digest_time` (default "07:00").
- **S3 backup**: `backup.rs` provides `create_backup` (gzip compress + upload), `list_backups` (newest first), `restore_backup` (download + decompress + safety `.pre-restore` copy), `delete_backup`, `check_backup_age` (startup overdue warning). Auto-prunes old backups beyond `max_backups` limit. Uses `new_runtime()` helper to deduplicate tokio runtime creation. CLI: `dodo backup` (create), `dodo backup list`, `dodo backup restore [latest]`, `dodo backup delete <name>`.
- **Turso sync (local-first, non-blocking)**: Two-DB architecture. Main DB (`dodo.db`) always local via `Database::new()`. Sync replica (`dodo-sync.db`) created on-demand during background sync. `do_remote_sync(url, token)` opens both DBs, pulls from Turso, bidirectionally merges using `merge_remote_data()` (modified_at timestamp wins), then pushes back. `sync_with_remote()` spawns this in a `std::thread`, returns `mpsc::Receiver<Result<()>>`. TUI stores receiver in `App.sync_receiver`, polls with `check_sync_result()` via `try_recv()` (non-blocking). CLI `dodo sync now` calls `do_remote_sync()` directly (blocking). `num_id` conflicts resolved deterministically: earlier `created` timestamp keeps the `num_id`, other gets bumped to `MAX+1`. Sessions use `INSERT OR IGNORE` (first-write-wins). `clean_sync_db()` removes `dodo-sync.db` when sync is disabled. `clean_sync_metadata()` removes old replica metadata from `dodo.db`. `recover_interrupted_migration()` runs at startup as safety net. No final sync on TUI quit — data is safe locally, syncs on next startup.
- **Sync tombstones**: `deleted_tasks` table tracks IDs of locally deleted tasks. `delete_task_by_id()` and `delete_template()` record tombstones before deleting. `merge_remote_data()` skips any task whose ID is in `deleted_tasks`, preventing sync from resurrecting deleted tasks. `propagate_tombstones()` deletes tombstoned tasks from the sync DB during `do_remote_sync()` so deletes propagate to the remote.
- **Daily view scroll**: `draw_tasks_daily()` takes `&mut App` and computes a symmetric scroll-margin offset (margin=3 items from each edge). Applied via `*list_state.offset_mut() = app.daily_scroll`. Cursor stays within margin of both top and bottom viewport edges, making up/down navigation feel consistent.
- **TUI sync status**: `SyncStatus` enum (`Disabled`, `Idle`, `Syncing`, `Synced(Instant)`, `Error(String)`) in `state.rs`. Header shows sync indicator: `● synced y:sync` (green) when connected, `↻ syncing` (yellow) during sync, `⚠ sync err y:retry` (red) on failure, nothing when disabled. Syncs on TUI launch (non-blocking), periodically (configurable interval, default 10min). No final sync on quit. `y` key triggers manual sync globally; `s` key triggers sync in Settings tab.
- **TUI Settings tab** (renamed from Backup, key `,`): Shows config instructions when not configured, or backup list with name/age/size. `SyncStatus`-aware sync line shows URL and time since last sync. Config editor (23 fields across 4 sections: Sync, Backup, Preferences, Email) includes scrolling via `config_scroll` with auto-follow on j/k navigation. Email section has 5 fields: Enabled, API Key, From Address, To Address, Digest Time. Keys: `j/k` navigate, `u` upload, `r` restore, `d` delete, `s` sync, `e` config. Status messages shown for operation results.
- **TUI date adjust follows cursor**: `+`/`-` keys call `adjust_selected_date()` which adjusts the scheduled date and follows the cursor to the task's new pane (Panes view) or new position (Daily view), replicating the `done()` cursor-follow pattern.
- **TUI report tab layout**: 3-row layout — Row 1: compact summary cards (metrics + gauge in 5 lines), Row 2: `BarChart` widgets for weekly activity (7 bars, teal) and hourly distribution (24 bars, peach), Row 3: project list with inline progress bars + completed tasks list. Uses `ratatui::widgets::{Bar, BarChart, BarGroup}`.
- **Email digest**: `dodo email digest` sends today's tasks via Resend API, `dodo email test` verifies config, `dodo email cron` prints crontab entry. `email.rs` builds HTML with overdue/today/running sections. Uses `ureq` for HTTP, `serde_json` for request body.
- **Startup backup check**: On every CLI invocation, if backup is configured and overdue (>= `schedule_days` since last backup), prints a reminder to stderr.

## Database

- libsql (SQLite-compatible) stored at `~/.local/share/dodo/dodo.db` (always local)
- Sync replica at `~/.local/share/dodo/dodo-sync.db` (created during background sync only)
- `Database::in_memory()` available for tests
- Tables: `tasks` (with `num_id INTEGER UNIQUE`, `estimate_minutes`, `deadline`, `scheduled`, `tags`, `task_notes`, `priority`, `modified_at`, `recurrence`, `is_template`, `template_id`), `sessions`, `deleted_tasks` (tombstone table: `id TEXT PRIMARY KEY`, `deleted_at TEXT`)

## Testing

- `cargo test` runs all 215 tests
- `src/backup.rs` (inline) — 25 unit tests for format_size, format_age, parse_backup_timestamp
- `src/tui/tests.rs` — 14 TUI tests using ratatui `TestBackend` and `Database::in_memory()`, covering: daily scroll symmetry (down margin, up margin, cursor-in-viewport invariant), daily navigation (skip headers, count multiplier, G jump-to-end), rendering (daily view, full UI across all 7 views/tabs, task titles in buffer, recurring mark), pane navigation (h/l movement), view cycling (v key: Panes→Daily→Weekly→Calendar), tombstone integration (delete + merge rejection), done pane stats
- `tests/config_test.rs` — 47 unit tests for config parsing, TOML deserialization, defaults, is_ready checks, serialize/deserialize roundtrip, PreferencesConfig/WeekStart, EmailConfig/is_ready/roundtrip
- `tests/fuzzy_test.rs` — 8 unit tests for fuzzy scoring (exact, prefix, substring, word-level, ranking)
- `tests/notation_test.rs` — 61 unit tests for notation parsing (duration, dates, token extraction, title cleanup, edge cases, recurrence patterns, next occurrence computation)
- `tests/workflow_test.rs` — 60 integration tests using `Database::in_memory()`, covering: simple daily list, Pomodoro start/pause/resume, GTD four horizons with contexts and projects, Eisenhower quadrants, freelance multi-project time tracking, numeric ID selection, fuzzy matching integration, academic multi-area workflow, session lifecycle, estimates, elapsed time, notes, edit command, multiple contexts, tags, deadlines, priority, recurring template CRUD, instance generation, pause/resume, history, update_notes_by_id, export/import roundtrip, done target/undo, move between areas, note line editing, reports, merge remote/local newer wins, num_id conflict resolution, session dedup, tombstone prevents sync resurrection, template tombstone prevents sync resurrection
- `test_ux.sh` — manual UX testing script using tmux to verify visual TUI functionality: marquee scrolling, recurring marks, DONE pane stats, timer display, header timer, view persistence, view selector spacing. Automatically builds, installs, tests, and cleans up tmux sessions.

## Conventions

- No `unwrap()` in production paths — use `anyhow::Result`, `?`, and `.context()`. Exception: `is_ready()`-guarded config field access (commented with safety invariant) and known-valid date constructions in TUI
- No silent `let _ =` in library code — use `if let Err(e) = ... { eprintln!("Warning: ...") }`. Exception: TUI event handlers (fire-and-forget, documented)
- Keep CLI output minimal and scannable
- Prefer editing existing files over creating new ones
- Every function should be actively used — no dead code kept for "future use"
- Tests mirror real-world use cases documented in USAGE.md
