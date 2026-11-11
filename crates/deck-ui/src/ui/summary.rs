//! Summary of a session (hold a tile): state, title, what it is doing, the subagents it has
//! running, your last message and Claude's last reply, and a button to open it. When Claude is
//! waiting on a menu (a question, a permission, a plan approval) the summary shows the question and
//! one button per option instead.
//!
//! Same visual language as the rest of the deck: a tracked label row at the top (as on Limits and
//! Usage), grey section labels, square flat rows for every button, orange only for state and
//! markers, and pressed = lighter grey.

use super::draw_centered;
use super::grid::Rect;
use super::theme::{FAINT, GREY, GREY_HI, MUTED, ORANGE, TEXT};
use crate::gfx::{fonts, Canvas, H, W};
use crate::protocol::{Question, Session, State, Subagent, Summary};
use crate::text::{fit, wrap};

const MARGIN: f32 = 24.0;
/// "Open session": a flat square row along the bottom, the same with or without a question.
const OPEN_H: f32 = 48.0;
const OPEN_TOP: f32 = H as f32 - MARGIN - OPEN_H;
/// Taps below this line hit the "Open session" button (a little slop above it).
pub const OPEN_BUTTON_Y: i32 = OPEN_TOP as i32 - 4;
/// Rows of the SUBAGENTS block, "+n more" included.
const AGENT_ROWS: usize = 4;
/// Option rows: flat and square like the tiles, finger-sized, stacked up from above the button.
const OPTION_H: f32 = 56.0;
const OPTION_GAP: f32 = 6.0;
const OPTIONS_BOTTOM: f32 = OPEN_TOP - 8.0;
/// Header row (state label and context), then the heading under it.
const HEADER_Y: i32 = 22;
const HEADING_Y: i32 = 54;

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

/// What the finger is on in the summary, highlighted while it stays there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Pressed {
    Option(usize),
    Open,
}

pub(super) fn draw(c: &mut Canvas, st: &State, sum: &Summary, pressed: Option<Pressed>) {
    let Some(s) = st.sessions.iter().find(|s| s.id == sum.id) else {
        draw_centered(c, "This session is gone", None);
        return;
    };
    let open_pressed = pressed == Some(Pressed::Open);
    if let Some(q) = st.question_for(&sum.id) {
        let option = match pressed {
            Some(Pressed::Option(k)) => Some(k),
            _ => None,
        };
        draw_question(c, s, q, option);
        open_button(c, open_pressed);
        return;
    }
    let (x, w) = (MARGIN as i32, W - 2 * MARGIN as i32);
    header(c, s, &s.age); // the repo is already in the meta line
    let mut y = HEADING_Y;

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
        let bottom_limit = OPEN_TOP as i32 - 12;
        let block = |c: &mut Canvas, y: &mut i32, label: &str, text: &str, max_lines: usize| {
            if text.is_empty() || *y + lh * 2 > bottom_limit {
                return;
            }
            c.text(&fonts::TAG, label, x, *y, FAINT, 2, 1.0);
            *y += 20;
            let room = usize::try_from((bottom_limit - *y) / lh).unwrap_or(0);
            let lines = wrap(&fonts::SUB, text, w, max_lines.min(room));
            for (l, line) in lines.iter().enumerate() {
                c.text(&fonts::SUB, line, x, *y + l as i32 * lh, TEXT, 0, 1.0);
            }
            *y += lines.len() as i32 * lh + 12;
        };
        block(c, &mut y, "NOW", &sum.tool, 2);
        agents_block(c, &mut y, x, w, bottom_limit, &sum.agents);
        block(c, &mut y, "YOU", &sum.you, 3);
        block(c, &mut y, "CLAUDE", &sum.claude, 6);
    } else {
        c.text(&fonts::SUB, "Loading…", x, y + 10, MUTED, 0, 1.0);
    }
    open_button(c, open_pressed);
}

/// Header row, like Limits and Usage: the state as a tracked label (orange unless idle, as on
/// the tiles) and `context` in grey on the right.
fn header(c: &mut Canvas, s: &Session, context: &str) {
    let (x, w) = (MARGIN as i32, W - 2 * MARGIN as i32);
    let label = s.state_label().to_uppercase();
    let col = if matches!(s.state.as_str(), "busy" | "wait" | "ready") { ORANGE } else { FAINT };
    c.text(&fonts::TAG, &label, x, HEADER_Y, col, 2, 1.0);
    let lw = fonts::TAG.width(&label, 2) + 16;
    let context = fit(&fonts::SUB, context, w - lw);
    c.text(&fonts::SUB, &context, x + w - fonts::SUB.width(&context, 0), HEADER_Y - 2, FAINT, 0, 1.0);
}

/// "Open session": a flat square row like the options, lighter under the finger.
fn open_button(c: &mut Canvas, pressed: bool) {
    let (x, w) = (MARGIN, W as f32 - 2.0 * MARGIN);
    c.round_rect(x, OPEN_TOP, w, OPEN_H, 0.0, if pressed { GREY_HI } else { GREY }, 1.0);
    c.text_centered(&fonts::TITLE, "Open session", W as f32 / 2.0, OPEN_TOP + OPEN_H / 2.0, TEXT, 1.0);
}

/// Claude is waiting on a menu: the header (with the session's title, shown nowhere else here),
/// the question in the heading font, then one flat row per option above "Open session".
fn draw_question(c: &mut Canvas, s: &Session, q: &Question, pressed: Option<usize>) {
    let (x, w) = (MARGIN as i32, W - 2 * MARGIN as i32);
    header(c, s, &s.title);

    let n = q.options.len();
    let y = HEADING_Y;
    let options_top = option_rect(n, 0).y as i32;
    let lh = fonts::HEAD.line_height();
    let max_lines = usize::try_from((options_top - 16 - y) / lh).unwrap_or(0).max(1);
    for (l, line) in wrap(&fonts::HEAD, &q.text, w, max_lines).iter().enumerate() {
        c.text(&fonts::HEAD, line, x, y + l as i32 * lh, TEXT, 0, 1.0);
    }

    for (k, (num, label)) in q.options.iter().enumerate() {
        option_row(c, option_rect(n, k), *num, label, pressed == Some(k));
    }
}

/// One option: a flat grey row (square, like an idle tile) with its number in orange and the label,
/// always in the title font, on up to two lines. Lighter under the finger, like a pressed tile.
fn option_row(c: &mut Canvas, r: Rect, num: u8, label: &str, pressed: bool) {
    c.round_rect(r.x, r.y, r.w, r.h, 0.0, if pressed { GREY_HI } else { GREY }, 1.0);
    let fg = TEXT;
    c.text_centered(&fonts::TITLE, &num.to_string(), r.x + 22.0, r.y + r.h / 2.0, ORANGE, 1.0);

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
    c.text(&fonts::TAG, &format!("SUBAGENTS · {}", agents.len()), x, *y, FAINT, 2, 1.0);
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
