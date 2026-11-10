//! Summary of a session (hold a tile): state, title, what it is doing, the subagents it has
//! running, your last message and Claude's last reply, and a button to open it. When Claude is
//! waiting on a menu (a question, a permission, a plan approval) the summary shows the question and
//! one button per option instead.

use super::draw_centered;
use super::grid::Rect;
use super::theme::{GREY, MUTED, ON_ORANGE, ORANGE, ORANGE_HI, TEXT};
use crate::gfx::{fonts, Canvas, Rgb, H, W};
use crate::protocol::{Question, Session, State, Subagent, Summary};
use crate::text::{fit, wrap};

/// Taps below this line hit the "Abrir sesión" button.
pub const OPEN_BUTTON_Y: i32 = H - 80;
/// Rows of the SUBAGENTES block, "+n más" included.
const AGENT_ROWS: usize = 4;

/// With a question, "Abrir sesión" becomes a smaller, secondary button at the very bottom.
const Q_OPEN_TOP: i32 = H - 60;
const Q_OPEN_H: f32 = 48.0;
/// Taps below this line hit "Abrir sesión" when a question is on screen.
pub const Q_OPEN_BUTTON_Y: i32 = Q_OPEN_TOP - 4;
/// Option buttons: finger-sized, stacked up from just above "Abrir sesión".
const OPTION_H: f32 = 56.0;
const OPTION_GAP: f32 = 8.0;
const OPTIONS_BOTTOM: f32 = 410.0;
const MARGIN: f32 = 24.0;

/// Button of option `k` of `n`: one column up to 4 options, two columns beyond.
pub(super) fn option_rect(n: usize, k: usize) -> Rect {
    let cols = if n > 4 { 2 } else { 1 };
    let rows = n.div_ceil(cols);
    let cw = (W as f32 - 2.0 * MARGIN - OPTION_GAP * (cols - 1) as f32) / cols as f32;
    let top = OPTIONS_BOTTOM - rows as f32 * OPTION_H - rows.saturating_sub(1) as f32 * OPTION_GAP;
    let (row, col) = (k / cols, k % cols);
    Rect {
        x: MARGIN + col as f32 * (cw + OPTION_GAP),
        y: top + row as f32 * (OPTION_H + OPTION_GAP),
        w: cw,
        h: OPTION_H,
    }
}

/// Index of the option button under (x, y), out of `n`.
pub(super) fn option_at(n: usize, x: i32, y: i32) -> Option<usize> {
    (0..n).find(|&k| option_rect(n, k).contains(x, y))
}

/// `pressed`: index of the option under the finger, highlighted.
pub(super) fn draw(c: &mut Canvas, st: &State, sum: &Summary, pressed: Option<usize>) {
    let Some(s) = st.sessions.iter().find(|s| s.id == sum.id) else {
        draw_centered(c, "La sesión ya no existe", None);
        return;
    };
    if let Some(q) = st.question_for(&sum.id) {
        draw_question(c, s, q, pressed);
        return;
    }
    let (x, w) = (24, W - 48);
    let mut y = 22;
    pill(c, s, x, y);
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
        let block = |c: &mut Canvas, y: &mut i32, label: &str, col: Rgb, text: &str, max_lines: usize| {
            if text.is_empty() || *y + lh * 2 > bottom_limit {
                return;
            }
            c.text(&fonts::TAG, label, x, *y, col, 1, 1.0);
            *y += 20;
            let room = usize::try_from((bottom_limit - *y) / lh).unwrap_or(0);
            let lines = wrap(&fonts::SUB, text, w, max_lines.min(room));
            for (l, line) in lines.iter().enumerate() {
                c.text(&fonts::SUB, line, x, *y + l as i32 * lh, TEXT, 0, 1.0);
            }
            *y += lines.len() as i32 * lh + 12;
        };
        block(c, &mut y, "AHORA", ORANGE, &sum.tool, 2);
        agents_block(c, &mut y, x, w, bottom_limit, &sum.agents);
        block(c, &mut y, "TÚ", MUTED, &sum.you, 3);
        block(c, &mut y, "CLAUDE", ORANGE, &sum.claude, 6);
    } else {
        c.text(&fonts::SUB, "Cargando…", x, y + 10, MUTED, 0, 1.0);
    }

    c.round_rect(x as f32, (H - 72) as f32, w as f32, 52.0, 26.0, ORANGE, 1.0);
    c.text_centered(&fonts::TITLE, "Abrir sesión", W as f32 / 2.0, (H - 46) as f32, ON_ORANGE, 1.0);
}

/// State label in a rounded pill at (x, y); returns its width.
fn pill(c: &mut Canvas, s: &Session, x: i32, y: i32) -> i32 {
    let busy = s.busy();
    let label = s.state_label();
    let lw = fonts::SUB.width(label, 0) + 24;
    c.round_rect(x as f32, y as f32, lw as f32, 30.0, 15.0, if busy { ORANGE } else { GREY }, 1.0);
    c.text(&fonts::SUB, label, x + 12, y + 5, if busy { ON_ORANGE } else { TEXT }, 0, 1.0);
    lw
}

/// Claude is waiting on a menu: state and title on one row, the question, one button per option
/// and a smaller "Abrir sesión" at the bottom.
fn draw_question(c: &mut Canvas, s: &Session, q: &Question, pressed: Option<usize>) {
    let (x, w) = (MARGIN as i32, W - 2 * MARGIN as i32);
    let mut y = 22;
    let lw = pill(c, s, x, y);
    c.text(&fonts::SUB, &fit(&fonts::SUB, &s.title, w - lw - 12), x + lw + 12, y + 5, MUTED, 0, 1.0);
    y += 46;

    let n = q.options.len();
    let options_top = option_rect(n, 0).y as i32;
    let lh = fonts::TITLE.line_height();
    let max_lines = usize::try_from((options_top - 12 - y) / lh).unwrap_or(0).max(1);
    for (l, line) in wrap(&fonts::TITLE, &q.text, w, max_lines).iter().enumerate() {
        c.text(&fonts::TITLE, line, x, y + l as i32 * lh, TEXT, 0, 1.0);
    }

    for (k, (num, label)) in q.options.iter().enumerate() {
        option_button(c, option_rect(n, k), *num, label, pressed == Some(k));
    }

    let top = Q_OPEN_TOP as f32;
    c.round_rect(MARGIN, top, w as f32, Q_OPEN_H, Q_OPEN_H / 2.0, GREY, 1.0);
    c.text_centered(&fonts::TITLE, "Abrir sesión", W as f32 / 2.0, top + Q_OPEN_H / 2.0, TEXT, 1.0);
}

/// One option: its number in orange, then the label (one line in the big font if it fits, else up
/// to two in the small one), inside an orange-bordered button; solid orange while pressed.
fn option_button(c: &mut Canvas, r: Rect, num: u8, label: &str, pressed: bool) {
    if pressed {
        c.round_rect(r.x, r.y, r.w, r.h, 14.0, ORANGE_HI, 1.0);
    } else {
        c.round_rect(r.x, r.y, r.w, r.h, 14.0, ORANGE, 1.0);
        c.round_rect(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, 12.0, GREY, 1.0);
    }
    let (fg, accent) = if pressed { (ON_ORANGE, ON_ORANGE) } else { (TEXT, ORANGE) };
    c.text_centered(&fonts::TITLE, &num.to_string(), r.x + 24.0, r.y + r.h / 2.0, accent, 1.0);

    let tx = (r.x + 44.0) as i32;
    let tw = (r.w - 44.0 - 12.0) as i32;
    let (font, lines) = if fonts::TITLE.width(label, 0) <= tw {
        (&fonts::TITLE, vec![label.to_string()])
    } else {
        (&fonts::SUB, wrap(&fonts::SUB, label, tw, 2))
    };
    let lh = font.line_height();
    let block = (lines.len() as i32 - 1) * lh + font.ascent;
    let ty = (r.y + r.h / 2.0) as i32 - block / 2 - 1;
    for (l, line) in lines.iter().enumerate() {
        c.text(font, line, tx, ty + l as i32 * lh, fg, 0, 1.0);
    }
}

/// SUBAGENTES: one row per running subagent ("Explore · Buscar el parser", how long it
/// has been running on the right); with one or two, what each is doing now under it. Never more
/// than [`AGENT_ROWS`] rows: the rest are counted in a "+n más" row.
fn agents_block(c: &mut Canvas, y: &mut i32, x: i32, w: i32, bottom: i32, agents: &[Subagent]) {
    let lh = fonts::SUB.line_height();
    if agents.is_empty() || *y + 20 + lh > bottom {
        return;
    }
    c.text(&fonts::TAG, &format!("SUBAGENTES · {}", agents.len()), x, *y, ORANGE, 1, 1.0);
    *y += 20;
    let room = usize::try_from((bottom - *y) / lh).unwrap_or(0).clamp(1, AGENT_ROWS);
    let shown = if agents.len() > room { (room - 1).max(1) } else { agents.len() };
    let detail = shown == agents.len() && room >= 2 * shown;
    let indent = 14;
    for a in &agents[..shown] {
        c.disc((x + 4) as f32, *y as f32 + fonts::SUB.ascent as f32 * 0.68, 3.0, ORANGE, 1.0);
        let age_w = if a.age.is_empty() { 0 } else { fonts::SUB.width(&a.age, 0) + 10 };
        let label = match (a.kind.is_empty(), a.description.is_empty()) {
            (false, false) => format!("{} · {}", a.kind, a.description),
            (true, _) => a.description.clone(),
            (false, true) => a.kind.clone(),
        };
        c.text(&fonts::SUB, &fit(&fonts::SUB, &label, w - indent - age_w), x + indent, *y, TEXT, 0, 1.0);
        if age_w > 0 {
            c.text(&fonts::SUB, &a.age, x + w - age_w + 10, *y, MUTED, 0, 1.0);
        }
        *y += lh;
        if detail && !a.now.is_empty() {
            c.text(&fonts::SUB, &fit(&fonts::SUB, &a.now, w - indent), x + indent, *y, MUTED, 0, 1.0);
            *y += lh;
        }
    }
    if shown < agents.len() && shown < room {
        c.text(&fonts::SUB, &format!("+{} más", agents.len() - shown), x + indent, *y, MUTED, 0, 1.0);
        *y += lh;
    }
    *y += 12;
}
