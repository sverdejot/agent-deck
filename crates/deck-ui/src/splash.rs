//! Custom start screen: a still image or an animation shown full screen while there is no link to
//! the Mac. The web flasher lays the picture and the hint out together, so the file says where the
//! deck writes the hint ([`Splash::text_y`]) and which background it sits on. The web flasher converts the user's GIF/WebP/APNG/PNG into this format and writes it to
//! the `splash` flash partition; the firmware reads it at boot. Format: `docs/splash.md`.
//!
//! The file is a background colour, the hint's position, a palette of up to 256 RGB565 colours and
//! a list of frames. Frame 0 covers the whole screen; later frames only carry the rectangle that changed, drawn over the previous one.
//! Pixels are palette indices, run-length encoded, and later frames can skip the pixels that
//! stay the same. Everything is checked once by [`Splash::parse`]
//! (CRC, bounds, every frame decoded), so playing it can't fail.

use crate::gfx::{Canvas, H, W};

/// Where the hint goes when the file doesn't say: centred this far from the bottom.
const DEFAULT_TEXT_FROM_BOTTOM: i32 = 48;

pub const MAGIC: [u8; 4] = *b"DKSP";
pub const VERSION: u8 = 1;
/// Bytes before the palette.
pub const HEADER_LEN: usize = 24;
/// Shortest time a frame stays on screen: GIFs often say 0 or 10 ms, which would spin the CPU.
pub const MIN_DELAY_MS: u32 = 20;
const FRAME_HEADER_LEN: usize = 14;
const PIXELS: usize = (W * H) as usize;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Not a splash file: an empty partition reads as 0xFF.
    Magic,
    Version(u8),
    /// Not 480×480, no frames or a bad palette size.
    Header,
    /// Shorter than the header says.
    Truncated,
    Crc,
    /// Frame `n` runs out of bounds, has a bad rectangle or uses a colour the palette lacks.
    Frame(usize),
}

/// Size of the whole file, read from its header; `None` if `head` is not a splash header.
pub fn file_len(head: &[u8]) -> Option<usize> {
    if head.len() < HEADER_LEN || head[..4] != MAGIC {
        return None;
    }
    Some(HEADER_LEN + u32_at(head, 16) as usize)
}

struct Frame {
    delay_ms: u32,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    /// run-length encoded pixels: `data[start..end]`
    start: usize,
    end: usize,
}

pub struct Splash {
    data: Vec<u8>,
    /// RGB565 background the hint sits on: picks light or dark text
    pub bg: u16,
    /// vertical centre of the two hint lines
    pub text_y: i32,
    palette: [u16; 256],
    frames: Vec<Frame>,
    /// the picture on screen now, one palette index per pixel
    pix: Vec<u8>,
    current: usize,
    /// when the next frame is due; set on the first tick
    next_ms: Option<u32>,
}

impl Splash {
    /// Checks a splash file and gets its first frame ready.
    ///
    /// # Errors
    ///
    /// When `data` is not a splash file, is cut short, fails its CRC or has a malformed frame.
    pub fn parse(mut data: Vec<u8>) -> Result<Splash, Error> {
        if data.len() < HEADER_LEN || data[..4] != MAGIC {
            return Err(Error::Magic);
        }
        if data[4] != VERSION {
            return Err(Error::Version(data[4]));
        }
        let (w, h) = (u16_at(&data, 6) as i32, u16_at(&data, 8) as i32);
        let n_frames = u16_at(&data, 10) as usize;
        let n_colors = u16_at(&data, 12) as usize;
        if w != W || h != H || n_frames == 0 || !(1..=256).contains(&n_colors) {
            return Err(Error::Header);
        }
        let len = file_len(&data).ok_or(Error::Magic)?;
        if data.len() < len {
            return Err(Error::Truncated);
        }
        data.truncate(len); // a partition read may bring trailing bytes
        if crc32(&data[HEADER_LEN..]) != u32_at(&data, 20) {
            return Err(Error::Crc);
        }

        let mut palette = [0u16; 256];
        let mut at = HEADER_LEN;
        if len < at + 2 * n_colors {
            return Err(Error::Truncated);
        }
        for (k, c) in palette.iter_mut().take(n_colors).enumerate() {
            *c = u16_at(&data, at + 2 * k);
        }
        at += 2 * n_colors;

        let mut frames = Vec::with_capacity(n_frames);
        for k in 0..n_frames {
            if len < at + FRAME_HEADER_LEN {
                return Err(Error::Frame(k));
            }
            let f = Frame {
                delay_ms: (u16_at(&data, at) as u32).max(MIN_DELAY_MS),
                x: u16_at(&data, at + 2) as usize,
                y: u16_at(&data, at + 4) as usize,
                w: u16_at(&data, at + 6) as usize,
                h: u16_at(&data, at + 8) as usize,
                start: at + FRAME_HEADER_LEN,
                end: at + FRAME_HEADER_LEN + u32_at(&data, at + 10) as usize,
            };
            let full = (f.x, f.y, f.w, f.h) == (0, 0, W as usize, H as usize);
            let inside = f.w > 0 && f.h > 0 && f.x + f.w <= W as usize && f.y + f.h <= H as usize;
            if !inside || (k == 0 && !full) || f.end > len {
                return Err(Error::Frame(k));
            }
            at = f.end;
            frames.push(f);
        }

        let bg = u16_at(&data, 14);
        let text_y = if data[5] == 0 { H - DEFAULT_TEXT_FROM_BOTTOM } else { 2 * data[5] as i32 };
        let mut s = Splash { data, bg, text_y, palette, frames, pix: vec![0; PIXELS], current: 0, next_ms: None };
        for k in 0..n_frames {
            if !s.decode(k, n_colors) {
                return Err(Error::Frame(k));
            }
        }
        s.decode(0, n_colors); // frame 0 covers everything: back to the start
        Ok(s)
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Moves on to the next frame when it is due. Returns whether the picture changed.
    pub fn tick(&mut self, now: u32) -> bool {
        if self.frames.len() < 2 {
            return false;
        }
        let due = *self.next_ms.get_or_insert(now.wrapping_add(self.frames[self.current].delay_ms));
        if (now.wrapping_sub(due) as i32) < 0 {
            return false;
        }
        self.current = (self.current + 1) % self.frames.len();
        self.decode(self.current, 256);
        // from now, not from `due`: after a pause (linked to the Mac) it doesn't race to catch up
        self.next_ms = Some(now.wrapping_add(self.frames[self.current].delay_ms));
        true
    }

    /// Draws the current frame over the whole screen.
    pub fn draw(&self, c: &mut Canvas) {
        c.indexed(&self.pix, &self.palette);
    }

    /// Paints frame `k` over `pix`. False if its data is malformed or uses an index ≥ `n_colors`.
    ///
    /// Packets: a byte `kind << 6 | n`, then for `n == 0` the real length as a u16. Kind 0: `n`
    /// literal indices follow. Kind 1: one index follows, repeated `n` times. Kind 2: skip `n`
    /// pixels (keep the previous frame's; never in frame 0, which must stand on its own).
    fn decode(&mut self, k: usize, n_colors: usize) -> bool {
        let f = &self.frames[k];
        let data = &self.data[f.start..f.end];
        let (fw, end) = (f.w, f.w * f.h);
        let at = |p: usize| (f.y + p / fw) * W as usize + f.x + p % fw;
        let (mut i, mut p) = (0, 0); // read position in data, pixel position in the rectangle
        while i < data.len() {
            let b = data[i];
            let mut n = (b & 0x3F) as usize;
            i += 1;
            if n == 0 {
                if i + 2 > data.len() {
                    return false;
                }
                n = u16_at(data, i) as usize;
                i += 2;
            }
            if n == 0 || p + n > end {
                return false;
            }
            match b >> 6 {
                0 => {
                    let Some(lit) = data.get(i..i + n) else { return false };
                    for &c in lit {
                        if c as usize >= n_colors {
                            return false;
                        }
                        self.pix[at(p)] = c;
                        p += 1;
                    }
                    i += n;
                }
                1 => {
                    let Some(&c) = data.get(i) else { return false };
                    if c as usize >= n_colors {
                        return false;
                    }
                    for q in p..p + n {
                        self.pix[at(q)] = c;
                    }
                    p += n;
                    i += 1;
                }
                2 if k > 0 => p += n,
                _ => return false,
            }
        }
        p == end
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// CRC-32 (IEEE, as in zip and PNG).
pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            t[n] = c;
            n += 1;
        }
        t
    };
    !data.iter().fold(!0u32, |c, &b| TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8))
}
