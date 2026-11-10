//! Minimal anti-aliased drawing on a 480x480 RGB565 frame (port of the Arduino gfx.cpp).
//!
//! A [`Canvas`] draws rows `clip_y0..clip_y1` only, so it can point at a band buffer that holds
//! just those rows: pixel (x, y) lives at `base + (y - clip_y0) * W + x`.

pub const W: i32 = 480;
pub const H: i32 = 480;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

pub const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r: r as f32, g: g as f32, b: b as f32 }
}

impl Rgb {
    #[must_use]
    pub fn mix(self, o: Rgb, t: f32) -> Rgb {
        Rgb { r: self.r + (o.r - self.r) * t, g: self.g + (o.g - self.g) * t, b: self.b + (o.b - self.b) * t }
    }
    #[must_use]
    pub fn scale(self, k: f32) -> Rgb {
        Rgb { r: self.r * k, g: self.g * k, b: self.b * k }
    }
    pub fn pack(self) -> u16 {
        let c = |v: f32| v.clamp(0.0, 255.0) as u16;
        ((c(self.r) & 0xF8) << 8) | ((c(self.g) & 0xFC) << 3) | (c(self.b) >> 3)
    }
}

#[inline]
pub fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Mixes RGB565 colour `c` over `dst`; alpha 0..=32. Spreads the channels apart so one multiply
/// blends all three.
#[inline(always)]
pub fn blend565(dst: &mut u16, c: u16, a32: u32) {
    let fg = (c as u32 | ((c as u32) << 16)) & 0x07E0_F81F;
    let bg = (*dst as u32 | ((*dst as u32) << 16)) & 0x07E0_F81F;
    let res = ((fg.wrapping_sub(bg).wrapping_mul(a32) >> 5).wrapping_add(bg)) & 0x07E0_F81F;
    *dst = (res | (res >> 16)) as u16;
}

#[inline(always)]
fn put(dst: &mut u16, c: u16, a: f32) {
    if a >= 0.985 {
        *dst = c;
    } else if a > 0.015 {
        blend565(dst, c, (a * 32.0 + 0.5) as u32);
    }
}

/// Fills a row span two pixels at a time (32-bit stores): about twice as fast as `slice::fill`
/// for u16 on the Xtensa core.
#[inline(always)]
fn fill_span(span: &mut [u16], c: u16) {
    let (head, body, tail) = unsafe { span.align_to_mut::<u32>() };
    head.fill(c);
    let pair = c as u32 | (c as u32) << 16;
    let (chunks, _) = body.as_chunks_mut::<4>();
    for chunk in chunks {
        chunk[0] = pair;
        chunk[1] = pair;
        chunk[2] = pair;
        chunk[3] = pair;
    }
    let rem = body.len() / 4 * 4;
    body[rem..].fill(pair);
    tail.fill(c);
}

// ---- fonts -------------------------------------------------------------------------------------

pub struct Glyph {
    pub off: u32,
    pub w: u8,
    pub h: u8,
    pub xo: i8,
    pub yo: i8,
    pub adv: u8,
    pub code: u32,
}

pub struct Font {
    pub glyphs: &'static [Glyph],
    pub data: &'static [u8],
    pub ascent: i32,
    /// Latin-1 code → glyph index + 1 (0 = missing)
    pub latin: [u8; 256],
}

impl Font {
    #[inline]
    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        let code = c as u32;
        if code < 256 {
            match self.latin[code as usize] {
                0 => None,
                i => Some(&self.glyphs[i as usize - 1]),
            }
        } else {
            self.glyphs.iter().find(|g| g.code == code)
        }
    }

    pub fn width(&self, s: &str, tracking: i32) -> i32 {
        let mut w = -tracking;
        for c in s.chars() {
            if let Some(g) = self.glyph(c) {
                w += g.adv as i32 + tracking;
            }
        }
        w
    }

    pub fn line_height(&self) -> i32 {
        self.ascent + self.ascent / 4
    }
}

pub mod fonts {
    use super::{Font, Glyph};
    include!(concat!(env!("OUT_DIR"), "/fonts.rs"));
}

// ---- canvas ------------------------------------------------------------------------------------

pub struct Canvas<'a> {
    buf: &'a mut [u16],
    pub clip_y0: i32,
    pub clip_y1: i32,
}

impl<'a> Canvas<'a> {
    /// `buf` holds rows `y0..y0 + buf.len() / W`.
    pub fn new(buf: &'a mut [u16], y0: i32) -> Self {
        let rows = buf.len() as i32 / W;
        Canvas { buf, clip_y0: y0, clip_y1: (y0 + rows).min(H) }
    }

    #[inline(always)]
    fn row(&mut self, y: i32) -> &mut [u16] {
        let start = ((y - self.clip_y0) * W) as usize;
        // SAFETY: callers clip y to clip_y0..clip_y1, which `new` sized to the buffer.
        unsafe { self.buf.get_unchecked_mut(start..start + W as usize) }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        let (x0, x1) = (x.max(0), (x + w).min(W));
        if x0 >= x1 {
            return;
        }
        for yy in y.max(self.clip_y0)..(y + h).min(self.clip_y1) {
            fill_span(&mut self.row(yy)[x0 as usize..x1 as usize], c);
        }
    }

    /// Square-cornered frame of width `bw` (solid, no overdraw of the inside).
    pub fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, bw: f32, c: Rgb) {
        self.round_rect(x, y, w, bw, 0.0, c, 1.0);
        self.round_rect(x, y + h - bw, w, bw, 0.0, c, 1.0);
        self.round_rect(x, y + bw, bw, h - 2.0 * bw, 0.0, c, 1.0);
        self.round_rect(x + w - bw, y + bw, bw, h - 2.0 * bw, 0.0, c, 1.0);
    }

    /// Anti-aliased rounded rectangle (r = 0 → square corners).
    #[allow(clippy::too_many_arguments)] // geometry + colour + alpha: a struct would only add noise
    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, alpha: f32) {
        let p = c.pack();
        // pixel-aligned, square and opaque: no anti-aliasing needed, plain span fills
        let aligned = |v: f32| v == v.trunc();
        if r <= 0.0 && alpha >= 0.985 && aligned(x) && aligned(y) && aligned(w) && aligned(h) {
            self.fill_rect(x as i32, y as i32, w as i32, h as i32, p);
            return;
        }
        let r = r.min(w.min(h) / 2.0).max(0.0);
        let x0 = (x.floor() as i32).max(0);
        let x1 = ((x + w).ceil() as i32).min(W);
        let y0 = (y.floor() as i32).max(self.clip_y0);
        let y1 = ((y + h).ceil() as i32).min(self.clip_y1);
        let (cx0, cx1, cy0, cy1) = (x + r, x + w - r, y + r, y + h - r);
        let solid = alpha >= 0.985;
        for yy in y0..y1 {
            let py = yy as f32 + 0.5;
            let dy = if py < cy0 {
                cy0 - py
            } else if py > cy1 {
                py - cy1
            } else {
                0.0
            };
            let ey = (py - y).min(y + h - py) + 0.5; // coverage of the straight top/bottom edges
            let inset = if dy > 0.0 { r - (r * r - dy * dy).max(0.0).sqrt() } else { 0.0 };
            let (mut fx0, mut fx1) =
                (((x + inset + 1.0).ceil() as i32).max(x0), ((x + w - inset - 1.0).floor() as i32).min(x1));
            if ey < 1.5 {
                fx0 = x0; // anti-aliased top/bottom rows: all per-pixel
                fx1 = x0;
            }
            let row = self.row(yy);
            let mut xx = x0;
            while xx < x1 {
                if xx == fx0 && fx1 > fx0 {
                    let span = &mut row[fx0 as usize..fx1 as usize];
                    if solid {
                        fill_span(span, p);
                    } else {
                        for d in span.iter_mut() {
                            put(d, p, alpha);
                        }
                    }
                    xx = fx1;
                    continue;
                }
                let px = xx as f32 + 0.5;
                let dx = if px < cx0 {
                    cx0 - px
                } else if px > cx1 {
                    px - cx1
                } else {
                    0.0
                };
                let a = if dx > 0.0 && dy > 0.0 {
                    r + 0.5 - (dx * dx + dy * dy).sqrt()
                } else {
                    ey.min((px - x).min(x + w - px) + 0.5)
                };
                if a > 0.0 {
                    put(&mut row[xx as usize], p, clamp01(a) * alpha);
                }
                xx += 1;
            }
        }
    }

    fn for_disc(&mut self, cx: f32, cy: f32, r: f32, mut f: impl FnMut(&mut u16, f32)) {
        let x0 = ((cx - r - 1.0) as i32).max(0);
        let x1 = ((cx + r + 1.0) as i32).min(W - 1);
        let y0 = ((cy - r - 1.0) as i32).max(self.clip_y0);
        let y1 = ((cy + r + 1.0) as i32).min(self.clip_y1 - 1);
        for y in y0..=y1 {
            let dy = y as f32 + 0.5 - cy;
            let row = self.row(y);
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                f(&mut row[x as usize], dx * dx + dy * dy);
            }
        }
    }

    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb, alpha: f32) {
        let p = c.pack();
        let inner = (r - 0.5).max(0.0);
        let (in2, out2) = (inner * inner, (r + 0.5) * (r + 0.5));
        self.for_disc(cx, cy, r, |d, d2| {
            if d2 < out2 {
                put(d, p, if d2 <= in2 { alpha } else { (r + 0.5 - d2.sqrt()) * alpha });
            }
        });
    }

    /// Anti-aliased convex polygon (up to 8 points, either winding).
    pub fn fill_convex(&mut self, xs: &[f32], ys: &[f32], c: Rgb) {
        let n = xs.len().min(8);
        let mut area = 0.0;
        for i in 0..n {
            area += xs[i] * ys[(i + 1) % n] - xs[(i + 1) % n] * ys[i];
        }
        let sgn = if area > 0.0 { 1.0 } else { -1.0 };
        let (mut nx, mut ny, mut d0) = ([0f32; 8], [0f32; 8], [0f32; 8]);
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for i in 0..n {
            let j = (i + 1) % n;
            let (ex, ey) = (xs[j] - xs[i], ys[j] - ys[i]);
            let len = (ex * ex + ey * ey).sqrt();
            nx[i] = -ey / len * sgn;
            ny[i] = ex / len * sgn;
            d0[i] = nx[i] * xs[i] + ny[i] * ys[i];
            min_x = min_x.min(xs[i]);
            max_x = max_x.max(xs[i]);
            min_y = min_y.min(ys[i]);
            max_y = max_y.max(ys[i]);
        }
        let p = c.pack();
        let x0 = (min_x as i32 - 1).max(0);
        let x1 = (max_x as i32 + 1).min(W - 1);
        let y0 = (min_y as i32 - 1).max(self.clip_y0);
        let y1 = (max_y as i32 + 1).min(self.clip_y1 - 1);
        for y in y0..=y1 {
            let row = self.row(y);
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut d = f32::MAX;
                for i in 0..n {
                    d = d.min(nx[i] * px + ny[i] * py - d0[i]);
                }
                if d > -0.5 {
                    put(&mut row[x as usize], p, clamp01(d + 0.5));
                }
            }
        }
    }

    /// Text with its left edge at x; y is the top of the font's ascender box.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, f: &Font, s: &str, mut x: i32, y: i32, c: Rgb, tracking: i32, alpha: f32) {
        let col = c.pack();
        let alpha32 = (clamp01(alpha) * 32.0 + 0.5) as u32;
        for ch in s.chars() {
            if x >= W {
                break;
            }
            let Some(g) = f.glyph(ch) else { continue };
            let (gw, gh) = (g.w as i32, g.h as i32);
            let top = y + g.yo as i32;
            if gw > 0 && top < self.clip_y1 && top + gh > self.clip_y0 {
                let px = &f.data[g.off as usize..(g.off as usize + (gw * gh) as usize)];
                // horizontal range of the glyph that is on screen, worked out once
                let left = x + g.xo as i32;
                let (gx0, gx1) = ((-left).max(0), gw.min(W - left));
                if gx0 < gx1 {
                    for gy in (self.clip_y0 - top).max(0)..gh.min(self.clip_y1 - top) {
                        let src = &px[(gy * gw + gx0) as usize..(gy * gw + gx1) as usize];
                        let dst = &mut self.row(top + gy)[(left + gx0) as usize..(left + gx1) as usize];
                        for (d, &a) in dst.iter_mut().zip(src) {
                            if a == 0 {
                                continue;
                            }
                            let a32 = (a as u32 * alpha32 + 128) >> 8;
                            if a32 >= 32 {
                                *d = col;
                            } else if a32 > 0 {
                                blend565(d, col, a32);
                            }
                        }
                    }
                }
            }
            x += g.adv as i32 + tracking;
        }
    }

    /// Text centred on (cx, cy) using the ink box of its characters.
    pub fn text_centered(&mut self, f: &Font, s: &str, cx: f32, cy: f32, c: Rgb, alpha: f32) {
        let (mut top, mut bottom) = (i32::MAX, i32::MIN);
        for g in s.chars().filter_map(|ch| f.glyph(ch)).filter(|g| g.h > 0) {
            top = top.min(g.yo as i32);
            bottom = bottom.max(g.yo as i32 + g.h as i32);
        }
        if top > bottom {
            return;
        }
        let x = (cx - f.width(s, 0) as f32 / 2.0) as i32;
        self.text(f, s, x, (cy - (top + bottom) as f32 / 2.0) as i32, c, 0, alpha);
    }
}
