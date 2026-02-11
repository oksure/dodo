# Usage Guide

Real-world workflows for dodo, from simple daily use to structured productivity systems.

---

## 1. Simple Daily List

The most basic use: dump tasks, knock them out.

```bash
dodo add "Buy groceries"
dodo add "Reply to Sarah's email"
dodo add "Fix leaky faucet"
dodo ls
# [3] [ ] TODAY Fix leaky faucet
# [2] [ ] TODAY Reply to Sarah's email
# [1] [ ] TODAY Buy groceries

dodo start 1
dodo done
# Completed: Buy groceries (0m)
```

No system. No framework. Just a list and a timer.

---

## 2. Pomodoro Technique

The Pomodoro method uses 25-minute focused work blocks. Dodo's start/pause/status cycle maps directly to this.

```bash
# Morning: plan your pomodoros
dodo add "Draft blog post"
dodo add "Review PR #42"
dodo add "Update dependencies"

# Pomodoro 1: start a task and focus
dodo start "blog"
# ... work for 25 minutes ...
dodo status
# Running: Draft blog post (25m)
dodo pause
# Take a 5-minute break

# Pomodoro 2: continue or switch
dodo start "blog"
# ... another 25 minutes ...
dodo done
# Completed: Draft blog post (50m)

# Pomodoro 3: next task
dodo start 2
```

The elapsed time in `status` and `done` gives you natural pomodoro tracking without a separate timer app. Fuzzy matching (`"blog"`) means you never type full titles.

---

## 3. Getting Things Done (GTD)

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
dodo add "Call dentist"
dodo add "Research new laptop"
dodo add "Prepare quarterly report" --project work
dodo add "Learn Rust macros"
```

### Clarify and organize

Move items to their proper horizon:

```bash
# "Research new laptop" is someday/maybe
dodo rm "laptop"
dodo add "Research new laptop" --area long

# "Learn Rust macros" is a this-week goal
dodo rm "macros"
dodo add "Learn Rust macros" --area week
```

### Review by area

```bash
# Weekly review: check all horizons
dodo ls long    # Someday/maybe — anything to promote?
dodo ls week    # This week — on track?
dodo ls today   # Next actions — what's left?
```

### Work with contexts

GTD uses contexts (@phone, @computer, @errands) to batch similar actions:

```bash
dodo add "Call dentist" --context phone
dodo add "Call insurance" --context phone
dodo add "Order cables" --context computer
dodo add "Pick up dry cleaning" --context errands
```

The context shows in the task display: `[1] [ ] TODAY Call dentist @phone`

---

## 4. Eisenhower Matrix

The Eisenhower matrix sorts tasks by urgency and importance. Map it to dodo areas:

| Quadrant | Dodo Area | Meaning |
|---|---|---|
| Urgent + Important | `today` | Do it now |
| Important, Not Urgent | `week` | Schedule it |
| Urgent, Not Important | `today` | Delegate or do fast |
| Neither | `long` | Maybe later, maybe never |

```bash
# Urgent + Important: client deadline
dodo add "Fix production bug" --project acme

# Important, not urgent: strategic work
dodo add "Write test suite" --area week --project acme

# Not important, but soon: quick chores
dodo add "Update Slack status"

# Neither: backlog
dodo add "Refactor auth module" --area long --project acme
```

Weekly review: scan `dodo ls long` and ask "has this become urgent?" If so, promote it.

---

## 5. Time-Tracked Freelancing

Freelancers billing by the hour can use projects to separate client work:

```bash
# Set up tasks per client
dodo add "Design landing page" --project clientA --context design
dodo add "API integration" --project clientB --context dev
dodo add "Write copy for homepage" --project clientA --context writing

# Work and track
dodo start "landing"
# ... work ...
dodo pause
# Timer paused.

dodo start "API"
# ... work ...
dodo done
# Completed: API integration (2h 15m)
```

The elapsed times reported by `done` and `status` give you per-task time data.

---

## 6. Numeric IDs for Speed

When your list is on screen, use numeric IDs instead of typing titles:

```bash
dodo ls
# [3] [ ] TODAY Fix production bug +acme
# [2] [ ] TODAY Write copy for homepage +clientA @writing
# [1] [ ] TODAY Design landing page +clientA @design

# Fast: start by number
dodo start 3

# Fast: remove by number
dodo rm 1
```

Numeric IDs survive across sessions. They auto-increment and never reuse.

---

## 7. Fuzzy Matching in Practice

You rarely need to type a full task title. Dodo ranks matches by quality:

```bash
dodo add "Write quarterly report"
dodo add "Write unit tests"
dodo add "Review writing style guide"

# Exact match wins
dodo start "Write unit tests"

# Prefix match: "write" matches "Write quarterly report" first
dodo start "write"

# Substring: "report" finds "Write quarterly report"
dodo start "report"

# Word match: "unit" finds "Write unit tests"
dodo start "unit"
```

Ranking: exact (100) > prefix (75) > word-start (60) > substring (50) > word-contains (40).

---

## 8. Academic / Research Workflow

For students or researchers managing papers, courses, and deadlines:

```bash
# Long-term goals
dodo add "Read 'Designing Data-Intensive Applications'" --area long --project thesis
dodo add "Learn category theory basics" --area long --project coursework

# This week's commitments
dodo add "Write literature review draft" --area week --project thesis --context writing
dodo add "Problem set 5" --area week --project coursework

# Today's actions
dodo add "Email advisor re: chapter outline" --project thesis --context email
dodo add "Fix citation formatting" --project thesis --context writing

# Track deep work sessions
dodo start "literature"
# ... 2 hours of focused writing ...
dodo done
# Completed: Write literature review draft (2h 3m)
```

---

## 9. TUI for Planning Sessions

When you need to survey and reorganize, the TUI gives a Blitzit-style view:

```bash
dodo tui
```

Keys:
- `t` / `w` / `l` — switch between Today, This Week, Long Term
- `j` / `k` — navigate up/down
- `s` — start timer on selected task
- `p` — pause
- `d` — mark done
- `q` — quit

Use it for morning planning: scan Long Term, promote items to This Week, then pick today's focus.
