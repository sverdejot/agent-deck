//! Renders every screen with sample data to PNG files, to review the design without flashing.
//!
//! ```sh
//! cargo run --example snapshot            # writes target/snapshots/*.png
//! ```

use std::fmt::Write as _;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use deck_ui::gfx::{Canvas, H, W};
use deck_ui::protocol::State;
use deck_ui::ui::Ui;

const NOW: u32 = 10_000;

fn state(sessions: &[(&str, &str, &str, &str, i32)]) -> State {
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

fn save(ui: &Ui, st: &State, name: &str, dir: &Path) {
    let mut fb = vec![0u16; (W * H) as usize];
    ui.draw(&mut Canvas::new(&mut fb, 0), st, NOW);
    let rgb: Vec<u8> = fb
        .iter()
        .flat_map(|&p| {
            let (r, g, b) = ((p >> 11) & 0x1F, (p >> 5) & 0x3F, p & 0x1F);
            [(r << 3 | r >> 2) as u8, (g << 2 | g >> 4) as u8, (b << 3 | b >> 2) as u8]
        })
        .collect();
    let file = BufWriter::new(File::create(dir.join(format!("{name}.png"))).expect("create png"));
    let mut enc = png::Encoder::new(file, W as u32, H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.write_header().and_then(|mut w| w.write_image_data(&rgb)).expect("write png");
    println!("{}", dir.join(format!("{name}.png")).display());
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/snapshots");
    std::fs::create_dir_all(&dir).expect("snapshots dir");
    let all = [
        ("busy", "Detección de Seeedstudio SenseCap Indicator", "esp", "", 30),
        ("ready", "Migración de Authentik a Pocket ID", "homelab", "ahora", 85),
        ("wait", "Glance como PWA sin icono", "homelab", "hace 10 min", 12),
        ("idle", "Revisión de sesiones pendientes", "repos", "hace 19 h", -1),
        ("idle", "Fallo de DNS en home.lab", "homelab", "hace 11 h", 40),
        ("busy", "Optimización de RAM en cluster", "homelab", "", 64),
    ];
    for n in [1, 2, 4, 6] {
        let st = state(&all[..n]);
        let mut ui = Ui::new();
        ui.layout(&st);
        save(&ui, &st, &format!("sessions-{n}"), &dir);
    }
    let mut st = state(&all[..3]);
    let mut ui = Ui::new();
    ui.layout(&st);
    ui.next_screen(&mut st, 0);
    save(&ui, &st, "limits", &dir);
    ui.next_screen(&mut st, 0);
    save(&ui, &st, "usage", &dir);
    save(&Ui::new(), &State::default(), "offline", &dir);
}
