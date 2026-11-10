//! Shown full screen, in Claude orange, while there is no link to the Mac.

use core::f32::consts::PI;

use super::theme::{ON_ORANGE, ON_ORANGE_MUTED, ORANGE};
use crate::gfx::{fonts, Canvas, Rgb, H, W};

/// Claude's spark: tapered spokes of uneven length around a centre.
fn draw_spark(c: &mut Canvas, cx: f32, cy: f32, r: f32, col: Rgb) {
    const LEN: [f32; 12] = [1.0, 0.78, 0.92, 0.7, 1.0, 0.82, 0.95, 0.72, 0.98, 0.8, 0.9, 0.75];
    for (i, len) in LEN.iter().enumerate() {
        let a = i as f32 * PI / 6.0 + 0.13;
        let (ca, sa) = (a.cos(), a.sin());
        let (l, w0, w1) = (r * len, r * 0.075, r * 0.03);
        let xs = [cx - sa * w0, cx + sa * w0, cx + ca * l + sa * w1, cx + ca * l - sa * w1];
        let ys = [cy + ca * w0, cy - ca * w0, cy + sa * l - ca * w1, cy + sa * l + ca * w1];
        c.fill_convex(&xs, &ys, col);
        c.disc(cx + ca * l, cy + sa * l, w1, col, 1.0);
    }
    c.disc(cx, cy, r * 0.12, col, 1.0);
}

pub(super) fn draw(c: &mut Canvas, ever_linked: bool) {
    c.fill_rect(0, 0, W, H, ORANGE.pack());
    let mid = W as f32 / 2.0;
    draw_spark(c, mid, 170.0, 78.0, ON_ORANGE);
    c.text_centered(&fonts::BIG, "Claude", mid, 302.0, ON_ORANGE, 1.0);
    let msg = if ever_linked { "Sin conexión con el ordenador" } else { "Conecta el deck al ordenador" };
    c.text_centered(&fonts::SUB, msg, mid, 352.0, ON_ORANGE_MUTED, 1.0);
    c.text_centered(&fonts::SUB, "y ejecuta agent-deck", mid, 378.0, ON_ORANGE_MUTED, 1.0);
}
