//! 480x480 ST7701S panel. Configured once over a bit-banged 3-wire SPI (9-bit words; CS on the
//! IO expander), then fed over the 16-bit RGB bus by ESP-IDF's `esp_lcd` RGB driver from two PSRAM
//! frame buffers: we draw into one while the other is on screen, and swap at vsync.
//!
//! Init sequence and timings from Seeed's `SenseCAP_Indicator_ESP32` BSP (`lcd_panel_config.c`).

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

use anyhow::{bail, Result};
use deck_ui::gfx::{H, W};
use esp_idf_svc::hal::delay::{Ets, FreeRtos};
use esp_idf_svc::hal::gpio::{Output, PinDriver};
use esp_idf_svc::hal::i2c::I2cDriver;
use esp_idf_svc::sys;

use super::expander::{self, Expander};
use super::millis;

const FB_PIXELS: usize = (W * H) as usize;

static VSYNC: AtomicBool = AtomicBool::new(false);

#[link_section = ".iram1.deck_vsync"]
unsafe extern "C" fn on_vsync(
    _: sys::esp_lcd_panel_handle_t,
    _: *const sys::esp_lcd_rgb_panel_event_data_t,
    _: *mut c_void,
) -> bool {
    VSYNC.store(true, Ordering::Release);
    false
}

/// ST7701S commands after reset: (command, parameters).
const INIT: &[(u8, &[u8])] = &[
    (0xFF, &[0x77, 0x01, 0x00, 0x00, 0x10]),
    (0xC0, &[0x3B, 0x00]),
    (0xC1, &[0x0D, 0x02]),
    (0xC2, &[0x31, 0x05]),
    (0xC7, &[0x04]),
    (0xCD, &[0x08]),
    (0xB0, &[0x00, 0x11, 0x18, 0x0E, 0x11, 0x06, 0x07, 0x08, 0x07, 0x22, 0x04, 0x12, 0x0F, 0xAA, 0x31, 0x18]),
    (0xB1, &[0x00, 0x11, 0x19, 0x0E, 0x12, 0x07, 0x08, 0x08, 0x08, 0x22, 0x04, 0x11, 0x11, 0xA9, 0x32, 0x18]),
    (0xFF, &[0x77, 0x01, 0x00, 0x00, 0x11]),
    (0xB0, &[0x60]),
    (0xB1, &[0x32]),
    (0xB2, &[0x07]),
    (0xB3, &[0x80]),
    (0xB5, &[0x49]),
    (0xB7, &[0x85]),
    (0xB8, &[0x21]),
    (0xC1, &[0x78]),
    (0xC2, &[0x78]),
];

/// Second half, sent after a 20 ms pause.
const INIT2: &[(u8, &[u8])] = &[
    (0xE0, &[0x00, 0x1B, 0x02]),
    (0xE1, &[0x08, 0xA0, 0x00, 0x00, 0x07, 0xA0, 0x00, 0x00, 0x00, 0x44, 0x44]),
    (0xE2, &[0x11, 0x11, 0x44, 0x44, 0xED, 0xA0, 0x00, 0x00, 0xEC, 0xA0, 0x00, 0x00]),
    (0xE3, &[0x00, 0x00, 0x11, 0x11]),
    (0xE4, &[0x44, 0x44]),
    (0xE5, &[0x0A, 0xE9, 0xD8, 0xA0, 0x0C, 0xEB, 0xD8, 0xA0, 0x0E, 0xED, 0xD8, 0xA0, 0x10, 0xEF, 0xD8, 0xA0]),
    (0xE6, &[0x00, 0x00, 0x11, 0x11]),
    (0xE7, &[0x44, 0x44]),
    (0xE8, &[0x09, 0xE8, 0xD8, 0xA0, 0x0B, 0xEA, 0xD8, 0xA0, 0x0D, 0xEC, 0xD8, 0xA0, 0x0F, 0xEE, 0xD8, 0xA0]),
    (0xEB, &[0x02, 0x00, 0xE4, 0xE4, 0x88, 0x00, 0x40]),
    (0xEC, &[0x3C, 0x00]),
    (0xED, &[0xAB, 0x89, 0x76, 0x54, 0x02, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x20, 0x45, 0x67, 0x98, 0xBA]),
    (0x36, &[0x10]),
    (0xFF, &[0x77, 0x01, 0x00, 0x00, 0x13]),
    (0xE5, &[0xE4]),
    (0xFF, &[0x77, 0x01, 0x00, 0x00, 0x00]),
    (0x3A, &[0x60]),
    (0x21, &[]), // inversion on
    (0x11, &[]), // sleep out
];

/// The bit-banged 3-wire SPI used only during init.
struct InitBus<'a> {
    i2c: &'a mut I2cDriver<'static>,
    exp: &'a mut Expander,
    sck: PinDriver<'static, Output>,
    mosi: PinDriver<'static, Output>,
}

impl InitBus<'_> {
    fn word(&mut self, w: u16) {
        for i in (0..9).rev() {
            let _ = self.mosi.set_level(((w >> i) & 1 == 1).into());
            Ets::delay_us(2);
            let _ = self.sck.set_high();
            Ets::delay_us(2);
            let _ = self.sck.set_low();
        }
    }

    fn cmd(&mut self, cmd: u8, data: &[u8]) {
        self.exp.set(self.i2c, expander::LCD_CS, false);
        self.word(u16::from(cmd));
        for &d in data {
            self.word(0x100 | u16::from(d));
        }
        self.exp.set(self.i2c, expander::LCD_CS, true);
    }
}

pub struct Display {
    panel: sys::esp_lcd_panel_handle_t,
    fbs: [*mut u16; 2],
    back: usize,
    swap_pending: bool,
}

impl Display {
    pub fn new(
        i2c: &mut I2cDriver<'static>,
        exp: &mut Expander,
        sck: PinDriver<'static, Output>,
        mosi: PinDriver<'static, Output>,
    ) -> Result<Self> {
        init_st7701(InitBus { i2c, exp, sck, mosi });
        Self::rgb_panel()
    }

    fn rgb_panel() -> Result<Self> {
        // SAFETY: plain C config struct; every field we rely on is set below.
        let mut cfg: sys::esp_lcd_rgb_panel_config_t = unsafe { core::mem::zeroed() };
        cfg.clk_src = sys::soc_periph_lcd_clk_src_t_LCD_CLK_SRC_DEFAULT;
        cfg.timings.pclk_hz = 18_000_000;
        cfg.timings.h_res = W as u32;
        cfg.timings.v_res = H as u32;
        cfg.timings.hsync_pulse_width = 8;
        cfg.timings.hsync_back_porch = 50;
        cfg.timings.hsync_front_porch = 10;
        cfg.timings.vsync_pulse_width = 8;
        cfg.timings.vsync_back_porch = 20;
        cfg.timings.vsync_front_porch = 10;
        cfg.data_width = 16;
        cfg.bits_per_pixel = 16;
        cfg.num_fbs = 2;
        cfg.bounce_buffer_size_px = (W * 20) as usize;
        cfg.hsync_gpio_num = 16;
        cfg.vsync_gpio_num = 17;
        cfg.de_gpio_num = 18;
        cfg.pclk_gpio_num = 21;
        cfg.disp_gpio_num = -1;
        // B0..B4, G0..G5, R0..R4
        cfg.data_gpio_nums[..16].copy_from_slice(&[15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0]);
        cfg.flags.set_fb_in_psram(1);

        let mut panel: sys::esp_lcd_panel_handle_t = ptr::null_mut();
        // SAFETY: zeroed callbacks are all "none"; on_vsync is set below.
        let cbs =
            sys::esp_lcd_rgb_panel_event_callbacks_t { on_vsync: Some(on_vsync), ..unsafe { core::mem::zeroed() } };
        let (mut a, mut b): (*mut c_void, *mut c_void) = (ptr::null_mut(), ptr::null_mut());
        // SAFETY: ESP-IDF calls with valid pointers; the panel lives for the whole program.
        unsafe {
            sys::esp!(sys::esp_lcd_new_rgb_panel(&raw const cfg, &raw mut panel))?;
            sys::esp!(sys::esp_lcd_rgb_panel_register_event_callbacks(panel, &raw const cbs, ptr::null_mut()))?;
            sys::esp!(sys::esp_lcd_panel_reset(panel))?;
            sys::esp!(sys::esp_lcd_panel_init(panel))?;
            sys::esp!(sys::esp_lcd_rgb_panel_get_frame_buffer(panel, 2, &raw mut a, &raw mut b))?;
        }
        if a.is_null() || b.is_null() {
            bail!("no frame buffers");
        }
        let fbs = [a.cast::<u16>(), b.cast::<u16>()];
        for fb in fbs {
            // SAFETY: each frame buffer is W*H u16 in PSRAM, owned by the panel driver.
            unsafe { ptr::write_bytes(fb, 0, FB_PIXELS) };
        }
        Ok(Display { panel, fbs, back: 1, swap_pending: false })
    }

    /// The frame to draw next. Waits (≤ 40 ms) until the panel has switched away from it.
    pub fn back_buffer(&mut self) -> *mut u16 {
        if self.swap_pending {
            let t0 = millis();
            while !VSYNC.load(Ordering::Acquire) && millis().wrapping_sub(t0) < 40 {
                FreeRtos::delay_ms(1);
            }
            self.swap_pending = false;
        }
        self.fbs[self.back]
    }

    /// Shows the back buffer (the driver switches to it, no copy); the other one becomes the back.
    pub fn present(&mut self) {
        VSYNC.store(false, Ordering::Release);
        // SAFETY: the pointer is one of the panel's own frame buffers.
        unsafe { sys::esp_lcd_panel_draw_bitmap(self.panel, 0, 0, W, H, self.fbs[self.back] as *const c_void) };
        self.swap_pending = true;
        self.back ^= 1;
    }
}

fn init_st7701(mut bus: InitBus) {
    let _ = bus.sck.set_low();
    let (i2c, exp) = (&mut *bus.i2c, &mut *bus.exp);
    exp.set(i2c, expander::LCD_CS, true);
    exp.output(i2c, expander::LCD_CS);
    exp.set(i2c, expander::LCD_RST, true);
    exp.output(i2c, expander::LCD_RST);
    exp.set(i2c, expander::LCD_RST, false);
    FreeRtos::delay_ms(10);
    exp.set(i2c, expander::LCD_RST, true);
    FreeRtos::delay_ms(120);

    for (c, d) in INIT {
        bus.cmd(*c, d);
    }
    FreeRtos::delay_ms(20);
    for (c, d) in INIT2 {
        bus.cmd(*c, d);
    }
    FreeRtos::delay_ms(120);
    bus.cmd(0x29, &[]); // display on
    FreeRtos::delay_ms(20);
}
