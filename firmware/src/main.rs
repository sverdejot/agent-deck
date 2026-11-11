//! Agent Deck — firmware for the SenseCAP Indicator D1 (ESP32-S3).
//!
//! Shows the Claude Code terminal sessions open on the Mac and opens the one you tap; the side
//! button cycles Sessions → Limits → Usage (long press dims). Everything on screen comes from
//! the `deck-ui` crate; this binary supplies the hardware: panel, touch, button, backlight, serial
//! link and the clock. The Mac side is `../host` (see `docs/protocol.md`).

mod board;
mod render;
mod splash;

use anyhow::Result;
use deck_ui::protocol::{State, HELLO};
use deck_ui::ui::{Screen, Ui};
use esp_idf_svc::hal::delay::FreeRtos;

use board::{millis, Board};
use render::Renderer;

const LONG_PRESS_MS: u32 = 700;
const FRAME_MS: u32 = 16;
const HELLO_EVERY_MS: u32 = 2000;
const STATS_EVERY_MS: u32 = 5000;
const BRIGHT: u32 = 80;
const DIM: u32 = 15;

/// Side button: short press → next screen, long press → dim / undim.
#[derive(Default)]
struct Button {
    was_down: bool,
    long: bool,
    down_at: u32,
}

enum Press {
    Short,
    Long,
}

impl Button {
    fn update(&mut self, down: bool, now: u32) -> Option<Press> {
        let mut press = None;
        if down && !self.was_down {
            self.down_at = now;
            self.long = false;
        }
        if down && !self.long && now.wrapping_sub(self.down_at) > LONG_PRESS_MS {
            self.long = true;
            press = Some(Press::Long);
        }
        if !down && self.was_down && !self.long && now.wrapping_sub(self.down_at) > 30 {
            press = Some(Press::Short);
        }
        self.was_down = down;
        press
    }
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let splash = splash::load(); // before the panel starts: flash reads stall the cache
    let mut board = Board::new()?;
    let mut renderer = Renderer::new()?;
    let mut st = State::default();
    let mut ui = Ui::new();
    if let Some(s) = splash {
        ui.set_splash(s);
    }
    let mut button = Button::default();

    let mut serial = [0u8; 512];
    let mut dirty = true;
    let (mut last_frame, mut last_hello, mut last_stats) = (0u32, 0u32, 0u32);
    let mut dim = false;
    let mut was_linked = false;

    loop {
        let now = millis();

        // serial link
        let n = board.read_serial(&mut serial);
        if n > 0 {
            let ch = st.feed(&serial[..n], now);
            ui.layout_dirty |= ch.sessions;
            dirty |= ch.sessions
                || ch.summary
                || (ch.limits && ui.screen == Screen::Limits)
                || (ch.usage && ui.screen == Screen::Usage);
        }
        if !st.ever_linked && now.wrapping_sub(last_hello) > HELLO_EVERY_MS {
            last_hello = now;
            board.write_serial(HELLO);
        }
        if st.linked(now) != was_linked {
            was_linked = st.linked(now);
            dirty = true;
        }

        // touch
        let (msg, changed) = ui.touch(&mut st, board.touch(), now);
        if let Some(m) = msg {
            board.write_serial(&m);
        }
        dirty |= changed | ui.tick(&mut st, now);

        // side button
        match button.update(board.button(), now) {
            Some(Press::Short) => {
                ui.next_screen(&mut st, now);
                dirty = true;
            }
            Some(Press::Long) => {
                dim = !dim;
                board.set_backlight(if dim { DIM } else { BRIGHT });
            }
            None => {}
        }

        // frame
        if (dirty || ui.animating(&st, now)) && now.wrapping_sub(last_frame) > FRAME_MS {
            dirty = false;
            last_frame = now;
            if ui.layout_dirty {
                ui.layout(&st);
            }
            let (total, drawing) = renderer.frame(&mut board.display, |c| ui.draw(c, &st, now));
            if now.wrapping_sub(last_stats) > STATS_EVERY_MS {
                last_stats = now;
                // the Mac ignores 'D' lines
                board.write_serial(&format!("D frame={}ms draw={}ms\n", total / 1000, drawing / 1000));
            }
        } else {
            FreeRtos::delay_ms(2);
        }
    }
}
