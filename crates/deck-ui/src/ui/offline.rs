//! Shown full screen while there is no link to the Mac: the agent-deck mark on orange, or the
//! user's own picture or animation (see [`crate::splash`]) with the hint where the file puts it,
//! light or dark to suit the background. The web flasher's preview draws the same layout (`docs/flasher/start-screen.js`).

use super::theme::{MUTED, ON_ORANGE, ON_ORANGE_MUTED, ORANGE, TEXT};
use crate::gfx::{fonts, rgb, Canvas, Rgb, H, W};
use crate::splash::Splash;

/// Text on a light background.
const INK: Rgb = rgb(40, 38, 36);
const INK_MUTED: Rgb = rgb(105, 100, 92);

/// The agent-deck mark: a 2x2 deck of square session tiles, the first one solid (working) and
/// the rest outlined, centred on (cx, cy).
fn draw_mark(c: &mut Canvas, cx: f32, cy: f32, col: Rgb) {
    const TILE: f32 = 64.0;
    const GAP: f32 = 16.0;
    const STROKE: f32 = 6.0;
    let side = 2.0 * TILE + GAP;
    let (x0, y0) = ((cx - side / 2.0).round(), (cy - side / 2.0).round());
    for k in 0..4 {
        let x = x0 + (k % 2) as f32 * (TILE + GAP);
        let y = y0 + (k / 2) as f32 * (TILE + GAP);
        if k == 0 {
            c.round_rect(x, y, TILE, TILE, 0.0, col, 1.0);
        } else {
            c.frame(x, y, TILE, TILE, STROKE, col);
        }
    }
}

fn hint(ever_linked: bool) -> [&'static str; 2] {
    let first = if ever_linked { "Lost the link to the computer" } else { "Connect the deck to the computer" };
    [first, "and run agent-deck"]
}

pub(super) fn draw(c: &mut Canvas, ever_linked: bool, splash: Option<&Splash>) {
    let mid = W as f32 / 2.0;
    let [first, second] = hint(ever_linked);
    if let Some(s) = splash {
        s.draw(c);
        let (main, muted) = if is_dark(s.bg) { (TEXT, MUTED) } else { (INK, INK_MUTED) };
        let y = s.text_y as f32;
        c.text_centered(&fonts::SUB, first, mid, y - 13.0, main, 1.0);
        c.text_centered(&fonts::SUB, second, mid, y + 13.0, muted, 1.0);
        return;
    }
    c.fill_rect(0, 0, W, H, ORANGE.pack());
    draw_mark(c, mid, 170.0, ON_ORANGE);
    c.text_centered(&fonts::BIG, "agent deck", mid, 302.0, ON_ORANGE, 1.0);
    c.text_centered(&fonts::SUB, first, mid, 352.0, ON_ORANGE_MUTED, 1.0);
    c.text_centered(&fonts::SUB, second, mid, 378.0, ON_ORANGE_MUTED, 1.0);
}

/// Whether an RGB565 colour is dark enough for light text (perceived lightness under ~55%).
fn is_dark(c: u16) -> bool {
    let (r, g, b) = (((c >> 11) & 31) * 255 / 31, ((c >> 5) & 63) * 255 / 63, (c & 31) * 255 / 31);
    (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) < 140_000
}
