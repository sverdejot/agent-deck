//! Loads the custom start screen from the `splash` flash partition (see `partitions.csv` and
//! `docs/splash.md`). Read once at boot, before the panel runs: flash reads stall the cache.

use deck_ui::splash::{file_len, Splash, HEADER_LEN};
use esp_idf_svc::sys;

/// The start screen written from the web flasher, if there is a valid one.
pub fn load() -> Option<Splash> {
    // SAFETY: plain ESP-IDF partition API calls; the buffers outlive them and their sizes are passed.
    unsafe {
        let p = sys::esp_partition_find_first(
            sys::esp_partition_type_t_ESP_PARTITION_TYPE_DATA,
            sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY,
            c"splash".as_ptr(),
        );
        if p.is_null() {
            log::warn!("splash: no partition (flash the firmware with partitions.csv)");
            return None;
        }
        let mut head = [0u8; HEADER_LEN];
        if sys::esp_partition_read(p, 0, head.as_mut_ptr().cast(), HEADER_LEN) != sys::ESP_OK {
            return None;
        }
        let len = file_len(&head)?; // None: nothing written yet (erased flash)
        if len > (*p).size as usize {
            log::warn!("splash: {len} bytes don't fit the partition");
            return None;
        }
        let mut data = vec![0u8; len]; // large: lands in PSRAM
        if sys::esp_partition_read(p, 0, data.as_mut_ptr().cast(), len) != sys::ESP_OK {
            return None;
        }
        match Splash::parse(data) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("splash: {e:?}");
                None
            }
        }
    }
}
