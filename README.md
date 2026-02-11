# Dodo CLI

A keyboard-first, Blitzit-inspired todo + time tracker CLI written in Rust.

## Features

- **Numeric task IDs** — short IDs (1, 2, 3) for quick selection
- **Fuzzy matching** — ranked search (exact > prefix > word > substring)
- **Keyboard-first** — tmux-style single-letter commands
- **Focus areas** — LongTerm, ThisWeek, Today, Completed
- **Time tracking** — Start, pause, complete tasks
- **TUI mode** — ratatui interface for browsing
- **Cloud sync** — R2/Dropbox support (optional)

## Quick Start

```bash
# Add a task (prints numeric ID)
dodo a "Write essay" +phd @uni --area week
# => Added: Write essay [#1]

# List today's tasks (shows numeric IDs)
dodo ls
# => [1] [ ] TODAY Write essay +phd @uni

# Start timer by numeric ID or fuzzy match
dodo s 1
dodo s essay

# Check status
dodo st

# Complete task
dodo d

# Remove by numeric ID or fuzzy match
dodo rm 1

# Open TUI
dodo tui
```

## Commands

| Command | Short | Description |
|---------|-------|-------------|
| `add` | `a` | Add new task |
| `list` | `ls` | List tasks (default: today) |
| `start` | `s` | Start timer on task |
| `pause` | `p` | Pause current timer |
| `done` | `d` | Complete running task |
| `status` | `st` | Show running task + elapsed time |
| `remove` | `rm` | Delete task (by ID or fuzzy match) |
| `tui` | `t` | Open TUI |

## Installation

```bash
cargo install --path .
```

## Testing

```bash
cargo test
```

28 tests cover fuzzy scoring, the full task lifecycle, and real-world workflows (GTD, Pomodoro, Eisenhower, freelancing). See [USAGE.md](USAGE.md) for the scenarios these tests exercise.

## Config

Data stored in:
- Database: `~/.local/share/dodo/dodo.db`
- Config: `~/.config/dodo/config.toml`

## License

MIT
