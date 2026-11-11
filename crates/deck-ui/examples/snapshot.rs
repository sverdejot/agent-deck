//! Renders every screen with sample data to PNG files, to review the design without flashing.
//!
//! ```sh
//! cargo run --example snapshot                    # writes target/snapshots/*.png
//! cargo run --example snapshot -- ../../docs/images # or into any other directory
//! ```
//!
//! Besides one image per screen it writes `overview.png`, a 2x2 grid of the main screens at half
//! size for the top of the README.

use std::fmt::Write as _;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use deck_ui::gfx::{Canvas, H, W};
use deck_ui::protocol::{State, Subagent, Summary};
use deck_ui::splash::Splash;
use deck_ui::ui::Ui;

const NOW: u32 = 10_000;
/// Background and gaps of the overview image (the UI's near-black).
const OVERVIEW_BG: [u8; 3] = [0x0c, 0x0c, 0x0b];
const OVERVIEW_GAP: usize = 24;

type Sample<'a> = (&'a str, &'a str, &'a str, &'a str, i32);

const SESSIONS: [Sample; 12] = [
    ("busy", "Detect the Seeedstudio SenseCap Indicator", "esp", "", 30),
    ("ready", "Migrate Authentik to Pocket ID", "homelab", "now", 85),
    ("wait", "Glance PWA has no icon", "homelab", "10 min ago", 12),
    ("idle", "Review pending sessions", "repos", "19 h ago", -1),
    ("idle", "DNS failure on home.lab", "homelab", "11 h ago", 40),
    ("busy", "Cut RAM use in the cluster", "homelab", "", 64),
    ("ready", "Tests for grid paging", "agent-deck", "2 min ago", 47),
    ("idle", "Back up Postgres to S3", "homelab", "2 d ago", 22),
    ("busy", "Refactor the protocol parser", "agent-deck", "", 58),
    ("idle", "Notes for the Rust talk", "notes", "3 d ago", -1),
    ("wait", "Write permissions on /srv", "homelab", "1 min ago", 33),
    ("idle", "Update dotfiles to zsh 5.9", "dotfiles", "6 h ago", 15),
];

/// Subagents running per session, for the `*-agents` snapshots: busy tiles with several, a
/// waiting one with one, an idle one with a background subagent, and one with more than 9.
const AGENTS: [u32; 12] = [3, 0, 1, 1, 0, 2, 0, 0, 12, 0, 0, 0];

fn state(sessions: &[Sample], agents: &[u32]) -> State {
    let mut snap = String::from("B\n");
    for (i, (st, title, place, age, ctx)) in sessions.iter().enumerate() {
        let n = agents.get(i).copied().unwrap_or(0);
        writeln!(snap, "S\tcli:{i}\tcode\t{st}\t{title}\t{place}\t{age}\t{ctx}\t{n}").unwrap();
    }
    snap += "E\n";
    snap += "L\t64\tresets 16:40\t90\t52\tresets Fri 22:00\t39\tAt this pace: 100% at 16:10\tnow\n";
    snap += "C\tB\nC\tH\t32\t36\t1.8 M\ttokens today · top: homelab\n";
    let bars = |k: u32| (0..36).map(|i| ((i * 37 + k * 11) % 90).to_string()).collect::<Vec<_>>().join(",");
    writeln!(snap, "C\tS\thomelab\t1.3 M\t{}\nC\tS\tesp\t464 k\t{}", bars(1), bars(2)).unwrap();
    snap += "C\tD\tT,W,T,F,S,S,M\t259,390,71,2585,2325,1776,1800\t6\nC\tE\n";
    let mut s = State::default();
    s.feed(snap.as_bytes(), NOW);
    s
}

/// An RGB888 image.
struct Image {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Image {
    fn filled(w: usize, h: usize, c: [u8; 3]) -> Self {
        Image { w, h, px: c.repeat(w * h) }
    }

    /// Half size, averaging each 2x2 block.
    fn half(&self) -> Image {
        let (w, h) = (self.w / 2, self.h / 2);
        let mut px = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                for ch in 0..3 {
                    let at = |dx: usize, dy: usize| self.px[((2 * y + dy) * self.w + 2 * x + dx) * 3 + ch] as u32;
                    px.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
                }
            }
        }
        Image { w, h, px }
    }

    fn blit(&mut self, src: &Image, x: usize, y: usize) {
        for row in 0..src.h {
            let d = ((y + row) * self.w + x) * 3;
            self.px[d..d + src.w * 3].copy_from_slice(&src.px[row * src.w * 3..(row + 1) * src.w * 3]);
        }
    }

    fn save(&self, path: &Path) {
        let file = BufWriter::new(File::create(path).expect("create png"));
        let mut enc = png::Encoder::new(file, self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().and_then(|mut w| w.write_image_data(&self.px)).expect("write png");
        println!("{}", path.display());
    }
}

fn render(ui: &Ui, st: &State) -> Image {
    let mut fb = vec![0u16; (W * H) as usize];
    ui.draw(&mut Canvas::new(&mut fb, 0), st, NOW);
    let px = fb
        .iter()
        .flat_map(|&p| {
            let (r, g, b) = ((p >> 11) & 0x1F, (p >> 5) & 0x3F, p & 0x1F);
            [(r << 3 | r >> 2) as u8, (g << 2 | g >> 4) as u8, (b << 3 | b >> 2) as u8]
        })
        .collect();
    Image { w: W as usize, h: H as usize, px }
}

/// Main screens on a dark background, two per row, at half size.
fn overview(shots: &[&Image]) -> Image {
    let cell = shots[0].w / 2;
    let rows = shots.len().div_ceil(2);
    let side = |n: usize| n * cell + (n + 1) * OVERVIEW_GAP;
    let mut out = Image::filled(side(2), side(rows), OVERVIEW_BG);
    for (i, shot) in shots.iter().enumerate() {
        let (col, row) = (i % 2, i / 2);
        out.blit(&shot.half(), OVERVIEW_GAP + col * (cell + OVERVIEW_GAP), OVERVIEW_GAP + row * (cell + OVERVIEW_GAP));
    }
    out
}

fn main() {
    let dir = std::env::args_os()
        .nth(1)
        .map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/snapshots"), PathBuf::from);
    std::fs::create_dir_all(&dir).expect("snapshots dir");
    let save = |img: &Image, name: &str| img.save(&dir.join(format!("{name}.png")));

    let mut six = None;
    for n in [1, 2, 4, 6, 12] {
        let st = state(&SESSIONS[..n], &[]);
        let mut ui = Ui::new();
        ui.layout(&st);
        let img = render(&ui, &st);
        save(&img, &format!("sessions-{n}"));
        if n == 6 {
            six = Some(img);
        }
    }
    // the same grids with subagents running (9 = the smallest tiles)
    for n in [1, 4, 6, 9, 12] {
        let st = state(&SESSIONS[..n], &AGENTS);
        let mut ui = Ui::new();
        ui.layout(&st);
        save(&render(&ui, &st), &format!("sessions-{n}-agents"));
    }

    // holding a tile: the summary of the session at work
    let mut st = state(&SESSIONS[..4], &[]);
    st.summary = Some(Summary {
        id: "cli:0".into(),
        loaded: true,
        meta: "esp · main · 30% of context".into(),
        you: "The panel stays black on boot, can you find out why?".into(),
        claude: "The panel reset goes through the I/O expander, and I was releasing it before setting up \
                 the SPI bus. I'll move the expander init to the start and try again."
            .into(),
        tool: "Editing firmware/src/display.rs".into(),
        agents: Vec::new(),
    });
    let mut ui = Ui::new();
    ui.layout(&st);
    save(&render(&ui, &st), "summary");

    // holding a session that waits on Claude's question: the options, ready to tap
    let mut st = state(&SESSIONS[..4], &[]);
    st.summary = Some(Summary { id: "cli:2".into(), loaded: true, ..Default::default() });
    let question = "Q\tcli:2\tWhere should the PWA icons go?\t1.In /static, served by Glance\t\
                    2.In an S3 bucket with its own cache\t3.Generate them at build time\n";
    st.feed(question.as_bytes(), NOW);
    let mut ui = Ui::new();
    ui.layout(&st);
    save(&render(&ui, &st), "summary-question");

    // the same session with three subagents at work
    let mut st = state(&SESSIONS[..4], &AGENTS);
    let agent = |kind: &str, description: &str, age: &str, now: &str| Subagent {
        kind: kind.into(),
        description: description.into(),
        age: age.into(),
        now: now.into(),
    };
    st.summary = Some(Summary {
        id: "cli:0".into(),
        loaded: true,
        meta: "Started 2 h ago · 14 messages · esp · active now".into(),
        you: "Find out why the panel flickers when switching screens and suggest a fix.".into(),
        claude: "Starting three subagents: one looks for where it redraws, one times each band and one \
                 checks the bus init."
            .into(),
        tool: "Agent: Find uses of tile_rect".into(),
        agents: vec![
            agent("Explore", "Find uses of tile_rect", "3 min", "Grep: tile_rect"),
            agent("general-purpose", "Time the drawing of each band", "2 min", "Bash: cargo test --release"),
            agent("Plan", "Check the SPI bus init", "40 s", ""),
        ],
    });
    let mut ui = Ui::new();
    ui.layout(&st);
    save(&render(&ui, &st), "summary-agents");

    let mut st = state(&SESSIONS[..3], &[]);
    let mut ui = Ui::new();
    ui.layout(&st);
    ui.next_screen(&mut st, 0);
    let limits = render(&ui, &st);
    save(&limits, "limits");
    ui.next_screen(&mut st, 0);
    let usage = render(&ui, &st);
    save(&usage, "usage");
    let offline = render(&Ui::new(), &State::default());
    save(&offline, "offline");

    let six = six.expect("6-session snapshot");
    save(&overview(&[&six, &limits, &usage, &offline]), "overview");

    // DECK_SPLASH=file.bin: the start screen with that splash, at the frames listed in
    // DECK_SPLASH_FRAMES (default 0), as splash-<frame>.png
    if let Ok(path) = std::env::var("DECK_SPLASH") {
        let data = std::fs::read(&path).expect("DECK_SPLASH file");
        let wanted = std::env::var("DECK_SPLASH_FRAMES").unwrap_or_else(|_| "0".into());
        for k in wanted.split(',').map(|k| k.trim().parse::<usize>().expect("frame number")) {
            let mut splash = Splash::parse(data.clone()).expect("valid splash");
            splash.tick(0);
            for step in 1..=k {
                splash.tick(step as u32 * 70_000); // longer than any frame: one frame per tick
            }
            let mut ui = Ui::new();
            ui.set_splash(splash);
            save(&render(&ui, &State::default()), &format!("splash-{k}"));
        }
    }
}
