//! Shown full screen, in orange, while there is no link to the Mac.

use super::theme::{ON_ORANGE, ON_ORANGE_MUTED, ORANGE};
use crate::gfx::{fonts, Canvas, Rgb, H, W};

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

pub(super) fn draw(c: &mut Canvas, ever_linked: bool) {
    c.fill_rect(0, 0, W, H, ORANGE.pack());
    let mid = W as f32 / 2.0;
    draw_mark(c, mid, 170.0, ON_ORANGE);
    c.text_centered(&fonts::BIG, "agent deck", mid, 302.0, ON_ORANGE, 1.0);
    let msg = if ever_linked { "Sin conexión con el ordenador" } else { "Conecta el deck al ordenador" };
    c.text_centered(&fonts::SUB, msg, mid, 352.0, ON_ORANGE_MUTED, 1.0);
    c.text_centered(&fonts::SUB, "y ejecuta agent-deck", mid, 378.0, ON_ORANGE_MUTED, 1.0);
}
