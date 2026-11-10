//! Sesiones screen: one tile per session (empty slots of a short last row outlined). Colour is
//! the state: working = solid orange, needs you = pulsing orange border, ready = orange border,
//! idle = faint border. Repo in small caps at the top, title anchored to the bottom, age under it,
//! context-window bar along the bottom edge.
//! Running subagents show at the right of the header row: one small square each (a square and
//! the count past four).

use super::grid::{self, Rect, PER_PAGE};
use super::theme::{
    AMBER, BG, FAINT, GREY, GREY_HI, MUTED, ON_ORANGE, ON_ORANGE_FAINT, ORANGE, ORANGE_DEEP, ORANGE_HI, TEXT, WHITE,
};
use super::Ui;
use crate::gfx::{fonts, rgb, Canvas, Rgb, H, W};
use crate::protocol::{Session, State};
use crate::text::{fit_tracked, wrap};

/// Text of a tile for the current grid, worked out when sessions or the grid change.
pub(super) struct TileText {
    title: Vec<String>,
    place: String,
    pad: f32,
    show_header: bool,
    show_age: bool,
}

impl Ui {
    /// Recomputes the text of every tile; call when the sessions change.
    pub fn layout(&mut self, st: &State) {
        let n = st.sessions.len();
        self.tiles = st.sessions.iter().enumerate().map(|(i, s)| tile_text(s, grid::session_rect(n, i))).collect();
        self.page = self.page.min(grid::page_count(n) - 1);
        self.layout_dirty = false;
    }

    pub(super) fn draw_sessions(&self, c: &mut Canvas, st: &State, now: u32) {
        self.draw_page(c, st, self.page as isize, self.page_x, now);
        if self.page_x > 0.0 {
            self.draw_page(c, st, self.page as isize - 1, self.page_x - W as f32, now);
        }
        if self.page_x < 0.0 {
            self.draw_page(c, st, self.page as isize + 1, self.page_x + W as f32, now);
        }
        let pc = grid::page_count(st.sessions.len());
        for p in (0..pc).filter(|_| pc > 1) {
            let on = p == self.page;
            let x = W as f32 / 2.0 + (p as f32 - (pc - 1) as f32 / 2.0) * 18.0;
            let (r, col, a) = if on { (4.5, TEXT, 1.0) } else { (3.5, MUTED, 0.6) };
            c.disc(x, H as f32 - 11.0, r, col, a);
        }
    }

    fn draw_page(&self, c: &mut Canvas, st: &State, p: isize, xoff: f32, now: u32) {
        let n = st.sessions.len();
        let Ok(p) = usize::try_from(p) else { return };
        if p >= grid::page_count(n) {
            return;
        }
        let count = grid::page_size(n, p);
        // empty slots of a short last row: a faint outline, so the grid reads as intentional
        for k in count..grid::slot_count(n, count) {
            let mut r = grid::tile_rect(n, k, count);
            r.x += xoff;
            if r.y < c.clip_y1 as f32 && r.y + r.h > c.clip_y0 as f32 && r.x < W as f32 && r.x + r.w > 0.0 {
                c.frame(r.x, r.y, r.w, r.h, 2.0, GREY);
            }
        }
        for k in 0..count {
            let mut r = grid::tile_rect(n, k, count);
            r.x += xoff;
            let i = p * PER_PAGE + k;
            if let (Some(s), Some(t)) = (st.sessions.get(i), self.tiles.get(i)) {
                self.draw_tile(c, s, t, i, r, now);
            }
        }
    }

    fn draw_tile(&self, c: &mut Canvas, s: &Session, t: &TileText, i: usize, r: Rect, now: u32) {
        if r.y >= c.clip_y1 as f32 || r.y + r.h <= c.clip_y0 as f32 || r.x >= W as f32 || r.x + r.w <= 0.0 {
            return;
        }
        let busy = s.busy();
        let pressed = self.pressed == Some(i);
        let since_flash = now.wrapping_sub(self.flash_ms);
        let flash = if s.id == self.flash_id && since_flash < 500 { 1.0 - since_flash as f32 / 500.0 } else { 0.0 };

        if busy {
            c.round_rect(r.x, r.y, r.w, r.h, 0.0, if pressed { ORANGE_HI } else { ORANGE }, 1.0);
        } else {
            let (mut bw, mut alpha) = (2.0, 1.0);
            if s.waiting() {
                let p = 0.5 + 0.5 * (now as f32 / 260.0).sin();
                bw = 2.0 + 3.0 * p;
                alpha = 0.6 + 0.4 * p;
            } else if s.state != "ready" {
                alpha = 0.35; // idle
            }
            c.frame(r.x, r.y, r.w, r.h, bw, BG.mix(ORANGE, alpha));
            c.round_rect(
                r.x + bw,
                r.y + bw,
                r.w - 2.0 * bw,
                r.h - 2.0 * bw,
                0.0,
                if pressed { GREY_HI } else { GREY },
                1.0,
            );
        }
        if flash > 0.0 {
            c.round_rect(r.x, r.y, r.w, r.h, 0.0, WHITE, flash * 0.45);
        }

        let fg = if busy { ON_ORANGE } else { TEXT };
        let faint = if busy { ON_ORANGE_FAINT } else { FAINT };
        let x = (r.x + t.pad) as i32;
        if t.show_header {
            c.text(&fonts::TAG, &t.place, x, (r.y + t.pad) as i32, faint, 2, 1.0);
            if s.agents > 0 {
                let col = if busy { ON_ORANGE } else { ORANGE };
                draw_marks(c, s.agents, r.x + r.w - t.pad, r.y + t.pad, col);
            }
        }
        let lh = fonts::TITLE.line_height();
        let mut bottom = (r.y + r.h - t.pad) as i32;
        if t.show_age {
            c.text(&fonts::SUB, &s.age, x, bottom - fonts::SUB.ascent, faint, 0, 1.0);
            bottom -= fonts::SUB.line_height() + 2;
        }
        let y0 = bottom - t.title.len() as i32 * lh + (lh - fonts::TITLE.ascent);
        for (l, line) in t.title.iter().enumerate() {
            c.text(&fonts::TITLE, line, x, y0 + l as i32 * lh, fg, 0, 1.0);
        }

        // context window used: thin bar along the bottom edge
        if s.ctx >= 0 {
            let inset = if busy { 0.0 } else { 2.0 };
            let (bw, by) = (r.w - 2.0 * inset, r.y + r.h - inset - 4.0);
            let track = if busy { ORANGE_DEEP } else { rgb(58, 57, 54) };
            let fill = if busy {
                ON_ORANGE
            } else if s.ctx >= 80 {
                AMBER
            } else {
                rgb(150, 146, 138)
            };
            c.round_rect(r.x + inset, by, bw, 4.0, 0.0, track, 1.0);
            c.round_rect(r.x + inset, by, bw * s.ctx.min(100) as f32 / 100.0, 4.0, 0.0, fill, 1.0);
        }
    }
}

/// Squares drawn for the subagents of a tile; beyond this the count is written next to one.
const MARKS: u32 = 4;
const MARK: f32 = 7.0;
const MARK_GAP: f32 = 4.0;

fn marks_label(n: u32) -> String {
    if n > 9 {
        "9+".into()
    } else {
        n.to_string()
    }
}

/// Width of the subagent marks for `n` subagents.
fn marks_width(n: u32) -> f32 {
    if n <= MARKS {
        n as f32 * MARK + (n as f32 - 1.0) * MARK_GAP
    } else {
        MARK + 5.0 + fonts::TAG.width(&marks_label(n), 1) as f32
    }
}

/// Subagents at work, at the right of the tile's header row: one small square per subagent (the
/// legend markers of Uso), or a square and the count when there are more than [`MARKS`]. `y` is
/// the top of the header text.
fn draw_marks(c: &mut Canvas, n: u32, right: f32, y: f32, col: Rgb) {
    let x = right - marks_width(n);
    let top = y + (fonts::TAG.ascent as f32 - MARK) / 2.0;
    if n <= MARKS {
        for k in 0..n {
            c.round_rect(x + k as f32 * (MARK + MARK_GAP), top, MARK, MARK, 0.0, col, 1.0);
        }
    } else {
        c.round_rect(x, top, MARK, MARK, 0.0, col, 1.0);
        c.text(&fonts::TAG, &marks_label(n), (x + MARK + 5.0) as i32, y as i32, col, 1, 1.0);
    }
}

fn tile_text(s: &Session, r: Rect) -> TileText {
    let pad = if r.w >= 400.0 && r.h >= 300.0 {
        28.0
    } else if r.h >= 140.0 && r.w >= 200.0 {
        20.0
    } else {
        14.0
    };
    let inner = (r.w - 2.0 * pad) as i32;
    let show_header = r.h >= 100.0;
    let show_age = !s.age.is_empty() && r.h >= 120.0;
    let top = pad + if show_header { 28.0 } else { 0.0 };
    let bottom = pad + if show_age { (fonts::SUB.line_height() + 2) as f32 } else { 0.0 };
    let max_lines = (((r.h - top - bottom) / fonts::TITLE.line_height() as f32) as usize).clamp(1, 4);
    let badge = if s.agents > 0 { marks_width(s.agents) as i32 + 10 } else { 0 };
    TileText {
        title: wrap(&fonts::TITLE, &s.title, inner, max_lines),
        place: fit_tracked(&fonts::TAG, &s.place.to_uppercase(), inner - badge, 2),
        pad,
        show_header,
        show_age,
    }
}
