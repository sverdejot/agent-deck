package claude

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

// Screens as `tmux capture-pane -p` returns them.
const (
	screenPermission = `⏺ Borro la carpeta de build y vuelvo a compilar.

⏺ Bash(rm -rf build)
  ⎿  Running…

╭──────────────────────────────────────────────────────────────────────────────╮
│ Bash command                                                                 │
│                                                                              │
│   rm -rf build                                                               │
│   Remove the build directory                                                 │
│                                                                              │
│ Do you want to proceed?                                                      │
│ ❯ 1. Yes                                                                     │
│   2. Yes, and don't ask again for rm commands in /Users/sam/esp              │
│   3. No, and tell Claude what to do differently (esc)                        │
╰──────────────────────────────────────────────────────────────────────────────╯
`

	screenAsk = `⏺ Antes de seguir necesito saber dónde van los iconos.

────────────────────────────────────────────────────────────────────────────────
 ←  ☐ Iconos  ✔ Submit  →

¿Dónde guardo los iconos de la PWA?

❯ 1. En /static
     Servidos por Glance, sin paso de build
  2. En un bucket de S3
     Con su propia caché delante
  3. Generarlos en el build
  4. Type something.

Enter to select · Tab/Arrow keys to navigate · Esc to cancel
`

	screenPlan = `⏺ User approved Claude's plan? Not yet.

────────────────────────────────────────────────────────────────────────────────
 Ready to code?

 Here is Claude's plan:
╭──────────────────────────────────────────────────────────────────────────────╮
│ Arreglar el arranque del panel                                               │
│                                                                              │
│ 1. Inicializar el expansor de E/S antes del bus SPI                          │
│ 2. Probar en el deck                                                         │
╰──────────────────────────────────────────────────────────────────────────────╯

 Would you like to proceed?

 ❯ 1. Yes, and auto-accept edits (shift+tab)
   2. Yes, and manually approve edits
   3. No, keep planning
   4. Type here to tell Claude what to change

 ctrl-g to edit in Vim
`

	screenIdle = `⏺ Hecho. Cambios:
  1. Muevo la inicialización del expansor al principio
  2. Pruebo de nuevo en el deck

╭──────────────────────────────────────────────────────────────────────────────╮
│ >                                                                            │
╰──────────────────────────────────────────────────────────────────────────────╯
  ⏵⏵ accept edits on (shift+tab to cycle)
`

	screenMulti = `¿Qué bases de datos soportamos?

❯ 1. [ ] Postgres
  2. [✔] SQLite
  3. [ ] Type something

Enter to select · Esc to cancel
`
)

func TestParseMenu(t *testing.T) {
	cases := []struct {
		name   string
		screen string
		want   Prompt
		ok     bool
	}{
		{"permission", screenPermission, Prompt{
			Question: "rm -rf build · Do you want to proceed?",
			Options:  []Option{
				{1, "Yes"},
				{2, "Yes, and don't ask again for rm commands in /Users/sam/esp"},
				{3, "No, and tell Claude what to do differently"},
			},
		}, true},
		{"AskUserQuestion", screenAsk, Prompt{
			Question: "¿Dónde guardo los iconos de la PWA?",
			Options:  []Option{{1, "En /static"}, {2, "En un bucket de S3"}, {3, "Generarlos en el build"}},
		}, true},
		{"plan approval", screenPlan, Prompt{
			Question: "Would you like to proceed?",
			Options:  []Option{
				{1, "Yes, and auto-accept edits"},
				{2, "Yes, and manually approve edits"},
				{3, "No, keep planning"},
			},
		}, true},
		{"numbered list, no menu", screenIdle, Prompt{}, false},
		{"multiple choice", screenMulti, Prompt{}, false},
		{"empty", "", Prompt{}, false},
	}
	for _, c := range cases {
		got, ok := ParseMenu(c.screen)
		if ok != c.ok || !reflect.DeepEqual(got, c.want) {
			t.Errorf("%s: got %+v, %v; want %+v, %v", c.name, got, ok, c.want, c.ok)
		}
	}
}

func TestParseMenuOnlyAtTheBottom(t *testing.T) {
	// a menu that scrolled up, with Claude's output under it, is not waiting any more
	screen := screenAsk + "\n⏺ Vale, en /static.\n\n⏺ Write(static/icon-192.png)\n  ⎿  Wrote 1 line\n" +
		"\n⏺ Write(static/icon-512.png)\n  ⎿  Wrote 1 line\n\n⏺ Listo.\n"
	if p, ok := ParseMenu(screen); ok {
		t.Errorf("stale menu parsed: %+v", p)
	}
}

func TestEnrichFromTranscript(t *testing.T) {
	old := Home
	Home = t.TempDir()
	t.Cleanup(func() { Home = old })
	dir := filepath.Join(Home, ".claude", "projects", "-Users-sam-glance")
	must(t, os.MkdirAll(dir, 0o755))
	ask := `{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion",` +
		`"input":{"questions":[{"question":"¿Dónde guardo los iconos de la PWA, para que Glance los sirva?",` +
		`"options":[{"label":"En /static"},{"label":"En un bucket de S3"},{"label":"Generarlos en el build"}]}]}}]}}`
	path := filepath.Join(dir, "s1.jsonl")
	must(t, os.WriteFile(path, []byte(ask+"\n"), 0o644))

	shown, _ := ParseMenu(screenAsk)
	got := Enrich(Session{SessionID: "s1"}, shown)
	if got.Question != "¿Dónde guardo los iconos de la PWA, para que Glance los sirva?" {
		t.Errorf("pending question: %q", got.Question)
	}

	answered := ask + "\n" + `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"En /static"}]}}`
	must(t, os.WriteFile(path, []byte(answered+"\n"), 0o644))
	if got := Enrich(Session{SessionID: "s1"}, shown); got.Question != shown.Question {
		t.Errorf("answered question still used: %q", got.Question)
	}
}
