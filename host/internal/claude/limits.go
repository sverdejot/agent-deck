package claude

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"time"
)

type window struct {
	UsedPercentage float64 `json:"used_percentage"`
	ResetsAt       int64   `json:"resets_at"`
}

type rateLimits struct {
	FiveHour *window `json:"five_hour"`
	SevenDay *window `json:"seven_day"`
}

// Limits is what the Limits screen shows; strings come preformatted (the deck has no clock).
type Limits struct {
	Five, Week               int    // % used, -1 if unknown
	FiveReset, WeekReset     string // "resets 18:30"
	FiveElapsed, WeekElapsed int    // % of the window already gone (pace marker)
	Projection               string
	Updated                  string // "2 min ago"
}

const (
	fiveHours = 5 * time.Hour
	sevenDays = 7 * 24 * time.Hour
)

// ReadLimits reads the plan rate limits recorded from the statusline.
func ReadLimits() Limits {
	l := Limits{Five: -1, Week: -1}
	f, err := os.Open(limitsLog())
	if err != nil {
		return l
	}
	defer f.Close()
	type sample struct {
		t    time.Time
		five window
	}
	var hist []sample
	var last rateLimits
	var lastT time.Time
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		var e struct {
			T          int64      `json:"t"`
			RateLimits rateLimits `json:"rate_limits"`
		}
		if json.Unmarshal(sc.Bytes(), &e) != nil {
			continue
		}
		last, lastT = e.RateLimits, time.Unix(e.T, 0)
		if e.RateLimits.FiveHour != nil {
			hist = append(hist, sample{lastT, *e.RateLimits.FiveHour})
		}
	}
	if lastT.IsZero() {
		return l
	}
	l.Updated = Ago(lastT)
	now := time.Now()
	if w := last.FiveHour; w != nil {
		reset := time.Unix(w.ResetsAt, 0)
		if reset.After(now) {
			l.Five = int(w.UsedPercentage + 0.5)
			l.FiveReset = "resets " + reset.Format("15:04")
			l.FiveElapsed = elapsed(reset, fiveHours, now)
			l.Projection = project(w, reset, now, func(yield func(time.Time, float64) bool) {
				for i := len(hist) - 1; i >= 0; i-- {
					if hist[i].five.ResetsAt != w.ResetsAt || !yield(hist[i].t, hist[i].five.UsedPercentage) {
						return
					}
				}
			})
		} else {
			l.Five, l.FiveReset, l.Projection = 0, "new window", "5-hour window just started"
		}
	}
	if w := last.SevenDay; w != nil {
		reset := time.Unix(w.ResetsAt, 0)
		if reset.After(now) {
			l.Week = int(w.UsedPercentage + 0.5)
			l.WeekReset = "resets " + weekday(reset) + " " + reset.Format("15:04")
			l.WeekElapsed = elapsed(reset, sevenDays, now)
		} else {
			l.Week, l.WeekReset = 0, "new week"
		}
	}
	return l
}

// elapsed is how much of the window ending at reset has gone by, in %.
func elapsed(reset time.Time, span time.Duration, now time.Time) int {
	start := reset.Add(-span)
	p := int(100 * now.Sub(start) / span)
	return max(0, min(100, p))
}

// project estimates when the 5-hour window hits 100% from the readings of the last hour
// (newest first from samples), and says whether that happens before the window resets.
func project(w *window, reset, now time.Time, samples func(func(time.Time, float64) bool)) string {
	if w.UsedPercentage >= 100 {
		return "Limit reached · resets " + reset.Format("15:04")
	}
	var oldT time.Time
	var oldP float64
	samples(func(t time.Time, p float64) bool {
		if now.Sub(t) > time.Hour {
			return false
		}
		oldT, oldP = t, p
		return true
	})
	if oldT.IsZero() || now.Sub(oldT) < 10*time.Minute || w.UsedPercentage <= oldP {
		return "Pace: not enough data yet"
	}
	rate := (w.UsedPercentage - oldP) / now.Sub(oldT).Minutes() // % per minute
	eta := now.Add(time.Duration((100 - w.UsedPercentage) / rate * float64(time.Minute)))
	if eta.Before(reset) {
		return "At this pace: 100% at " + eta.Format("15:04")
	}
	return "At this pace you won't hit the limit"
}

// ContextOf returns the context-window use (0-100) the statusline last reported for a session,
// or -1 if unknown.
func ContextOf(sessionID string) int {
	b, err := os.ReadFile(filepath.Join(statusDir(), sessionID+".json"))
	if err != nil {
		return -1
	}
	var s struct {
		ContextWindow struct {
			UsedPercentage *float64 `json:"used_percentage"`
		} `json:"context_window"`
	}
	if json.Unmarshal(b, &s) != nil || s.ContextWindow.UsedPercentage == nil {
		return -1
	}
	return int(*s.ContextWindow.UsedPercentage + 0.5)
}
