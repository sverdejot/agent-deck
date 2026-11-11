# Start screen format

While the Mac is not connected, the deck can show a custom picture or animation, full screen,
with its "Connect the deck…" hint. The web flasher lays the two out as one block (picture, a 20 px
gap, the two-line hint) centred on the screen, and records in the file where the hint goes; the
deck writes it there, light or dark to suit the background colour.

The [web flasher](https://sverdejot.github.io/agent-deck/) makes the file from a GIF, animated
WebP/PNG, PNG or JPEG and writes it; nobody needs to write one by hand. This page is for whoever
changes the encoder (`docs/flasher/splash.js`) or the decoder (`crates/deck-ui/src/splash.rs`).

## Where it lives

The `splash` data partition, 4 MB at `0x400000` (`firmware/partitions.csv`). Flashing the firmware
writes only up to the end of the app, so the start screen survives updates. The firmware reads the
file once at boot; anything that doesn't parse (an erased partition reads as `0xFF`) falls back to
the orange start screen. Writing 4 KB of `0xFF` at `0x400000` restores the default.

```sh
espflash write-bin 0x400000 agent-deck-start-screen.bin
```

## Layout

Little-endian throughout.

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic `DKSP` |
| 4 | 1 | version, `1` |
| 5 | 1 | vertical centre of the hint ÷ 2 (`0`: 48 px above the bottom) |
| 6 | 2 | width, `480` |
| 8 | 2 | height, `480` |
| 10 | 2 | frame count, ≥ 1 |
| 12 | 2 | palette size, 1-256 |
| 14 | 2 | background, RGB565: the colour the hint sits on |
| 16 | 4 | body length: bytes after this 24-byte header |
| 20 | 4 | CRC-32 (IEEE) of the body |
| 24 | 2 × n | palette, RGB565 |
| … | | frames, one after another |

Each frame:

| Size | Field |
|---|---|
| 2 | delay in ms (shorter than 20 is played as 20) |
| 2 × 4 | rectangle it repaints: x, y, w, h |
| 4 | length of its pixel data |
| … | pixel data |

Frame 0 covers the whole screen. Later frames repaint only their rectangle, over the previous
frame. After the last frame the animation loops to frame 0. A single frame is a still picture.

## Pixel data

Palette indices for the rectangle, row by row, packed into packets. A packet starts with a byte
`kind << 6 | n`. If `n` is 0, the real length follows as a u16. Lengths are at least 1.

| kind | meaning |
|---|---|
| 0 | `n` literal indices follow |
| 1 | one index follows, repeated `n` times |
| 2 | skip `n` pixels: they keep the previous frame (not allowed in frame 0) |

A frame's packets must cover its rectangle exactly. The deck checks the CRC and decodes every
frame at boot, and rejects the file if anything is off.

## Size and write time

The deck's CH340 serial chip only takes 115200 baud, about 11 KB/s after the flasher's
compression. A mostly still animation is small; one where the whole picture moves every frame
(say, a wobbly hand-drawn line) costs about one byte per changed pixel per frame. Fewer colours,
fewer frames or a smaller moving area make it quicker.
