//! Everything that touches the device. No UI code belongs below this module.

pub mod device;
pub mod keycode;
pub mod profile;
pub mod protocol;

pub use device::{surface_score, Device, Error, Identity};
pub use profile::{
    Button, ButtonAction, DpiStep, Effect, Led, LedMode, MacroEvent, PollingRate, Profile,
    BUTTONS, EFFECTS,
};
