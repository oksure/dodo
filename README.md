<div align="center">

# dodo

**Keyboard-first todo + time tracker CLI in Rust**

[![Crates.io](https://img.shields.io/crates/v/dodo.svg)](https://crates.io/crates/dodo)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Build](https://img.shields.io/github/actions/workflow/status/oksure/dodo/ci.yml?branch=main)](https://github.com/oksure/dodo/actions)

Single-binary CLI with a built-in TUI. Add tasks with inline notation, track time, and sync across devices.

</div>

---

## Features

- **Inline notation** -- `+project @context #tag ~2h ^friday =wed !!!` parsed directly from input
- **Numeric task IDs** -- short IDs (1, 2, 3) for quick selection
- **Fuzzy matching** -- ranked search (exact > prefix > word > substring)
- **Date-based areas** -- scheduled dates drive grouping into Today, This Week, Long Term
- **Time tracking** -- start/stop toggle, elapsed vs estimate display
- **Recurring tasks** -- template-based with `*daily`, `*weekly`, `*mon,wed,fri`, etc.
- **Four-view TUI** -- Panes, Daily, Weekly, Calendar views with full keyboard control
- **Report tab** -- productivity stats, streak counter, sparkline charts, weekday activity
- **Cloud sync** -- optional Turso embedded replica with configurable auto-sync
- **S3 backup** -- compressed backups to any S3-compatible storage with auto-prune

## Quick Install

```bash
# From crates.io
cargo install dodo

# From source
git clone https://github.com/oksure/dodo.git
cd dodo
cargo install --path .
```

## Quick Start

```bash
# Add tasks with inline notation (no quotes needed)
dodo a fix login bug +backend @laptop ~2h ^friday !!!

# List all groups
dodo ls

# Start timer by ID or fuzzy match
dodo s 1
dodo s login

# Pause running task
dodo s

# Complete task
dodo d

# Edit task metadata
dodo e 1 ~3h ^tmr +frontend

# Add notes
dodo n 1

# Open TUI (default command)
dodo
```

## Inline Notation

Add metadata directly in task text -- no flags needed:

| Symbol | Meaning | Example | Notes |
|--------|---------|---------|-------|
| `+word` | Project | `+backend` | Single (last wins) |
| `@word` | Context | `@john @laptop` | Multiple |
| `#word` | Tag | `#urgent #bug` | Multiple |
| `~dur` | Estimate | `~2h30m` | `m` `h` `d`(8h) `w`(40h) |
| `^date` | Deadline | `^friday` `^tmr` `^0215` | Named, relative, MMDD, ISO |
| `=date` | Scheduled | `=wed` `=2w` | Same formats as deadline |
| `!`--`!!!!` | Priority | `!!!` | 1--4 (4 = most urgent) |
| `*pattern` | Recurrence | `*daily` `*2w` `*mon,fri` | For recurring templates |

Notation tokens are extracted and the remaining text becomes the title.

## Commands

| Command | Short | Description |
|---------|-------|-------------|
| `add` | `a` | Add new task with inline notation |
| `list` | `ls` | List tasks by area (no args = all groups) |
| `start` | `s` | Start/pause timer (no args = pause) |
| `done` | `d` | Complete task (no args = running task) |
| `status` | `st` | Show running task + elapsed time |
| `edit` | `e` | Edit task metadata via notation |
| `note` | `n` | Add/view/edit notes on a task |
| `remove` | `rm` | Delete task by ID or fuzzy match |
| `move` | `mv` | Move task between areas |
| `recurring` | `rec` | Manage recurring templates |
| `report` | `rp` | Productivity reports (day/week/month/year/all) |
| `config` | `cfg` | Show/edit configuration |
| `sync` | | Manage Turso sync |
| `backup` | | Manage S3 backups |

Running `dodo` with no command launches the TUI.

## TUI Keyboard Shortcuts

### Global

| Key | Action |
|-----|--------|
| `t` | Tasks tab (or jump to today) |
| `c` | Recurring tab |
| `r` | Report tab |
| `,` | Settings tab |
| `Tab` / `Shift+Tab` | Cycle tabs |
| `y` | Sync now |
| `?` | Help |
| `q` / `Esc` | Quit |

### Tasks Tab -- All Views

| Key | Action |
|-----|--------|
| `a` | Add new task |
| `s` | Start/pause timer |
| `d` | Toggle done/undone |
| `n` | Open notes |
| `Enter` | Edit task detail |
| `Backspace` | Delete task |
| `+` / `-` | Shift scheduled date +/- 1 day |
| `v` / `V` | Next/previous view |
| `/` | Search/filter |

### Tasks Tab -- Panes View

| Key | Action |
|-----|--------|
| `h` / `l` | Move between panes |
| `j` / `k` | Navigate tasks |
| `G` / `gg` | Jump to last/first task |
| `o` | Cycle sort mode |
| `<` / `>` | Quick-move task between panes |
| `m` | Move task (pick target) |

### Tasks Tab -- Daily View

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate tasks (skips headers) |
| `t` | Jump to today |
| `o` | Cycle sort mode |

### Tasks Tab -- Weekly View

| Key | Action |
|-----|--------|
| `h` / `l` | Move between day tiles |
| `j` / `k` | Navigate tasks in tile |
| `[` / `]` | Previous/next week |
| `t` | Jump to today |

### Tasks Tab -- Calendar View

| Key | Action |
|-----|--------|
| `h` `j` `k` `l` | Navigate days |
| `[` / `]` | Previous/next month |
| `t` | Jump to today |
| `Tab` | Switch to task list |
| `Esc` | Back to grid |

### Recurring Tab

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate templates |
| `a` | Add template |
| `e` / `Enter` | Edit template |
| `d` | Delete template |
| `p` | Pause/resume |
| `g` | Generate instances |
| `G` / `gg` | Jump to last/first |

### Report Tab

| Key | Action |
|-----|--------|
| `h` / `l` | Change range (Day/Week/Month/Year/All) |

### Settings Tab

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate backups |
| `u` | Upload backup |
| `r` | Restore backup |
| `d` | Delete backup |
| `s` | Trigger sync |
| `e` | Edit config |

### Search / Filter

| Token | Matches |
|-------|---------|
| `+backend` | Project contains "backend" |
| `@laptop` | Context contains "laptop" |
| `!!` | Priority >= 2 |
| `^<3d` | Deadline within 3 days |
| `=<1w` | Scheduled within 1 week |
| `keyword` | Title contains "keyword" |

All tokens are AND-ed. Filter persists after closing search bar.

## Configuration

Config file: `~/.config/dodo/config.toml`

```toml
[sync]
enabled = true
turso_url = "libsql://mydb.turso.io"
turso_token = "your-token"
sync_interval = 10  # minutes

[backup]
enabled = true
endpoint = "https://s3.example.com"
bucket = "my-bucket"
prefix = "dodo/"
access_key = "your-key"
secret_key = "your-secret"
region = "us-east-1"       # optional
schedule_days = 7
max_backups = 10

[preferences]
week_start = "monday"      # or "sunday"
```

Environment variable fallbacks: `DODO_TURSO_TOKEN`, `DODO_S3_ACCESS_KEY`, `DODO_S3_SECRET_KEY`

Data: `~/.local/share/dodo/dodo.db`

## Testing

```bash
cargo test
```

179 tests covering notation parsing (61), fuzzy scoring (8), config parsing (27), backup formatting (25), and workflow integration (58) including recurring tasks, sync merge, time tracking, and note editing. See [USAGE.md](USAGE.md) for real-world scenarios.

## Contributing

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Run tests (`cargo test`)
4. Submit a pull request

## License

MIT
