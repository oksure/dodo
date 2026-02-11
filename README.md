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
- **Notes** — timestamped notes on any task
- **TUI mode** — four-pane task layout + report tab with productivity stats
- **TUI colors** — animated running tasks, color-coded priority/deadlines/progress
- **Sorting** — `--sort created|modified|title|area` on list command
- **Cloud sync** — R2/Dropbox support (optional)

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

# Open TUI (four-pane tasks + report tab)
dodo tui
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
| `tui` | `t` | Open TUI |

## TUI Keys

The TUI has two tabs: **Tasks** (four-pane layout) and **Report** (productivity stats).

### Tasks Tab

| Key | Action |
|-----|--------|
| `h`/`l` | Move between panes |
| `j`/`k` | Navigate tasks within pane |
| `s` | Toggle start/stop on selected task |
| `d` | Mark running task done |
| `n` | Open note modal for selected task |
| `o` | Cycle sort (created → modified → title) |
| `r` | Refresh |
| `1`/`2`/`Tab` | Switch tabs |
| `q`/`Esc` | Quit |

### Report Tab

| Key | Action |
|-----|--------|
| `h`/`l` | Change time range (Day/Week/Month/Year/All) |
| `1`/`2`/`Tab` | Switch tabs |
| `q`/`Esc` | Quit |

## Installation

```bash
cargo install --path .
```

## Testing

```bash
cargo test
```

79 tests cover notation parsing, fuzzy scoring, and the full task lifecycle including estimates, elapsed time, notes, priority, and editing. See [USAGE.md](USAGE.md) for the scenarios these tests exercise.

## Config

Data stored in:
- Database: `~/.local/share/dodo/dodo.db`
- Config: `~/.config/dodo/config.toml`

## License

MIT
