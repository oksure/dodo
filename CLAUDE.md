# Dodo CLI

A keyboard-first, Blitzit-inspired todo + time tracker CLI in Rust.

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
- `src/tui.rs` — ratatui terminal UI with Blitzit-style four-group navigation (binary-only, not in lib)
- `tests/fuzzy_test.rs` — 8 unit tests for fuzzy scoring logic
- `tests/workflow_test.rs` — 20 integration tests covering real-world workflows
- `USAGE.md` — real-world use cases with GTD, Pomodoro, Eisenhower frameworks

## Key Patterns

- **Lib/bin split**: `src/lib.rs` exposes `cli`, `db`, `fuzzy`, `session`, `task` as public modules. `src/main.rs` is the binary that also owns `tui` (since it depends on ratatui/crossterm). Integration tests import via `dodo::`.
- **Numeric task IDs**: Tasks have an auto-incrementing `num_id` (integer) in addition to a ULID string `id`. Commands like `start`, `remove` accept either a numeric ID or a fuzzy text query. Resolution logic is in `db.rs::resolve_task()`.
- **Fuzzy matching**: `fuzzy.rs::score()` ranks matches: exact (100) > prefix (75) > word-start (60) > substring (50) > word-contains (40). `find_best_match()` picks the top result; `rank_matches()` sorts all results by relevance. `find_tasks()` loads all non-done tasks and returns them ranked.
- **Task resolution**: `resolve_task(query)` tries `parse::<i64>()` first for numeric ID lookup, then falls back to fuzzy-ranked search via `find_tasks()` + `find_best_match()`.
- **Session lifecycle**: `Session` methods (`elapsed_seconds`, `stop`, `is_running`) are used by `pause_timer`, `complete_task`, and `get_running_task` in `db.rs`. Sessions are loaded from DB via `row_to_session()` / `get_active_session()`.
- **Blitzit four groups**: Tasks belong to an `Area` (LongTerm, ThisWeek, Today, Completed). `Task::area_str()` returns short labels (LONG, WEEK, TODAY, DONE) shown in list output and TUI sidebar.
- **Display format**: Tasks render as `[num_id] [status_icon] AREA title tags` (e.g., `[1] [▶] TODAY My task +project @context [running]`).
- **DB migrations**: Schema changes use check-then-alter pattern in `db.rs::migrate()`. New columns are added with `ALTER TABLE` guarded by a `SELECT` probe.
- **Complete prefers running**: `complete_task()` uses `ORDER BY` to prefer Running tasks over Paused ones when multiple are active.

## Database

- SQLite stored at `~/.local/share/dodo/dodo.db`
- `Database::in_memory()` available for tests
- Tables: `tasks` (with `num_id INTEGER UNIQUE`), `sessions`

## Testing

- `cargo test` runs all 28 tests
- `tests/fuzzy_test.rs` — unit tests for fuzzy scoring (exact, prefix, substring, word-level, ranking)
- `tests/workflow_test.rs` — integration tests using `Database::in_memory()`, covering: simple daily list, Pomodoro start/pause/resume, GTD four horizons with contexts and projects, Eisenhower quadrants, freelance multi-project time tracking, numeric ID selection, fuzzy matching integration, academic multi-area workflow, session lifecycle

## Conventions

- No `unwrap()` in production paths — use `anyhow::Result` and `?`
- Keep CLI output minimal and scannable
- Prefer editing existing files over creating new ones
- Every function should be actively used — no dead code kept for "future use"
- Tests mirror real-world use cases documented in USAGE.md
