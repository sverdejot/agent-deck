# Deck ↔ Mac protocol

USB serial (the Indicator's CH340, `1a86:7523`), **115200 8N1**. The Mac must not toggle DTR/RTS:
on the Indicator they drive the ESP32-S3's EN/IO0 (auto-reset), so opening the port with them
asserted reboots the deck.

One message per line (`\n`), fields separated by TAB. Values never contain TAB, CR or LF (the Mac
replaces them with spaces) and are UTF-8, cut on a character boundary to the sizes below. Unknown
lines are ignored on both sides (the ESP32 boot ROM prints to the same port).

Implementations: `host/internal/deck` (Mac, Go) and `crates/deck-ui/src/protocol.rs` (deck, Rust).

## Mac → deck

### Sessions snapshot

Sent when the list changes (checked every second) and at least every 3 s as keepalive. The deck
shows a snapshot only once `E` arrives, and drops to the offline screen after **7 s** without any
message.

```
B
S  <id> <kind> <state> <title> <place> <age> <ctx>      one per session, oldest first
E
```

| Field | Max bytes | Meaning |
|---|---|---|
| `id` | 70 | stable session id, `cli:<Claude Code session id>` |
| `kind` | — | always `code` (terminal sessions) |
| `state` | — | `busy` working · `wait` needs you · `ready` finished < 30 min ago · `idle` |
| `title` | 94 | name given by the user, else the transcript's AI title |
| `place` | 70 | git repository name (worktrees → main repo), else a short path |
| `age` | 22 | `ahora`, `hace 5 min`, `hace 3 h`…; empty while `busy` |
| `ctx` | — | context window used, 0-100; `-1` unknown (optional, defaults to -1) |

### Summary — answer to `R`

```
Y  <id> <meta> <you> <claude> <tool>
```

`meta` (150): "Empezó hace 3 h · 12 mensajes · esp · activa ahora". `you` / `claude` (600): last
message of each side, markdown flattened to one line. `tool` (150): tool running now ("Bash:
go build"), only while `busy`.

### Pending question — while a summary is open

```
Q  <id> <question> <option> <option> …
Q  <id>                                    nothing to answer (clears it)
```

Sent after `Y` and again whenever it changes (the Mac re-reads the session's terminal every second
for ~65 s after the last `R` or `A`), only for sessions in a tmux pane. It is the select menu
Claude Code is waiting on: a question (AskUserQuestion), a tool permission or a plan approval.
`question` (300): the question as shown, for permissions with what it asks about ("rm -rf build ·
Do you want to proceed?"). Each `option` is `<n>.<label>` (label ≤ 80), `n` the number Claude Code
shows (1-9, not necessarily contiguous: options that need typing, like "Type something", are left
out); at most 6. The deck keeps it only for the summary it has open.

### Limits — every 20 s

```
L  <five> <fiveReset> <fiveElapsed> <week> <weekReset> <weekElapsed> <projection> <updated>
```

`five` / `week`: % of the 5-hour / weekly plan window used (`-1` until there is a reading).
`*Reset`: "reinicia 16:40", "reinicia vie 22:00". `*Elapsed`: % of the window already gone (the
pace marker). `projection`: "A este ritmo: 100% a las 17:10" / "A este ritmo no llegas al límite".
`updated`: age of the reading. All strings ≤ 100 bytes.

The data comes from Claude Code's statusline input, recorded by `agent-deck statusline`.

### Usage — every 20 s

```
C  B                                       start
C  H  <start> <n> <total> <subtitle>       today's chart: n 15-minute buckets from bucket <start>
C  S  <name> <total> <v,v,…>               one per repo (≤ 4, the rest folded into "otros")
C  D  <labels> <values> <today>            last 7 days, oldest first; <today> is its index
C  E                                       commit: the deck shows the block now
```

Buckets count from midnight (bucket 32 = 08:00). Values are **thousands of tokens** (input +
cache writes + output; cache reads are left out). `total` is preformatted ("1,8 M", "464 k").

## Deck → Mac

```
H                     hello: sent every 2 s until the first snapshot arrives (the Mac answers with everything)
O  <id>               open: bring this session to the front
R  <id>               summary: answer with Y (and Q)
A  <id> <n> <label>   answer the pending question with option n (the label as received in Q)
D  …                  stats ("D frame=18ms draw=14ms"), every 5 s; ignored by the Mac
```

Before typing `n` into the session's tmux pane the Mac reads the menu on screen again and only
answers if option `n` still has that label, so a late tap can't answer a different question. Then
it waits ~1.5 s before sending the next `Q`.
