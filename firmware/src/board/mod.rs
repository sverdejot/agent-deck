//! SenseCAP Indicator D1, ESP32-S3 side: display, touch, IO expander, user button, backlight and
//! the UART0 link (CH340) to the Mac.
//!
//! I2C (GPIO 39/40) is shared by the IO expander and the touch controller.

mod display;
mod expander;
mod touch;

use anyhow::Result;
use esp_idf_svc::hal::delay::{FreeRtos, NON_BLOCK};
use esp_idf_svc::hal::gpio::{AnyIOPin, Input, PinDriver, Pull};
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::uart::{config::Config as UartConfig, UartDriver};
use esp_idf_svc::hal::units::Hertz;
use esp_idf_svc::sys;

pub use display::Display;
use expander::Expander;

/// I2C timeout in ticks (1 ms each).
const I2C_TIMEOUT: u32 = 20;

pub struct Board {
    i2c: I2cDriver<'static>,
    button: PinDriver<'static, Input>,
    backlight: LedcDriver<'static>,
    uart: UartDriver<'static>,
    pub display: Display,
}

impl Board {
    pub fn new() -> Result<Self> {
        let p = Peripherals::take()?;
        let pins = p.pins;

        let timer = LedcTimerDriver::new(p.ledc.timer0, &TimerConfig::new().frequency(Hertz(5000)))?;
        let mut backlight = LedcDriver::new(p.ledc.channel0, timer, pins.gpio45)?;
        backlight.set_duty(0)?;

        let button = PinDriver::input(pins.gpio38, Pull::Up)?;
        let mut i2c = I2cDriver::new(p.i2c0, pins.gpio39, pins.gpio40, &I2cConfig::new().baudrate(Hertz(400_000)))?;
        let uart = UartDriver::new(
            p.uart0,
            pins.gpio43,
            pins.gpio44,
            Option::<AnyIOPin>::None,
            Option::<AnyIOPin>::None,
            &UartConfig::default().baudrate(Hertz(115_200)).rx_fifo_size(4096),
        )?;

        let mut exp = Expander::new(&mut i2c);
        exp.set(&mut i2c, expander::RP2040_RST, true); // keep the RP2040 running
        exp.output(&mut i2c, expander::RP2040_RST);
        exp.set(&mut i2c, expander::TP_RST, false);
        exp.output(&mut i2c, expander::TP_RST);
        FreeRtos::delay_ms(5);
        exp.set(&mut i2c, expander::TP_RST, true);

        let display =
            Display::new(&mut i2c, &mut exp, PinDriver::output(pins.gpio41)?, PinDriver::output(pins.gpio48)?)?;

        let mut board = Board { i2c, button, backlight, uart, display };
        board.set_backlight(80);
        Ok(board)
    }

    pub fn set_backlight(&mut self, pct: u32) {
        let max = self.backlight.get_max_duty();
        let _ = self.backlight.set_duty(max * pct.min(100) / 100);
    }

    /// Finger position, in screen coordinates, while touching.
    pub fn touch(&mut self) -> Option<(i32, i32)> {
        touch::read(&mut self.i2c)
    }

    pub fn button(&self) -> bool {
        self.button.is_low()
    }

    /// Whatever has arrived on the serial link, without blocking.
    pub fn read_serial(&mut self, buf: &mut [u8]) -> usize {
        self.uart.read(buf, NON_BLOCK).unwrap_or(0)
    }

    pub fn write_serial(&mut self, s: &str) {
        let _ = self.uart.write(s.as_bytes());
    }
}

/// Milliseconds since boot (wraps after ~49 days; compare with `wrapping_sub`).
pub fn millis() -> u32 {
    // SAFETY: plain ESP-IDF getter.
    (unsafe { sys::esp_timer_get_time() } / 1000) as u32
}
