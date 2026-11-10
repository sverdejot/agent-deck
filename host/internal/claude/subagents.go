package claude

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"
)

// Subagents: what a session has handed to the Task / Agent tool and is still running.
//
// Claude Code doesn't document its transcripts, so this rests on what it is known to write and
// is read leniently (anything unexpected is skipped, never an error):
//
//   - The main transcript (<project>/<sessionId>.jsonl) logs every launch as an assistant
//     tool_use block named "Task" (older versions) or "Agent" (newer), whose input carries
//     description, subagent_type, prompt and, for background agents, run_in_background.
//   - A foreground subagent runs until a user line brings the matching tool_result
//     (tool_use_id). It can only be running while the session is: when the session is not busy
//     (or waiting for you, e.g. a permission asked by the subagent) a launch with no result is a
//     leftover of an interrupted turn and is dropped.
//   - A background subagent gets its tool_result straight away ("async launched", with its
//     agentId in toolUseResult or in the text), and a later message wrapped in
//     <task-notification> announces when it is done.
//   - Each subagent writes its own transcript to <project>/<sessionId>/subagents/agent-<agentId>.jsonl
//     (isSidechain lines carrying agentId); its first user line is the prompt it was given. A
//     background subagent whose file hasn't changed for agentFresh is taken as finished: that
//     covers notifications this code doesn't recognise. Older versions wrote these sidechain
//     lines into the main transcript itself: they are skipped there.
//
// Reading is incremental: each file is parsed once, then only the bytes appended since (Claude
// Code only ever appends), so this is cheap enough to run every second for every session.

// Subagent is a subagent running for a session.
type Subagent struct {
	ID          string // tool_use id of the launch
	Type        string // subagent_type: "Explore", "general-purpose"…
	Description string // the few words it was launched with
	Age         string // running for: "40 s", "3 min", "2 h"
	Now         string // what it is doing now ("Grep: tile_rect"); filled with details only
	Background  bool
	Started     time.Time
}

const (
	// agentFresh: a background subagent whose transcript hasn't been written for this long is
	// taken as finished; also how long one counts as running before its transcript shows up.
	agentFresh = 90 * time.Second

	// agentScanAge: subagent transcripts older than this are not even opened.
	agentScanAge = 30 * time.Minute

	// agentTail: the first time a transcript is read, only its last bytes are parsed. Launches
	// further back are assumed finished.
	agentTail = 4 << 20

	// agentForget: background launches older than this whose transcript went quiet are dropped.
	agentForget = time.Hour
)

// launch is a Task / Agent tool_use seen in the main transcript.
type launch struct {
	Subagent
	prompt  string
	agentID string
	result  bool // its tool_result arrived
	async   bool // …saying it went to the background
}

// agentLog is what is known of a main transcript.
type agentLog struct {
	offset   int64 // bytes parsed (whole lines only); -1 before the first read
	launches map[string]*launch
}

// agentFile is what is known of a subagent's own transcript.
type agentFile struct {
	offset  int64
	agentID string
	prompt  string // first user text: the prompt it was given
	now     string // its last tool call
}

type agentEntry struct {
	mod time.Time
	f   *agentFile
}

// agentMu guards the caches below: the poll loop and the summary goroutines both ask.
var agentMu sync.Mutex

var (
	agentLogs  = map[string]*agentLog{}  // by main transcript path
	agentFiles = map[string]*agentFile{} // by subagent transcript path
)

// Subagents lists the subagents running for a session, oldest first. state is the session's
// deck state (busy, wait, ready, idle). With details each one also says what it is doing now,
// read from its own transcript.
func Subagents(sessionID, state string, details bool) []Subagent {
	path, ok := transcript(sessionID)
	if !ok {
		return nil
	}
	return subagentsAt(path, sessionID, state, details, time.Now())
}

func subagentsAt(path, sessionID, state string, details bool, now time.Time) []Subagent {
	agentMu.Lock()
	defer agentMu.Unlock()
	lg := agentLogs[path]
	if lg == nil {
		lg = &agentLog{offset: -1, launches: map[string]*launch{}}
		agentLogs[path] = lg
	}
	lg.update(path)

	live := state == "busy" || state == "wait"
	ls := make([]*launch, 0, len(lg.launches))
	needFiles := details
	for id, l := range lg.launches {
		bg := l.Background || l.async
		if !bg && !live {
			delete(lg.launches, id) // a session at rest has no foreground subagent running
			continue
		}
		needFiles = needFiles || bg
		ls = append(ls, l)
	}
	if len(ls) == 0 {
		return nil
	}
	sort.Slice(ls, func(i, j int) bool {
		if !ls[i].Started.Equal(ls[j].Started) {
			return ls[i].Started.Before(ls[j].Started)
		}
		return ls[i].ID < ls[j].ID
	})

	var files []agentEntry
	if needFiles {
		files = scanAgentFiles(filepath.Join(filepath.Dir(path), sessionID, "subagents"), now)
	}
	claimed := map[*agentFile]bool{}
	var out []Subagent
	for _, l := range ls {
		var e *agentEntry
		if needFiles {
			e = l.match(files, claimed)
		}
		if l.Background || l.async {
			// running while its transcript keeps changing (or, before it shows up, for a while)
			last := l.Started
			if e != nil {
				last = e.mod
			}
			if now.Sub(last) > agentFresh {
				if now.Sub(l.Started) > agentForget {
					delete(lg.launches, l.ID)
				}
				continue
			}
		}
		a := l.Subagent
		a.Age = runningFor(now.Sub(l.Started))
		if details && e != nil {
			a.Now = e.f.now
		}
		out = append(out, a)
	}
	return out
}

// match finds the launch's own transcript: by agentId when known, else by its prompt.
func (l *launch) match(files []agentEntry, claimed map[*agentFile]bool) *agentEntry {
	for i := range files {
		if f := files[i].f; l.agentID != "" && f.agentID == l.agentID && !claimed[f] {
			claimed[f] = true
			return &files[i]
		}
	}
	p := strings.TrimSpace(l.prompt)
	if p == "" {
		return nil
	}
	for i := range files {
		if f := files[i].f; !claimed[f] && f.prompt != "" && samePrompt(p, strings.TrimSpace(f.prompt)) {
			claimed[f] = true
			return &files[i]
		}
	}
	return nil
}

// samePrompt: equal, or one wraps the other (the subagent may get it with extra context).
func samePrompt(a, b string) bool {
	if a == b {
		return true
	}
	if len(a) < 32 || len(b) < 32 {
		return false
	}
	return strings.Contains(a, b) || strings.Contains(b, a)
}

// runningFor says how long a subagent has been running, compactly: "40 s", "3 min", "2 h".
// Seconds go in steps of 10 so the list doesn't change every second.
func runningFor(d time.Duration) string {
	switch {
	case d < time.Minute:
		return fmt.Sprintf("%d s", int(max(d, 0).Seconds())/10*10)
	case d < time.Hour:
		return fmt.Sprintf("%d min", int(d.Minutes()))
	}
	return fmt.Sprintf("%d h", int(d.Hours()))
}

// ---- the main transcript -----------------------------------------------------------------

type agentLine struct {
	Type          string          `json:"type"`
	IsSidechain   bool            `json:"isSidechain"`
	Timestamp     string          `json:"timestamp"`
	AgentID       string          `json:"agentId"`
	ToolUseResult json.RawMessage `json:"toolUseResult"`
	Message       struct {
		Content json.RawMessage `json:"content"`
	} `json:"message"`
}

type agentBlock struct {
	Type      string          `json:"type"`
	ID        string          `json:"id"`
	Name      string          `json:"name"`
	Text      string          `json:"text"`
	Input     json.RawMessage `json:"input"`
	ToolUseID string          `json:"tool_use_id"`
	IsError   bool            `json:"is_error"`
	Content   json.RawMessage `json:"content"`
}

// update parses what was appended to the transcript since the last call.
func (lg *agentLog) update(path string) {
	f, err := os.Open(path)
	if err != nil {
		return
	}
	defer f.Close()
	st, err := f.Stat()
	if err != nil {
		return
	}
	size := st.Size()
	if size < lg.offset { // rewritten: start over
		lg.offset, lg.launches = -1, map[string]*launch{}
	}
	if size == lg.offset {
		return
	}
	start, skip := lg.offset, false
	if start < 0 {
		start = 0
		if size > agentTail {
			start, skip = size-agentTail, true // mid-line: the first one is cut
		}
	}
	if _, err := f.Seek(start, io.SeekStart); err != nil {
		return
	}
	r := bufio.NewReaderSize(f, 64<<10)
	pos := start
	for {
		line, err := r.ReadBytes('\n')
		if err != nil {
			break // EOF; a line still being written is read whole next time
		}
		pos += int64(len(line))
		if skip {
			skip = false
			continue
		}
		lg.parse(line)
	}
	lg.offset = pos
}

func (lg *agentLog) parse(line []byte) {
	pending := len(lg.launches) > 0
	notice := pending && bytes.Contains(line, []byte("task-notification"))
	if !bytes.Contains(line, []byte("tool_use")) && !notice { // tool_use, tool_use_id
		return
	}
	var l agentLine
	if json.Unmarshal(line, &l) != nil || l.IsSidechain {
		return
	}
	if notice {
		lg.notified(line)
	}
	ts, _ := time.Parse(time.RFC3339, l.Timestamp)
	for _, b := range agentBlocks(l.Message.Content) {
		switch {
		case l.Type == "assistant" && b.Type == "tool_use" && (b.Name == "Task" || b.Name == "Agent") && b.ID != "":
			lg.launched(b, ts)
		case l.Type == "user" && b.Type == "tool_result" && b.ToolUseID != "":
			lg.finished(b, l.ToolUseResult)
		}
	}
}

func (lg *agentLog) launched(b agentBlock, ts time.Time) {
	var in struct {
		Description  string          `json:"description"`
		SubagentType string          `json:"subagent_type"`
		Prompt       string          `json:"prompt"`
		Background   json.RawMessage `json:"run_in_background"` // true, or "true"
	}
	_ = json.Unmarshal(b.Input, &in)
	if ts.IsZero() {
		ts = time.Now()
	}
	bg := strings.EqualFold(strings.Trim(string(in.Background), `" `), "true")
	lg.launches[b.ID] = &launch{
		Subagent: Subagent{ID: b.ID, Type: in.SubagentType, Description: in.Description, Background: bg, Started: ts},
		prompt:   in.Prompt,
	}
}

var reAgentID = regexp.MustCompile(`agentId"?\s*[:=]\s*"?([A-Za-z0-9_-]+)`)

// finished handles a tool_result: the end of a foreground subagent, or the acknowledgement that
// a background one started.
func (lg *agentLog) finished(b agentBlock, extra json.RawMessage) {
	l := lg.launches[b.ToolUseID]
	if l == nil {
		return
	}
	l.result = true
	text := blockText(b.Content)
	var res map[string]any // toolUseResult: an object in recent versions, a string in others
	_ = json.Unmarshal(extra, &res)
	if id, ok := res["agentId"].(string); ok && id != "" {
		l.agentID = id
	} else if m := reAgentID.FindStringSubmatch(text); m != nil {
		l.agentID = m[1]
	}
	status, _ := res["status"].(string)
	isAsync, _ := res["isAsync"].(bool)
	lower := strings.ToLower(text)
	l.async = isAsync || strings.Contains(strings.ToLower(status), "async") ||
		strings.Contains(lower, "async agent") || strings.Contains(lower, "in the background")
	if b.IsError || !(l.async || l.Background) {
		delete(lg.launches, b.ToolUseID) // done (or failed to start)
	}
}

// notified handles a <task-notification>: the background subagents it names are done.
func (lg *agentLog) notified(line []byte) {
	for id, l := range lg.launches {
		if !l.result {
			continue
		}
		if bytes.Contains(line, []byte(id)) ||
			(l.agentID != "" && bytes.Contains(line, []byte(l.agentID))) ||
			(len(l.Description) >= 8 && bytes.Contains(line, []byte(l.Description))) {
			delete(lg.launches, id)
		}
	}
}

// agentBlocks returns the blocks of a message.content that is a list (a plain string has none).
func agentBlocks(raw json.RawMessage) []agentBlock {
	var blocks []agentBlock
	if len(raw) == 0 || raw[0] != '[' {
		return nil
	}
	_ = json.Unmarshal(raw, &blocks)
	return blocks
}

// blockText is the text of a content that is a string or a list of blocks.
func blockText(raw json.RawMessage) string {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s
	}
	var parts []string
	for _, b := range agentBlocks(raw) {
		if b.Type == "text" && b.Text != "" {
			parts = append(parts, b.Text)
		}
	}
	return strings.Join(parts, "\n")
}

// ---- the subagents' own transcripts -----------------------------------------------------------

// scanAgentFiles brings up to date the subagent transcripts of a session written recently.
func scanAgentFiles(dir string, now time.Time) []agentEntry {
	des, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var out []agentEntry
	for _, de := range des {
		name := de.Name()
		if de.IsDir() || !strings.HasPrefix(name, "agent-") || !strings.HasSuffix(name, ".jsonl") {
			continue
		}
		info, err := de.Info()
		if err != nil || now.Sub(info.ModTime()) > agentScanAge {
			continue
		}
		p := filepath.Join(dir, name)
		af := agentFiles[p]
		if af == nil {
			af = &agentFile{agentID: strings.TrimSuffix(strings.TrimPrefix(name, "agent-"), ".jsonl")}
			agentFiles[p] = af
		}
		if info.Size() != af.offset {
			af.update(p, info.Size())
		}
		out = append(out, agentEntry{mod: info.ModTime(), f: af})
	}
	return out
}

// update reads what was appended: the prompt from the first lines, then only the tail.
func (af *agentFile) update(path string, size int64) {
	f, err := os.Open(path)
	if err != nil {
		return
	}
	defer f.Close()
	if size < af.offset {
		*af = agentFile{agentID: af.agentID}
	}
	pos := af.offset
	if _, err := f.Seek(pos, io.SeekStart); err != nil {
		return
	}
	r := bufio.NewReaderSize(f, 64<<10)
	if pos == 0 {
		for i := 0; i < 5 && af.prompt == ""; i++ {
			line, err := r.ReadBytes('\n')
			if err != nil {
				return // not even the first lines yet: try again next time
			}
			pos += int64(len(line))
			af.parse(line)
		}
		if size-pos > agentTail {
			pos = size - agentTail
			if _, err := f.Seek(pos, io.SeekStart); err != nil {
				return
			}
			r.Reset(f)
			cut, err := r.ReadBytes('\n') // mid-line
			pos += int64(len(cut))
			if err != nil {
				af.offset = pos
				return
			}
		}
	}
	for {
		line, err := r.ReadBytes('\n')
		if err != nil {
			break
		}
		pos += int64(len(line))
		af.parse(line)
	}
	af.offset = pos
}

func (af *agentFile) parse(line []byte) {
	if af.prompt != "" && !bytes.Contains(line, []byte("tool_use")) {
		return
	}
	var l agentLine
	if json.Unmarshal(line, &l) != nil {
		return
	}
	if l.AgentID != "" {
		af.agentID = l.AgentID
	}
	switch l.Type {
	case "user":
		if af.prompt == "" {
			af.prompt = blockText(l.Message.Content)
		}
	case "assistant":
		for _, b := range agentBlocks(l.Message.Content) {
			if b.Type == "tool_use" {
				af.now = describeTool(contentBlock{Type: b.Type, Name: b.Name, Input: b.Input})
			}
		}
	}
}
