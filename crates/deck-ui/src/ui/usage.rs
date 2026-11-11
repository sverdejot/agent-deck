//! Usage screen: today's tokens in 15-minute stacked bars (one colour per repo), legend, and the
//! last 7 days.

use super::draw_centered;
use super::theme::{FAINT, GREY_HI, MUTED, ORANGE, SERIES, TEXT};
use crate::gfx::{fonts, rgb, Canvas, W};
use crate::protocol::State;
use crate::text::fit;

pub(super) fn draw(c: &mut Canvas, st: &State) {
    c.text(&fonts::TAG, "USAGE TODAY", 24, 22, FAINT, 2, 1.0);
    let Some(u) = &st.usage else {
        draw_centered(c, "Calculating usage…", None);
        return;
    };
    c.text(&fonts::BIG, &u.total, 24, 44, TEXT, 0, 1.0);
    c.text(&fonts::SUB, &u.subtitle, 24, 96, MUTED, 0, 1.0);

    // today: stacked bars, 15 minutes each
    let (cx, cw, cy, ch) = (24.0, (W - 48) as f32, 130.0, 150.0);
    let value = |k: usize, i: usize| u.series[k].2.get(i).copied().unwrap_or(0);
    let peak = (0..u.n).map(|i| (0..u.series.len()).map(|k| value(k, i)).sum::<u32>()).max().unwrap_or(0).max(1);
    let slot = cw / u.n.max(1) as f32;
    let bw = (slot - 1.0).max(1.0);
    for i in 0..u.n {
        let mut y = cy + ch;
        for (k, colour) in SERIES.iter().enumerate().take(u.series.len()) {
            let h = ch * value(k, i) as f32 / peak as f32;
            if h > 0.0 {
                c.round_rect(cx + i as f32 * slot, y - h, bw, h, 0.0, *colour, 1.0);
                y -= h;
            }
        }
    }
    c.round_rect(cx, cy + ch, cw, 1.0, 0.0, rgb(70, 68, 64), 1.0);

    // hour labels: about five along the axis
    let step = match u.n.div_ceil(4) {
        0..=6 => 1,
        7..=12 => 2,
        13..=18 => 3,
        _ => 4,
    };
    for h in (0..).step_by(step).take_while(|h| h * 4 < u.n) {
        let label = format!("{:02}", (u.start / 4 + h) % 24);
        c.text(&fonts::TAG, &label, (cx + (h * 4) as f32 * slot) as i32, (cy + ch) as i32 + 6, FAINT, 0, 1.0);
    }

    // legend, two columns
    for (k, ((name, total, _), colour)) in u.series.iter().zip(SERIES).enumerate() {
        let (lx, ly) = (24 + (k as i32 % 2) * 216, 312 + (k as i32 / 2) * 26);
        c.round_rect(lx as f32, (ly + 3) as f32, 10.0, 10.0, 0.0, colour, 1.0);
        c.text(&fonts::SUB, &fit(&fonts::SUB, name, 120), lx + 18, ly - 1, TEXT, 0, 1.0);
        c.text(&fonts::SUB, total, lx + 200 - fonts::SUB.width(total, 0), ly - 1, MUTED, 0, 1.0);
    }

    // last 7 days
    c.text(&fonts::TAG, "LAST 7 DAYS", 24, 372, FAINT, 2, 1.0);
    let dmax = u.days.iter().map(|d| d.1).max().unwrap_or(0).max(1);
    let dslot = (W - 48) as f32 / 7.0;
    for (d, (label, v)) in u.days.iter().enumerate() {
        let h = (52.0 * *v as f32 / dmax as f32).max(1.0);
        let x = 24.0 + d as f32 * dslot;
        let today = d == u.today;
        c.round_rect(x + 6.0, 446.0 - h, dslot - 12.0, h, 0.0, if today { ORANGE } else { GREY_HI }, 1.0);
        let lx = (x + (dslot - fonts::TAG.width(label, 0) as f32) / 2.0) as i32;
        c.text(&fonts::TAG, label, lx, 452, if today { TEXT } else { FAINT }, 0, 1.0);
    }
}
