use deck_ui::gfx::{H, W};
use deck_ui::ui::grid::{page_count, page_size, session_rect, tile_rect, Rect, PER_PAGE};

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[test]
fn layouts_by_count() {
    // (sessions, columns in the first row, rows)
    for (n, cols, rows) in [(1, 1, 1), (2, 1, 2), (3, 1, 3), (4, 2, 2), (5, 3, 2), (6, 3, 2), (7, 3, 3), (9, 3, 3)] {
        let rects: Vec<Rect> = (0..n).map(|i| session_rect(n, i)).collect();
        let first_row = rects.iter().filter(|r| r.y == rects[0].y).count();
        let distinct_rows = {
            let mut ys: Vec<i32> = rects.iter().map(|r| r.y as i32).collect();
            ys.dedup();
            ys.len()
        };
        assert_eq!((first_row, distinct_rows), (cols, rows), "n = {n}");
    }
}

#[test]
fn one_session_fills_the_screen() {
    let r = tile_rect(1, 0, 1);
    assert_eq!(r, Rect { x: 10.0, y: 10.0, w: W as f32 - 20.0, h: H as f32 - 20.0 });
}

#[test]
fn tiles_stay_on_screen_whole_pixels_and_never_overlap() {
    for n in 1..=27 {
        for p in 0..page_count(n) {
            let count = page_size(n, p);
            let rects: Vec<Rect> = (0..count).map(|k| tile_rect(n, k, count)).collect();
            for (i, r) in rects.iter().enumerate() {
                assert!(r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= W as f32 && r.y + r.h <= H as f32, "n={n} {r:?}");
                assert!([r.x, r.y, r.w, r.h].iter().all(|v| v.fract() == 0.0), "not pixel-aligned: {r:?}");
                for o in &rects[i + 1..] {
                    assert!(!overlaps(*r, *o), "n={n}: {r:?} overlaps {o:?}");
                }
            }
        }
    }
}

#[test]
fn short_last_row_stretches() {
    // 5 sessions: 3 on top, the last 2 share the full width
    let top = session_rect(5, 0);
    let bottom = session_rect(5, 3);
    assert!(bottom.w > top.w);
    assert_eq!(bottom.x, top.x);
}

#[test]
fn paging() {
    assert_eq!(page_count(0), 1);
    assert_eq!(page_count(PER_PAGE), 1);
    assert_eq!(page_count(PER_PAGE + 1), 2);
    assert_eq!(page_size(20, 2), 2);
    // with more than one page every page is 3x3 and leaves room for the dots
    let r = tile_rect(10, 8, 9);
    assert!(r.y + r.h <= H as f32 - 22.0);
}
