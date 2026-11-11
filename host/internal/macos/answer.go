package macos

import (
	"fmt"
	"os/exec"
	"strconv"
)

// Screen is the text a tmux pane shows right now.
func Screen(tmuxPane string) (string, error) {
	out, err := exec.Command(tmuxPath(), "capture-pane", "-p", "-t", tmuxPane).Output()
	if err != nil {
		return "", fmt.Errorf("tmux capture-pane: %w", err)
	}
	return string(out), nil
}

// Answer picks option n of the menu Claude Code shows in a tmux pane: typing its number selects
// it, without focusing the terminal.
func Answer(tmuxPane string, n int) error {
	if n < 1 || n > 9 {
		return fmt.Errorf("option %d out of range", n)
	}
	return run(tmuxPath(), "send-keys", "-t", tmuxPane, strconv.Itoa(n))
}
