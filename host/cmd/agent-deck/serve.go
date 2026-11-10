package main

import (
	"bufio"
	"fmt"
	"log"
	"os"
	"strings"
	"sync"
	"time"

	"go.bug.st/serial"

	"github.com/sverdejot/agent-deck/host/internal/claude"
	"github.com/sverdejot/agent-deck/host/internal/deck"
	"github.com/sverdejot/agent-deck/host/internal/macos"
)

const (
	pollEvery      = time.Second      // sessions are re-read this often; sent when they change
	keepaliveEvery = 3 * time.Second  // …and at least this often, so the deck knows we're here
	slowEvery      = 20 * time.Second // Limits and Usage change slowly
	retryEvery     = 2 * time.Second
	watchFor       = 65 * time.Second        // the deck closes a summary after 30-60 s
	answerPause    = 1500 * time.Millisecond // after an answer, let Claude Code move on before looking again
	agentsEvery    = 2 * time.Second         // subagents of the open summary are refreshed this often…
	summaryFor     = 30 * time.Second        // …for as long as the deck keeps a summary open
)

// run waits for the deck, serves it while connected, and starts over when it goes away.
func run(portOverride string, verbose bool) {
	waiting := false
	for {
		name := deck.FindPort(portOverride)
		if name == "" {
			if !waiting {
				log.Print("esperando al deck (USB)…")
				waiting = true
			}
			time.Sleep(retryEvery)
			continue
		}
		waiting = false
		p, err := deck.Open(name)
		if err != nil {
			log.Printf("abrir %s: %v", name, err)
			time.Sleep(retryEvery)
			continue
		}
		log.Printf("conectado a %s", name)
		err = (&conn{port: p, name: name, verbose: verbose}).serve()
		_ = p.Close()
		log.Printf("desconectado: %v", err)
		time.Sleep(time.Second)
	}
}

// conn is one connection to the deck.
type conn struct {
	port    serial.Port
	name    string
	verbose bool

	writeMu sync.Mutex // the poll loop and summary goroutines both write

	mu       sync.Mutex
	sessions []claude.Session // last sent, to resolve the ids the deck sends back

	// the summary the deck asked for last (R), whose subagent list is kept up to date
	summaryID    string
	summaryUntil time.Time
	agentsSent   string
	agentsAt     time.Time

	last   string
	sentAt time.Time
	slowAt time.Time

	// the menu (question) of the session whose summary is open on the deck; serve loop only
	watch      string        // session id, "" none
	watchUntil time.Time     // the deck has closed the summary by then
	qPause     time.Time     // just answered: no looking until then
	qLine      string        // last Q sent
	qKey       string        // Key of the menu on screen it was built from
	qSent      claude.Prompt // what it offered
}

func (c *conn) write(s string) error {
	c.writeMu.Lock()
	defer c.writeMu.Unlock()
	_, err := c.port.Write([]byte(s))
	return err
}

// send pushes the session snapshot when it changed (or force), plus Limits and Usage every 20 s.
func (c *conn) send(force bool) error {
	ss := claude.Sessions()
	msg := deck.Snapshot(ss, func(s claude.Session) int { return claude.ContextOf(s.SessionID) })
	c.mu.Lock()
	c.sessions = ss
	c.mu.Unlock()
	if !force && msg == c.last && time.Since(c.sentAt) < keepaliveEvery {
		return nil
	}
	if c.verbose || msg != c.last {
		log.Printf("%d sesiones", len(ss))
	}
	c.last, c.sentAt = msg, time.Now()
	if force || time.Since(c.slowAt) > slowEvery {
		msg += deck.LimitsLine(claude.ReadLimits()) + deck.UsageLines(claude.ReadUsage())
		c.slowAt = time.Now()
	}
	return c.write(msg)
}

// refreshAgents re-sends the subagents of the summary open on the deck when they changed.
func (c *conn) refreshAgents() error {
	c.mu.Lock()
	id, open, due := c.summaryID, time.Now().Before(c.summaryUntil), time.Since(c.agentsAt) >= agentsEvery
	c.mu.Unlock()
	if !open || !due {
		return nil
	}
	s, ok := c.find(id)
	if !ok {
		return nil
	}
	msg := deck.AgentLines(s.ID, claude.Subagents(s.SessionID, s.State, true))
	c.mu.Lock()
	changed := msg != c.agentsSent
	c.agentsSent, c.agentsAt = msg, time.Now()
	c.mu.Unlock()
	if !changed {
		return nil
	}
	return c.write(msg)
}

func (c *conn) find(id string) (claude.Session, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	for _, s := range c.sessions {
		if s.ID == id {
			return s, true
		}
	}
	return claude.Session{}, false
}

func (c *conn) serve() error {
	lines := make(chan string)
	readErr := make(chan error, 1)
	go func() {
		sc := bufio.NewScanner(c.port)
		for sc.Scan() {
			lines <- strings.TrimSpace(sc.Text())
		}
		readErr <- fmt.Errorf("lectura: %w", sc.Err())
	}()

	if err := c.send(true); err != nil {
		return err
	}
	tick := time.NewTicker(pollEvery)
	defer tick.Stop()
	for {
		select {
		case err := <-readErr:
			return err
		case <-tick.C:
			if err := c.send(false); err != nil {
				return err
			}
			if err := c.refreshAgents(); err != nil {
				return err
			}
			if err := c.pollQuestion(); err != nil {
				return err
			}
			if _, err := os.Stat(c.name); err != nil {
				return fmt.Errorf("puerto desaparecido")
			}
		case l := <-lines:
			if err := c.handle(l); err != nil {
				return err
			}
		}
	}
}

// handle reacts to a line from the deck: H (hello), O id (open), R id (summary, then its
// subagents, kept up to date by refreshAgents, and its pending question, by pollQuestion), A id n
// label (answer). Anything else (boot messages, D stats) is ignored.
func (c *conn) handle(l string) error {
	cmd, id, _ := strings.Cut(l, "\t")
	switch cmd {
	case "H":
		return c.send(true)
	case "R":
		if s, ok := c.find(id); ok {
			c.mu.Lock()
			c.summaryID, c.summaryUntil, c.agentsAt = s.ID, time.Now().Add(summaryFor), time.Now()
			c.mu.Unlock()
			go func() {
				agents := deck.AgentLines(s.ID, claude.Subagents(s.SessionID, s.State, true))
				c.mu.Lock()
				c.agentsSent = agents
				c.mu.Unlock()
				_ = c.write(deck.SummaryLine(s.ID, claude.Summarize(s)) + agents)
			}()
			c.watch, c.watchUntil = s.ID, time.Now().Add(watchFor)
			c.qLine, c.qKey, c.qSent = "", "", claude.Prompt{}
			return c.pollQuestion()
		}
	case "A":
		c.answer(l)
	case "O":
		s, ok := c.find(id)
		if !ok {
			log.Printf("abrir %s: no encontrada", id)
			return nil
		}
		log.Printf("abrir %s", s.Title)
		go func() {
			if err := macos.Focus(s.TmuxPane, s.TTY); err != nil {
				log.Printf("abrir %s: %v", s.Title, err)
			}
		}()
	}
	return nil
}

// pollQuestion sends Q when the menu the watched session waits on changes (or goes away). The
// menu is read from its tmux pane, so sessions outside tmux never get one.
func (c *conn) pollQuestion() error {
	now := time.Now()
	if c.watch == "" || now.After(c.watchUntil) {
		c.watch = ""
		return nil
	}
	if now.Before(c.qPause) {
		return nil
	}
	var p claude.Prompt
	key := ""
	if s, ok := c.find(c.watch); ok && s.TmuxPane != "" {
		if m, ok := menuOn(s.TmuxPane); ok {
			key = m.Key()
			if key == c.qKey {
				p = c.qSent // unchanged: skip reading the transcript again
			} else {
				p = claude.Enrich(s, m)
			}
		}
	}
	c.qKey, c.qSent = key, p
	line := deck.QuestionLine(c.watch, p)
	if line == c.qLine {
		return nil
	}
	c.qLine = line
	if c.verbose {
		log.Printf("pregunta: %q", strings.TrimSpace(line))
	}
	return c.write(line)
}

// answer types the option the deck picked into the session's pane, if the menu it was offered
// from is still on screen.
func (c *conn) answer(line string) {
	a, ok := deck.ParseAnswer(line)
	if !ok {
		return
	}
	s, ok := c.find(a.ID)
	if !ok || s.TmuxPane == "" || a.ID != c.watch {
		log.Printf("responder %s: no encontrada", a.ID)
		return
	}
	c.watchUntil = time.Now().Add(watchFor)
	c.qLine = "" // the deck dropped its copy: send it what is on screen next, even if it looks the same
	m, ok := menuOn(s.TmuxPane)
	if !ok || m.Key() != c.qKey || !offered(c.qSent, a) {
		log.Printf("responder %s: la pregunta ya no está en pantalla", s.Title)
		return
	}
	if err := macos.Answer(s.TmuxPane, a.N); err != nil {
		log.Printf("responder %s: %v", s.Title, err)
		return
	}
	log.Printf("respuesta %d (%s) a %s", a.N, a.Label, s.Title)
	c.qPause = time.Now().Add(answerPause)
}

// menuOn is the menu Claude Code is waiting on in a tmux pane.
func menuOn(pane string) (claude.Prompt, bool) {
	screen, err := macos.Screen(pane)
	if err != nil {
		return claude.Prompt{}, false
	}
	return claude.ParseMenu(screen)
}

// offered: a is one of the options of p, as the deck got it.
func offered(p claude.Prompt, a deck.Answer) bool {
	for _, o := range p.Options {
		if o.N == a.N && deck.OptionLabel(o.Label) == a.Label {
			return true
		}
	}
	return false
}
