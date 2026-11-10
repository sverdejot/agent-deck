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

	last   string
	sentAt time.Time
	slowAt time.Time
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

// handle reacts to a line from the deck: H (hello), O id (open), R id (summary). Anything else
// (boot messages, D stats) is ignored.
func (c *conn) handle(l string) error {
	cmd, id, _ := strings.Cut(l, "\t")
	switch cmd {
	case "H":
		return c.send(true)
	case "R":
		if s, ok := c.find(id); ok {
			go func() { _ = c.write(deck.SummaryLine(s.ID, claude.Summarize(s))) }()
		}
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
