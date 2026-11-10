// Package claude reads what Claude Code leaves on disk: running sessions, transcripts, and the
// statusline snapshots that agent-deck records (plan limits, context window).
package claude

import (
	"os"
	"path/filepath"
)

// Home is the user's home directory.
var Home, _ = os.UserHomeDir()

func claudeDir() string { return filepath.Join(Home, ".claude") }

// DataDir is where agent-deck keeps what it records.
func DataDir() string {
	return filepath.Join(Home, "Library", "Application Support", "agent-deck")
}

func statusDir() string { return filepath.Join(DataDir(), "status") }

// limitsLog is a history of rate-limit readings (one JSON object per line), for the projection.
func limitsLog() string { return filepath.Join(DataDir(), "limits.jsonl") }

// transcript finds the JSONL transcript of a Claude Code session.
func transcript(sessionID string) (string, bool) {
	m, _ := filepath.Glob(filepath.Join(claudeDir(), "projects", "*", sessionID+".jsonl"))
	if len(m) == 0 {
		return "", false
	}
	return m[0], true
}
