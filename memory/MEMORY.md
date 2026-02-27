# Dodo Project Memory

## Unicode Safety Rules
- Never use `&s[..s.len()-1]` or `&s[n..m]` with byte offsets derived from `.len()` on user-input strings.
- Always guard with `last_byte.is_ascii()` before byte-slicing, OR use `.get(n..m)` (returns Option), OR collect `.chars()` into a Vec and index that.
- Priority blocks (■ = U+25A0) are 3 bytes in UTF-8. Marquee scrolling must use `.chars().count()` not `.len()`.
- `split_note_entries` uses `line.get(1..5)` to safely check timestamp prefix.

## Key Bugs Fixed
- **Korean text panic** (notation.rs `parse_relative_date` + `parse_recurrence`): byte-level slicing `&s[..s.len()-1]` panics on multi-byte chars. Fix: guard with `last_byte.is_ascii()` before slicing.
- **Notation tokens in title** (`prepare_task` fallback): when ALL input is recognized notation tokens and no plain-text title remains, old code fell back to raw_input (showing token strings in title). Fix: only use raw_input fallback when `!parsed.has_updates()`.
- **Recurring tab red dates**: `build_compact_meta` showed `=Feb13` scheduled anchor date as red (past). Fix: use new `build_template_meta` that only shows estimate/project/context, not scheduled/deadline.
- **DONE pane stats**: refactored `build_pane_stats` to return `Vec<Span<'static>>` — on-time count green, overdue count red, no text labels.
- **CPU at idle / animation speed coupling**: animations now use `app.anim_frame()` = wall-clock time (`start_time.elapsed().as_millis() / 16`) instead of render `frame_count`. This decouples animation speed from fps. Poll rate: 33ms (30fps) when running task, 100ms (10fps) when idle.
- **Marquee meta byte-slice panic**: was using `.len()` (byte count) for marquee scroll offsets; priority ■ blocks and CJK names are multi-byte. Fixed: use `.chars().count()` and `Vec<char>` indexing throughout.
- **Note entry byte-slice panic**: `split_note_entries` did `line[1..5]` which panics if note starts with `[` + emoji/CJK. Fixed with `line.get(1..5)`.

## Architecture Notes
- TUI is binary-only (not in lib), in `src/tui/` with `pub(super)` visibility throughout.
- `build_template_meta` in draw.rs: minimal meta for recurring tab (estimate + project + context only).
- `build_compact_meta` in draw.rs: full meta for regular task rows (includes scheduled/deadline with color coding).
- `build_pane_stats` returns `Vec<Span<'static>>` (changed from String). Draw site in `draw_pane` constructs the line manually with a leading space span.

## Testing
- `cargo test` runs 215 tests total. All pass.
- Notation tests in `tests/notation_test.rs` include `unicode_no_panic` and `unicode_with_notation` cases.
