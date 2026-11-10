mod common;

use common::{snapshot, state_with, NOW};
use deck_ui::protocol::{open_msg, summary_msg, State, Summary, LINK_TIMEOUT_MS};

#[test]
fn snapshot_replaces_sessions() {
    let mut st = State::default();
    let ch = st
        .feed(snapshot(&[("busy", "Uno", "esp", "", 30), ("idle", "Dos", "homelab", "hace 3 h", -1)]).as_bytes(), NOW);
    assert!(ch.sessions);
    assert_eq!(st.sessions.len(), 2);
    let s = &st.sessions[1];
    assert_eq!((s.state.as_str(), s.title.as_str(), s.place.as_str()), ("idle", "Dos", "homelab"));
    assert_eq!((s.age.as_str(), s.ctx), ("hace 3 h", -1));
    assert!(st.sessions[0].busy());
}

#[test]
fn identical_snapshot_changes_nothing() {
    let snap = snapshot(&[("ready", "Uno", "esp", "ahora", 10)]);
    let mut st = State::default();
    st.feed(snap.as_bytes(), NOW);
    let ch = st.feed(snap.as_bytes(), NOW + 1000);
    assert!(!ch.sessions);
}

#[test]
fn lines_can_arrive_in_pieces() {
    let snap = snapshot(&[("wait", "Partida", "esp", "", 50)]);
    let mut st = State::default();
    let (a, b) = snap.as_bytes().split_at(7);
    assert!(!st.feed(a, NOW).sessions);
    assert!(st.feed(b, NOW).sessions);
    assert!(st.sessions[0].waiting());
}

#[test]
fn older_snapshot_without_ctx_or_age_still_parses() {
    let mut st = State::default();
    st.feed(b"B\nS\tid\tcode\tidle\tViejo\tesp\nE\n", NOW);
    assert_eq!(st.sessions[0].ctx, -1);
    assert_eq!(st.sessions[0].age, "");
}

#[test]
fn link_times_out() {
    let st = state_with(&[]);
    assert!(st.linked(NOW));
    assert!(st.linked(NOW + LINK_TIMEOUT_MS - 1));
    assert!(!st.linked(NOW + LINK_TIMEOUT_MS));
    assert!(!State::default().linked(NOW));
}

#[test]
fn summary_only_fills_the_one_requested() {
    let mut st = state_with(&[("idle", "A", "esp", "", 0)]);
    st.summary = Some(Summary { id: "cli:0".into(), ..Default::default() });
    st.feed(b"Y\tcli:9\tm\tyo\tclaude\ttool\n", NOW);
    assert!(!st.summary.as_ref().unwrap().loaded);
    let ch = st.feed(b"Y\tcli:0\tEmpezo hace 1 h\thola\tadios\tBash: ls\n", NOW);
    let sum = st.summary.as_ref().unwrap();
    assert!(ch.summary && sum.loaded);
    assert_eq!((sum.you.as_str(), sum.tool.as_str()), ("hola", "Bash: ls"));
}

#[test]
fn limits_line() {
    let mut st = State::default();
    st.feed(b"L\t64\treinicia 16:40\t90\t52\treinicia vie 22:00\t39\tA este ritmo no llegas al limite\tahora\n", NOW);
    let l = &st.limits;
    assert_eq!((l.five, l.five_elapsed, l.week, l.week_elapsed), (64, 90, 52, 39));
    assert_eq!(l.week_reset, "reinicia vie 22:00");
}

#[test]
fn usage_block_is_committed_on_end() {
    let mut st = State::default();
    st.feed(b"C\tB\nC\tH\t4\t3\t1,8 M\ttokens hoy\nC\tS\thomelab\t1,3 M\t0,205,599\n", NOW);
    assert!(st.usage.is_none(), "nothing shown until C E");
    let ch = st.feed(b"C\tS\tesp\t464 k\t1,2,3\nC\tD\tL,M,X\t10,20,30\t2\nC\tE\n", NOW);
    let u = st.usage.as_ref().unwrap();
    assert!(ch.usage);
    assert_eq!((u.start, u.n, u.total.as_str()), (4, 3, "1,8 M"));
    assert_eq!(u.series[0].2, vec![0, 205, 599]);
    assert_eq!(u.days, vec![("L".into(), 10), ("M".into(), 20), ("X".into(), 30)]);
    assert_eq!(u.today, 2);
}

#[test]
fn outgoing_messages() {
    assert_eq!(open_msg("cli:abc"), "O\tcli:abc\n");
    assert_eq!(summary_msg("cli:abc"), "R\tcli:abc\n");
}
