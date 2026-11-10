mod common;

use common::{pixel, render, render_banded, snapshot, state_with, NOW};
use deck_ui::protocol::{State, Summary, LINK_TIMEOUT_MS};
use deck_ui::ui::theme::{BG, GREY, ORANGE, ORANGE_DEEP};
use deck_ui::ui::{Screen, Ui};

const SAMPLE: [(&str, &str, &str, &str, i32); 5] = [
    ("busy", "Detección de Seeedstudio SenseCap Indicator", "esp", "", 30),
    ("ready", "Migración de Authentik a Pocket ID", "homelab", "ahora", 85),
    ("wait", "Glance como PWA sin icono", "homelab", "hace 10 min", 12),
    ("idle", "Revisión de sesiones pendientes", "repos", "hace 19 h", -1),
    ("idle", "Fallo de DNS en home.lab", "homelab", "hace 11 h", 40),
];

fn sample() -> State {
    let mut st = state_with(&SAMPLE);
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
    // 5 sessions → 2x3: busy top-left is solid orange, idle tiles are grey inside
    assert_eq!(pixel(&frame, 60, 60), ORANGE.pack());
    assert_eq!(pixel(&frame, 60, 300), GREY.pack());
}

#[test]
fn subagent_badge_on_the_tile() {
    // 5 sessions → 2 columns x 3 rows; the first tile (busy) is 225 px wide with 20 px of padding,
    // so the badge ends at x = 215 and is centred on y = 37; its right end is plain badge colour
    let mut st = sample();
    let without = render(&ui_for(&st), &st, NOW);
    assert_eq!(pixel(&without, 211, 37), ORANGE.pack());
    // the same snapshot, with 2 subagents on the first session
    st.feed(snapshot(&SAMPLE).replacen("\t30\n", "\t30\t2\n", 1).as_bytes(), NOW);
    assert_eq!(st.sessions[0].agents, 2);
    let with = render(&ui_for(&st), &st, NOW);
    assert_eq!(pixel(&with, 211, 37), ORANGE_DEEP.pack());
}

#[test]
fn summary_with_subagents_renders_in_bands() {
    let mut st = sample();
    st.summary = Some(Summary { id: "cli:0".into(), ..Default::default() });
    st.feed(b"Y\tcli:0\tEmpezo hace 1 h\thola\tadios\tAgent: Buscar usos\n", NOW);
    st.feed(
        concat!(
            "G\tB\tcli:0\n",
            "G\tS\tcli:0\tExplore\tBuscar usos de tile_rect en todo el repositorio\t2 min\tGrep: tile_rect\n",
            "G\tS\tcli:0\tgeneral-purpose\tTests\t40 s\t\n",
            "G\tS\tcli:0\tPlan\tDiseno\t10 s\t\n",
            "G\tE\tcli:0\n",
        )
        .as_bytes(),
        NOW,
    );
    assert_eq!(st.summary.as_ref().unwrap().agents.len(), 3);
    let ui = ui_for(&st);
    let full = render(&ui, &st, NOW);
    assert!(full == render_banded(&ui, &st, NOW, 37), "summary with subagents, bands of 37");
}

#[test]
fn tap_opens_and_hold_requests_summary() {
    let mut st = sample();
    let mut ui = ui_for(&st);
    // tap the first tile
    assert_eq!(ui.touch(&mut st, Some((60, 60)), NOW).0, None);
    assert_eq!(ui.touch(&mut st, None, NOW + 100).0.as_deref(), Some("O\tcli:0\n"));
    // hold the second one
    ui.touch(&mut st, Some((300, 60)), NOW + 1000);
    let (msg, _) = ui.touch(&mut st, Some((300, 60)), NOW + 1700);
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

/// The waiting session's summary open, with a permission prompt pending.
fn asking() -> (State, Ui) {
    let mut st = sample();
    let mut ui = ui_for(&st);
    ui.touch(&mut st, Some((100, 240)), NOW); // third tile: "wait"
    let (msg, _) = ui.touch(&mut st, Some((100, 240)), NOW + 700);
    assert_eq!(msg.as_deref(), Some("R\tcli:2\n"));
    ui.touch(&mut st, None, NOW + 800);
    let q = "Q\tcli:2\trm -rf build - Do you want to proceed?\t1.Yes\t2.Yes, and don't ask again\t3.No\n";
    st.feed(q.as_bytes(), NOW + 900);
    assert!(st.question_for("cli:2").is_some());
    (st, ui)
}

#[test]
fn question_summary_renders_in_bands() {
    let (mut st, ui) = asking();
    let full = render(&ui, &st, NOW + 1000);
    assert!(full == render_banded(&ui, &st, NOW + 1000, 80), "summary with a question, bands of 80");
    st.question = None;
    assert!(full != render(&ui, &st, NOW + 1000), "the question changes the summary");
}

#[test]
fn tapping_an_option_answers_it() {
    let (mut st, mut ui) = asking();
    // three options stacked above "Abrir sesión": the second one is around y = 318
    assert_eq!(ui.touch(&mut st, Some((240, 318)), NOW + 2000).0, None);
    let (msg, _) = ui.touch(&mut st, None, NOW + 2100);
    assert_eq!(msg.as_deref(), Some("A\tcli:2\t2\tYes, and don't ask again\n"));
    assert!(st.question.is_none(), "cleared until the Mac sends the next one");
    assert!(st.summary.is_some(), "the summary stays open");
}

#[test]
fn sliding_off_an_option_does_nothing() {
    let (mut st, mut ui) = asking();
    ui.touch(&mut st, Some((240, 318)), NOW + 2000);
    ui.touch(&mut st, Some((240, 360)), NOW + 2050);
    assert_eq!(ui.touch(&mut st, None, NOW + 2100).0, None);
    assert!(st.question.is_some() && st.summary.is_some());
}

#[test]
fn open_button_still_works_with_a_question() {
    let (mut st, mut ui) = asking();
    ui.touch(&mut st, Some((240, 450)), NOW + 2000);
    assert_eq!(ui.touch(&mut st, None, NOW + 2100).0.as_deref(), Some("O\tcli:2\n"));
    assert!(st.summary.is_none());
}
