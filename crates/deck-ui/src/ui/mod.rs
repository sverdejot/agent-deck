//! Screens and gestures.
//!
//! The side button cycles Sesiones → Límites → Consumo (the firmware calls [`Ui::next_screen`]).
//! On Sesiones a tap opens the session on the Mac, holding it shows its summary, and with more
//! than 9 sessions pages are swiped sideways. Límites and Consumo are read-only. Without a link to
//! the Mac every screen gives way to the offline screen.

pub mod grid;
mod limits;
mod offline;
mod sessions;
mod summary;
pub mod theme;
mod usage;

use crate::gfx::{fonts, Canvas, H, W};
use crate::protocol::{open_msg, summary_msg, Session, State, Summary};
use theme::{BG, MUTED, TEXT};

const HOLD_MS: u32 = 600;
const SLOP_PX: i32 = 14;
const TOAST_MS: u32 = 1400;
const FLASH_MS: u32 = 500;
const SUMMARY_TIMEOUT_MS: u32 = 30_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Sessions,
    Limits,
    Usage,
}

impl Screen {
    pub const ALL: [Screen; 3] = [Screen::Sessions, Screen::Limits, Screen::Usage];

    pub fn name(self) -> &'static str {
        match self {
            Screen::Sessions => "Sesiones",
            Screen::Limits => "Límites",
            Screen::Usage => "Consumo",
        }
    }

    fn next(self) -> Screen {
        let i = Screen::ALL.iter().position(|&s| s == self).unwrap_or(0);
        Screen::ALL[(i + 1) % Screen::ALL.len()]
    }
}

/// UI state: current screen, page, gesture in progress and transient effects.
pub struct Ui {
    pub screen: Screen,
    page: usize,
    /// horizontal drag offset of the page (px)
    page_x: f32,
    touching: bool,
    swiping: bool,
    held: bool,
    touch_start: (i32, i32),
    touch_start_ms: u32,
    pressed: Option<usize>,
    flash_id: String,
    flash_ms: u32,
    toast_ms: Option<u32>,
    summary_opened_ms: u32,
    tiles: Vec<sessions::TileText>,
    /// set when the sessions changed: [`Ui::layout`] must run before the next draw
    pub layout_dirty: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            screen: Screen::Sessions,
            page: 0,
            page_x: 0.0,
            touching: false,
            swiping: false,
            held: false,
            touch_start: (0, 0),
            touch_start_ms: 0,
            pressed: None,
            flash_id: String::new(),
            flash_ms: 0,
            toast_ms: None,
            summary_opened_ms: 0,
            tiles: Vec::new(),
            layout_dirty: true,
        }
    }
}

impl Ui {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- drawing ------------------------------------------------------------------------------

    /// Draws the current screen into `c` (called once per band).
    pub fn draw(&self, c: &mut Canvas, st: &State, now: u32) {
        if !st.linked(now) {
            offline::draw(c, st.ever_linked);
            return;
        }
        c.fill_rect(0, 0, W, H, BG.pack());
        match self.screen {
            Screen::Limits => limits::draw(c, st),
            Screen::Usage => usage::draw(c, st),
            Screen::Sessions => match &st.summary {
                Some(sum) => summary::draw(c, st, sum),
                None if st.sessions.is_empty() => {
                    draw_centered(c, "Sin sesiones abiertas", Some("Abre Claude Code en la terminal"));
                }
                None => self.draw_sessions(c, st, now),
            },
        }
        self.draw_toast(c, now);
    }

    /// Name of the screen, briefly, after switching with the button.
    fn draw_toast(&self, c: &mut Canvas, now: u32) {
        let Some(t0) = self.toast_ms else { return };
        let t = now.wrapping_sub(t0);
        if t > TOAST_MS {
            return;
        }
        let a = if t < 1000 { 1.0 } else { 1.0 - (t - 1000) as f32 / 400.0 };
        let name = self.screen.name();
        let tw = (fonts::TITLE.width(name, 0) + 48) as f32;
        let (x, y) = ((W as f32 - tw) / 2.0, H as f32 - 92.0);
        c.round_rect(x, y, tw, 64.0, 0.0, theme::BG.mix(theme::GREY, 0.7), 0.92 * a);
        c.text_centered(&fonts::TITLE, name, W as f32 / 2.0, y + 24.0, TEXT, a);
        for (i, s) in Screen::ALL.iter().enumerate() {
            let on = *s == self.screen;
            let x = W as f32 / 2.0 + (i as f32 - 1.0) * 14.0;
            c.disc(x, y + 48.0, 3.0, if on { TEXT } else { MUTED }, if on { a } else { 0.5 * a });
        }
    }

    /// Needs another frame soon: something moves or pulses.
    pub fn animating(&self, st: &State, now: u32) -> bool {
        if self.page_x != 0.0 && !self.touching {
            return true;
        }
        if now.wrapping_sub(self.flash_ms) < FLASH_MS + 50
            || self.toast_ms.is_some_and(|t| now.wrapping_sub(t) < TOAST_MS + 50)
        {
            return true;
        }
        if self.screen == Screen::Sessions && st.summary.is_none() && st.linked(now) {
            let n = st.sessions.len();
            let start = (self.page * grid::PER_PAGE).min(n);
            let end = (start + grid::page_size(n, self.page)).min(n);
            return st.sessions[start..end].iter().any(Session::waiting);
        }
        false
    }

    /// Per-frame motion (page settling, summary timeout). Returns true if something changed.
    pub fn tick(&mut self, st: &mut State, now: u32) -> bool {
        let mut changed = false;
        if !self.touching && self.page_x != 0.0 {
            self.page_x *= 0.72;
            if self.page_x.abs() < 0.5 {
                self.page_x = 0.0;
            }
            changed = true;
        }
        if st.summary.is_some() && !self.touching && now.wrapping_sub(self.summary_opened_ms) > SUMMARY_TIMEOUT_MS {
            st.summary = None;
            changed = true;
        }
        changed
    }

    // ---- input --------------------------------------------------------------------------------

    /// Side button, short press.
    pub fn next_screen(&mut self, st: &mut State, now: u32) {
        self.screen = self.screen.next();
        st.summary = None;
        self.pressed = None;
        self.toast_ms = Some(now);
    }

    fn tile_at(&self, st: &State, x: i32, y: i32, now: u32) -> Option<usize> {
        if !st.linked(now) || st.summary.is_some() {
            return None;
        }
        let n = st.sessions.len();
        let count = grid::page_size(n, self.page);
        (0..count).find(|&k| grid::tile_rect(n, k, count).contains(x, y)).map(|k| self.page * grid::PER_PAGE + k)
    }

    /// Handles this loop's touch reading (`None` = no finger). Returns a message for the Mac, if
    /// any, and whether the screen needs a redraw.
    pub fn touch(&mut self, st: &mut State, touch: Option<(i32, i32)>, now: u32) -> (Option<String>, bool) {
        if self.screen != Screen::Sessions {
            self.touching = touch.is_some(); // other screens are read-only
            return (None, false);
        }
        match (touch, self.touching) {
            (Some((x, y)), false) => {
                self.touching = true;
                self.swiping = false;
                self.held = false;
                self.touch_start = (x, y);
                self.touch_start_ms = now;
                self.pressed = self.tile_at(st, x, y, now);
                (None, true)
            }
            (Some((x, y)), true) => self.finger_moved(st, x, y, now),
            (None, true) => (self.finger_lifted(st, now), true),
            (None, false) => (None, false),
        }
    }

    fn finger_moved(&mut self, st: &mut State, x: i32, y: i32, now: u32) -> (Option<String>, bool) {
        let pages = grid::page_count(st.sessions.len());
        let (sx, sy) = self.touch_start;
        if !self.swiping && !self.held && st.summary.is_none() && pages > 1 && (x - sx).abs() > SLOP_PX {
            self.swiping = true;
            self.pressed = None;
        }
        if self.swiping {
            let mut dx = (x - sx) as f32;
            if (self.page == 0 && dx > 0.0) || (self.page == pages - 1 && dx < 0.0) {
                dx *= 0.3; // rubber band at the ends
            }
            self.page_x = dx;
            return (None, true);
        }
        if !self.held && self.pressed.is_some() && ((x - sx).abs() > SLOP_PX || (y - sy).abs() > SLOP_PX) {
            self.pressed = None; // finger slid off: cancel the tap
            return (None, true);
        }
        if let Some(i) = self.pressed.filter(|_| !self.held && now.wrapping_sub(self.touch_start_ms) > HOLD_MS) {
            self.held = true;
            self.pressed = None;
            let id = st.sessions[i].id.clone();
            self.summary_opened_ms = now;
            let msg = summary_msg(&id);
            st.summary = Some(Summary { id, ..Default::default() });
            return (Some(msg), true);
        }
        (None, false)
    }

    fn finger_lifted(&mut self, st: &mut State, now: u32) -> Option<String> {
        self.touching = false;
        let pages = grid::page_count(st.sessions.len());
        let mut msg = None;
        if self.swiping {
            if self.page_x < -70.0 && self.page < pages - 1 {
                self.page += 1;
                self.page_x += W as f32;
            } else if self.page_x > 70.0 && self.page > 0 {
                self.page -= 1;
                self.page_x -= W as f32;
            }
        } else if let Some(id) = st.summary.as_ref().filter(|_| !self.held).map(|s| s.id.clone()) {
            if self.touch_start.1 >= summary::OPEN_BUTTON_Y {
                msg = Some(self.open(&id, now));
                st.summary = None;
            } else if now.wrapping_sub(self.summary_opened_ms) > 300 {
                st.summary = None;
            }
        } else if let Some(i) = self.pressed.filter(|_| !self.held) {
            msg = Some(self.open(&st.sessions[i].id.clone(), now));
        }
        self.pressed = None;
        self.swiping = false;
        self.held = false;
        msg
    }

    fn open(&mut self, id: &str, now: u32) -> String {
        self.flash_id = id.to_string();
        self.flash_ms = now;
        open_msg(id)
    }
}

fn draw_centered(c: &mut Canvas, a: &str, b: Option<&str>) {
    c.text_centered(&fonts::TITLE, a, W as f32 / 2.0, 225.0, TEXT, 1.0);
    if let Some(b) = b {
        c.text_centered(&fonts::SUB, b, W as f32 / 2.0, 265.0, MUTED, 1.0);
    }
}
