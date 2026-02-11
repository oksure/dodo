# Usage Guide

Real-world workflows for dodo, from simple daily use to structured productivity systems.

---

## 1. Simple Daily List

The most basic use: dump tasks, knock them out.

```bash
dodo a Buy groceries
dodo a Reply to Sarah email
dodo a Fix leaky faucet
dodo ls
# [3] [ ] TODAY Fix leaky faucet
# [2] [ ] TODAY Reply to Sarah email
# [1] [ ] TODAY Buy groceries

dodo s 1
dodo d
# Completed: Buy groceries (0m)
```

No system. No framework. Just a list and a timer.

---

## 2. Inline Notation

Add metadata directly in the task text — no flags or quotes needed:

```bash
# Project, context, estimate, and deadline in one line
dodo a fix login bug +backend @john #urgent ~2h $friday

# Multiple contexts and tags
dodo a team standup @john @sarah #meeting ~30m ^mon

# Estimate with composite duration
dodo a redesign homepage +frontend ~2d4h $2w

# Deadline with specific date
dodo a submit tax return #personal $4/15
```

### Symbol reference

| Symbol | Meaning | Examples |
|--------|---------|---------|
| `+word` | Project | `+backend`, `+client-a` |
| `@word` | Context/person | `@john`, `@phone` |
| `#word` | Tag | `#urgent`, `#bug` |
| `~dur` | Time estimate | `~30m`, `~1h`, `~1h30m`, `~1d` (8h), `~1w` (40h) |
| `$date` | Deadline | `$today`, `$tmr`, `$fri`, `$3d`, `$1/15`, `$2025-06-01` |
| `^date` | Scheduled start | `^wed`, `^2w`, `^tmr` |

Tokens are extracted from anywhere in the input. The remaining text becomes the title.

---

## 3. Editing Tasks

Update any task's metadata without recreating it:

```bash
# Change estimate and deadline by numeric ID
dodo e 1 ~3h $tmr

# Add a project tag by fuzzy match
dodo e fix bug +backend

# Move task to a different area
dodo e 3 --area week

# Change contexts
dodo e 1 @sarah @mike
```

---

## 4. Task Notes

Add timestamped notes to any task:

```bash
# View existing notes
dodo n 1 --show

# Add a note (interactive, Ctrl+D to finish)
dodo n 1
# Notes for: fix login bug
# Enter note (Ctrl+D to finish):
# Found the root cause — session token not refreshing
# ^D
# Note added to: fix login bug

# Clear all notes
dodo n 1 --clear
```

Notes are stored with timestamps like `[2025-01-15 14:30] Found the root cause...`

---

## 5. Pomodoro Technique

The Pomodoro method uses 25-minute focused work blocks. Dodo's start/pause/status cycle maps directly to this.

```bash
# Morning: plan your pomodoros
dodo a Draft blog post ~2h
dodo a Review PR 42 ~30m
dodo a Update dependencies ~1h

# Pomodoro 1: start a task and focus
dodo s blog
# ... work for 25 minutes ...
dodo st
# Running: Draft blog post (25m)
dodo p
# Take a 5-minute break

# Pomodoro 2: continue or switch
dodo s blog
# ... another 25 minutes ...
dodo d
# Completed: Draft blog post (50m)

# Pomodoro 3: next task
dodo s 2
```

The elapsed time in `status` and `done` gives you natural pomodoro tracking without a separate timer app. Fuzzy matching (`blog`) means you never type full titles.

---

## 6. Getting Things Done (GTD)

David Allen's GTD framework captures everything, then organizes by actionability. Dodo's four areas map to GTD horizons:

| GTD Concept | Dodo Area | Usage |
|---|---|---|
| Someday/Maybe | `--area long` | Ideas, aspirations, "would be nice" |
| Active Projects | `--area week` | Committed work for this week |
| Next Actions | `--area today` (default) | Do it now |
| Done | automatic | Completed via `dodo done` |

### Capture everything (inbox zero)

```bash
# Brain dump — everything goes to today by default
dodo a Call dentist @phone
dodo a Research new laptop #personal
dodo a Prepare quarterly report +work ~4h
dodo a Learn Rust macros #learning
```

### Clarify and organize

Move items to their proper horizon using edit:

```bash
# "Research new laptop" is someday/maybe
dodo e laptop --area long

# "Learn Rust macros" is a this-week goal
dodo e macros --area week
```

### Review by area

```bash
# Weekly review: check all horizons
dodo ls long    # Someday/maybe — anything to promote?
dodo ls week    # This week — on track?
dodo ls today   # Next actions — what's left?
dodo ls done    # Completed — what did I finish?
```

### Work with contexts

GTD uses contexts (@phone, @computer, @errands) to batch similar actions:

```bash
dodo a Call dentist @phone
dodo a Call insurance @phone
dodo a Order cables @computer
dodo a Pick up dry cleaning @errands
```

The context shows in the task display: `[1] [ ] TODAY Call dentist @phone`

---

## 7. Eisenhower Matrix

The Eisenhower matrix sorts tasks by urgency and importance. Map it to dodo areas:

| Quadrant | Dodo Area | Meaning |
|---|---|---|
| Urgent + Important | `today` | Do it now |
| Important, Not Urgent | `week` | Schedule it |
| Urgent, Not Important | `today` | Delegate or do fast |
| Neither | `long` | Maybe later, maybe never |

```bash
# Urgent + Important: client deadline
dodo a Fix production bug +acme #urgent $today

# Important, not urgent: strategic work
dodo a Write test suite +acme ~4h --area week

# Not important, but soon: quick chores
dodo a Update Slack status ~5m

# Neither: backlog
dodo a Refactor auth module +acme --area long
```

Weekly review: scan `dodo ls long` and ask "has this become urgent?" If so, promote it with `dodo e refactor --area today`.

---

## 8. Time-Tracked Freelancing

Freelancers billing by the hour can use projects to separate client work:

```bash
# Set up tasks per client with estimates
dodo a Design landing page +clientA @design ~4h
dodo a API integration +clientB @dev ~8h
dodo a Write copy for homepage +clientA @writing ~2h

# Work and track
dodo s landing
# ... work ...
dodo p
# Timer paused.

dodo s API
# ... work ...
dodo d
# Completed: API integration (2h 15m)
```

The elapsed times reported by `done` and `status` give you per-task time data. Estimates help you plan: `(45m/4h)` shows progress at a glance.

---

## 9. Numeric IDs for Speed

When your list is on screen, use numeric IDs instead of typing titles:

```bash
dodo ls
# [3] [ ] TODAY Fix production bug +acme #urgent ~2h $Feb11
# [2] [ ] TODAY Write copy +clientA @writing ~2h
# [1] [ ] TODAY Design landing page +clientA @design ~4h

# Fast: start by number
dodo s 3

# Fast: edit by number
dodo e 1 ~6h $fri

# Fast: remove by number
dodo rm 1
```

Numeric IDs survive across sessions. They auto-increment and never reuse.

---

## 10. Fuzzy Matching in Practice

You rarely need to type a full task title. Dodo ranks matches by quality:

```bash
dodo a Write quarterly report
dodo a Write unit tests
dodo a Review writing style guide

# Exact match wins
dodo s Write unit tests

# Prefix match: "write" matches "Write unit tests" first
dodo s write

# Substring: "report" finds "Write quarterly report"
dodo s report

# Word match: "unit" finds "Write unit tests"
dodo s unit
```

Ranking: exact (100) > prefix (75) > word-start (60) > substring (50) > word-contains (40).

---

## 11. Academic / Research Workflow

For students or researchers managing papers, courses, and deadlines:

```bash
# Long-term goals
dodo a Read DDIA +thesis --area long
dodo a Learn category theory basics +coursework --area long

# This week's commitments with estimates
dodo a Write literature review draft +thesis @writing ~8h --area week
dodo a Problem set 5 +coursework ~3h --area week

# Today's actions with deadlines
dodo a Email advisor re chapter outline +thesis @email $fri
dodo a Fix citation formatting +thesis @writing ~1h

# Track deep work sessions
dodo s literature
# ... 2 hours of focused writing ...
dodo d
# Completed: Write literature review draft (2h 3m)

# Add research notes
dodo n 1
# Found relevant paper: Smith et al. 2023
# ^D
```

---

## 12. TUI for Planning Sessions

When you need to survey and reorganize, the TUI gives a Blitzit-style view:

```bash
dodo tui
```

Keys:
- `t` / `w` / `l` / `c` — switch between Today, This Week, Long Term, Completed
- `j` / `k` — navigate up/down
- `s` — start timer on selected task
- `p` — pause
- `d` — mark done
- `r` — refresh
- `q` — quit

Use it for morning planning: scan Long Term, promote items to This Week, then pick today's focus. Check Completed to review what you've accomplished.
