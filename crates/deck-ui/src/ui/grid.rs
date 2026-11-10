//! Session grid: 1 → whole screen, 2-3 → rows, 4 → 2x2, 5-6 → 2 columns x 3 rows, 7-9 → 3x3;
//! more than 9 → pages of 3x3. Every tile of a grid has the same size: a short last row leaves
//! empty slots (drawn as faint outlines) instead of stretching, so tiles keep their place.

use crate::gfx::{H, W};

pub const PER_PAGE: usize = 9;
const MARGIN: f32 = 10.0;
const GAP: f32 = 10.0;
/// Room for the page dots when there is more than one page.
const DOTS: f32 = 22.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (x, y) = (x as f32, y as f32);
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Pages needed for `n` sessions (at least one).
pub fn page_count(n: usize) -> usize {
    n.div_ceil(PER_PAGE).max(1)
}

/// Sessions on page `p`.
pub fn page_size(n: usize, p: usize) -> usize {
    n.saturating_sub(p * PER_PAGE).min(PER_PAGE)
}

/// Columns and rows of a page holding `count` tiles, out of `n` sessions in total.
pub fn dims(n: usize, count: usize) -> (usize, usize) {
    match count {
        _ if n > PER_PAGE => (3, 3),
        0..=3 => (1, count.max(1)),
        4 => (2, 2),
        5 | 6 => (2, 3),
        _ => (3, 3),
    }
}

/// Slots of a page holding `count` tiles: the tiles plus the empty places of a short last row.
pub fn slot_count(n: usize, count: usize) -> usize {
    let (cols, rows) = dims(n, count);
    cols * rows
}

/// Slot `i` of a page holding `count` tiles, out of `n` sessions in total (`i` may go up to
/// `slot_count` to place the empty slots). Snapped to whole pixels: crisp edges, and solid fills
/// take the fast path.
pub fn tile_rect(n: usize, i: usize, count: usize) -> Rect {
    let (cols, rows) = dims(n, count);
    let (row, col) = (i / cols, i % cols);
    let bottom = if page_count(n) > 1 { DOTS } else { 0.0 };
    let cw = (W as f32 - 2.0 * MARGIN - (cols as f32 - 1.0) * GAP) / cols as f32;
    let ch = (H as f32 - 2.0 * MARGIN - bottom - (rows as f32 - 1.0) * GAP) / rows as f32;
    let (fx, fy) = (MARGIN + col as f32 * (cw + GAP), MARGIN + row as f32 * (ch + GAP));
    let (x0, y0, x1, y1) = (fx.round(), fy.round(), (fx + cw).round(), (fy + ch).round());
    Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/// Where session `i` (of `n`) is drawn on its own page.
pub fn session_rect(n: usize, i: usize) -> Rect {
    let (p, k) = (i / PER_PAGE, i % PER_PAGE);
    tile_rect(n, k, page_size(n, p))
}
