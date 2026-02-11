# Dodo CLI

A keyboard-first, Blitzit-inspired todo + time tracker CLI written in Rust.

## Features

- **No task IDs** — all fuzzy matching
- **Keyboard-first** — tmux-style single-letter commands
- **Focus areas** — LongTerm, ThisWeek, Today, Completed
- **Time tracking** — Start, pause, complete tasks
- **TUI mode** — ratatui interface for browsing
- **Cloud sync** — R2/Dropbox support (optional)

## Quick Start

```bash
# Add a task
dodo a "Write essay" +phd @uni --area week

# List today's tasks
dodo ls

# Start timer
dodo s essay

# Check status
dodo st

# Complete task
dodo d

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
| `remove` | `rm` | Delete task (fuzzy match) |
| `tui` | `t` | Open TUI |

## Installation

```bash
cargo install --path .
```

## Config

Data stored in:
- Database: `~/.local/share/dodo/dodo.db`
- Config: `~/.config/dodo/config.toml`

## License

MIT
