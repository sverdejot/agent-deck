package claude

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// Claude Code hands the statusline command a JSON snapshot of the session (context window, cost,
// plan rate limits…). `agent-deck statusline -- <original command>` keeps a copy of it and then
// runs the original statusline command with the same input, so the terminal looks unchanged.

// Statusline records the snapshot on stdin, then runs the original statusline command (args)
// with the same input, passing its output and exit code through.
func Statusline(args []string) {
	in, _ := io.ReadAll(os.Stdin)
	record(in)
	if len(args) == 0 {
		return
	}
	cmd := exec.Command(args[0], args[1:]...)
	cmd.Stdin = bytes.NewReader(in)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		if ee, ok := err.(*exec.ExitError); ok {
			os.Exit(ee.ExitCode())
		}
		os.Exit(1)
	}
}

// record never fails loudly: the statusline must keep working whatever happens here.
func record(in []byte) {
	var head struct {
		SessionID  string          `json:"session_id"`
		RateLimits json.RawMessage `json:"rate_limits"`
	}
	if json.Unmarshal(in, &head) != nil || head.SessionID == "" || strings.ContainsAny(head.SessionID, "/\\") {
		return
	}
	os.MkdirAll(statusDir(), 0o700)
	writeAtomic(filepath.Join(statusDir(), head.SessionID+".json"), in)

	if len(head.RateLimits) == 0 || string(head.RateLimits) == "null" {
		return
	}
	// append to the history only when the reading changed
	last := filepath.Join(DataDir(), "limits-last.json")
	if prev, err := os.ReadFile(last); err == nil && bytes.Equal(prev, head.RateLimits) {
		return
	}
	writeAtomic(last, head.RateLimits)
	line, _ := json.Marshal(map[string]any{"t": time.Now().Unix(), "rate_limits": head.RateLimits})
	if f, err := os.OpenFile(limitsLog(), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600); err == nil {
		f.Write(append(line, '\n'))
		f.Close()
	}
}

func writeAtomic(path string, b []byte) {
	tmp := fmt.Sprintf("%s.%d.tmp", path, os.Getpid())
	if os.WriteFile(tmp, b, 0o600) == nil {
		os.Rename(tmp, path)
	}
}

// ---- wiring it into ~/.claude/settings.json --------------------------------------------------

func settingsPath() string { return filepath.Join(claudeDir(), "settings.json") }

// statusCommand returns the current statusLine.command (empty if none).
func statusCommand(settings []byte) (string, error) {
	var s struct {
		StatusLine *struct {
			Command string `json:"command"`
		} `json:"statusLine"`
	}
	if err := json.Unmarshal(settings, &s); err != nil {
		return "", fmt.Errorf("settings.json: %v", err)
	}
	if s.StatusLine == nil {
		return "", nil
	}
	return s.StatusLine.Command, nil
}

// replaceCommand swaps the JSON string value old for new in the file text, touching nothing else.
func replaceCommand(settings []byte, old, new string) ([]byte, error) {
	o, _ := json.Marshal(old)
	n, _ := json.Marshal(new)
	if bytes.Count(settings, o) != 1 {
		return nil, fmt.Errorf("can't find a unique statusLine.command in settings.json")
	}
	return bytes.Replace(settings, o, n, 1), nil
}

// InstallStatusline wraps the statusLine.command in ~/.claude/settings.json with
// `<exe> statusline -- sh -c <original>`, touching nothing else (backup in settings.json.agent-deck.bak).
func InstallStatusline(exe string) error {
	b, err := os.ReadFile(settingsPath())
	if err != nil {
		return err
	}
	cur, err := statusCommand(b)
	if err != nil {
		return err
	}
	if strings.Contains(cur, " statusline --") || strings.HasSuffix(cur, " statusline") {
		fmt.Println("statusline already hooked up")
		return nil
	}
	if cur == "" {
		return fmt.Errorf("no statusLine in settings.json; add one (even just `true`) and run this again")
	}
	next := fmt.Sprintf("%q statusline -- sh -c %q", exe, cur)
	out, err := replaceCommand(b, cur, next)
	if err != nil {
		return err
	}
	os.WriteFile(settingsPath()+".agent-deck.bak", b, 0o600)
	if err := os.WriteFile(settingsPath(), out, 0o600); err != nil {
		return err
	}
	fmt.Printf("statusline hooked up (backup in %s.agent-deck.bak)\n", settingsPath())
	return nil
}

// UninstallStatusline restores the original statusLine.command.
func UninstallStatusline() error {
	b, err := os.ReadFile(settingsPath())
	if err != nil {
		return err
	}
	cur, err := statusCommand(b)
	if err != nil {
		return err
	}
	i := strings.Index(cur, " statusline -- sh -c ")
	if i < 0 {
		fmt.Println("statusline was not hooked up")
		return nil
	}
	var orig string
	if _, err := fmt.Sscanf(cur[i+len(" statusline -- sh -c "):], "%q", &orig); err != nil {
		return fmt.Errorf("can't recover the original command: %v", err)
	}
	out, err := replaceCommand(b, cur, orig)
	if err != nil {
		return err
	}
	if err := os.WriteFile(settingsPath(), out, 0o600); err != nil {
		return err
	}
	fmt.Println("statusline restored")
	return nil
}
