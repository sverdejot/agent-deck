package claude

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// Transcript lines as Claude Code writes them (trimmed to the fields that matter).
const (
	lineAsk = `{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"revisa el parpadeo del panel"},"uuid":"u1","timestamp":"2026-11-10T10:00:00.000Z","sessionId":"s1","cwd":"/Users/me/agent-deck"}`

	// foreground, older name (Task), still running
	lineTaskFg = `{"parentUuid":"u1","isSidechain":false,"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"tool_use","id":"toolu_fg","name":"Task","input":{"description":"Buscar usos de tile_rect","subagent_type":"Explore","prompt":"Find every use of tile_rect in crates/deck-ui and explain how tiles are laid out."}}]},"uuid":"a1","timestamp":"2026-11-10T10:00:05.000Z"}`

	// foreground, newer name (Agent), already answered
	lineAgentDone       = `{"isSidechain":false,"type":"assistant","message":{"id":"msg_2","role":"assistant","content":[{"type":"tool_use","id":"toolu_done","name":"Agent","input":{"description":"Leer el protocolo","subagent_type":"general-purpose","prompt":"Read docs/protocol.md"}}]},"timestamp":"2026-11-10T10:00:06.000Z"}`
	lineAgentDoneResult = `{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_done","type":"tool_result","content":[{"type":"text","text":"The protocol has B, S, E…"}]}]},"toolUseResult":{"status":"completed","agentId":"done1","totalDurationMs":4100},"timestamp":"2026-11-10T10:01:00.000Z"}`

	// background (Agent), launched and acknowledged, still running
	lineAgentBg       = `{"isSidechain":false,"type":"assistant","message":{"id":"msg_3","role":"assistant","content":[{"type":"tool_use","id":"toolu_bg","name":"Agent","input":{"description":"Pasar los tests","subagent_type":"general-purpose","prompt":"Run the whole test suite and report failures.","run_in_background":true}}]},"timestamp":"2026-11-10T10:02:00.000Z"}`
	lineAgentBgResult = `{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_bg","type":"tool_result","content":[{"type":"text","text":"Async agent launched successfully.\nagentId: bg1 (internal ID)\nThe agent is working in the background."}]}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"bg1"},"timestamp":"2026-11-10T10:02:01.000Z"}`

	// background (Task), finished: announced by a task notification
	lineTaskBg       = `{"isSidechain":false,"type":"assistant","message":{"id":"msg_4","role":"assistant","content":[{"type":"tool_use","id":"toolu_bg2","name":"Task","input":{"description":"Medir bandas","subagent_type":"general-purpose","prompt":"Measure the draw time of each band.","run_in_background":true}}]},"timestamp":"2026-11-10T10:02:30.000Z"}`
	lineTaskBgResult = `{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_bg2","type":"tool_result","content":"Async agent launched successfully. agentId: bg2"}]},"timestamp":"2026-11-10T10:02:31.000Z"}`
	lineNotice       = `{"isSidechain":false,"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>bg2</task-id>\n<status>completed</status>\n<summary>Agent \"Medir bandas\" completed</summary>\n</task-notification>"},"timestamp":"2026-11-10T10:03:30.000Z"}`

	// background whose transcript went quiet long ago
	lineAgentStale       = `{"isSidechain":false,"type":"assistant","message":{"id":"msg_5","role":"assistant","content":[{"type":"tool_use","id":"toolu_stale","name":"Agent","input":{"description":"Algo viejo","subagent_type":"Explore","prompt":"Something old.","run_in_background":true}}]},"timestamp":"2026-11-10T10:00:10.000Z"}`
	lineAgentStaleResult = `{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_stale","type":"tool_result","content":[{"type":"text","text":"Async agent launched successfully."}]}]},"toolUseResult":{"isAsync":true,"agentId":"old1"},"timestamp":"2026-11-10T10:00:11.000Z"}`

	// older versions logged the subagent's own messages in the main transcript: not launches
	lineSidechain = `{"isSidechain":true,"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_side","name":"Task","input":{"description":"Anidado","prompt":"nested"}}]},"timestamp":"2026-11-10T10:00:20.000Z"}`
)

var agentNow = time.Date(2026, 11, 10, 10, 4, 0, 0, time.UTC)

// fixture writes a project dir with the session's transcript and its subagents' transcripts
// (name → lines, last written `ago` before agentNow).
func fixture(t *testing.T, lines []string, subagents map[string][]string, ago map[string]time.Duration) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "s1.jsonl")
	must(t, os.WriteFile(path, []byte(strings.Join(lines, "\n")+"\n"), 0o644))
	sub := filepath.Join(dir, "s1", "subagents")
	must(t, os.MkdirAll(sub, 0o755))
	for name, ls := range subagents {
		p := filepath.Join(sub, name)
		must(t, os.WriteFile(p, []byte(strings.Join(ls, "\n")+"\n"), 0o644))
		when := agentNow.Add(-ago[name])
		must(t, os.Chtimes(p, when, when))
	}
	return path
}

func agentSummary(as []Subagent) []string {
	var out []string
	for _, a := range as {
		out = append(out, strings.Join([]string{a.ID, a.Type, a.Description, a.Age, a.Now}, "|"))
	}
	return out
}

func sameStrings(a, b []string) bool {
	return strings.Join(a, "\n") == strings.Join(b, "\n")
}

func TestSubagents(t *testing.T) {
	path := fixture(t,
		[]string{lineAsk, lineTaskFg, lineSidechain, lineAgentDone, lineAgentDoneResult, lineAgentStale,
			lineAgentStaleResult, lineAgentBg, lineAgentBgResult, lineTaskBg, lineTaskBgResult, lineNotice},
		map[string][]string{
			// the foreground one: no agentId known yet, found by its prompt
			"agent-fg1.jsonl": {
				`{"isSidechain":true,"agentId":"fg1","type":"user","message":{"role":"user","content":"Find every use of tile_rect in crates/deck-ui and explain how tiles are laid out."},"timestamp":"2026-11-10T10:00:05.500Z"}`,
				`{"isSidechain":true,"agentId":"fg1","type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_g1","name":"Grep","input":{"pattern":"tile_rect","path":"crates"}}]},"timestamp":"2026-11-10T10:00:07.000Z"}`,
			},
			"agent-bg1.jsonl": {
				`{"isSidechain":true,"agentId":"bg1","type":"user","message":{"role":"user","content":"Run the whole test suite and report failures."}}`,
				`{"isSidechain":true,"agentId":"bg1","type":"assistant","message":{"content":[{"type":"text","text":"Running."},{"type":"tool_use","id":"toolu_b1","name":"Bash","input":{"command":"go test ./..."}}]}}`,
			},
			"agent-bg2.jsonl":  {`{"isSidechain":true,"agentId":"bg2","type":"user","message":{"role":"user","content":"Measure the draw time of each band."}}`},
			"agent-old1.jsonl": {`{"isSidechain":true,"agentId":"old1","type":"user","message":{"role":"user","content":"Something old."}}`},
		},
		map[string]time.Duration{
			"agent-fg1.jsonl":  5 * time.Second,
			"agent-bg1.jsonl":  10 * time.Second,
			"agent-bg2.jsonl":  20 * time.Second, // fresh, but its notification arrived
			"agent-old1.jsonl": 3 * time.Minute,
		})

	got := agentSummary(subagentsAt(path, "s1", "busy", true, agentNow))
	want := []string{
		"toolu_fg|Explore|Buscar usos de tile_rect|3 min|Grep: tile_rect",
		"toolu_bg|general-purpose|Pasar los tests|2 min|Bash: go test ./...",
	}
	if !sameStrings(got, want) {
		t.Errorf("busy, with details:\n%s\nwant\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}

	// the count path doesn't read what they are doing
	if got := subagentsAt(path, "s1", "busy", false, agentNow); len(got) != 2 || got[0].Now != "" {
		t.Errorf("without details: %+v", got)
	}

	// the foreground subagent ends with its tool_result; appended lines are picked up
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0)
	must(t, err)
	_, err = f.WriteString(`{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_fg","type":"tool_result","content":[{"type":"text","text":"tile_rect is used in…"}]}]},"timestamp":"2026-11-10T10:03:50.000Z"}` + "\n")
	must(t, err)
	must(t, f.Close())
	got = agentSummary(subagentsAt(path, "s1", "busy", false, agentNow))
	if want := []string{"toolu_bg|general-purpose|Pasar los tests|2 min|"}; !sameStrings(got, want) {
		t.Errorf("after the result: %q", got)
	}

	// once its transcript goes quiet the background one counts as finished
	got = agentSummary(subagentsAt(path, "s1", "ready", false, agentNow.Add(2*time.Minute)))
	if len(got) != 0 {
		t.Errorf("quiet background subagent: %q", got)
	}
}

func TestForegroundSubagentNeedsALiveSession(t *testing.T) {
	path := fixture(t, []string{lineAsk, lineTaskFg}, nil, nil)
	if got := subagentsAt(path, "s1", "wait", false, agentNow); len(got) != 1 {
		t.Fatalf("waiting for a permission the subagent asked: %+v", got)
	}
	// a session at rest has no foreground subagent: the launch is a leftover of an interrupted turn
	if got := subagentsAt(path, "s1", "idle", false, agentNow); len(got) != 0 {
		t.Errorf("idle: %+v", got)
	}
	if got := subagentsAt(path, "s1", "busy", false, agentNow); len(got) != 0 {
		t.Errorf("busy again: the old launch must stay forgotten: %+v", got)
	}
}

func TestBackgroundSubagentBeforeItsTranscript(t *testing.T) {
	path := fixture(t, []string{lineAsk, lineAgentBg, lineAgentBgResult}, nil, nil)
	launched := time.Date(2026, 11, 10, 10, 2, 0, 0, time.UTC)
	if got := subagentsAt(path, "s1", "idle", false, launched.Add(30*time.Second)); len(got) != 1 || got[0].Age != "30 s" {
		t.Errorf("just launched: %+v", got)
	}
	if got := subagentsAt(path, "s1", "idle", false, launched.Add(agentFresh+time.Second)); len(got) != 0 {
		t.Errorf("never showed up: %+v", got)
	}
}

func TestRunningFor(t *testing.T) {
	for d, want := range map[time.Duration]string{
		-time.Second:     "0 s",
		37 * time.Second: "30 s",
		3 * time.Minute:  "3 min",
		2 * time.Hour:    "2 h",
	} {
		if got := runningFor(d); got != want {
			t.Errorf("runningFor(%v) = %q, want %q", d, got, want)
		}
	}
}
