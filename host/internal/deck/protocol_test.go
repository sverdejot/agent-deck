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
		{ID: "cli:2", State: "idle", Title: "Two", Place: "homelab", Age: "3 h ago", Agents: 2},
	}
	got := Snapshot(ss, func(s claude.Session) int {
		if s.ID == "cli:1" {
			return 30
		}
		return -1
	})
	want := "B\n" +
		"S\tcli:1\tcode\tbusy\tUno con tab\tesp\t\t30\t0\n" +
		"S\tcli:2\tcode\tidle\tTwo\thomelab\t3 h ago\t-1\t2\n" +
		"E\n"
	if got != want {
		t.Errorf("Snapshot =\n%q\nwant\n%q", got, want)
	}
}

func TestLimitsLine(t *testing.T) {
	l := claude.Limits{Five: 64, FiveReset: "resets 16:40", FiveElapsed: 90, Week: 52, WeekReset: "resets Fri 22:00",
		WeekElapsed: 39, Projection: "At this pace you won't hit the limit", Updated: "now"}
	want := "L\t64\tresets 16:40\t90\t52\tresets Fri 22:00\t39\tAt this pace you won't hit the limit\tnow\n"
	if got := LimitsLine(l); got != want {
		t.Errorf("LimitsLine = %q", got)
	}
}

func TestUsageLines(t *testing.T) {
	u := claude.Usage{
		Start: 32, N: 3, Total: 1_812_000,
		Series: []claude.Series{{Name: "homelab", Total: 1_300_000, Values: []int64{0, 205_400, 599_999}}},
		Days:   []claude.Day{{Label: "M", Tokens: 10_000}, {Label: "T", Tokens: 20_500}},
	}
	got := strings.Split(strings.TrimSuffix(UsageLines(u), "\n"), "\n")
	want := []string{
		"C\tB",
		"C\tH\t32\t3\t1.8 M\ttokens today · top: homelab",
		"C\tS\thomelab\t1.3 M\t0,205,599",
		"C\tD\tM,T\t10,20\t1",
		"C\tE",
	}
	if strings.Join(got, "\n") != strings.Join(want, "\n") {
		t.Errorf("UsageLines =\n%s\nwant\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
}

func TestSummaryLine(t *testing.T) {
	got := SummaryLine("cli:1", claude.Summary{Meta: "m", You: "hello\nhow are you", Claude: "c", Tool: ""})
	if got != "Y\tcli:1\tm\thello how are you\tc\t\n" {
		t.Errorf("SummaryLine = %q", got)
	}
}

func TestQuestionLine(t *testing.T) {
	p := claude.Prompt{
		Question: "rm -rf build · Do you want\tto proceed?",
		Options: []claude.Option{
			{N: 1, Label: "Yes"},
			{N: 2, Label: strings.Repeat("ñ", 50)}, // 100 bytes: cut to 80 on a character boundary
			{N: 3, Label: "No, and tell Claude what to do differently"},
		},
	}
	want := "Q\tcli:1\trm -rf build · Do you want to proceed?\t1.Yes\t2." + strings.Repeat("ñ", 40) +
		"\t3.No, and tell Claude what to do differently\n"
	if got := QuestionLine("cli:1", p); got != want {
		t.Errorf("QuestionLine =\n%q\nwant\n%q", got, want)
	}
	if got := QuestionLine("cli:1", claude.Prompt{}); got != "Q\tcli:1\n" {
		t.Errorf("no menu: %q", got)
	}

	var many []claude.Option
	for n := 1; n <= 12; n++ {
		many = append(many, claude.Option{N: n, Label: "x"})
	}
	got := QuestionLine("cli:1", claude.Prompt{Question: "¿?", Options: many})
	if want := "Q\tcli:1\t¿?\t1.x\t2.x\t3.x\t4.x\t5.x\t6.x\n"; got != want {
		t.Errorf("at most 6 options: %q", got)
	}
}

func TestParseAnswer(t *testing.T) {
	a, ok := ParseAnswer("A\tcli:1\t2\tYes, and don't ask again")
	if !ok || a != (Answer{ID: "cli:1", N: 2, Label: "Yes, and don't ask again"}) {
		t.Errorf("ParseAnswer = %+v, %v", a, ok)
	}
	for _, bad := range []string{"A\tcli:1\t2", "A\tcli:1\tdos\tYes", "A\tcli:1\t0\tYes", "A\t\t1\tYes", "R\tcli:1\t1\tYes"} {
		if _, ok := ParseAnswer(bad); ok {
			t.Errorf("ParseAnswer(%q) accepted", bad)
		}
	}
	// what the deck sends back is what it got
	label := OptionLabel(strings.Repeat("ñ", 50) + " ")
	if a, ok := ParseAnswer("A\tcli:1\t2\t" + label); !ok || a.Label != label {
		t.Errorf("round trip: %q", a.Label)
	}
}

func TestAgentLines(t *testing.T) {
	agents := []claude.Subagent{
		{Type: "Explore", Description: "Buscar usos\tde tile_rect", Age: "3 min", Now: "Grep: tile_rect"},
		{Type: "general-purpose", Description: "Tests", Age: "40 s"},
	}
	want := "G\tB\tcli:1\n" +
		"G\tS\tcli:1\tExplore\tBuscar usos de tile_rect\t3 min\tGrep: tile_rect\n" +
		"G\tS\tcli:1\tgeneral-purpose\tTests\t40 s\t\n" +
		"G\tE\tcli:1\n"
	if got := AgentLines("cli:1", agents); got != want {
		t.Errorf("AgentLines =\n%q\nwant\n%q", got, want)
	}
	if got := AgentLines("cli:1", nil); got != "G\tB\tcli:1\nG\tE\tcli:1\n" {
		t.Errorf("no subagents: %q", got)
	}
	many := make([]claude.Subagent, 20)
	if n := strings.Count(AgentLines("cli:1", many), "G\tS\t"); n != maxAgents {
		t.Errorf("%d subagents listed, want %d", n, maxAgents)
	}
}
