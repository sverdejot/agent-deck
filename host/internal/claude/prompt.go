package claude

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"slices"
	"strconv"
	"strings"
)

// Prompt is a select menu Claude Code shows and waits on: a question (AskUserQuestion), a tool
// permission or a plan approval.
type Prompt struct {
	Question string
	Options  []Option
}

// Option is one choice of a Prompt, numbered as Claude Code shows it: typing the number picks it.
type Option struct {
	N     int
	Label string
}

// Key identifies the menu (question and options), to notice when it changes.
func (p Prompt) Key() string {
	var b strings.Builder
	b.WriteString(p.Question)
	for _, o := range p.Options {
		fmt.Fprintf(&b, "\x00%d.%s", o.N, o.Label)
	}
	return b.String()
}

const (
	menuBelow     = 6 // most non-blank lines under a menu (key hints, box bottom, the statusline)
	menuGap       = 6 // most lines between two options (descriptions, wrapped labels)
	questionLines = 4 // most lines of question kept
)

var (
	// "❯ 1. Yes", "2. No, and tell Claude what to do differently (esc)" (frame and indent removed)
	reOption = regexp.MustCompile(`^([❯›>]\s*)?(\d{1,2})\.\s+(\S.*)$`)

	// key hint at the end of a label: "(esc)", "(shift+tab)"
	reKeyHint = regexp.MustCompile(`\s*\((esc|tab|enter|shift\+tab|ctrl\+[a-z])\)$`)

	// multiple choice: "[ ] Postgres", "[✔] SQLite", "☐ Redis"
	reCheckbox = regexp.MustCompile(`^(\[[ xX✔✓]\]|[☐☑☒])`)
)

// ParseMenu finds the select menu at the bottom of a terminal screen (tmux capture-pane): options
// numbered 1…n with the ❯ pointer on one of them, and the question right above. Options that open
// a text field ("Type something") are left out. Multiple-choice menus (toggled, then submitted)
// are not answerable with one key and give false.
func ParseMenu(screen string) (Prompt, bool) {
	lines := strings.Split(screen, "\n")
	for i, l := range lines {
		lines[i] = unbox(l)
	}

	// last option, near the bottom: only hints and the like go under a menu
	last, below := -1, 0
	for i := len(lines) - 1; i >= 0 && below <= menuBelow; i-- {
		if _, _, _, ok := optionLine(lines[i]); ok {
			last = i
			break
		}
		if lines[i] != "" {
			below++
		}
	}
	if last < 0 {
		return Prompt{}, false
	}

	// up through options n, n-1 … 1; descriptions and wrapped labels may sit between them
	var opts []Option
	pointer := false
	first, want, gap := -1, 0, 0
	for i := last; i >= 0 && first < 0; i-- {
		n, label, cur, ok := optionLine(lines[i])
		if !ok {
			gap++
			if gap > menuGap {
				return Prompt{}, false
			}
			continue
		}
		if want != 0 && n != want {
			return Prompt{}, false
		}
		gap, want = 0, n-1
		pointer = pointer || cur
		opts = append(opts, Option{N: n, Label: label})
		if n == 1 {
			first = i
		}
	}
	if first < 0 || !pointer || len(opts) < 2 {
		return Prompt{}, false
	}
	slices.Reverse(opts)

	var keep []Option
	for _, o := range opts {
		if reCheckbox.MatchString(o.Label) {
			return Prompt{}, false
		}
		if !needsTyping(o.Label) {
			keep = append(keep, o)
		}
	}
	if len(keep) == 0 {
		return Prompt{}, false
	}
	return Prompt{Question: question(lines[:first]), Options: keep}, true
}

// unbox drops the frame Claude Code may draw around a dialog ("│ … │") and the indentation.
func unbox(l string) string {
	return strings.Trim(l, " \t\r │┃")
}

// isRule is a horizontal line or the top/bottom of a box.
func isRule(l string) bool {
	return l != "" && strings.Trim(l, "─━═╌┄╭╮╰╯┌┐└┘ ") == ""
}

func optionLine(l string) (n int, label string, pointer, ok bool) {
	m := reOption.FindStringSubmatch(l)
	if m == nil {
		return 0, "", false, false
	}
	n, _ = strconv.Atoi(m[2])
	label = strings.TrimSpace(reKeyHint.ReplaceAllString(strings.TrimSpace(m[3]), ""))
	return n, label, m[1] != "", label != ""
}

// needsTyping: options that open a text field, useless without a keyboard.
func needsTyping(label string) bool {
	l := strings.ToLower(strings.TrimRight(label, ". "))
	return l == "other" || strings.HasPrefix(l, "type something") || strings.HasPrefix(l, "type here") ||
		strings.HasPrefix(l, "chat about this")
}

// question is the paragraph right above a menu. Claude Code's bare "Do you want to proceed?" gets
// the first line of the paragraph above it: the command or tool call it asks about.
func question(above []string) string {
	para, rest := paragraph(above)
	q := strings.Join(para, " ")
	if q == "" {
		return "Claude espera tu respuesta"
	}
	if strings.HasPrefix(q, "Do you want to proceed") {
		if ctx, _ := paragraph(rest); len(ctx) > 0 {
			q = ctx[0] + " · " + q
		}
	}
	return q
}

// paragraph is the last block of non-blank lines (at most questionLines, stopping at a rule) and
// the lines above it.
func paragraph(lines []string) (para, rest []string) {
	end := len(lines)
	for end > 0 && lines[end-1] == "" {
		end--
	}
	start := end
	for start > 0 && lines[start-1] != "" && !isRule(lines[start-1]) && end-start < questionLines {
		start--
	}
	return lines[start:end], lines[:start]
}

// Enrich takes the question from the transcript when the menu is one of the questions of a
// pending AskUserQuestion call: the screen may wrap or cut it.
func Enrich(s Session, p Prompt) Prompt {
	for _, q := range pendingAsk(s.SessionID) {
		if sameOptions(q.Options, p.Options) && strings.TrimSpace(q.Question) != "" {
			p.Question = q.Question
			break
		}
	}
	return p
}

type askQuestion struct {
	Question string      `json:"question"`
	Options  []askOption `json:"options"`
}

type askOption struct {
	Label string `json:"label"`
}

// pendingAsk is the questions of the last AskUserQuestion call of the transcript, unless it has
// its answer (tool_result) already.
func pendingAsk(sessionID string) []askQuestion {
	path, ok := transcript(sessionID)
	if !ok {
		return nil
	}
	f, err := os.Open(path)
	if err != nil {
		return nil
	}
	defer f.Close()

	var asked []askQuestion
	askID := ""
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 64<<20)
	for sc.Scan() {
		line := sc.Bytes()
		if !bytes.Contains(line, []byte(`"AskUserQuestion"`)) && (askID == "" || !bytes.Contains(line, []byte(askID))) {
			continue
		}
		var l transcriptLine
		if json.Unmarshal(line, &l) != nil {
			continue
		}
		for _, b := range parseContent(l.Message.Content) {
			switch {
			case b.Type == "tool_use" && b.Name == "AskUserQuestion":
				var in struct {
					Questions []askQuestion `json:"questions"`
				}
				if json.Unmarshal(b.Input, &in) == nil {
					askID, asked = b.ID, in.Questions
				}
			case b.Type == "tool_result" && askID != "" && b.ToolUseID == askID:
				askID, asked = "", nil
			}
		}
	}
	return asked
}

// sameOptions: every option on screen is one of the question's (the screen may cut labels).
func sameOptions(asked []askOption, shown []Option) bool {
	if len(shown) == 0 {
		return false
	}
	for _, o := range shown {
		label := strings.TrimSuffix(o.Label, "…")
		if !slices.ContainsFunc(asked, func(a askOption) bool { return strings.HasPrefix(a.Label, label) }) {
			return false
		}
	}
	return true
}
