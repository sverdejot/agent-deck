package claude

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"strings"
	"time"
)

// Summary is what the deck shows when a tile is held: built from the transcript, no model call.
type Summary struct {
	Meta   string // "Empezó hace 3 h · 12 mensajes · esp"
	You    string // your last message
	Claude string // Claude's last text reply
	Tool   string // tool running right now (busy sessions)
}

type transcriptLine struct {
	Type      string `json:"type"`
	IsMeta    bool   `json:"isMeta"`
	Timestamp string `json:"timestamp"`
	Message   struct {
		Content json.RawMessage `json:"content"`
	} `json:"message"`
}

type contentBlock struct {
	Type      string          `json:"type"`
	Text      string          `json:"text"`
	Name      string          `json:"name"`
	Input     json.RawMessage `json:"input"`
	ID        string          `json:"id"`          // tool_use
	ToolUseID string          `json:"tool_use_id"` // tool_result
}

// Summarize reads the session's transcript.
func Summarize(s Session) Summary {
	sum := Summary{Meta: s.Place}
	path, ok := transcript(s.SessionID)
	if !ok {
		return sum
	}
	f, err := os.Open(path)
	if err != nil {
		return sum
	}
	defer f.Close()

	var first time.Time
	turns := 0
	lastWasTool := false
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 64<<20)
	for sc.Scan() {
		var l transcriptLine
		if json.Unmarshal(sc.Bytes(), &l) != nil || (l.Type != "user" && l.Type != "assistant") || l.IsMeta {
			continue
		}
		if first.IsZero() {
			first, _ = time.Parse(time.RFC3339, l.Timestamp)
		}
		for _, b := range parseContent(l.Message.Content) {
			switch {
			case l.Type == "user" && b.Type == "text" && isHumanText(b.Text):
				sum.You = b.Text
				turns++
				lastWasTool = false
			case l.Type == "assistant" && b.Type == "text" && strings.TrimSpace(b.Text) != "":
				sum.Claude = b.Text
				lastWasTool = false
			case l.Type == "assistant" && b.Type == "tool_use":
				sum.Tool = describeTool(b)
				lastWasTool = true
			}
		}
	}
	if s.State != "busy" || !lastWasTool {
		sum.Tool = ""
	}
	var meta []string
	if !first.IsZero() {
		meta = append(meta, "Empezó "+Ago(first))
	}
	if turns == 1 {
		meta = append(meta, "1 mensaje")
	} else if turns > 1 {
		meta = append(meta, fmt.Sprintf("%d mensajes", turns))
	}
	meta = append(meta, s.Place)
	if s.Age != "" {
		meta = append(meta, "activa "+s.Age)
	}
	sum.Meta = strings.Join(meta, " · ")
	sum.You = plain(sum.You)
	sum.Claude = plain(sum.Claude)
	return sum
}

// parseContent accepts both forms of message.content: a string or a list of blocks.
func parseContent(raw json.RawMessage) []contentBlock {
	var str string
	if json.Unmarshal(raw, &str) == nil {
		return []contentBlock{{Type: "text", Text: str}}
	}
	var blocks []contentBlock
	_ = json.Unmarshal(raw, &blocks)
	return blocks
}

// isHumanText filters out what the CLI injects as "user" text: command wrappers, reminders, hooks.
func isHumanText(t string) bool {
	t = strings.TrimSpace(t)
	return t != "" && !strings.HasPrefix(t, "<") && !strings.HasPrefix(t, "[Request interrupted")
}

func describeTool(b contentBlock) string {
	var in map[string]any
	_ = json.Unmarshal(b.Input, &in)
	for _, k := range []string{"description", "command", "file_path", "pattern", "url", "prompt"} {
		if v, ok := in[k].(string); ok && v != "" {
			if k == "file_path" {
				v = ShortPath(v)
			}
			return b.Name + ": " + v
		}
	}
	return b.Name
}

var (
	reCode   = regexp.MustCompile("(?s)```.*?```")
	reMarks  = regexp.MustCompile("[*_`#>|]+")
	reBullet = regexp.MustCompile(`(?m)^\s*([-•]|\d+\.)\s+`)
	reSpaces = regexp.MustCompile(`\s+`)
)

// plain turns markdown into one line of plain text for the small screen.
func plain(s string) string {
	s = reCode.ReplaceAllString(s, " [código] ")
	s = reBullet.ReplaceAllString(s, "· ")
	s = reMarks.ReplaceAllString(s, "")
	return strings.TrimSpace(reSpaces.ReplaceAllString(s, " "))
}
