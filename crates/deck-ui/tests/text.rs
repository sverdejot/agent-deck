use deck_ui::gfx::fonts;
use deck_ui::text::{fit, wrap};

#[test]
fn fit_keeps_short_text() {
    assert_eq!(fit(&fonts::SUB, "esp", 200), "esp");
}

#[test]
fn fit_cuts_with_ellipsis_within_width() {
    let s = fit(&fonts::SUB, "Migración de Authentik a Pocket ID", 120);
    assert!(s.ends_with('…'), "{s}");
    assert!(fonts::SUB.width(&s, 0) <= 120);
}

#[test]
fn wrap_breaks_at_spaces_and_respects_width() {
    let lines = wrap(&fonts::TITLE, "Detección de Seeedstudio SenseCap Indicator", 200, 4);
    assert!(lines.len() > 1);
    for l in &lines {
        assert!(fonts::TITLE.width(l, 0) <= 200, "{l}");
        assert!(!l.starts_with(' ') && !l.ends_with(' '), "{l:?}");
    }
    assert_eq!(lines.join(" "), "Detección de Seeedstudio SenseCap Indicator");
}

#[test]
fn wrap_ellipsis_on_last_allowed_line() {
    let lines = wrap(&fonts::TITLE, "una dos tres cuatro cinco seis siete ocho nueve diez", 120, 2);
    assert_eq!(lines.len(), 2);
    assert!(lines[1].ends_with('…'));
}

#[test]
fn wrap_splits_words_longer_than_a_line() {
    let lines = wrap(&fonts::SUB, "supercalifragilisticoexpialidoso", 60, 10);
    assert!(lines.len() > 1);
    assert!(lines.iter().all(|l| fonts::SUB.width(l, 0) <= 60));
}

#[test]
fn spanish_and_typographic_characters_have_glyphs() {
    for c in "áéíóúñÁÉÍÓÚÑü¿¡·…—“”".chars() {
        assert!(fonts::SUB.glyph(c).is_some(), "missing {c}");
    }
    for c in "SESIÓN · ÚLTIMOS DÍAS".chars() {
        assert!(fonts::TAG.glyph(c).is_some(), "missing {c} in TAG");
    }
}
