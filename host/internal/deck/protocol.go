// Package deck speaks the serial protocol of the deck (see docs/protocol.md) and finds its port.
package deck

import (
	"fmt"
	"strings"

	"github.com/sverdejot/agent-deck/host/internal/claude"
)

// Field limits, in bytes: what the deck keeps of each value.
const (
	maxID     = 70
	maxTitle  = 94
	maxPlace  = 70
	maxAge    = 22
	maxMeta   = 150
	maxText   = 600
	maxString = 100
)

// Snapshot is the full list of sessions: B, one S per session, E.
// ctx returns the context-window use (0-100, -1 unknown) of a session.
func Snapshot(sessions []claude.Session, ctx func(claude.Session) int) string {
	var b strings.Builder
	b.WriteString("B\n")
	for _, s := range sessions {
		fmt.Fprintf(&b, "S\t%s\tcode\t%s\t%s\t%s\t%s\t%d\n",
			Clean(s.ID, maxID), s.State, Clean(s.Title, maxTitle), Clean(s.Place, maxPlace), Clean(s.Age, maxAge), ctx(s))
	}
	b.WriteString("E\n")
	return b.String()
}

// SummaryLine answers an R request.
func SummaryLine(id string, sum claude.Summary) string {
	return fmt.Sprintf("Y\t%s\t%s\t%s\t%s\t%s\n",
		Clean(id, maxID), Clean(sum.Meta, maxMeta), Clean(sum.You, maxText), Clean(sum.Claude, maxText), Clean(sum.Tool, maxMeta))
}

// LimitsLine feeds the Limits screen.
func LimitsLine(l claude.Limits) string {
	f := []string{"L", fmt.Sprint(l.Five), l.FiveReset, fmt.Sprint(l.FiveElapsed),
		fmt.Sprint(l.Week), l.WeekReset, fmt.Sprint(l.WeekElapsed), l.Projection, l.Updated}
	for i := range f {
		f[i] = Clean(f[i], maxString)
	}
	return strings.Join(f, "\t") + "\n"
}

// UsageLines feeds the Usage screen: C B, C H, one C S per repo, C D, C E. Values go in
// thousands of tokens.
func UsageLines(u claude.Usage) string {
	var b strings.Builder
	b.WriteString("C\tB\n")
	sub := "tokens hoy"
	if len(u.Series) > 0 {
		sub = fmt.Sprintf("tokens hoy · %s el que más", u.Series[0].Name)
	}
	fmt.Fprintf(&b, "C\tH\t%d\t%d\t%s\t%s\n", u.Start, u.N, claude.HumanTokens(u.Total), Clean(sub, 60))
	for _, s := range u.Series {
		vals := make([]string, len(s.Values))
		for i, v := range s.Values {
			vals[i] = fmt.Sprint(v / 1000)
		}
		fmt.Fprintf(&b, "C\tS\t%s\t%s\t%s\n", Clean(s.Name, 30), claude.HumanTokens(s.Total), strings.Join(vals, ","))
	}
	labels := make([]string, len(u.Days))
	vals := make([]string, len(u.Days))
	for i, d := range u.Days {
		labels[i], vals[i] = d.Label, fmt.Sprint(d.Tokens/1000)
	}
	fmt.Fprintf(&b, "C\tD\t%s\t%s\t%d\n", strings.Join(labels, ","), strings.Join(vals, ","), max(len(u.Days)-1, 0))
	b.WriteString("C\tE\n")
	return b.String()
}

// Clean makes a value safe for the line protocol (no TAB/CR/LF) and cuts it to n bytes on a
// UTF-8 boundary.
func Clean(s string, n int) string {
	s = strings.NewReplacer("\t", " ", "\n", " ", "\r", " ").Replace(s)
	if len(s) <= n {
		return s
	}
	for n > 0 && (s[n]&0xC0) == 0x80 {
		n--
	}
	return s[:n]
}
