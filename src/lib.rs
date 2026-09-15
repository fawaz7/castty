//! Configuration support for the Mionix Castor (22d4:1316).
//!
//! The wire protocol is documented in `PROTOCOL.md`; it was derived by capturing
//! the Windows vendor application under Wine. Anything this crate does to the
//! device should be traceable to a line in that document.

pub mod config;
pub mod hardware;
pub mod ui;
