//! Text fitting: cut with "…" to a width, or word-wrap into lines.

use crate::gfx::Font;

/// `s` cut with "…" so it fits in `max_w` pixels.
pub fn fit(f: &Font, s: &str, max_w: i32) -> String {
    fit_tracked(f, s, max_w, 0)
}

/// Same, with letter spacing.
pub fn fit_tracked(f: &Font, s: &str, max_w: i32, tracking: i32) -> String {
    if f.width(s, tracking) <= max_w {
        return s.to_string();
    }
    let mut out = s.to_string();
    while let Some((i, _)) = out.char_indices().last() {
        out.truncate(i);
        let candidate = format!("{}…", out.trim_end());
        if f.width(&candidate, tracking) <= max_w {
            return candidate;
        }
    }
    String::new()
}

/// Word-wraps `s` into at most `max_lines` lines of `max_w` pixels; the last line ends in "…" if
/// text is left over.
pub fn wrap(f: &Font, s: &str, max_w: i32, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = s.trim_start();
    while !rest.is_empty() && lines.len() < max_lines {
        // longest prefix that fits, preferring to break at a space
        let (mut end, mut last_space) = (0, None);
        let mut width = 0;
        for (i, ch) in rest.char_indices() {
            let adv = f.glyph(ch).map_or(0, |g| g.adv as i32);
            if width + adv > max_w && end > 0 {
                break;
            }
            width += adv;
            if ch == ' ' {
                last_space = Some(i);
            }
            end = i + ch.len_utf8();
        }
        if end == 0 {
            break;
        }
        let more = end < rest.len();
        if more && !rest[end..].starts_with(' ') {
            if let Some(sp) = last_space.filter(|&sp| sp > 0) {
                end = sp; // break at the last space
            }
        }
        if lines.len() == max_lines - 1 && end < rest.len() {
            lines.push(fit(f, rest, max_w)); // last allowed line: cut with …
            return lines;
        }
        lines.push(rest[..end].trim_end().to_string());
        rest = rest[end..].trim_start();
    }
    lines
}
