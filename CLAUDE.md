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
- `src/main.rs` — binary entry point, command dispatch, output formatting
- `src/cli.rs` — clap command/argument definitions
- `src/db.rs` — SQLite database (rusqlite), migrations, all queries, session lifecycle
- `src/task.rs` — `Task` struct, `Area`/`TaskStatus` enums, `Display` impl
- `src/session.rs` — `Session` struct with `elapsed_seconds()`, `stop()`, `is_running()`
- `src/fuzzy.rs` — fuzzy matching with scored ranking (`score()`, `find_best_match()`, `rank_matches()`)
- `src/notation.rs` — inline notation parser (`parse_notation()`, `parse_duration()`, `parse_date()`)
- `src/tui.rs` — ratatui terminal UI with four-pane layout and report tab (binary-only, not in lib)
- `tests/fuzzy_test.rs` — 8 unit tests for fuzzy scoring logic
- `tests/notation_test.rs` — 41 unit tests for notation/duration/date/priority parsing
- `tests/workflow_test.rs` — 30 integration tests covering real-world workflows
- `USAGE.md` — real-world use cases with GTD, Pomodoro, Eisenhower frameworks

## Key Patterns

- **Lib/bin split**: `src/lib.rs` exposes `cli`, `db`, `fuzzy`, `notation`, `session`, `task` as public modules. `src/main.rs` is the binary that also owns `tui` (since it depends on ratatui/crossterm). Integration tests import via `dodo::`.
- **Inline notation**: `notation.rs::parse_notation()` extracts 7 token types from input: `+project`, `@context` (multiple), `#tag` (multiple), `~duration`, `^deadline`, `=scheduled`, `!`–`!!!!` priority. Remaining text becomes the title. Single-value tokens use last-wins; multi-value tokens collect all. Tokens must be preceded by whitespace (e.g., `email@test` is not parsed).
- **Duration parsing**: `~30m`, `~1h`, `~1h30m`, `~1d` (480m = 8h workday), `~1w` (2400m = 5×8h). Units are composable: `~2d4h`.
- **Date parsing**: Named (`today`/`tdy`, `tomorrow`/`tmr`, `yesterday`/`ytd`), day names (`mon`–`sun` → next occurrence), relative (`3d`, `2w`, `1m`, `-3d`), MMDD (`0115`), YYYYMMDD (`20250502`), ISO (`2025-05-02`).
- **Numeric task IDs**: Tasks have an auto-incrementing `num_id` (integer) in addition to a ULID string `id`. Commands like `start`, `remove`, `edit`, `note` accept either a numeric ID or a fuzzy text query. Resolution logic is in `db.rs::resolve_task()`.
- **Fuzzy matching**: `fuzzy.rs::score()` ranks matches: exact (100) > prefix (75) > word-start (60) > substring (50) > word-contains (40). `find_best_match()` picks the top result; `rank_matches()` sorts all results by relevance. `find_tasks()` loads all non-done tasks and returns them ranked.
- **Task resolution**: `resolve_task(query)` tries `parse::<i64>()` first for numeric ID lookup, then falls back to fuzzy-ranked search via `find_tasks()` + `find_best_match()`.
- **Session lifecycle**: `Session` methods (`elapsed_seconds`, `stop`, `is_running`) are used by `pause_timer`, `complete_task`, and `get_running_task` in `db.rs`. Sessions are loaded from DB via `row_to_session()` / `get_active_session()`.
- **Elapsed time**: `list_tasks()` and `find_tasks()` use a LEFT JOIN on sessions to compute total elapsed seconds per task, including live running sessions via `julianday('now')`.
- **Four groups**: Tasks belong to an `Area` (LongTerm, ThisWeek, Today, Completed). `Task::area_str()` returns short labels (LONG, WEEK, TODAY, DONE) shown in list output and TUI panes.
- **Display format**: Tasks render as `[num_id] [status_icon] AREA title [*] !priority +project @context #tag ~estimate ^deadline =scheduled (elapsed/estimate) [running]`. The `*` appears after the title if the task has notes.
- **Sorting**: `SortBy` enum in `cli.rs` (`Created`, `Modified`, `Area`, `Title`). `list_tasks_sorted()` in `db.rs` accepts sort param; `list_tasks()` defaults to `Created`. TUI cycles sort with `o` key.
- **Modified tracking**: `modified_at` column on tasks, updated by all mutation methods (`start_timer`, `pause_timer`, `complete_task`, `update_task_fields`, `append_note`, `clear_notes`). Used for `--sort modified` ordering.
- **TUI four-pane layout**: Four vertical panes (LONG TERM, THIS WEEK, TODAY, DONE) rendered simultaneously. Each pane has its own `ListState`. `h`/`l` navigates panes, `j`/`k` navigates tasks within a pane. Active pane highlighted in yellow. Compact two-line task rendering: line 1 = `num_id icon title [*]`, line 2 = metadata (priority, project, estimate, elapsed, deadline, scheduled).
- **DB migrations**: Schema changes use check-then-alter pattern in `db.rs::migrate()`. New columns are added with `ALTER TABLE` guarded by a `SELECT` probe.
- **Complete prefers running**: `complete_task()` uses `ORDER BY` to prefer Running tasks over Paused ones when multiple are active.
- **Quote-free input**: Add, Start, Remove, Edit, Note commands use `Vec<String>` with `trailing_var_arg` so users can type `dodo a fix login bug +backend ~2h !!!` without quotes.
- **Notation precedence**: Inline notation tokens override CLI flags (e.g., `+backend` in text overrides `--project`).

## Database

- SQLite stored at `~/.local/share/dodo/dodo.db`
- `Database::in_memory()` available for tests
- Tables: `tasks` (with `num_id INTEGER UNIQUE`, `estimate_minutes`, `deadline`, `scheduled`, `tags`, `task_notes`, `priority`, `modified_at`), `sessions`

## Testing

- `cargo test` runs all 79 tests
- `tests/fuzzy_test.rs` — unit tests for fuzzy scoring (exact, prefix, substring, word-level, ranking)
- `tests/notation_test.rs` — unit tests for notation parsing (duration, dates, token extraction, title cleanup, edge cases)
- `tests/workflow_test.rs` — integration tests using `Database::in_memory()`, covering: simple daily list, Pomodoro start/pause/resume, GTD four horizons with contexts and projects, Eisenhower quadrants, freelance multi-project time tracking, numeric ID selection, fuzzy matching integration, academic multi-area workflow, session lifecycle, estimates, elapsed time, notes, edit command, multiple contexts, tags, deadlines, priority

## Conventions

- No `unwrap()` in production paths — use `anyhow::Result` and `?`
- Keep CLI output minimal and scannable
- Prefer editing existing files over creating new ones
- Every function should be actively used — no dead code kept for "future use"
- Tests mirror real-world use cases documented in USAGE.md
