//! Colours (Claude's palette on a near-black background).

use crate::gfx::{rgb, Rgb};
use crate::protocol::MAX_SERIES;

pub const BG: Rgb = rgb(12, 12, 11);
/// Claude's dark grey: tiles that aren't working
pub const GREY: Rgb = rgb(38, 38, 36);
pub const GREY_HI: Rgb = rgb(56, 55, 52);
pub const TEXT: Rgb = rgb(245, 242, 236);
pub const MUTED: Rgb = rgb(160, 156, 148);
pub const FAINT: Rgb = rgb(128, 125, 118);
/// Claude's orange: working
pub const ORANGE: Rgb = rgb(217, 119, 87);
pub const ORANGE_HI: Rgb = rgb(232, 146, 116);
pub const ON_ORANGE: Rgb = rgb(255, 250, 245);
pub const ON_ORANGE_MUTED: Rgb = rgb(255, 226, 212);
pub const ON_ORANGE_FAINT: Rgb = rgb(255, 222, 205);
pub const AMBER: Rgb = rgb(240, 180, 60);
pub const RED: Rgb = rgb(225, 85, 70);
pub const WHITE: Rgb = rgb(255, 255, 255);
/// Usage chart, one per repo
pub const SERIES: [Rgb; MAX_SERIES] = [rgb(217, 119, 87), rgb(242, 184, 158), rgb(150, 146, 138), rgb(120, 72, 54)];
