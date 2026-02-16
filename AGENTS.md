# Agentic Coding Guidelines for Dodo

A keyboard-first todo + time tracker CLI in Rust.

## Build & Test Commands

```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run a specific test (e.g., workflow tests)
cargo test --test workflow_test

# Run a specific test function
cargo test test_name_here

# Run with output visible
cargo test -- --nocapture

# Install locally for testing
cargo install --path .

# UX testing with tmux
bash test_ux.sh
```

## Code Style Guidelines

### Error Handling
- **Never use `unwrap()` in production paths** — use `anyhow::Result`, `?`, and `.context()`
- Exception: `is_ready()`-guarded config field access (comment with safety invariant)
- Exception: Known-valid date constructions in TUI
- **No silent `let _ =` in library code** — use `if let Err(e) = ... { eprintln!("Warning: ...") }`
- Exception: TUI event handlers (fire-and-forget, documented with `let _ =`)

### Imports & Formatting
- Group imports: std, external crates, internal modules
- Use `use crate::` for internal imports, not relative paths
- No wildcard imports (`use module::*`)
- Max line length: 100 characters
- Use `cargo fmt` before committing

### Naming Conventions
- Functions/variables: `snake_case`
- Types/traits/enums: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Boolean getters: `is_foo()`, not `get_is_foo()`
- Database methods: verb + noun (e.g., `start_timer`, `complete_task`)

### Types & Safety
- Prefer `i64` over `i32` for database IDs
- Use `chrono::DateTime<Utc>` for timestamps, `NaiveDate` for dates
- Use `Option<T>` for nullable fields, never `null` pointers
- Use `anyhow::Result<T>` for fallible operations
- Database queries use async bridge with `block_on()`

## Project Structure

```
src/
  lib.rs          # Library root, re-exports public modules
  main.rs         # Binary entry, cmd_* handlers
  cli.rs          # Clap definitions, Area enum
  db.rs           # SQLite (libsql), migrations, queries
  task.rs         # Task struct, TaskStatus, Area enum
  session.rs      # Session struct
  fuzzy.rs        # Fuzzy matching
  notation.rs     # Inline notation parser
  config.rs       # Config file parsing
  backup.rs       # S3 backup operations
  email.rs        # Email digest
  tui/            # Terminal UI (binary-only)
    mod.rs        # run_tui() entry
    constants.rs  # Colors, sort modes
    format.rs     # Formatting helpers
    state.rs      # App state, modes
    event.rs      # Event loop
    draw.rs       # Rendering functions
```

## Key Patterns

### Lib/Bin Split
- `src/lib.rs` exposes public modules for integration tests
- `src/main.rs` owns TUI (ratatui/crossterm dependency)
- TUI modules use `pub(super)` visibility

### Database Queries
- Use `TASK_SELECT_WITH_ELAPSED` constant for consistent queries
- Extract values with helpers: `val_string()`, `val_i64()`, `val_bool()`
- Sessions use `INSERT OR IGNORE` (first-write-wins)

### TUI Patterns
- Dependency graph: `mod.rs → state, event`; `event → state, draw`; `draw → state, constants, format`
- Event loop polls at 16ms (~60fps)
- Use `AppMode` enum for modal states
- `is_active` flag for pane focus, `is_selected` for cursor position

### Notation Parsing
- 7 token types: `+project`, `@context`, `#tag`, `~duration`, `^deadline`, `=scheduled`, `!`–`!!!!`
- Single-value tokens: last-wins
- Multi-value tokens: collect all
- Tokens must be preceded by whitespace

## Testing

- Unit tests inline in source files or in `tests/` directory
- Use `Database::in_memory()` for integration tests
- Test files: `fuzzy_test.rs`, `notation_test.rs`, `config_test.rs`, `workflow_test.rs`
- UX tests: `test_ux.sh` uses tmux for TUI verification
- Tests must mirror real-world use cases from `USAGE.md`

## Conventions

- Keep CLI output minimal and scannable
- Prefer editing existing files over creating new ones
- Every function should be actively used — no dead code
- Add comments for safety invariants when using `unwrap()`
- Run `cargo test` before committing
- Run `cargo fmt` to ensure consistent formatting

## Git Workflow

```bash
# Check what changed
git diff --stat

# Stage and commit
git add -A
git commit -m "descriptive message"
git push
```
