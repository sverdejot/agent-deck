//! Limits screen: 5-hour and weekly plan usage, when they reset, how much of each window has gone
//! by (pace marker) and when the 5-hour window would hit 100% at the current pace.

use super::draw_centered;
use super::theme::{AMBER, FAINT, GREY, MUTED, ORANGE, RED, TEXT};
use crate::gfx::{fonts, Canvas, Rgb, W};
use crate::protocol::State;
use crate::text::wrap;

fn level_color(p: i32) -> Rgb {
    match p {
        95.. => RED,
        80.. => AMBER,
        _ => ORANGE,
    }
}

fn draw_block(c: &mut Canvas, y: i32, label: &str, pct: i32, reset: &str, elapsed: i32) {
    let (x, w) = (24, W - 48);
    c.text(&fonts::TAG, label, x, y, FAINT, 2, 1.0);
    c.text(&fonts::SUB, reset, x + w - fonts::SUB.width(reset, 0), y - 2, MUTED, 0, 1.0);
    c.text(&fonts::BIG, &format!("{pct}%"), x, y + 22, if pct >= 80 { level_color(pct) } else { TEXT }, 0, 1.0);
    let by = (y + 84) as f32;
    c.round_rect(x as f32, by, w as f32, 12.0, 0.0, GREY, 1.0);
    c.round_rect(x as f32, by, w as f32 * pct.min(100) as f32 / 100.0, 12.0, 0.0, level_color(pct), 1.0);
    let mx = x as f32 + w as f32 * elapsed as f32 / 100.0; // how much of the window has gone by
    c.round_rect(mx - 1.0, by - 5.0, 2.0, 22.0, 0.0, TEXT, 1.0);
    c.text(&fonts::SUB, &format!("{elapsed}% of the window gone by"), x, by as i32 + 22, FAINT, 0, 1.0);
}

pub(super) fn draw(c: &mut Canvas, st: &State) {
    let l = &st.limits;
    c.text(&fonts::TAG, "LIMITS", 24, 22, FAINT, 2, 1.0);
    if l.five < 0 {
        draw_centered(c, "No limits data yet", Some("It comes from Claude Code's statusline"));
        return;
    }
    if !l.updated.is_empty() {
        let s = format!("updated {}", l.updated);
        c.text(&fonts::SUB, &s, W - 24 - fonts::SUB.width(&s, 0), 20, FAINT, 0, 1.0);
    }
    draw_block(c, 70, "SESSION · 5 H", l.five, &l.five_reset, l.five_elapsed);
    if l.week >= 0 {
        draw_block(c, 220, "WEEK", l.week, &l.week_reset, l.week_elapsed);
    }
    let warn = l.projection.contains("100% at") || l.projection.contains("reached");
    c.round_rect(24.0, 384.0, (W - 48) as f32, 72.0, 0.0, if warn { AMBER.scale(0.22) } else { GREY }, 1.0);
    let lines = wrap(&fonts::TITLE, &l.projection, W - 80, 2);
    let lh = fonts::TITLE.line_height();
    let y = 420 - lines.len() as i32 * lh / 2;
    for (i, line) in lines.iter().enumerate() {
        c.text(&fonts::TITLE, line, 40, y + i as i32 * lh, if warn { AMBER } else { TEXT }, 0, 1.0);
    }
}
