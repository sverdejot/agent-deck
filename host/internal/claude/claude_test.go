package claude

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestState(t *testing.T) {
	now := time.Now()
	cases := []struct {
		status string
		when   time.Time
		want   string
	}{
		{"busy", now, "busy"},
		{"Running", now, "busy"},
		{"waiting_for_permission", now, "wait"},
		{"needs_input", now, "wait"},
		{"idle", now.Add(-5 * time.Minute), "ready"},
		{"idle", now.Add(-2 * time.Hour), "idle"},
		{"", now.Add(-readyFor - time.Second), "idle"},
	}
	for _, c := range cases {
		if got := state(c.status, c.when, now); got != c.want {
			t.Errorf("state(%q, -%v) = %q, want %q", c.status, now.Sub(c.when), got, c.want)
		}
	}
}

func TestPlace(t *testing.T) {
	root := t.TempDir()
	repo := filepath.Join(root, "agent-deck")
	sub := filepath.Join(repo, "firmware", "src")
	must(t, os.MkdirAll(filepath.Join(repo, ".git", "worktrees", "feature"), 0o755))
	must(t, os.MkdirAll(sub, 0o755))
	if got := Place(sub); got != "agent-deck" {
		t.Errorf("inside a repo: got %q", got)
	}

	// a worktree has a .git file pointing at <main>/.git/worktrees/<name>
	wt := filepath.Join(root, "agent-deck-feature")
	must(t, os.MkdirAll(wt, 0o755))
	must(t, os.WriteFile(filepath.Join(wt, ".git"), []byte("gitdir: "+repo+"/.git/worktrees/feature\n"), 0o644))
	if got := Place(wt); got != "agent-deck" {
		t.Errorf("worktree: got %q, want the main repo's name", got)
	}

	plain := filepath.Join(root, "a", "b", "c")
	must(t, os.MkdirAll(plain, 0o755))
	if got := Place(plain); got != "…/b/c" {
		t.Errorf("no repo: got %q", got)
	}
}

func TestAgo(t *testing.T) {
	now := time.Now()
	for d, want := range map[time.Duration]string{
		10 * time.Second: "ahora",
		5 * time.Minute:  "hace 5 min",
		3 * time.Hour:    "hace 3 h",
		50 * time.Hour:   "hace 2 d",
	} {
		if got := agoAt(now.Add(-d), now); got != want {
			t.Errorf("agoAt(-%v) = %q, want %q", d, got, want)
		}
	}
}

func TestElapsed(t *testing.T) {
	now := time.Now()
	if got := elapsed(now.Add(90*time.Minute), fiveHours, now); got != 70 {
		t.Errorf("3.5 h into a 5 h window: got %d", got)
	}
	if got := elapsed(now.Add(-time.Minute), fiveHours, now); got != 100 {
		t.Errorf("past the reset: got %d", got)
	}
}

func TestProject(t *testing.T) {
	now := time.Date(2026, 11, 10, 15, 0, 0, 0, time.Local)
	reset := now.Add(2 * time.Hour)
	history := func(pts ...struct {
		ago time.Duration
		p   float64
	}) func(func(time.Time, float64) bool) {
		return func(yield func(time.Time, float64) bool) {
			for _, pt := range pts { // newest first
				if !yield(now.Add(-pt.ago), pt.p) {
					return
				}
			}
		}
	}
	type pt = struct {
		ago time.Duration
		p   float64
	}

	// 40% → 60% in 30 min: 40% left at 0.667%/min → 60 min → 16:00, before the reset at 17:00
	got := project(&window{UsedPercentage: 60}, reset, now, history(pt{0, 60}, pt{30 * time.Minute, 40}))
	if got != "A este ritmo: 100% a las 16:00" {
		t.Errorf("fast pace: %q", got)
	}
	// 59% → 60% in 30 min: would take 20 h, after the reset
	got = project(&window{UsedPercentage: 60}, reset, now, history(pt{0, 60}, pt{30 * time.Minute, 59}))
	if got != "A este ritmo no llegas al límite" {
		t.Errorf("slow pace: %q", got)
	}
	// only 5 minutes of history
	got = project(&window{UsedPercentage: 60}, reset, now, history(pt{0, 60}, pt{5 * time.Minute, 50}))
	if got != "Ritmo: aún sin datos suficientes" {
		t.Errorf("little history: %q", got)
	}
	if got := project(&window{UsedPercentage: 100}, reset, now, history()); got != "Límite alcanzado · reinicia 17:00" {
		t.Errorf("at the limit: %q", got)
	}
}

func TestPlain(t *testing.T) {
	in := "## Hecho\n\n- **uno**\n- `dos`\n\n```go\nfmt.Println()\n```\nfin"
	if got := plain(in); got != "Hecho · uno · dos [código] fin" {
		t.Errorf("plain = %q", got)
	}
}

func TestIsHumanText(t *testing.T) {
	for text, want := range map[string]bool{
		"arregla el bug":                      true,
		"<command-name>/clear</command-name>": false,
		"[Request interrupted by user]":       false,
		"   ":                                 false,
	} {
		if got := isHumanText(text); got != want {
			t.Errorf("isHumanText(%q) = %v", text, got)
		}
	}
}

func TestFoldOthers(t *testing.T) {
	var series []Series
	for i, name := range []string{"a", "b", "c", "d", "e"} {
		series = append(series, Series{Name: name, Total: int64(50 - i), Values: []int64{1, 2}})
	}
	got := foldOthers(series)
	if len(got) != maxSeries || got[3].Name != "otros" || got[3].Total != 47+46 || got[3].Values[1] != 4 {
		t.Errorf("foldOthers = %+v", got)
	}
	if got := foldOthers(series[:2]); len(got) != 2 {
		t.Errorf("few series are kept as is: %+v", got)
	}
}

func TestHumanTokens(t *testing.T) {
	for n, want := range map[int64]string{950: "950", 464_312: "464 k", 1_812_000: "1,8 M"} {
		if got := HumanTokens(n); got != want {
			t.Errorf("HumanTokens(%d) = %q, want %q", n, got, want)
		}
	}
}

func must(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}
