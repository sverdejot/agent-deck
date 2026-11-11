mod common;

use common::{pixel, render, NOW};
use deck_ui::gfx::{H, W};
use deck_ui::protocol::State;
use deck_ui::splash::{crc32, file_len, Error, Splash, HEADER_LEN, MIN_DELAY_MS};

/// Hint centre the test files ask for (byte 5 holds half of it).
const TEXT_Y: i32 = 300;
use deck_ui::ui::theme::ORANGE;
use deck_ui::ui::Ui;

/// One frame: delay, rectangle (x, y, w, h) and its palette indices, row by row.
type Frame = (u16, [u16; 4], Vec<u8>);

/// Encodes a packet header: kind (0 literal, 1 repeat, 2 skip) and length.
fn packet(out: &mut Vec<u8>, kind: u8, n: usize) {
    if n < 64 {
        out.push(kind << 6 | n as u8);
    } else {
        out.push(kind << 6);
        out.extend_from_slice(&(n as u16).to_le_bytes());
    }
}

/// Run-length encodes indices the way docs/splash.md says (literals and repeats; the web flasher
/// also skips unchanged pixels, tested separately).
fn rle(px: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < px.len() {
        let run = px[i..].iter().take(65535).take_while(|&&p| p == px[i]).count();
        if run >= 3 {
            packet(&mut out, 1, run);
            out.push(px[i]);
            i += run;
        } else {
            let mut j = i;
            while j < px.len() && j - i < 65535 && !(j + 2 < px.len() && px[j] == px[j + 1] && px[j] == px[j + 2]) {
                j += 1;
            }
            packet(&mut out, 0, j - i);
            out.extend_from_slice(&px[i..j]);
            i = j;
        }
    }
    out
}

/// A file whose frames are given as raw packet data instead of pixels.
fn build_raw(palette: &[u16], frames: &[(u16, [u16; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    for c in palette {
        body.extend_from_slice(&c.to_le_bytes());
    }
    for (delay, rect, data) in frames {
        body.extend_from_slice(&delay.to_le_bytes());
        for v in rect {
            body.extend_from_slice(&v.to_le_bytes());
        }
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
    }
    wrap(frames.len(), palette.len(), &body)
}

fn wrap(n_frames: usize, n_colors: usize, body: &[u8]) -> Vec<u8> {
    let mut f = b"DKSP".to_vec();
    f.extend_from_slice(&[1, (TEXT_Y / 2) as u8]);
    for v in [W as u16, H as u16, n_frames as u16, n_colors as u16, BACK] {
        f.extend_from_slice(&v.to_le_bytes());
    }
    f.extend_from_slice(&(body.len() as u32).to_le_bytes());
    f.extend_from_slice(&crc32(body).to_le_bytes());
    f.extend_from_slice(body);
    f
}

fn build(palette: &[u16], frames: &[Frame]) -> Vec<u8> {
    let mut body = Vec::new();
    for c in palette {
        body.extend_from_slice(&c.to_le_bytes());
    }
    for (delay, rect, px) in frames {
        body.extend_from_slice(&delay.to_le_bytes());
        for v in rect {
            body.extend_from_slice(&v.to_le_bytes());
        }
        let data = rle(px);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(&data);
    }
    wrap(frames.len(), palette.len(), &body)
}

const FULL: [u16; 4] = [0, 0, W as u16, H as u16];
/// Background around the picture (RGB565).
const BACK: u16 = 0x2124;
const RED: u16 = 0xF800;
const BLUE: u16 = 0x001F;

/// Red screen, then a 20×10 blue box at (100, 50) for a while, then red again.
fn blinking() -> Vec<u8> {
    let all_red = vec![0u8; (W * H) as usize];
    build(
        &[RED, BLUE],
        &[(500, FULL, all_red), (100, [100, 50, 20, 10], vec![1; 200]), (300, [100, 50, 20, 10], vec![0; 200])],
    )
}

#[test]
fn crc32_matches_the_standard() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn file_len_from_the_header() {
    let f = blinking();
    assert_eq!(file_len(&f[..HEADER_LEN]), Some(f.len()));
    assert_eq!(file_len(&[0xFF; HEADER_LEN]), None);
}

#[test]
fn erased_flash_is_not_a_splash() {
    assert_eq!(Splash::parse(vec![0xFF; 4096]).err(), Some(Error::Magic));
}

#[test]
fn trailing_bytes_of_the_partition_are_ignored() {
    let mut f = blinking();
    f.extend_from_slice(&[0xFF; 1000]);
    assert!(Splash::parse(f).is_ok());
}

#[test]
fn corrupt_files_are_rejected() {
    let f = blinking();
    let mut flipped = f.clone();
    *flipped.last_mut().unwrap() ^= 1;
    assert_eq!(Splash::parse(flipped).err(), Some(Error::Crc));
    assert_eq!(Splash::parse(f[..f.len() - 5].to_vec()).err(), Some(Error::Truncated));
    let mut v2 = f.clone();
    v2[4] = 2;
    assert_eq!(Splash::parse(v2).err(), Some(Error::Version(2)));
}

#[test]
fn bad_frames_are_rejected() {
    let red = vec![0u8; (W * H) as usize];
    // first frame must cover the screen
    let f = build(&[RED], &[(100, [0, 0, 10, 10], vec![0; 100])]);
    assert_eq!(Splash::parse(f).err(), Some(Error::Frame(0)));
    // rectangle off screen
    let f = build(&[RED], &[(100, FULL, red.clone()), (100, [475, 0, 10, 1], vec![0; 10])]);
    assert_eq!(Splash::parse(f).err(), Some(Error::Frame(1)));
    // index past the palette
    let f = build(&[RED], &[(100, FULL, red.clone()), (100, [0, 0, 2, 1], vec![0, 1])]);
    assert_eq!(Splash::parse(f).err(), Some(Error::Frame(1)));
    // fewer pixels than the rectangle
    let f = build(&[RED], &[(100, FULL, red), (100, [0, 0, 4, 4], vec![0; 15])]);
    assert_eq!(Splash::parse(f).err(), Some(Error::Frame(1)));
}

#[test]
fn plays_frames_on_time_and_loops() {
    let mut s = Splash::parse(blinking()).unwrap();
    assert_eq!(s.frame_count(), 3);
    let t0 = 1000;
    assert!(!s.tick(t0), "first tick only starts the clock");
    assert!(!s.tick(t0 + 499));
    assert!(s.tick(t0 + 500), "box appears");
    assert!(!s.tick(t0 + 599));
    assert!(s.tick(t0 + 600), "box goes");
    assert!(s.tick(t0 + 900), "back to frame 0");
}

#[test]
fn still_image_never_ticks() {
    let s = build(&[RED], &[(0, FULL, vec![0; (W * H) as usize])]);
    let mut s = Splash::parse(s).unwrap();
    assert!(!s.tick(0) && !s.tick(1_000_000));
}

#[test]
fn short_delays_are_stretched() {
    let red = vec![0u8; (W * H) as usize];
    let mut s = Splash::parse(build(&[RED], &[(0, FULL, red.clone()), (0, FULL, red)])).unwrap();
    s.tick(0);
    assert!(!s.tick(MIN_DELAY_MS - 1));
    assert!(s.tick(MIN_DELAY_MS));
}

#[test]
fn offline_screen_shows_the_splash_with_the_hint_where_the_file_says() {
    let st = State::default(); // never linked
    let mut ui = Ui::new();
    let frame = render(&ui, &st, NOW);
    assert_eq!(pixel(&frame, 5, 5), ORANGE.pack(), "default start screen");

    ui.set_splash(Splash::parse(blinking()).unwrap());
    let frame = render(&ui, &st, NOW);
    assert_eq!(pixel(&frame, 5, 5), RED, "picture at the top");
    assert_eq!(pixel(&frame, 5, H - 1), RED, "the picture fills the screen");
    let ink = |y0: i32, y1: i32| {
        (0..W).flat_map(|x| (y0..y1).map(move |y| (x, y))).filter(|&(x, y)| pixel(&frame, x, y) != RED).count()
    };
    assert!(ink(TEXT_Y - 30, TEXT_Y + 30) > 500, "the hint where the file says");
    assert_eq!(ink(TEXT_Y + 40, H), 0, "and nowhere else");

    // the box shows up once its frame is due
    let mut st = st;
    ui.tick(&mut st, 0);
    assert!(ui.tick(&mut st, 500));
    assert_eq!(pixel(&render(&ui, &st, NOW), 105, 55), BLUE);
}

#[test]
fn skipped_pixels_keep_the_previous_frame() {
    let px = (W * H) as usize;
    // frame 1, a 3×1 box at (10, 0): skip 1, one literal blue pixel, skip 1
    let f1 = vec![0x81, 0x01, 1, 0x81];
    let file = build_raw(&[RED, BLUE], &[(100, FULL, rle(&vec![0; px])), (100, [10, 0, 3, 1], f1)]);
    let (mut ui, mut st) = (Ui::new(), State::default());
    ui.set_splash(Splash::parse(file).unwrap());
    ui.tick(&mut st, 0);
    assert!(ui.tick(&mut st, 100));
    let frame = render(&ui, &st, NOW);
    assert_eq!([pixel(&frame, 10, 0), pixel(&frame, 11, 0), pixel(&frame, 12, 0)], [RED, BLUE, RED]);

    // frame 0 must stand on its own: no skips
    let mut f0 = vec![0x81];
    f0.extend(rle(&vec![0; px - 1]));
    assert_eq!(Splash::parse(build_raw(&[RED], &[(100, FULL, f0)])).err(), Some(Error::Frame(0)));
}

#[test]
fn long_runs_take_a_u16_length() {
    let px = (W * H) as usize;
    let mut f0 = Vec::new();
    for _ in 0..px / 57_600 {
        packet(&mut f0, 1, 57_600);
        f0.push(0);
    }
    assert_eq!(f0.len(), 4 * 4, "four 3-byte headers + index");
    assert!(Splash::parse(build_raw(&[RED], &[(100, FULL, f0)])).is_ok());
}
