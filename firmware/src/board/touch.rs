//! FT6336 capacitive touch controller (I2C 0x48).

use esp_idf_svc::hal::i2c::I2cDriver;

use super::I2C_TIMEOUT;

const ADDR: u8 = 0x48;
const REG_POINTS: u8 = 0x02; // number of points, then X/Y of the first one

/// Finger position in screen coordinates, while touching.
pub fn read(i2c: &mut I2cDriver<'static>) -> Option<(i32, i32)> {
    let mut r = [0u8; 5];
    i2c.write_read(ADDR, &[REG_POINTS], &mut r, I2C_TIMEOUT).ok()?;
    if r[0] & 0x0F == 0 {
        return None;
    }
    let x = ((i32::from(r[1] & 0x0F) << 8) | i32::from(r[2])).min(479);
    let y = ((i32::from(r[3] & 0x0F) << 8) | i32::from(r[4])).min(479);
    Some((479 - x, 479 - y)) // the touch panel is mounted rotated 180° relative to the display
}
