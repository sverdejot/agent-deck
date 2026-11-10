//! TCA9535 16-bit IO expander (I2C 0x20). Drives the LCD chip-select and reset, the touch reset
//! and the RP2040 reset.

use esp_idf_svc::hal::i2c::I2cDriver;

use super::I2C_TIMEOUT;

const ADDR: u8 = 0x20;
const REG_OUTPUT: u8 = 0x02;
const REG_CONFIG: u8 = 0x06; // bit = 1 → input

pub const LCD_CS: u8 = 4;
pub const LCD_RST: u8 = 5;
pub const TP_RST: u8 = 7;
pub const RP2040_RST: u8 = 8;

pub struct Expander {
    out: u16,
    dir: u16,
}

impl Expander {
    /// All pins inputs, outputs latched high.
    pub fn new(i2c: &mut I2cDriver<'static>) -> Self {
        let e = Expander { out: 0xFFFF, dir: 0xFFFF };
        Self::write(i2c, REG_OUTPUT, e.out);
        Self::write(i2c, REG_CONFIG, e.dir);
        e
    }

    fn write(i2c: &mut I2cDriver<'static>, reg: u8, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        let _ = i2c.write(ADDR, &[reg, lo, hi], I2C_TIMEOUT);
    }

    pub fn set(&mut self, i2c: &mut I2cDriver<'static>, pin: u8, high: bool) {
        if high {
            self.out |= 1 << pin;
        } else {
            self.out &= !(1 << pin);
        }
        Self::write(i2c, REG_OUTPUT, self.out);
    }

    /// Makes `pin` an output (at its current latched level).
    pub fn output(&mut self, i2c: &mut I2cDriver<'static>, pin: u8) {
        self.dir &= !(1 << pin);
        Self::write(i2c, REG_CONFIG, self.dir);
    }
}
