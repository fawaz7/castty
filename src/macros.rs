//! The named macro library.
//!
//! The device stores only a button's event list -- no names, confirmed by
//! captures where the profile name is the sole text in the blob. So names and
//! the library itself live here, and a button assignment copies the events onto
//! the device.
//!
//! A consequence worth knowing: assigning one macro to two buttons stores its
//! events twice on the device, because each button entry points at its own list.

use crate::hardware::{Macro, MacroEvent};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{env, fs, io};

/// How timing was captured. The vendor editor offers these as two mutually
/// exclusive checkboxes, which is the same thing as three states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Timing {
    /// Events only; every delay is zero.
    #[default]
    None,
    /// Real millisecond deltas between events.
    Delay,
    /// Keys stay down while the button is held; presses only, no timing.
    Hold,
}

impl Timing {
    pub fn label(self) -> &'static str {
        match self {
            Timing::None => "No timing",
            Timing::Delay => "Record delay",
            Timing::Hold => "Record hold",
        }
    }

    pub const ALL: [Timing; 3] = [Timing::None, Timing::Delay, Timing::Hold];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedMacro {
    pub name: String,
    pub timing: Timing,
    pub events: Vec<MacroEvent>,
}

impl NamedMacro {
    /// The form written to the device. Only hold mode changes the button entry;
    /// the difference between "none" and "delay" is simply whether the stored
    /// deltas are zero.
    pub fn to_device(&self) -> Macro {
        Macro {
            events: self.events.clone(),
            hold: self.timing == Timing::Hold,
        }
    }

    /// Storage slots this macro occupies, including its terminator.
    pub fn slots(&self) -> usize {
        self.events.len() + 1
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    pub macros: Vec<NamedMacro>,
}

fn path() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("castty").join("macros.json")
}

impl Library {
    pub fn load() -> Self {
        fs::read_to_string(path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, text)
    }

    pub fn find(&self, name: &str) -> Option<&NamedMacro> {
        self.macros.iter().find(|m| m.name == name)
    }

    /// Insert or replace by name, so editing keeps a macro in place.
    pub fn put(&mut self, macro_: NamedMacro) {
        match self.macros.iter_mut().find(|m| m.name == macro_.name) {
            Some(existing) => *existing = macro_,
            None => self.macros.push(macro_),
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.macros.retain(|m| m.name != name);
    }

    /// A name not already taken, for a new recording.
    pub fn unused_name(&self) -> String {
        (1..)
            .map(|n| format!("Macro {n}"))
            .find(|name| self.find(name).is_none())
            .unwrap_or_else(|| "Macro".to_string())
    }
}
