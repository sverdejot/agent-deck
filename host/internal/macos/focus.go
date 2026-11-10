// Package macos does the Mac-side actions: bringing a terminal session to the front (tmux +
// Ghostty), and installing agent-deck (binary, LaunchAgent).
package macos

import (
	"fmt"
	"os/exec"
	"strings"
)

// Focus brings a terminal session to the front: selects its tmux pane (if it runs inside tmux)
// and focuses the Ghostty terminal showing it. tty is used when there is no tmux pane.
func Focus(tmuxPane, tty string) error {
	if tmuxPane != "" {
		tmux := tmuxPath()
		client := activeClient(tmux)
		if client != "" {
			_ = run(tmux, "switch-client", "-c", client, "-t", tmuxPane)
		}
		_ = run(tmux, "select-window", "-t", tmuxPane)
		_ = run(tmux, "select-pane", "-t", tmuxPane)
		tty = client
	}
	if tty == "" {
		return osa(`tell application "Ghostty" to activate`)
	}
	return osa(fmt.Sprintf(`
tell application "Ghostty"
	set ts to (terminals whose tty is %q)
	if (count of ts) > 0 then focus (item 1 of ts)
	activate
end tell`, tty))
}

// activeClient is the tty of the most recently active tmux client.
func activeClient(tmux string) string {
	out, err := exec.Command(tmux, "list-clients", "-F", "#{client_activity} #{client_tty}").Output()
	if err != nil {
		return ""
	}
	best := ""
	for _, l := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if l > best { // activity is a unix timestamp: same width, so string order works
			best = l
		}
	}
	if f := strings.Fields(best); len(f) == 2 {
		return f[1]
	}
	return ""
}

func tmuxPath() string {
	for _, p := range []string{"/opt/homebrew/bin/tmux", "/usr/local/bin/tmux"} {
		if _, err := exec.LookPath(p); err == nil {
			return p
		}
	}
	return "tmux"
}

func osa(script string) error { return run("osascript", "-e", script) }

func run(name string, args ...string) error {
	out, err := exec.Command(name, args...).CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s: %w: %s", name, err, strings.TrimSpace(string(out)))
	}
	return nil
}
