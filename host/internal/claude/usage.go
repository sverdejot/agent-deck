package claude

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// Token use per repository, from the `usage` of every assistant message in the transcripts
// (subagents included). Counted: input + cache writes + output; cache reads are left out, they
// would drown everything else.

// BucketsPerHour: the day is split in 15-minute buckets.
const BucketsPerHour = 4

const (
	bucketsPerDay = 24 * BucketsPerHour
	maxSeries     = 4 // repos shown; the rest are folded into "others"
)

// Series is one repo's tokens per bucket.
type Series struct {
	Name   string
	Total  int64
	Values []int64 // one per bucket from Usage.Start
}

// Day is the total of one day.
type Day struct {
	Label  string // "M", "T", "W"…
	Tokens int64
}

// Usage is what the Usage screen shows.
type Usage struct {
	Start  int // first bucket shown (from midnight)
	N      int // buckets shown, up to now
	Total  int64
	Series []Series // biggest first
	Days   []Day    // the last 7, today last
}

type fileUsage struct {
	size  int64
	repo  string
	byDay map[string]*[bucketsPerDay]int64 // "2026-11-09" → tokens per bucket
}

// usageCache keeps per-file totals; a file is re-read only when its size changes.
var usageCache = map[string]*fileUsage{}

type usageLine struct {
	Type      string `json:"type"`
	Timestamp string `json:"timestamp"`
	Cwd       string `json:"cwd"`
	Message   struct {
		ID    string `json:"id"`
		Usage *struct {
			Input       int64 `json:"input_tokens"`
			CacheCreate int64 `json:"cache_creation_input_tokens"`
			Output      int64 `json:"output_tokens"`
		} `json:"usage"`
	} `json:"message"`
}

func scanUsage(days int) {
	cutoff := time.Now().AddDate(0, 0, -days)
	_ = filepath.WalkDir(filepath.Join(claudeDir(), "projects"), func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(path, ".jsonl") {
			return nil
		}
		info, err := d.Info()
		if err != nil || info.ModTime().Before(cutoff) {
			return nil
		}
		if c, ok := usageCache[path]; ok && c.size == info.Size() {
			return nil
		}
		usageCache[path] = parseUsage(path, info.Size())
		return nil
	})
}

func parseUsage(path string, size int64) *fileUsage {
	fu := &fileUsage{size: size, byDay: map[string]*[bucketsPerDay]int64{}}
	f, err := os.Open(path)
	if err != nil {
		return fu
	}
	defer f.Close()
	seen := map[string]bool{}
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 64<<20)
	for sc.Scan() {
		b := sc.Bytes()
		if fu.repo == "" && bytes.Contains(b, []byte(`"cwd"`)) {
			var l usageLine
			if json.Unmarshal(b, &l) == nil && l.Cwd != "" {
				fu.repo = Place(l.Cwd)
			}
		}
		if !bytes.Contains(b, []byte(`"usage"`)) || !bytes.Contains(b, []byte(`"assistant"`)) {
			continue
		}
		var l usageLine
		if json.Unmarshal(b, &l) != nil || l.Type != "assistant" || l.Message.Usage == nil {
			continue
		}
		if l.Message.ID != "" { // the same message is logged once per content block
			if seen[l.Message.ID] {
				continue
			}
			seen[l.Message.ID] = true
		}
		t, err := time.Parse(time.RFC3339, l.Timestamp)
		if err != nil {
			continue
		}
		t = t.Local()
		day := t.Format("2006-01-02")
		if fu.byDay[day] == nil {
			fu.byDay[day] = new([bucketsPerDay]int64)
		}
		u := l.Message.Usage
		fu.byDay[day][bucket(t)] += u.Input + u.CacheCreate + u.Output
	}
	if fu.repo == "" {
		fu.repo = "?"
	}
	return fu
}

func bucket(t time.Time) int { return t.Hour()*BucketsPerHour + t.Minute()/(60/BucketsPerHour) }

// ReadUsage totals today's tokens per repo (from the first active hour, at most 8:00, to now) and
// the last 7 days.
func ReadUsage() Usage {
	scanUsage(8)
	now := time.Now()
	today := now.Format("2006-01-02")

	perRepo := map[string]*[bucketsPerDay]int64{}
	var u Usage
	for _, fu := range usageCache {
		b := fu.byDay[today]
		if b == nil {
			continue
		}
		if perRepo[fu.repo] == nil {
			perRepo[fu.repo] = new([bucketsPerDay]int64)
		}
		for i, v := range b {
			perRepo[fu.repo][i] += v
			u.Total += v
		}
	}

	// chart span: from the first active hour (at most 8:00, so there is always a morning) to now
	nowBucket := bucket(now)
	u.Start = 8 * BucketsPerHour
	for i := 0; i < u.Start; i++ {
		if anyAt(perRepo, i) {
			u.Start = i / BucketsPerHour * BucketsPerHour
			break
		}
	}
	if nowBucket < u.Start {
		u.Start = 0
	}
	u.N = nowBucket - u.Start + 1

	for name, b := range perRepo {
		s := Series{Name: name, Values: b[u.Start : nowBucket+1]}
		for _, v := range b {
			s.Total += v
		}
		u.Series = append(u.Series, s)
	}
	sort.Slice(u.Series, func(i, j int) bool { return u.Series[i].Total > u.Series[j].Total })
	u.Series = foldOthers(u.Series)

	for d := 6; d >= 0; d-- {
		day := now.AddDate(0, 0, -d)
		key := day.Format("2006-01-02")
		var sum int64
		for _, fu := range usageCache {
			if b := fu.byDay[key]; b != nil {
				for _, v := range b {
					sum += v
				}
			}
		}
		u.Days = append(u.Days, Day{Label: strings.ToUpper(weekday(day)[:1]), Tokens: sum})
	}
	return u
}

func anyAt(perRepo map[string]*[bucketsPerDay]int64, i int) bool {
	for _, b := range perRepo {
		if b[i] > 0 {
			return true
		}
	}
	return false
}

// foldOthers keeps the biggest series and adds up the rest as "others" (input sorted, biggest first).
func foldOthers(series []Series) []Series {
	if len(series) <= maxSeries {
		return series
	}
	other := Series{Name: "others", Values: make([]int64, len(series[0].Values))}
	for _, s := range series[maxSeries-1:] {
		other.Total += s.Total
		for i, v := range s.Values {
			other.Values[i] += v
		}
	}
	return append(series[:maxSeries-1:maxSeries-1], other)
}

// HumanTokens formats a token count for the screen: "1.8 M", "464 k", "950".
func HumanTokens(n int64) string {
	switch {
	case n >= 1_000_000:
		return fmt.Sprintf("%.1f M", float64(n)/1e6)
	case n >= 1000:
		return fmt.Sprintf("%d k", n/1000)
	}
	return fmt.Sprint(n)
}
