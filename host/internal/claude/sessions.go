package claude

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"syscall"
	"time"
)

// Session is one Claude Code session running in a terminal.
type Session struct {
	ID      string    // stable id sent to the deck ("cli:<sessionId>")
	State   string    // busy | wait | ready | idle
	Title   string    // name given by the user, else the AI title of the transcript
	Place   string    // repository name (or short path)
	Age     string    // "hace 3 h"; empty while working
	When    time.Time // last activity
	Started time.Time // sessions are listed oldest first, so tiles keep their place
	Agents  int       // subagents running now

	SessionID string // Claude Code session id (transcript, statusline snapshot)

	// where it runs, to bring it to the front
	TmuxPane string // "%32" (inside tmux)
	TTY      string // "/dev/ttys002" (outside tmux)
}

// readyFor is how long a finished session stays "ready" before it counts as idle.
const readyFor = 30 * time.Minute

// Sessions lists the interactive terminal sessions alive right now, oldest first.
func Sessions() []Session {
	files, _ := filepath.Glob(filepath.Join(claudeDir(), "sessions", "*.json"))
	var out []Session
	for _, f := range files {
		if s, ok := readSession(f); ok {
			out = append(out, s)
		}
	}
	sort.SliceStable(out, func(i, j int) bool {
		if !out[i].Started.Equal(out[j].Started) {
			return out[i].Started.Before(out[j].Started)
		}
		return out[i].ID < out[j].ID
	})
	return out
}

// sessionFile is ~/.claude/sessions/<pid>.json, written by every running Claude Code.
type sessionFile struct {
	PID        int    `json:"pid"`
	SessionID  string `json:"sessionId"`
	Cwd        string `json:"cwd"`
	Kind       string `json:"kind"`
	Entrypoint string `json:"entrypoint"`
	Tmux       string `json:"tmux"` // "0:@20.%32"
	Name       string `json:"name"`
	NameSource string `json:"nameSource"`
	Status     string `json:"status"`
	StartedAt  int64  `json:"startedAt"`
	UpdatedAt  int64  `json:"updatedAt"`
	StatusAt   int64  `json:"statusUpdatedAt"`
}

func readSession(path string) (Session, bool) {
	b, err := os.ReadFile(path)
	if err != nil {
		return Session{}, false
	}
	var c sessionFile
	if json.Unmarshal(b, &c) != nil || c.PID == 0 || c.SessionID == "" {
		return Session{}, false
	}
	if c.Kind != "" && c.Kind != "interactive" {
		return Session{}, false
	}
	if c.Entrypoint != "" && c.Entrypoint != "cli" {
		return Session{}, false // started by the desktop app or an SDK, not a terminal
	}
	if err := syscall.Kill(c.PID, 0); err != nil && err != syscall.EPERM {
		return Session{}, false // process gone, stale file
	}
	when := time.UnixMilli(max(c.StatusAt, c.UpdatedAt))
	s := Session{
		ID:        "cli:" + c.SessionID,
		State:     state(c.Status, when, time.Now()),
		Title:     title(c),
		Place:     Place(c.Cwd),
		When:      when,
		Started:   time.UnixMilli(c.StartedAt),
		SessionID: c.SessionID,
	}
	if s.State != "busy" {
		s.Age = Ago(when)
	}
	s.Agents = len(Subagents(c.SessionID, s.State, false))
	if i := strings.LastIndex(c.Tmux, "."); i >= 0 {
		s.TmuxPane = c.Tmux[i+1:]
	}
	if s.TmuxPane == "" {
		s.TTY = ttyOf(c.PID)
	}
	return s, true
}

// state maps Claude Code's status to the deck's four states.
func state(status string, when, now time.Time) string {
	st := strings.ToLower(status)
	switch {
	case st == "busy" || st == "running" || st == "working":
		return "busy"
	case strings.Contains(st, "wait") || strings.Contains(st, "permission") || strings.Contains(st, "input") || strings.Contains(st, "approv"):
		return "wait"
	case now.Sub(when) < readyFor:
		return "ready"
	}
	return "idle"
}

var titleCache sync.Map // transcript path → titleEntry

type titleEntry struct {
	size  int64
	title string
}

// title prefers a name the user gave, then the AI title from the transcript, then the derived name.
func title(c sessionFile) string {
	if c.NameSource != "" && c.NameSource != "derived" && c.Name != "" {
		return c.Name
	}
	if path, ok := transcript(c.SessionID); ok {
		if t := transcriptTitle(path); t != "" {
			return t
		}
	}
	if c.Name != "" {
		return c.Name
	}
	return filepath.Base(c.Cwd)
}

func transcriptTitle(path string) string {
	st, err := os.Stat(path)
	if err != nil {
		return ""
	}
	if e, ok := titleCache.Load(path); ok && e.(titleEntry).size == st.Size() {
		return e.(titleEntry).title
	}
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	var aiTitle, custom string
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 64<<20)
	for sc.Scan() {
		line := sc.Bytes()
		if !bytes.Contains(line, []byte(`"ai-title"`)) && !bytes.Contains(line, []byte(`"custom-title"`)) {
			continue
		}
		var e struct {
			Type        string `json:"type"`
			AITitle     string `json:"aiTitle"`
			CustomTitle string `json:"customTitle"`
		}
		if json.Unmarshal(line, &e) != nil {
			continue
		}
		if e.Type == "ai-title" && e.AITitle != "" {
			aiTitle = e.AITitle
		}
		if e.Type == "custom-title" && e.CustomTitle != "" {
			custom = e.CustomTitle
		}
	}
	t := custom
	if t == "" {
		t = aiTitle
	}
	titleCache.Store(path, titleEntry{st.Size(), t})
	return t
}

func ttyOf(pid int) string {
	out, err := exec.Command("ps", "-o", "tty=", "-p", fmt.Sprint(pid)).Output()
	t := strings.TrimSpace(string(out))
	if err != nil || t == "" || t == "??" {
		return ""
	}
	return "/dev/" + t
}

var placeCache sync.Map // dir → place

// Place names where a session runs: the git repository's name if dir is inside one (worktrees
// resolve to their main repository), otherwise the short path.
func Place(dir string) string {
	if v, ok := placeCache.Load(dir); ok {
		return v.(string)
	}
	name := ShortPath(dir)
	for d := dir; d != "/" && d != "."; d = filepath.Dir(d) {
		st, err := os.Stat(filepath.Join(d, ".git"))
		if err != nil {
			continue
		}
		name = filepath.Base(d)
		if !st.IsDir() { // worktree: ".git" file pointing at <main>/.git/worktrees/<name>
			if b, err := os.ReadFile(filepath.Join(d, ".git")); err == nil {
				gitdir := strings.TrimSpace(strings.TrimPrefix(string(b), "gitdir:"))
				if i := strings.Index(gitdir, "/.git/worktrees/"); i > 0 {
					name = filepath.Base(gitdir[:i])
				}
			}
		}
		break
	}
	placeCache.Store(dir, name)
	return name
}

// ShortPath abbreviates a path for the small screen: ~ for home, …/ for deep paths.
func ShortPath(p string) string {
	if strings.HasPrefix(p, Home) {
		p = "~" + p[len(Home):]
	}
	parts := strings.Split(p, "/")
	if len(parts) > 3 {
		p = "…/" + strings.Join(parts[len(parts)-2:], "/")
	}
	return p
}

// Ago says how long ago t was, in Spanish ("ahora", "hace 5 min", "hace 3 h", "hace 2 d").
func Ago(t time.Time) string { return agoAt(t, time.Now()) }

func agoAt(t, now time.Time) string {
	d := now.Sub(t)
	switch {
	case d < time.Minute:
		return "ahora"
	case d < time.Hour:
		return fmt.Sprintf("hace %d min", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("hace %d h", int(d.Hours()))
	}
	return fmt.Sprintf("hace %d d", int(d.Hours()/24))
}

func weekday(t time.Time) string {
	return [...]string{"dom", "lun", "mar", "mié", "jue", "vie", "sáb"}[t.Weekday()]
}
