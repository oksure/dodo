# Dodo CLI

A keyboard-first todo + time tracker CLI written in Rust.

## Features

- **Inline notation** — `+project @context #tag ~2h ^friday =wed !!!` parsed from input
- **Numeric task IDs** — short IDs (1, 2, 3) for quick selection
- **Fuzzy matching** — ranked search (exact > prefix > word > substring)
- **Keyboard-first** — tmux-style single-letter commands, quote-free input
- **Date-based areas** — scheduled/deadline dates drive grouping into Today, ThisWeek, LongTerm
- **Time tracking** — start/stop toggle, elapsed/estimate display with seconds
- **Estimates & deadlines** — `~2h` estimates, `^friday` deadlines, `=wed` scheduling
- **Priority** — `!` to `!!!!` urgency levels (4 = most urgent)
- **Notes** — timestamped notes on any task, per-note editing/deletion in TUI
- **Recurring tasks** — template-based recurring tasks (`*daily`, `*weekly`, `*mon,wed,fri`, etc.)
- **TUI mode** — four-tab layout: Tasks (four panes), Recurring, Report, Backup
- **TUI search** — live-filtering search bar with `+project`, `@context`, priority (`!!`), date ranges (`^<3d`, `=<1w`)
- **TUI colors** — pastel rainbow sweep on running tasks, color-coded priority/deadlines/progress
- **Sorting** — per-pane sort cycling (`o` key): created/modified/title with ↑asc/↓desc
- **Cloud sync** — Turso embedded replica with 60s auto-sync (optional)
- **S3 backup** — compressed backups to any S3-compatible storage with auto-prune

## Quick Start

```bash
# Add tasks with inline notation (no quotes needed)
dodo a fix login bug +backend @john #urgent ~2h ^friday !!!
# => Added: fix login bug [#1]

# List all groups (TODAY, THIS WEEK, LONG TERM, DONE)
dodo ls
# --- TODAY (1) ---
# [1] [ ] TODAY fix login bug !!! +backend @john #urgent ~2h ^Feb14

# Filter by area or project
dodo ls week
dodo ls --project backend

# Start timer by numeric ID or fuzzy match
dodo s 1
dodo s login
# => Started: fix login bug [#1]

# Pause running task (no args = pause)
dodo s

# Check status / complete task
dodo st
dodo d

# Edit task metadata
dodo e 1 ~3h ^tmr +frontend

# Add notes
dodo n 1 --show
dodo n 1              # interactive: type note, Ctrl+D to finish

# Remove by numeric ID or fuzzy match
dodo rm 1

# Open TUI (default when running dodo with no command)
dodo
```

## Inline Notation

Add metadata directly in task text — no flags needed:

| Symbol | Meaning | Example | Notes |
|--------|---------|---------|-------|
| `+word` | Project | `+backend` | Single (last wins) |
| `@word` | Context | `@john @sarah` | Multiple |
| `#word` | Tag | `#urgent #bug` | Multiple |
| `~dur` | Estimate | `~2h30m` | `m` `h` `d`(8h) `w`(40h) |
| `^date` | Deadline | `^friday` `^tmr` `^0115` | Named, relative, MMDD, ISO |
| `=date` | Scheduled | `=wed` `=2w` | Same formats as deadline |
| `!`–`!!!!` | Priority | `!!!` | 1–4 (4 = most urgent) |
| `*pattern` | Recurrence | `*daily` `*2w` `*mon,fri` | For recurring templates |

Notation tokens are extracted and the remaining text becomes the title. Flags (`--project`, `--context`, etc.) still work; inline notation takes precedence.

## Commands

| Command | Short | Description |
|---------|-------|-------------|
| `add` | `a` | Add new task (with inline notation) |
| `list` | `ls` | List tasks (no area = all groups, done limited to 5) |
| `start` | `s` | Start/stop timer (no args = pause running task) |
| `done` | `d` | Complete running task |
| `status` | `st` | Show running task + elapsed time |
| `edit` | `e` | Edit task metadata via notation |
| `note` | `n` | Add/view/clear notes on a task |
| `remove` | `rm` | Delete task (by ID or fuzzy match) |
| `recurring` | `rec` | Manage recurring tasks (add/edit/delete/pause/resume/generate/history) |
| `sync` | | Manage Turso sync (status/enable/disable) |
| `backup` | | Manage S3 backups (create/list/restore/delete) |
| `help` | `h` | Show CLI help |

Running `dodo` with no command launches the TUI.

## TUI Keys

The TUI has four tabs: **Tasks** (`t`), **Recurring** (`c`), **Report** (`r`), and **Backup** (`b`).

### Tasks Tab

| Key | Action |
|-----|--------|
| `h`/`l` | Move between panes |
| `j`/`k` | Navigate tasks within pane |
| `s` | Toggle start/stop on selected task |
| `d` | Mark done/undone (cursor follows task) |
| `a` | Add new task |
| `n` | Open note modal for selected task |
| `o` | Cycle sort (created↑ → created↓ → modified↑ → ... → title↓) |
| `/` | Focus search bar (filter by `+project`, `@context`, or text) |
| `<`/`>` | Quick-move task between panes |
| `Enter` | Open task detail/edit modal |
| `Backspace` | Delete task (with confirmation) |
| `m` | Move task to another pane |
| `t`/`c`/`r`/`b`/`Tab` | Switch tabs |
| `q`/`Esc` | Quit |

### Recurring Tab

| Key | Action |
|-----|--------|
| `j`/`k` | Navigate templates |
| `a` | Add new recurring template |
| `p` | Pause/resume template |
| `g` | Generate instances |
| `Enter` | Edit template |
| `Backspace` | Delete template |

### Search Bar

| Key | Action |
|-----|--------|
| `/` | Focus search bar |
| type | Live-filter tasks across all panes |
| `Enter`/`Esc` | Exit search (filter stays active) |

Filter syntax: `+backend` (project), `@john` (context), `!!` (priority >= 2), `^<3d` (deadline within 3 days), `=<1w` (scheduled within 1 week), `fix bug` (title substring). All terms are AND-ed.

### Report Tab

| Key | Action |
|-----|--------|
| `h`/`l` | Change time range (Day/Week/Month/Year/All) |
| `q`/`Esc` | Quit |

### Backup Tab

| Key | Action |
|-----|--------|
| `j`/`k` | Navigate backups |
| `u` | Upload new backup |
| `r` | Restore selected backup |
| `d` | Delete selected backup |
| `e` | Edit sync/backup config |

## Installation

```bash
cargo install --path .
```

## Testing

```bash
cargo test
```

159 tests cover notation parsing (61), fuzzy scoring (8), config parsing (22), backup formatting (25), and full workflow integration (43) including recurring tasks, estimates, elapsed time, notes, priority, and editing. See [USAGE.md](USAGE.md) for the scenarios these tests exercise.

## Config

Data stored in:
- Database: `~/.local/share/dodo/dodo.db`
- Config: `~/.config/dodo/config.toml` (with `[sync]` and `[backup]` sections)
- Env var fallbacks: `DODO_TURSO_TOKEN`, `DODO_S3_ACCESS_KEY`, `DODO_S3_SECRET_KEY`

## License

MIT
