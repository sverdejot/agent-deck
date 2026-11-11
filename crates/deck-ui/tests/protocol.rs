mod common;

use common::{snapshot, state_with, NOW};
use deck_ui::protocol::{
    answer_msg, open_msg, summary_msg, State, Subagent, Summary, LINK_TIMEOUT_MS, MAX_AGENTS, MAX_OPTIONS,
};

#[test]
fn snapshot_replaces_sessions() {
    let mut st = State::default();
    let ch =
        st.feed(snapshot(&[("busy", "One", "esp", "", 30), ("idle", "Two", "homelab", "3 h ago", -1)]).as_bytes(), NOW);
    assert!(ch.sessions);
    assert_eq!(st.sessions.len(), 2);
    let s = &st.sessions[1];
    assert_eq!((s.state.as_str(), s.title.as_str(), s.place.as_str()), ("idle", "Two", "homelab"));
    assert_eq!((s.age.as_str(), s.ctx), ("3 h ago", -1));
    assert!(st.sessions[0].busy());
}

#[test]
fn identical_snapshot_changes_nothing() {
    let snap = snapshot(&[("ready", "One", "esp", "now", 10)]);
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
fn snapshot_carries_running_subagents() {
    let snap = |agents: u32| {
        format!("B\nS\tcli:0\tcode\tbusy\tOne\tesp\t\t30\t{agents}\nS\tcli:1\tcode\tidle\tTwo\tesp\t1 h ago\t-1\nE\n")
    };
    let mut st = State::default();
    st.feed(snap(3).as_bytes(), NOW);
    assert_eq!(st.sessions[0].agents, 3);
    assert_eq!(st.sessions[1].agents, 0, "missing field means none");
    // a change in the count alone is a change of the snapshot
    assert!(st.feed(snap(1).as_bytes(), NOW).sessions);
    assert_eq!(st.sessions[0].agents, 1);
}

const EXPLORE: &str = "G\tS\tcli:0\tExplore\tBuscar usos de tile_rect\t2 min\tGrep: tile_rect\n";
const TESTS: &str = "G\tS\tcli:0\tgeneral-purpose\tTests\t40 s\n";

#[test]
fn subagent_block_fills_the_open_summary() {
    let mut st = state_with(&[("busy", "A", "esp", "", 0)]);
    st.summary = Some(Summary { id: "cli:0".into(), ..Default::default() });
    // a block for another session is ignored
    let ch = st.feed(b"G\tB\tcli:9\nG\tS\tcli:9\tExplore\tOtra\t1 min\t\nG\tE\tcli:9\n", NOW);
    assert!(!ch.summary && st.summary.as_ref().unwrap().agents.is_empty());
    // nothing changes until G E
    st.feed(format!("G\tB\tcli:0\n{EXPLORE}").as_bytes(), NOW);
    assert!(st.summary.as_ref().unwrap().agents.is_empty());
    let ch = st.feed(format!("{TESTS}G\tE\tcli:0\n").as_bytes(), NOW);
    let agents = &st.summary.as_ref().unwrap().agents;
    assert!(ch.summary);
    assert_eq!(
        agents[0],
        Subagent {
            kind: "Explore".into(),
            description: "Buscar usos de tile_rect".into(),
            age: "2 min".into(),
            now: "Grep: tile_rect".into(),
        }
    );
    assert_eq!((agents[1].kind.as_str(), agents[1].now.as_str()), ("general-purpose", ""), "now is optional");
    // the same block again changes nothing; an empty one clears the list
    let ch = st.feed(format!("G\tB\tcli:0\n{EXPLORE}{TESTS}G\tE\tcli:0\n").as_bytes(), NOW);
    assert!(!ch.summary);
    let ch = st.feed(b"G\tB\tcli:0\nG\tE\tcli:0\n", NOW);
    assert!(ch.summary && st.summary.as_ref().unwrap().agents.is_empty());
}

#[test]
fn subagent_list_is_capped() {
    let mut st = state_with(&[("busy", "A", "esp", "", 0)]);
    st.summary = Some(Summary { id: "cli:0".into(), ..Default::default() });
    let rows = "G\tS\tcli:0\tExplore\tTarea\t1 min\t\n".repeat(20);
    let block = format!("G\tB\tcli:0\n{rows}G\tE\tcli:0\n");
    st.feed(block.as_bytes(), NOW);
    assert_eq!(st.summary.as_ref().unwrap().agents.len(), MAX_AGENTS);
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
    st.feed(b"Y\tcli:9\tm\tme\tclaude\ttool\n", NOW);
    assert!(!st.summary.as_ref().unwrap().loaded);
    let ch = st.feed(b"Y\tcli:0\tStarted 1 h ago\thello\tbye\tBash: ls\n", NOW);
    let sum = st.summary.as_ref().unwrap();
    assert!(ch.summary && sum.loaded);
    assert_eq!((sum.you.as_str(), sum.tool.as_str()), ("hello", "Bash: ls"));
}

#[test]
fn question_only_for_the_open_summary() {
    let mut st = state_with(&[("wait", "A", "esp", "", 0)]);
    let q = "Q\tcli:0\tWhich database?\t1.Postgres\t2.SQLite\t4.None\n";
    st.feed(q.as_bytes(), NOW);
    assert!(st.question.is_none(), "no summary open");

    st.summary = Some(Summary { id: "cli:0".into(), ..Default::default() });
    st.feed(b"Q\tcli:9\tOtra\t1.Si\n", NOW);
    assert!(st.question_for("cli:0").is_none(), "another session");
    let ch = st.feed(q.as_bytes(), NOW);
    assert!(ch.summary);
    let got = st.question_for("cli:0").unwrap();
    assert_eq!(got.text, "Which database?");
    assert_eq!(got.options, vec![(1, "Postgres".into()), (2, "SQLite".into()), (4, "None".into())]);
    assert!(!st.feed(q.as_bytes(), NOW).summary, "same question: nothing to redraw");

    st.feed(b"Q\tcli:0\n", NOW);
    assert!(st.question_for("cli:0").is_none(), "cleared");
}

#[test]
fn question_options_are_checked_and_capped() {
    let mut st = state_with(&[("wait", "A", "esp", "", 0)]);
    st.summary = Some(Summary { id: "cli:0".into(), ..Default::default() });
    st.feed(b"Q\tcli:0\tSolo basura\tsin numero\t0.Cero\t10.Diez\t3.\n", NOW);
    assert!(st.question.is_none(), "no valid option, no question");
    st.feed(b"Q\tcli:0\tMuchas\t1.a\t2.b\t3.c\t4.d\t5.e\t6.f\t7.g\t8.h\n", NOW);
    assert_eq!(st.question.as_ref().unwrap().options.len(), MAX_OPTIONS);
    // a label may itself contain dots: only the first one separates the number
    st.feed(b"Q\tcli:0\tVersion\t1.v1.2.3\n", NOW);
    assert_eq!(st.question.as_ref().unwrap().options, vec![(1, "v1.2.3".into())]);
}

#[test]
fn limits_line() {
    let mut st = State::default();
    st.feed(b"L\t64\tresets 16:40\t90\t52\tresets Fri 22:00\t39\tAt this pace you won't hit the limit\tnow\n", NOW);
    let l = &st.limits;
    assert_eq!((l.five, l.five_elapsed, l.week, l.week_elapsed), (64, 90, 52, 39));
    assert_eq!(l.week_reset, "resets Fri 22:00");
}

#[test]
fn usage_block_is_committed_on_end() {
    let mut st = State::default();
    st.feed(b"C\tB\nC\tH\t4\t3\t1.8 M\ttokens today\nC\tS\thomelab\t1.3 M\t0,205,599\n", NOW);
    assert!(st.usage.is_none(), "nothing shown until C E");
    let ch = st.feed(b"C\tS\tesp\t464 k\t1,2,3\nC\tD\tM,T,W\t10,20,30\t2\nC\tE\n", NOW);
    let u = st.usage.as_ref().unwrap();
    assert!(ch.usage);
    assert_eq!((u.start, u.n, u.total.as_str()), (4, 3, "1.8 M"));
    assert_eq!(u.series[0].2, vec![0, 205, 599]);
    assert_eq!(u.days, vec![("M".into(), 10), ("T".into(), 20), ("W".into(), 30)]);
    assert_eq!(u.today, 2);
}

#[test]
fn outgoing_messages() {
    assert_eq!(open_msg("cli:abc"), "O\tcli:abc\n");
    assert_eq!(summary_msg("cli:abc"), "R\tcli:abc\n");
    assert_eq!(answer_msg("cli:abc", 2, "SQLite"), "A\tcli:abc\t2\tSQLite\n");
}
