//! Summary of a session (hold a tile): state, title, what it is doing, your last message and
//! Claude's last reply, and a button to open it.

use super::draw_centered;
use super::theme::{GREY, MUTED, ON_ORANGE, ORANGE, TEXT};
use crate::gfx::{fonts, Canvas, Rgb, H, W};
use crate::protocol::{State, Summary};
use crate::text::{fit, wrap};

/// Taps below this line hit the "Abrir sesión" button.
pub const OPEN_BUTTON_Y: i32 = H - 80;

pub(super) fn draw(c: &mut Canvas, st: &State, sum: &Summary) {
    let Some(s) = st.sessions.iter().find(|s| s.id == sum.id) else {
        draw_centered(c, "La sesión ya no existe", None);
        return;
    };
    let busy = s.busy();
    let (x, w) = (24, W - 48);
    let mut y = 22;

    let label = s.state_label();
    let lw = fonts::SUB.width(label, 0) + 24;
    c.round_rect(x as f32, y as f32, lw as f32, 30.0, 15.0, if busy { ORANGE } else { GREY }, 1.0);
    c.text(&fonts::SUB, label, x + 12, y + 5, if busy { ON_ORANGE } else { TEXT }, 0, 1.0);
    y += 44;

    let head = wrap(&fonts::HEAD, &s.title, w, 2);
    for (l, line) in head.iter().enumerate() {
        c.text(&fonts::HEAD, line, x, y + l as i32 * fonts::HEAD.line_height(), TEXT, 0, 1.0);
    }
    y += head.len() as i32 * fonts::HEAD.line_height() + 4;

    if sum.loaded {
        let lh = fonts::SUB.line_height();
        if !sum.meta.is_empty() {
            c.text(&fonts::SUB, &fit(&fonts::SUB, &sum.meta, w), x, y, MUTED, 0, 1.0);
            y += lh + 10;
        }
        let bottom_limit = H - 84;
        let mut block = |c: &mut Canvas, label: &str, col: Rgb, text: &str, max_lines: usize| {
            if text.is_empty() || y + lh * 2 > bottom_limit {
                return;
            }
            c.text(&fonts::TAG, label, x, y, col, 1, 1.0);
            y += 20;
            let room = usize::try_from((bottom_limit - y) / lh).unwrap_or(0);
            let lines = wrap(&fonts::SUB, text, w, max_lines.min(room));
            for (l, line) in lines.iter().enumerate() {
                c.text(&fonts::SUB, line, x, y + l as i32 * lh, TEXT, 0, 1.0);
            }
            y += lines.len() as i32 * lh + 12;
        };
        block(c, "AHORA", ORANGE, &sum.tool, 2);
        block(c, "TÚ", MUTED, &sum.you, 3);
        block(c, "CLAUDE", ORANGE, &sum.claude, 6);
    } else {
        c.text(&fonts::SUB, "Cargando…", x, y + 10, MUTED, 0, 1.0);
    }

    c.round_rect(x as f32, (H - 72) as f32, w as f32, 52.0, 26.0, ORANGE, 1.0);
    c.text_centered(&fonts::TITLE, "Abrir sesión", W as f32 / 2.0, (H - 46) as f32, ON_ORANGE, 1.0);
}
