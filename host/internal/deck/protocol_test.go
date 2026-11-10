package deck

import (
	"strings"
	"testing"
	"unicode/utf8"

	"github.com/sverdejot/agent-deck/host/internal/claude"
)

func TestClean(t *testing.T) {
	if got := Clean("a\tb\nc\rd", 100); got != "a b c d" {
		t.Errorf("separators: %q", got)
	}
	// "ó" is two bytes: cutting at 2 must not split it
	if got := Clean("Detección", 6); got != "Detecc" || !utf8.ValidString(got) {
		t.Errorf("ascii cut: %q", got)
	}
	if got := Clean("Detección", 8); got != "Detecci" || !utf8.ValidString(got) {
		t.Errorf("cut inside ó: %q", got)
	}
}

func TestSnapshot(t *testing.T) {
	ss := []claude.Session{
		{ID: "cli:1", State: "busy", Title: "Uno\tcon tab", Place: "esp"},
		{ID: "cli:2", State: "idle", Title: "Dos", Place: "homelab", Age: "hace 3 h"},
	}
	got := Snapshot(ss, func(s claude.Session) int {
		if s.ID == "cli:1" {
			return 30
		}
		return -1
	})
	want := "B\n" +
		"S\tcli:1\tcode\tbusy\tUno con tab\tesp\t\t30\n" +
		"S\tcli:2\tcode\tidle\tDos\thomelab\thace 3 h\t-1\n" +
		"E\n"
	if got != want {
		t.Errorf("Snapshot =\n%q\nwant\n%q", got, want)
	}
}

func TestLimitsLine(t *testing.T) {
	l := claude.Limits{Five: 64, FiveReset: "reinicia 16:40", FiveElapsed: 90, Week: 52, WeekReset: "reinicia vie 22:00",
		WeekElapsed: 39, Projection: "A este ritmo no llegas al límite", Updated: "ahora"}
	want := "L\t64\treinicia 16:40\t90\t52\treinicia vie 22:00\t39\tA este ritmo no llegas al límite\tahora\n"
	if got := LimitsLine(l); got != want {
		t.Errorf("LimitsLine = %q", got)
	}
}

func TestUsageLines(t *testing.T) {
	u := claude.Usage{
		Start: 32, N: 3, Total: 1_812_000,
		Series: []claude.Series{{Name: "homelab", Total: 1_300_000, Values: []int64{0, 205_400, 599_999}}},
		Days:   []claude.Day{{Label: "L", Tokens: 10_000}, {Label: "M", Tokens: 20_500}},
	}
	got := strings.Split(strings.TrimSuffix(UsageLines(u), "\n"), "\n")
	want := []string{
		"C\tB",
		"C\tH\t32\t3\t1,8 M\ttokens hoy · homelab el que más",
		"C\tS\thomelab\t1,3 M\t0,205,599",
		"C\tD\tL,M\t10,20\t1",
		"C\tE",
	}
	if strings.Join(got, "\n") != strings.Join(want, "\n") {
		t.Errorf("UsageLines =\n%s\nwant\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
}

func TestSummaryLine(t *testing.T) {
	got := SummaryLine("cli:1", claude.Summary{Meta: "m", You: "hola\nqué tal", Claude: "c", Tool: ""})
	if got != "Y\tcli:1\tm\thola qué tal\tc\t\n" {
		t.Errorf("SummaryLine = %q", got)
	}
}
