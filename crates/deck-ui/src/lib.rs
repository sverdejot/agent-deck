//! Agent Deck UI: everything the deck shows and how it reacts, independent of the hardware.
//!
//! - [`protocol`]: the serial protocol with the Mac (parsing, outgoing messages) and the state it builds.
//! - [`ui`]: screens, grid layout and gestures.
//! - [`gfx`] / [`text`]: anti-aliased software rendering into RGB565 band buffers, text fitting.
//! - [`splash`]: the custom start screen (image or animation) written from the web flasher.
//!
//! Time is always passed in (`now`, milliseconds from any fixed origin), so the crate runs and is
//! tested on the host; the firmware (`../../firmware`) supplies the clock, the panel and the input.

pub mod gfx;
pub mod protocol;
pub mod splash;
pub mod text;
pub mod ui;
