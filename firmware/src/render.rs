//! Banded renderer: each frame is drawn in bands into fast internal RAM, and every finished band
//! is DMA'd to the PSRAM frame buffer while the CPU draws the next one. Much faster than
//! scattered PSRAM writes, and it leaves the PSRAM bandwidth to the panel refresh (no flicker).

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

use anyhow::{bail, Result};
use esp_idf_svc::sys;

use deck_ui::gfx::{Canvas, H, W};

use crate::board::Display;

const BAND_H: i32 = 80;
const BAND_PX: usize = (W * BAND_H) as usize;

// esp_cache.h flags (BIT(n) macros, which bindgen doesn't evaluate)
const CACHE_MSYNC_INVALIDATE: i32 = 1 << 0;
const CACHE_MSYNC_DIR_C2M: i32 = 1 << 2;

static DMA_DONE: AtomicU32 = AtomicU32::new(0);

#[link_section = ".iram1.deck_dma"]
unsafe extern "C" fn on_dma_done(
    _: sys::ext::async_memcpy_handle_t,
    _: *mut sys::ext::async_memcpy_event_t,
    _: *mut c_void,
) -> bool {
    DMA_DONE.fetch_add(1, Ordering::AcqRel);
    false
}

pub struct Renderer {
    bands: [&'static mut [u16]; 2],
    dma: sys::ext::async_memcpy_handle_t,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let alloc = || -> Result<&'static mut [u16]> {
            // SAFETY: 64-byte aligned, DMA-capable internal RAM, never freed.
            let p = unsafe {
                sys::heap_caps_aligned_alloc(64, BAND_PX * 2, sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_DMA)
            }
            .cast::<u16>();
            if p.is_null() {
                bail!("no internal RAM for the band buffers");
            }
            Ok(unsafe { core::slice::from_raw_parts_mut(p, BAND_PX) })
        };
        let bands = [alloc()?, alloc()?];
        let mut cfg: sys::ext::async_memcpy_config_t = unsafe { core::mem::zeroed() };
        cfg.backlog = 4;
        cfg.__bindgen_anon_1.dma_burst_size = 32; // the panel frame buffers are 32-byte aligned
        let mut dma = ptr::null_mut();
        // SAFETY: valid config and out pointer.
        unsafe { sys::esp!(sys::ext::esp_async_memcpy_install(&raw const cfg, &raw mut dma))? };
        Ok(Renderer { bands, dma })
    }

    /// Draws one frame with `draw` (called once per band) and shows it.
    /// Returns (total, drawing) time in µs.
    pub fn frame(&mut self, display: &mut Display, mut draw: impl FnMut(&mut Canvas)) -> (u32, u32) {
        let mut drawing = 0i64;
        let t0 = unsafe { sys::esp_timer_get_time() };
        let mut dst: *mut u16 = ptr::null_mut();
        let base = DMA_DONE.load(Ordering::Acquire);
        let mut started = 0u32;
        for (k, y) in (0..H).step_by(BAND_H as usize).enumerate() {
            // a band buffer is free again once the copy started two bands ago has finished
            while started >= 2 && DMA_DONE.load(Ordering::Acquire).wrapping_sub(base) < started - 1 {}
            let band = &mut *self.bands[k & 1];
            let rows = BAND_H.min(H - y);
            let n = (rows * W) as usize;
            {
                let t = unsafe { sys::esp_timer_get_time() };
                let mut c = Canvas::new(&mut band[..n], y);
                draw(&mut c);
                drawing += unsafe { sys::esp_timer_get_time() } - t;
            }
            if dst.is_null() {
                dst = display.back_buffer(); // waits for the previous swap, if needed
            }
            // SAFETY: dst is a W*H frame buffer; the band is DMA-capable internal RAM. The cache
            // lines of the destination are written back and dropped first, so nothing stale can be
            // flushed over the DMA'd data later.
            unsafe {
                let d = dst.add((y * W) as usize);
                sys::ext::esp_cache_msync(d.cast::<c_void>(), n * 2, CACHE_MSYNC_DIR_C2M | CACHE_MSYNC_INVALIDATE);
                sys::esp!(sys::ext::esp_async_memcpy(
                    self.dma,
                    d.cast::<c_void>(),
                    band.as_mut_ptr().cast::<c_void>(),
                    n * 2,
                    Some(on_dma_done),
                    ptr::null_mut()
                ))
                .expect("async memcpy");
            }
            started += 1;
        }
        while DMA_DONE.load(Ordering::Acquire).wrapping_sub(base) < started {}
        display.present();
        ((unsafe { sys::esp_timer_get_time() } - t0) as u32, drawing as u32)
    }
}
