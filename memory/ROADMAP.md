# Dodo — What to Build Next

> Updated: March 1, 2026  
> Reflects completed UI/UX overhaul (commit 6158317) + bug-fix session (03d8352).

---

## Recently Completed (reference)

- P0 bugs: estimate double-render, calendar title overlap, done-task timer, view-selector byte-length
- Search bar: always-3-line slot, consistent focus/unfocus styling (no jump)
- Pane scroll: symmetric margin, `&mut PaneState` render (no clone offset loss)
- Report tab: time navigation (`[`/`J` prev period, `]`/`K` next period), 2-line header
- Help modal: LEGEND section (status icons + notation + duration)
- Header: moved symbol legend out, compact sync indicator, rainbow animation via wall-clock
- Empty pane state: `(empty)  a:add task` placeholder
- Selected-item contrast bump

---

## Near-Term (next 1–2 sessions)

### N1. Tags rendered in task rows  ← highest visible impact
`#tag` tokens are parsed and stored but silently dropped from `build_compact_meta`.  
Add tag spans in `ACCENT_PEACH` after contexts, before elapsed/estimate.  
Affects: `draw.rs::build_compact_meta`, all four panes + daily/weekly/calendar task items.

### N2. Toast notification system
Task operations (add, start, pause, complete, delete, move) are currently silent.
Users have to watch the pane to confirm something happened.

Add `app.toast: Option<(String, Instant, bool)>` (message, timestamp, is_error).  
Render as a 1-line overlay above the footer, auto-dismiss after 3 s (errors: 6 s).  
`draw_toast()` in `draw.rs`, set in all key handlers that mutate tasks in `event.rs`.

### N3. Daily view: cursor skips date headers
`j`/`k` can land the cursor on a date-header entry in `daily_entries`.
Actions (`s`, `d`, `n`, `Enter`) then have no task target — silent failure.
Fix: `handle_daily_nav` in `event.rs` should skip `DailyEntry::Header` variants.

### N4. Note count badge in task row
The current `" *"` asterisk after the title gives no quantity signal.
Replace with `" ✎N"` (Unicode pencil U+270E + count) in `FG_OVERLAY`.
Affects `build_compact_meta` and `build_task_list_item`.

### N5. Inactive pane cursor dimming
When focus is on TODAY, the selected row in THIS WEEK still shows the full highlight background.
Show inactive cursor as a dim `▏` left-bar indicator (`FG_OVERLAY`, no background fill).
Affects `draw_pane`; check `is_active` boolean for style selection.

---

## Medium-Term (next major session)

### M1. Calendar view polish (bug 7b–7d)

**Count badge instead of task list in tiny cells**  
When cell height ≤ 2 rows, suppress task rendering, show only the date number + a pip count:  
`● ● ● +2` in green/red/teal (pending/overdue/done). Currently the task list overflows the cell.

**Side panel for selected day's tasks**  
When CalendarFocus::TaskList is active, render tasks in a fixed-height panel below the grid instead of overlapping the cell. Layout: `Constraint::Min(0)` (grid) + `Constraint::Length(8)` (panel).

**Tab key overload**  
Global `Tab` cycles tabs; within Calendar a second `Tab` enters TaskList focus.  
Change: `Enter` enters TaskList focus, `Esc` returns to grid.

### M2. Recurring tab improvements (10a, 10b)

**Human-readable patterns**  
`*daily` → "Every day", `*mon,wed,fri` → "Mon, Wed, Fri", `*2w` → "Every 2 weeks".  
New helper `humanize_recurrence(pattern: &str) -> String` in `format.rs`.

**Active instance status per template**  
Show whether an active instance exists and its status (○ pending / ▶ running / ⏸ paused) inline
after the template row. Currently you must switch to Tasks to check.

### M3. Report tab scrollable done list (13a)
`draw_report_tab` renders completed tasks as a `Paragraph` — no scrolling possible.
Convert to a `List` with `ListState` (add `report_done_scroll: ListState` to `App`).
Add `j`/`k` navigation when hovering the done-tasks panel. Document in help modal.

### M4. Footer width adaptability (5a)
At terminals narrower than ~100 cols, the footer's 13 key-action pairs wrap or clip.
Implement a priority-aware truncation: compute available width, greedily include pairs from
priority order: `a add  s start  d done  n note  ↵ edit  ⌫ del  / find  ? help  q quit`.
Truncate with `…` when the next pair won't fit.

### M5. Note timestamp visual hierarchy (12a)
Notes are stored as `[YYYY-MM-DD HH:MM] body text`. The current renderer treats the whole
string as plain text. Parse the `[…]` prefix, render it in `FG_OVERLAY`, body in `FG_TEXT`.
Add a horizontal separator line between note entries.

### M6. Config modal scrollbar (11a)
The config modal has 23 fields but no scroll position indicator.
Add a `Scrollbar` widget on the right edge using `ScrollbarState` driven by `app.config_scroll`.

---

## Longer-Term (feature-level)

### L1. Export / import
- `dodo export --json` / `--csv` for all tasks + sessions  
- `dodo import <file>` for bulk add from exported JSON or simple CSV  
- Useful for migration, backup scripts, and data analysis outside the TUI

### L2. Natural language date input improvements
- Relative weekday with ordinal: `^next fri`, `^3rd tue`
- Business-day awareness: `^3bd` (3 business days from today)
- Time-of-day in scheduled: `=mon@09:00` (anchor to calendar slot)

### L3. Keyboard shortcut customization
Store a `[keybindings]` section in `config.toml` that overrides default key mappings.
Useful for users who have muscle memory from other tools (vim, emacs, taskwarrior).

### L4. Web/terminal sharing
- `dodo serve` — lightweight HTTP server exposing a read-only JSON API of today's tasks  
- Useful for scripting integrations (status bar widgets, Raycast, Alfred, Home-screen shortcuts)

### L5. Smart scheduling suggestions
When adding a task with no `=scheduled`, analyze:
- The user's historical peak productivity hours (from `report_by_hour`)
- Current TODAY pane load (estimate sum vs available hours)
- Suggest: "Add to THIS WEEK? TODAY looks full (6h estimated)."

### L6. Context / project summary view
A new TUI tab (or modal) showing projects and contexts as aggregated rows:  
`+backend  5 tasks  3h done  2h remaining  2 running`  
Drill-down with `Enter` to filter to that project in the main pane.

---

## Technical Debt

| Item | File | Note |
|---|---|---|
| `build_compact_meta` + `build_task_list_item` duplication | `draw.rs` | Plan item 1e — `draw_pane` should call shared helper |
| Neon animation heap pressure | `draw.rs::apply_neon` | Pre-compute color LUT per (frame, col) — ~6k allocs/s at 60fps |
| Meta row indent hardcoded 7-space prefix | `draw.rs::build_compact_meta` | Compute from actual `prefix_width` |
| Config section index ranges hardcoded in draw | `draw.rs` | Extract to `constants.rs::CONFIG_SECTION_STARTS` |
| `< >` keys missing from footer | `draw.rs::draw_footer_tasks` | Quick-move keys listed in help modal but not the footer |
| Report loading blocks render | `state.rs::refresh_report` | Move to background thread with loading spinner on tab switch |

---

## Implementation Priority Order

```
N1 (tags)  →  N2 (toasts)  →  N3 (daily nav)  →  N4 (note badge)  →  N5 (inactive cursor)
   ↓
M1 (calendar)  →  M2 (recurring)  →  M3 (report scroll)  →  M4 (footer width)
   ↓
L1 (export)  →  L4 (serve)  →  L2 (NL dates)  →  L6 (project view)
```

Technical debt items can be done opportunistically alongside feature work.
