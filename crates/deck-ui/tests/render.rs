mod common;

use common::{pixel, render, render_banded, state_with, NOW};
use deck_ui::protocol::{State, LINK_TIMEOUT_MS};
use deck_ui::ui::theme::{BG, GREY, ORANGE};
use deck_ui::ui::{Screen, Ui};

fn sample() -> State {
    let mut st = state_with(&[
        ("busy", "Detección de Seeedstudio SenseCap Indicator", "esp", "", 30),
        ("ready", "Migración de Authentik a Pocket ID", "homelab", "ahora", 85),
        ("wait", "Glance como PWA sin icono", "homelab", "hace 10 min", 12),
        ("idle", "Revisión de sesiones pendientes", "repos", "hace 19 h", -1),
        ("idle", "Fallo de DNS en home.lab", "homelab", "hace 11 h", 40),
    ]);
    st.feed(b"L\t64\treinicia 16:40\t90\t52\treinicia vie 22:00\t39\tA este ritmo: 100% a las 16:10\tahora\n", NOW);
    st.feed(b"C\tB\nC\tH\t32\t4\t1,8 M\ttokens hoy\nC\tS\tesp\t464 k\t1,2,3,4\nC\tD\tL,M\t10,20\t1\nC\tE\n", NOW);
    st
}

fn ui_for(st: &State) -> Ui {
    let mut ui = Ui::new();
    ui.layout(st);
    ui
}

#[test]
fn banded_rendering_matches_full_frame() {
    // the firmware draws in bands of 80 rows; clipping must make that invisible
    let st = sample();
    let mut ui = ui_for(&st);
    for screen in Screen::ALL {
        while ui.screen != screen {
            ui.next_screen(&mut State::default(), 0);
        }
        let full = render(&ui, &st, NOW);
        for band in [80, 37, 1] {
            assert!(full == render_banded(&ui, &st, NOW, band), "{screen:?}, bands of {band}");
        }
    }
}

#[test]
fn offline_screen_is_orange() {
    let st = State::default();
    let frame = render(&Ui::new(), &st, NOW);
    assert_eq!(pixel(&frame, 2, 2), ORANGE.pack());

    let st = sample();
    let frame = render(&ui_for(&st), &st, NOW + LINK_TIMEOUT_MS + 1);
    assert_eq!(pixel(&frame, 2, 2), ORANGE.pack(), "link lost");
}

#[test]
fn tile_colours_follow_state() {
    let st = sample();
    let frame = render(&ui_for(&st), &st, NOW);
    assert_eq!(pixel(&frame, 2, 2), BG.pack(), "margin");
    // 5 sessions → 3x2: busy top-left is solid orange, idle tiles are grey inside
    assert_eq!(pixel(&frame, 60, 60), ORANGE.pack());
    assert_eq!(pixel(&frame, 60, 300), GREY.pack());
}

#[test]
fn tap_opens_and_hold_requests_summary() {
    let mut st = sample();
    let mut ui = ui_for(&st);
    // tap the first tile
    assert_eq!(ui.touch(&mut st, Some((60, 60)), NOW).0, None);
    assert_eq!(ui.touch(&mut st, None, NOW + 100).0.as_deref(), Some("O\tcli:0\n"));
    // hold the second one
    ui.touch(&mut st, Some((240, 60)), NOW + 1000);
    let (msg, _) = ui.touch(&mut st, Some((240, 60)), NOW + 1700);
    assert_eq!(msg.as_deref(), Some("R\tcli:1\n"));
    assert_eq!(st.summary.as_ref().map(|s| s.id.as_str()), Some("cli:1"));
    // releasing after the hold doesn't also open it
    assert_eq!(ui.touch(&mut st, None, NOW + 1800).0, None);
}

#[test]
fn sliding_off_cancels_the_tap() {
    let mut st = sample();
    let mut ui = ui_for(&st);
    ui.touch(&mut st, Some((60, 60)), NOW);
    ui.touch(&mut st, Some((60, 120)), NOW + 50);
    assert_eq!(ui.touch(&mut st, None, NOW + 100).0, None);
}

#[test]
fn button_cycles_screens() {
    let mut st = sample();
    let mut ui = Ui::new();
    let names: Vec<_> = (0..4)
        .map(|_| {
            let s = ui.screen;
            ui.next_screen(&mut st, NOW);
            s
        })
        .collect();
    assert_eq!(names, [Screen::Sessions, Screen::Limits, Screen::Usage, Screen::Sessions]);
}
