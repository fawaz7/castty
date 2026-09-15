//! GTK4 / libadwaita front end. One page per file; no device I/O in this layer
//! beyond handing jobs to `worker`.

pub mod buttons_page;
pub mod colour_picker;
pub mod dpi_page;
pub mod led_page;
pub mod macro_editor;
pub mod macros_page;
pub mod mouse_preview;
pub mod profiles_page;
pub mod window;
pub mod worker;

pub use window::run;
