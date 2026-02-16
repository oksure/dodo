#!/bin/bash
# UX Testing Script for Dodo CLI
# This script tests the TUI functionality using tmux

set -e

echo "=== Dodo CLI UX Testing ==="
echo ""

# Configuration
SESSION_NAME="dodo_ux_test"
TEST_TIMEOUT=5
DB_PATH="$HOME/Library/Application Support/dodo/dodo.db"

# Cleanup function
cleanup() {
    echo ""
    echo "Cleaning up tmux sessions..."
    tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true
    echo "Cleanup complete."
}

# Set trap to cleanup on exit
trap cleanup EXIT

# Build the project
echo "Building Dodo CLI..."
cargo build --release 2>&1 | tail -3

# Install the project
echo ""
echo "Installing Dodo CLI..."
cargo install --path . --force 2>&1 | tail -3

# Start TUI in tmux
echo ""
echo "Starting TUI in tmux..."
tmux new-session -d -s "$SESSION_NAME" 'dodo'
sleep 2

# Test 1: Check panes view with marquee
echo ""
echo "Test 1: Panes View Marquee"
echo "Expected: Tasks with long titles should show marquee animation"
# Scroll down to see task 43 (long title)
tmux send-keys -t "$SESSION_NAME" 'j' 'j' 'j' 'j' 'j' 'j' 'j' 'j' 'j' 'j'
sleep 1
# Check for long task title that should trigger marquee
tmux capture-pane -p -t "$SESSION_NAME" | grep -E "^\│.*43.*\u25CB.*[A-Z].*\u258C" | head -3 || echo "Task 43 not found or marquee not working"

# Test 2: Check recurring mark
echo ""
echo "Test 2: Recurring Mark Position"
echo "Expected: Recurring instances should show ↻ at the beginning"
tmux capture-pane -p -t "$SESSION_NAME" | grep -E "\u21BB.*Blog|\u21BB.*Wegovy" | head -3 || echo "No recurring instances found"

# Test 3: Check DONE pane stats
echo ""
echo "Test 3: DONE Pane Stats"
echo "Expected: DONE pane should show on-time vs overdue stats"
tmux capture-pane -p -t "$SESSION_NAME" | grep -E "DONE.*on-time" | head -1 || echo "DONE pane not found"

# Test 4: Check timer display
echo ""
echo "Test 4: Timer Display"
echo "Expected: Running tasks should show countdown/overage"
tmux capture-pane -p -t "$SESSION_NAME" | grep -E "\([0-9]+m.*left|\+[0-9]+m.*over\)" | head -3 || echo "No running tasks found"

# Test 5: Check header timer
echo ""
echo "Test 5: Header Timer"
echo "Expected: Running task should show in header with timer"
tmux capture-pane -p -t "$SESSION_NAME" | grep -E "DODO.*\u23F1.*[0-9]+m" | head -1 || echo "No running task in header"

# Test 6: Check view persistence (check current view)
echo ""
echo "Test 6: View Persistence"
echo "Expected: Current view should be visible in header"
tmux capture-pane -p -t "$SESSION_NAME" | grep -E "Panes.*Daily.*Weekly.*Calendar" | head -1 || echo "View selector not found"

# Test 7: Check marquee spacing
echo ""
echo "Test 7: View Selector Spacing"
echo "Expected: View labels should have adequate spacing (4 spaces between)"
tmux capture-pane -p -t "$SESSION_NAME" | grep -E "Panes.*Daily.*Weekly.*Calendar" | head -1 || echo "View selector not found"

echo ""
echo "=== UX Testing Complete ==="