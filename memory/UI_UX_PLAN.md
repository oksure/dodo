# Dodo TUI — UI/UX Overhaul Plan

> Generated: March 1, 2026  
> Based on full analysis of `src/tui/draw.rs`, `src/tui/state.rs`, `src/tui/constants.rs`, `src/tui/format.rs`, `src/tui/event.rs`

---

## 1. Bug Fixes (Correctness Before Polish)

**1a. `build_compact_meta` double-renders estimate**
In `draw.rs`, when `elapsed > 0` and an estimate exists, the function shows a countdown like `(23m left)` AND then also falls through to render `~1h` because the estimate block has no `elapsed == 0` guard. The `~estimate` span should only render when there is no elapsed time.

**1b. Calendar title renders into grid area**
`draw.rs` renders `title_line` to `layout[2]` (the grid `Rect`) instead of `layout[0]` (the title row), causing the month/year label to overlap the first row of calendar cells. Fix: render title to `title_cols[0]`, not `layout[2]`.

**1c. Done tasks show timer info**
`build_compact_meta` shows "Xm left" or "+Xm over" for `Done` tasks that have `elapsed_seconds`. A done task should show neither a countdown nor an overage.

**1d. View-selector pad calculation uses byte length**
`draw_view_selector` uses `s.content.len()` (byte count) to compute padding, not `.chars().count()`. Multi-byte characters (e.g., the `●` bullet U+25CF = 3 bytes) cause misalignment.

**1e. `draw_pane` duplicates all task-rendering logic from `build_task_list_item`**
Both functions independently implement marquee, neon, meta, and highlight logic. `draw_pane` should call `build_task_list_item` instead of duplicating it.

---

## 2. Layout & Screen Real-Estate

**2a. Search bar: always-on 3-line slot wastes space**
`search_height = 3` is always allocated on the Tasks tab. Collapse it to 1 line when inactive (just a muted hint "/" inline), expand to 3 lines only when `AppMode::Search` is active. Saves 2 rows most of the time.

**2b. View selector: reduce to 1 line**
The view selector is 2 lines but carries ~40 chars of content. Combine it with the tab bar row or collapse it to a single status line: `● Panes | Daily | Weekly | Calendar`.

**2c. Weighted pane widths in Panes view**
Equal `25%×4` wastes the TODAY pane (busiest) and over-allocates LONG TERM (often small). Adjust to: LONG TERM 18%, THIS WEEK 22%, TODAY 35%, DONE 25%.

**2d. Add bar overlaps the footer**
`draw_add_bar` places itself at `area.height - 4`, which covers the 1-line footer at the absolute bottom. The bar should be placed above the footer row (`area.height - footer_height - bar_height`), or the outer layout's footer coordinate should be used.

---

## 3. Header Simplification

The header tries to pack: app name, running task title, countdown timer, sync status indicator, and a 14-token symbol legend — all on one 2-line slot.

**3a. Move legend to the help modal / footer hint**
Remove the always-visible symbol legend from the header. Users learn the notation quickly. Replace it with a compact right-side status line showing only the sync state (e.g., `● sync 2m ago`) and today's date.

**3b. Running task header: shorten**
When a task is running, show: `▶ task-title  ⏱ 23m left`. Strip the leading space-padded decoration and the extra animation phase that oscillates between 3 green tones (distracting, not informative).

**3c. Sync indicator: less verbose**
`● synced y:sync` → `● synced` with a smaller tooltip. The `y:sync` keybinding hint belongs in the footer, not the header.

---

## 4. Task Row Improvements

**4a. Tags are stored but never rendered**
`build_compact_meta` renders priority, project, contexts, elapsed/estimate, scheduled, deadline — but silently drops `#tags`. Add tag rendering (e.g., `#urgent #frontend`) in `ACCENT_PEACH` after contexts.

**4b. Meta row indent alignment**
The meta row uses a hard-coded `"       "` (7 spaces) prefix, which doesn't align with the actual content column. Compute it from `prefix_width` so it always lines up under the task title.

**4c. Visual distinction for overdue `=scheduled` items**
Overdue scheduled dates use a red background badge `=Mar01`. A consistent style should be defined for all "date-overdue" cases (scheduled past due, deadline past due) rather than the current ad-hoc checks scattered in `build_compact_meta`.

**4d. Empty pane state**
When a pane has 0 tasks, render a centered empty-state message: `(empty)  a:add task` in `FG_OVERLAY` color. Currently it just shows a blank box.

---

## 5. Footer Adaptability

**5a. Footer overflows narrow terminals**
In `TasksView::Panes`, the footer has 13 key-action pairs. At 80 columns this wraps or clips. Add a `width`-aware priority system: show the most important keys first and truncate with `…` if the terminal is too narrow. Priority order: `a add  s start  d done  n note  ↵ edit  ⌫ del  / find  ? help  q quit`.

**5b. Show active sort in footer**
The sort mode is shown right-aligned in the pane header. Also surface it briefly in the footer when `o` is pressed (toast-style).

---

## 6. Toast Notification System

Currently only Settings/backup has a status message. Task operations are silent. Add a global `app.toast: Option<(String, Instant, bool)>` (message, timestamp, is_error) rendered as a 1-line overlay near the bottom of the content area. Duration: 3 s for success, 6 s for errors. Show toasts for: task added, timer started/paused, task completed/uncompleted, task deleted, task moved. This removes the need for users to watch whether the pane changed.

---

## 7. Calendar View

**7a. Fix title overlap (bug 1b)**

**7b. Cells too small at common terminal sizes**
A 7-column calendar at 120 cols gives ~17 chars per cell minus borders = ~15 usable. The date number + task list competes for those. When cell height ≤ 2 rows, only show the date number + a count badge; suppress task list rendering.

**7c. Task count badge**
Instead of `(3)` in overlay gray next to the date number, render a colored pip count using `●` repeated up to 5 times (excess as `+N`) at the bottom of the cell. Colors: green (pending), red (overdue), teal (done).

**7d. Calendar task list panel**
When `CalendarFocus::TaskList` is active, the task list shows on the right side of the selected day cell. Consider rendering it in a side panel below the calendar grid instead (using `Constraint::Min(0)` for grid + `Constraint::Length(8)` for panel), so it doesn't overlap.

---

## 8. Daily View

**8a. Cursor skips non-task entries**
Date headers are in `daily_entries` alongside tasks. Pressing `j`/`k` can land on a header entry. Actions (start, done, delete) then have no target. The navigation handlers should skip header entries automatically (or `event.rs` should make them visually unselectable with a different cursor indicator).

**8b. Section headers show task count but not scheduled vs overdue breakdown**
`(3)` tells you there are 3 tasks but not if any are overdue. Show `(3, 1 overdue)` in red when applicable.

---

## 9. Weekly View

**9a. No date-range header**
The weekly view shows 8 day tiles but there's no overall "Week of Mar 1" label. Add a 1-line title above the grid showing the week span.

**9b. `week_start_date` vs "today's week"**
The state has `week_start_date` but navigation with `[`/`]` to jump weeks isn't reflected in any header. Show current week range prominently.

---

## 10. Recurring Tab

**10a. Human-readable pattern display**
`*daily` → "Every day", `*weekly` → "Every week", `*mon,wed,fri` → "Mon, Wed, Fri", `*2w` → "Every 2 weeks". A `humanize_recurrence()` helper in `format.rs`.

**10b. Active instance status per template**
Show whether an active instance exists and its current status (pending/running/done) inline after the template title. Currently you have to switch back to the Tasks tab to see this.

---

## 11. Config Modal

**11a. Add a scrollbar indicator**
The config modal scrolls via `config_scroll` but there's no visual scrollbar or scroll position indicator. Add a right-side scrollbar using `ScrollbarState`.

**11b. Section header ranges as constants**
Section headers use hardcoded field index ranges in `draw_config_modal`. Extract them to `constants.rs` as:
```rust
pub(super) const CONFIG_SECTION_STARTS: [usize; 4] = [0, 4, 13, 18];
pub(super) const CONFIG_SECTION_NAMES: [&str; 4] = ["Sync", "Backup", "Preferences", "Email"];
```

---

## 12. Note Modal

**12a. Show note timestamps prominently**
Notes are stored with `[YYYY-MM-DD HH:MM]` prefixes. The current renderer just shows them as part of the text. Parse and style the timestamp in `FG_OVERLAY` and the note body in `FG_TEXT` for visual hierarchy.

**12b. Note entry count in task row**
The notes mark in task rows is `" *"` (a bare asterisk). Replace with `" ✎N"` (pencil + count) so users can see how many notes exist without opening the modal.

---

## 13. Report Tab

**13a. Done tasks list is not scrollable**
`draw_report_tab` renders the done tasks list as a `Paragraph`, which doesn't scroll. On a busy period there may be 20+ completed tasks but only 10 visible. Use a `List` with `ListState` and add `j`/`k` navigation.

**13b. Summary card second-line info is clipped on short terminals**
The second line of the summary card (avg/task, best hour, best day) is often cut off. Move those metrics into the bar-chart row as chart titles, or use a `Table` widget for the summary card.

**13c. Report loading latency**
Report data is computed synchronously on tab switch. On large databases this can cause a visible frame drop. Move computation to a background thread with a loading indicator.

---

## 14. Visual Polish

**14a. Selected item contrast**
The selected-item background `Color::Rgb(65, 75, 120)` is only ~16 Luma units above the base `Color::Rgb(49, 50, 68)`. Increase to `Color::Rgb(72, 84, 140)` for better legibility, especially over dim text.

**14b. Neon animation performance**
`apply_neon` allocates a new `Vec<Span>` per character per frame for the running task. At 60fps × ~100 chars = 6,000 heap allocations/second for one running task. Pre-compute the neon gradient as a color lookup table indexed by column position per frame and reuse it.

**14c. Consistent border types**
Some widgets use `BorderType::Rounded`, some use the default (straight). Standardize all bordered widgets to `BorderType::Rounded` via a helper `styled_block(title, color)` that encapsulates border type + style.

**14d. Inactive pane cursor visibility**
When focus is on pane 2 (TODAY), the selected row in pane 1 (THIS WEEK) still shows a highlight background. Inactive pane cursor should show a dimmer background or just a `FG_OVERLAY` left-bar `▏` indicator.

---

## 15. Keymap Rationalization

**15a. `<`/`>` for quick-move aren't in the footer**
The help modal (Tasks section) lists `< / >` for quick-move but the footer key list does not. Add `< >` as a footer hint or remove the discrepancy.

**15b. Calendar focus toggle is `Tab` (overloaded)**
The global `Tab` key cycles tabs. Within Calendar, a second `Tab` press switches focus to the task list. This is an overloaded key. Consider `Enter` to enter task-list focus within Calendar and `Esc` to return to grid.

**15c. `H` for hide/show done is undiscoverable**
It's in the footer for Daily/Weekly but not for Calendar's task list panel. Make it consistent.

---

## Implementation Priority

| Priority | Items | Complexity |
|---|---|---|
| P0 — bugs | 1a, 1b, 1c, 1d, 1e | Small — isolated fixes |
| P1 — high impact UX | 2a, 2d, 3a, 4a, 4d, 5a, 6 | Medium — layout + new toast state |
| P2 — visual coherence | 3b, 3c, 4b, 4c, 8a, 10a, 14a–14d | Medium — style + helpers |
| P3 — feature enrichment | 7b–7d, 9a, 10b, 11a, 12a, 12b, 13a | Larger — new state + views |
| P4 — perf + polish | 14b, 13c, 2b, 2c | Architecture changes |
