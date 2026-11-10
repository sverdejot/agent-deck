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
use deck_ui::protocol::{State, Summary};
use deck_ui::ui::Ui;

const NOW: u32 = 10_000;
/// Background and gaps of the overview image (the UI's near-black).
const OVERVIEW_BG: [u8; 3] = [0x0c, 0x0c, 0x0b];
const OVERVIEW_GAP: usize = 24;

type Sample<'a> = (&'a str, &'a str, &'a str, &'a str, i32);

const SESSIONS: [Sample; 12] = [
    ("busy", "Detección de Seeedstudio SenseCap Indicator", "esp", "", 30),
    ("ready", "Migración de Authentik a Pocket ID", "homelab", "ahora", 85),
    ("wait", "Glance como PWA sin icono", "homelab", "hace 10 min", 12),
    ("idle", "Revisión de sesiones pendientes", "repos", "hace 19 h", -1),
    ("idle", "Fallo de DNS en home.lab", "homelab", "hace 11 h", 40),
    ("busy", "Optimización de RAM en cluster", "homelab", "", 64),
    ("ready", "Tests de la paginación del grid", "agent-deck", "hace 2 min", 47),
    ("idle", "Backup de Postgres a S3", "homelab", "hace 2 d", 22),
    ("busy", "Refactor del parser de protocolo", "agent-deck", "", 58),
    ("idle", "Notas de la charla de Rust", "notes", "hace 3 d", -1),
    ("wait", "Permisos de escritura en /srv", "homelab", "hace 1 min", 33),
    ("idle", "Actualizar dotfiles a zsh 5.9", "dotfiles", "hace 6 h", 15),
];

fn state(sessions: &[Sample]) -> State {
    let mut snap = String::from("B\n");
    for (i, (st, title, place, age, ctx)) in sessions.iter().enumerate() {
        writeln!(snap, "S\tcli:{i}\tcode\t{st}\t{title}\t{place}\t{age}\t{ctx}").unwrap();
    }
    snap += "E\n";
    snap += "L\t64\treinicia 16:40\t90\t52\treinicia vie 22:00\t39\tA este ritmo: 100% a las 16:10\tahora\n";
    snap += "C\tB\nC\tH\t32\t36\t1,8 M\ttokens hoy · homelab el que más\n";
    let bars = |k: u32| (0..36).map(|i| ((i * 37 + k * 11) % 90).to_string()).collect::<Vec<_>>().join(",");
    writeln!(snap, "C\tS\thomelab\t1,3 M\t{}\nC\tS\tesp\t464 k\t{}", bars(1), bars(2)).unwrap();
    snap += "C\tD\tM,M,J,V,S,D,L\t259,390,71,2585,2325,1776,1800\t6\nC\tE\n";
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
        let st = state(&SESSIONS[..n]);
        let mut ui = Ui::new();
        ui.layout(&st);
        let img = render(&ui, &st);
        save(&img, &format!("sessions-{n}"));
        if n == 6 {
            six = Some(img);
        }
    }

    // holding a tile: the summary of the session at work
    let mut st = state(&SESSIONS[..4]);
    st.summary = Some(Summary {
        id: "cli:0".into(),
        loaded: true,
        meta: "esp · main · 30% del contexto".into(),
        you: "El panel se queda en negro al arrancar, ¿puedes averiguar por qué?".into(),
        claude: "El reset del panel va por el expansor de E/S y lo estaba soltando antes de configurar el \
                 bus SPI. Muevo la inicialización del expansor al principio y pruebo de nuevo."
            .into(),
        tool: "Editando firmware/src/display.rs".into(),
    });
    let mut ui = Ui::new();
    ui.layout(&st);
    save(&render(&ui, &st), "summary");

    let mut st = state(&SESSIONS[..3]);
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
}
