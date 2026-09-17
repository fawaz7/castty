//! The local fallback copy of profile state.
//!
//! This module used to be the only place the configuration existed, because the
//! project believed the device could not be queried. It can: `0x60`/`0x07`
//! returns any profile out of flash byte-exactly (see research/PROTOCOL.md), and
//! both the CLI and the GUI now read the mouse and show what it actually holds.
//!
//! What stays here is the fallback, and it earns its keep. A read has to be
//! validated before it can be trusted -- for a few seconds after the mouse is
//! plugged in, a read returns 1041 zero bytes and reports success -- so a read
//! can legitimately be refused, and the app has to have something to show with
//! no mouse attached at all. That is this: the last state written to flash,
//! seeded from the factory blobs the vendor app itself writes on a reset.
//!
//! Both front ends go through this module so they cannot drift apart.

use crate::hardware::Profile;
use std::path::PathBuf;
use std::{env, fs, io};

/// The mouse stores five profiles; the commit frame selects which is live.
pub const PROFILE_COUNT: usize = 5;

/// Starting points when no local state exists, captured from the vendor app's
/// own "reset to default" writing all five profiles.
const FACTORY: [&[u8]; PROFILE_COUNT] = [
    include_bytes!("../research/captures/factory-default-p0.bin"),
    include_bytes!("../research/captures/factory-default-p1.bin"),
    include_bytes!("../research/captures/factory-default-p2.bin"),
    include_bytes!("../research/captures/factory-default-p3.bin"),
    include_bytes!("../research/captures/factory-default-p4.bin"),
];

fn config_dir() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("castty")
}

pub fn state_path(index: usize) -> PathBuf {
    config_dir().join(format!("profile{index}.bin"))
}

pub fn factory_default(index: usize) -> Profile {
    let raw = FACTORY[index.min(PROFILE_COUNT - 1)];
    // The fixtures are captured from hardware and covered by tests; a failure
    // here is a build-time mistake, not a runtime condition.
    Profile::decode(raw).expect("embedded factory default must decode")
}

pub fn load(index: usize) -> Profile {
    match fs::read(state_path(index)) {
        Ok(raw) => Profile::decode(&raw).unwrap_or_else(|_| factory_default(index)),
        Err(_) => factory_default(index),
    }
}

pub fn load_all() -> Vec<Profile> {
    (0..PROFILE_COUNT).map(load).collect()
}

pub fn save(index: usize, p: &Profile) -> io::Result<()> {
    let path = state_path(index);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let bytes = p
        .encode()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    fs::write(&path, bytes)
}
