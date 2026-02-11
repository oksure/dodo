# Dodo CLI

A keyboard-first, Blitzit-inspired todo + time tracker CLI written in Rust.

## Features

- **Inline notation** — `+project @context #tag ~2h $friday ^wed` parsed from input
- **Numeric task IDs** — short IDs (1, 2, 3) for quick selection
- **Fuzzy matching** — ranked search (exact > prefix > word > substring)
- **Keyboard-first** — tmux-style single-letter commands, quote-free input
- **Focus areas** — LongTerm, ThisWeek, Today, Completed
- **Time tracking** — start, pause, complete with elapsed/estimate display
- **Estimates & deadlines** — `~2h` estimates, `$friday` deadlines, `^wed` scheduling
- **Notes** — timestamped notes on any task
- **TUI mode** — ratatui interface with all four groups including Completed
- **Cloud sync** — R2/Dropbox support (optional)

## Quick Start

```bash
# Add tasks with inline notation (no quotes needed)
dodo a fix login bug +backend @john #urgent ~2h $friday
# => Added: fix login bug [#1]

# List today's tasks (shows elapsed time, estimates, metadata)
dodo ls
# => [1] [ ] TODAY fix login bug +backend @john #urgent ~2h $Feb14

# Start timer by numeric ID or fuzzy match
dodo s 1
dodo s login

# Check status
dodo st

# Complete task
dodo d

# Edit task metadata
dodo e 1 ~3h $tmr +frontend

# Add notes
dodo n 1 --show
dodo n 1              # interactive: type note, Ctrl+D to finish

# Remove by numeric ID or fuzzy match
dodo rm 1

# Open TUI
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
| `$date` | Deadline | `$friday` `$tmr` `$1/15` | Named, relative, M/D, ISO |
| `^date` | Scheduled | `^wed` `^2w` | Same formats as deadline |

Notation tokens are extracted and the remaining text becomes the title. Flags (`--project`, `--context`, etc.) still work; inline notation takes precedence.

## Commands

| Command | Short | Description |
|---------|-------|-------------|
| `add` | `a` | Add new task (with inline notation) |
| `list` | `ls` | List tasks (default: today + running) |
| `start` | `s` | Start timer on task |
| `pause` | `p` | Pause current timer |
| `done` | `d` | Complete running task |
| `status` | `st` | Show running task + elapsed time |
| `edit` | `e` | Edit task metadata via notation |
| `note` | `n` | Add/view/clear notes on a task |
| `remove` | `rm` | Delete task (by ID or fuzzy match) |
| `tui` | `t` | Open TUI |

## TUI Keys

| Key | Action |
|-----|--------|
| `t` | Today view |
| `w` | This Week view |
| `l` | Long Term view |
| `c` | Completed view |
| `j`/`k` | Navigate up/down |
| `s` | Start timer on selected |
| `p` | Pause timer |
| `d` | Mark done |
| `r` | Refresh |
| `q` | Quit |

## Installation

```bash
cargo install --path .
```

## Testing

```bash
cargo test
```

67 tests cover notation parsing, fuzzy scoring, and the full task lifecycle including estimates, elapsed time, notes, and editing. See [USAGE.md](USAGE.md) for the scenarios these tests exercise.

## Config

Data stored in:
- Database: `~/.local/share/dodo/dodo.db`
- Config: `~/.config/dodo/config.toml`

## License

MIT
