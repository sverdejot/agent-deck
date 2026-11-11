//! Summary of a session (hold a tile): state, title, what it is doing, the subagents it has
//! running, your last message and Claude's last reply, and a button to open it. When Claude is
//! waiting on a menu (a question, a permission, a plan approval) the summary shows the question and
//! one button per option instead.

use super::draw_centered;
use super::grid::Rect;
use super::theme::{FAINT, GREY, MUTED, ON_ORANGE, ORANGE, TEXT};
use crate::gfx::{fonts, Canvas, Rgb, H, W};
use crate::protocol::{Question, Session, State, Subagent, Summary};
use crate::text::{fit, wrap};

/// Taps below this line hit the "Open session" button.
pub const OPEN_BUTTON_Y: i32 = H - 80;
/// Rows of the SUBAGENTS block, "+n more" included.
const AGENT_ROWS: usize = 4;

/// With a question, "Open session" becomes a quiet text link at the very bottom.
const Q_OPEN_CY: f32 = H as f32 - 28.0;
/// Taps below this line hit "Open session" when a question is on screen.
pub const Q_OPEN_BUTTON_Y: i32 = H - 54;
/// Option rows: flat and square like the tiles, finger-sized, stacked up from above the link.
const OPTION_H: f32 = 60.0;
const OPTION_GAP: f32 = 6.0;
const OPTIONS_BOTTOM: f32 = (H - 62) as f32;
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
        draw_centered(c, "This session is gone", None);
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
        block(c, &mut y, "NOW", ORANGE, &sum.tool, 2);
        agents_block(c, &mut y, x, w, bottom_limit, &sum.agents);
        block(c, &mut y, "YOU", MUTED, &sum.you, 3);
        block(c, &mut y, "CLAUDE", ORANGE, &sum.claude, 6);
    } else {
        c.text(&fonts::SUB, "Loading…", x, y + 10, MUTED, 0, 1.0);
    }

    c.round_rect(x as f32, (H - 72) as f32, w as f32, 52.0, 26.0, ORANGE, 1.0);
    c.text_centered(&fonts::TITLE, "Open session", W as f32 / 2.0, (H - 46) as f32, ON_ORANGE, 1.0);
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

/// Claude is waiting on a menu. Laid out like the other screens: a tracked label with the session
/// on the right, the question in the heading font, then one flat row per option and a quiet
/// "Open session" link at the bottom.
fn draw_question(c: &mut Canvas, s: &Session, q: &Question, pressed: Option<usize>) {
    let (x, w) = (MARGIN as i32, W - 2 * MARGIN as i32);
    let label = "NEEDS YOU";
    c.text(&fonts::TAG, label, x, 22, ORANGE, 2, 1.0);
    let lw = fonts::TAG.width(label, 2) + 16;
    let title = fit(&fonts::SUB, &s.title, w - lw);
    c.text(&fonts::SUB, &title, x + w - fonts::SUB.width(&title, 0), 20, FAINT, 0, 1.0);

    let n = q.options.len();
    let y = 54;
    let options_top = option_rect(n, 0).y as i32;
    let lh = fonts::HEAD.line_height();
    let max_lines = usize::try_from((options_top - 16 - y) / lh).unwrap_or(0).max(1);
    for (l, line) in wrap(&fonts::HEAD, &q.text, w, max_lines).iter().enumerate() {
        c.text(&fonts::HEAD, line, x, y + l as i32 * lh, TEXT, 0, 1.0);
    }

    for (k, (num, label)) in q.options.iter().enumerate() {
        option_row(c, option_rect(n, k), *num, label, pressed == Some(k));
    }

    c.text_centered(&fonts::SUB, "Open session", W as f32 / 2.0, Q_OPEN_CY, MUTED, 1.0);
}

/// One option: a flat grey row (square, like an idle tile) with its number in orange and the label,
/// always in the title font, on up to two lines. Pressed, it turns solid orange like a working tile.
fn option_row(c: &mut Canvas, r: Rect, num: u8, label: &str, pressed: bool) {
    c.round_rect(r.x, r.y, r.w, r.h, 0.0, if pressed { ORANGE } else { GREY }, 1.0);
    let (fg, accent) = if pressed { (ON_ORANGE, ON_ORANGE) } else { (TEXT, ORANGE) };
    c.text_centered(&fonts::TITLE, &num.to_string(), r.x + 22.0, r.y + r.h / 2.0, accent, 1.0);

    let tx = (r.x + 44.0) as i32;
    let tw = (r.w - 44.0 - 14.0) as i32;
    let font = &fonts::TITLE;
    let lines = wrap(font, label, tw, 2);
    let lh = font.line_height();
    let block = (lines.len() as i32 - 1) * lh + font.ascent;
    let ty = (r.y + r.h / 2.0) as i32 - block / 2 - 1;
    for (l, line) in lines.iter().enumerate() {
        c.text(font, line, tx, ty + l as i32 * lh, fg, 0, 1.0);
    }
}

/// SUBAGENTS: one row per running subagent, like the Usage legend: a square marker, what it was
/// asked to do, and how long it has been running on the right; with one or two, its type and what
/// it is doing now in grey under it. Never more than [`AGENT_ROWS`] rows: the rest are counted in a
/// "+n more" row.
fn agents_block(c: &mut Canvas, y: &mut i32, x: i32, w: i32, bottom: i32, agents: &[Subagent]) {
    let lh = fonts::SUB.line_height();
    if agents.is_empty() || *y + 20 + lh > bottom {
        return;
    }
    c.text(&fonts::TAG, &format!("SUBAGENTS · {}", agents.len()), x, *y, ORANGE, 1, 1.0);
    *y += 20;
    let room = usize::try_from((bottom - *y) / lh).unwrap_or(0).clamp(1, AGENT_ROWS);
    let shown = if agents.len() > room { (room - 1).max(1) } else { agents.len() };
    let detail = shown == agents.len() && room >= 2 * shown;
    let indent = 18;
    for a in &agents[..shown] {
        c.round_rect(x as f32, (*y + 3) as f32, 10.0, 10.0, 0.0, ORANGE, 1.0);
        let age_w = if a.age.is_empty() { 0 } else { fonts::SUB.width(&a.age, 0) + 12 };
        let label = if a.description.is_empty() { &a.kind } else { &a.description };
        c.text(&fonts::SUB, &fit(&fonts::SUB, label, w - indent - age_w), x + indent, *y, TEXT, 0, 1.0);
        if age_w > 0 {
            c.text(&fonts::SUB, &a.age, x + w - age_w + 12, *y, MUTED, 0, 1.0);
        }
        *y += lh;
        let sub = [a.kind.as_str(), a.now.as_str()]
            .into_iter()
            .filter(|t| !t.is_empty() && *t != label.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        if detail && !sub.is_empty() {
            c.text(&fonts::SUB, &fit(&fonts::SUB, &sub, w - indent), x + indent, *y, FAINT, 0, 1.0);
            *y += lh;
        }
    }
    if shown < agents.len() && shown < room {
        c.text(&fonts::SUB, &format!("+{} more", agents.len() - shown), x + indent, *y, MUTED, 0, 1.0);
        *y += lh;
    }
    *y += 12;
}
