//! Shared helpers for the integration tests.
#![allow(dead_code)] // each test binary uses a different subset

use std::fmt::Write as _;

use deck_ui::gfx::{Canvas, H, W};
use deck_ui::protocol::State;
use deck_ui::ui::Ui;

pub const NOW: u32 = 10_000;

/// A snapshot as the Mac would send it: `(state, title, place, age, ctx)` per session.
pub fn snapshot(sessions: &[(&str, &str, &str, &str, i32)]) -> String {
    let mut s = String::from("B\n");
    for (i, (state, title, place, age, ctx)) in sessions.iter().enumerate() {
        writeln!(s, "S\tcli:{i}\tcode\t{state}\t{title}\t{place}\t{age}\t{ctx}").unwrap();
    }
    s + "E\n"
}

pub fn state_with(sessions: &[(&str, &str, &str, &str, i32)]) -> State {
    let mut st = State::default();
    st.feed(snapshot(sessions).as_bytes(), NOW);
    st
}

/// Renders a whole frame in one go.
pub fn render(ui: &Ui, st: &State, now: u32) -> Vec<u16> {
    let mut buf = vec![0u16; (W * H) as usize];
    let mut c = Canvas::new(&mut buf, 0);
    ui.draw(&mut c, st, now);
    buf
}

/// Renders a frame in bands of `band` rows, like the firmware does.
pub fn render_banded(ui: &Ui, st: &State, now: u32, band: i32) -> Vec<u16> {
    let mut frame = vec![0u16; (W * H) as usize];
    for y in (0..H).step_by(band as usize) {
        let rows = band.min(H - y);
        let mut buf = vec![0u16; (rows * W) as usize];
        let mut c = Canvas::new(&mut buf, y);
        ui.draw(&mut c, st, now);
        frame[(y * W) as usize..((y + rows) * W) as usize].copy_from_slice(&buf);
    }
    frame
}

pub fn pixel(frame: &[u16], x: i32, y: i32) -> u16 {
    frame[(y * W + x) as usize]
}
