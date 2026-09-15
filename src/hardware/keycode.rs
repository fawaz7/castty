//! Mapping between HID keyboard usage codes and something a person can read.
//!
//! The device stores a standard HID usage code for single-key assignments --
//! confirmed by a capture where assigning `w` wrote `0x1a`, exactly what the
//! HID usage table specifies. So the standard table applies throughout.

/// Non-alphanumeric keys, as (usage, label). Letters and digits are computed.
const NAMED: &[(u8, &str)] = &[
    (0x28, "Enter"),
    (0x29, "Escape"),
    (0x2a, "Backspace"),
    (0x2b, "Tab"),
    (0x2c, "Space"),
    (0x2d, "-"),
    (0x2e, "="),
    (0x2f, "["),
    (0x30, "]"),
    (0x31, "\\"),
    (0x33, ";"),
    (0x34, "'"),
    (0x35, "`"),
    (0x36, ","),
    (0x37, "."),
    (0x38, "/"),
    (0x39, "Caps Lock"),
    (0x46, "Print Screen"),
    (0x47, "Scroll Lock"),
    (0x48, "Pause"),
    (0x49, "Insert"),
    (0x4a, "Home"),
    (0x4b, "Page Up"),
    (0x4c, "Delete"),
    (0x4d, "End"),
    (0x4e, "Page Down"),
    (0x4f, "Right"),
    (0x50, "Left"),
    (0x51, "Down"),
    (0x52, "Up"),
];

/// Human-readable name for a HID usage code.
pub fn label(usage: u8) -> String {
    match usage {
        0x04..=0x1d => ((b'A' + (usage - 0x04)) as char).to_string(),
        0x1e..=0x26 => ((b'1' + (usage - 0x1e)) as char).to_string(),
        0x27 => "0".to_string(),
        0x3a..=0x45 => format!("F{}", usage - 0x39),
        other => NAMED
            .iter()
            .find(|(u, _)| *u == other)
            .map(|(_, name)| (*name).to_string())
            .unwrap_or_else(|| format!("HID 0x{other:02x}")),
    }
}

/// Translate a GDK key value into a HID usage code.
///
/// Returns `None` for keys the device has no code for, so the caller can ask
/// for a different key rather than storing something meaningless.
pub fn from_keyval(keyval: u32) -> Option<u8> {
    // Letters: accept either case, the device stores the key not the character.
    if let Some(c) = char::from_u32(keyval) {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_lowercase() {
            return Some(0x04 + (lower as u8 - b'a'));
        }
        if c.is_ascii_digit() {
            return Some(if c == '0' { 0x27 } else { 0x1e + (c as u8 - b'1') });
        }
        if let Some((usage, _)) = NAMED
            .iter()
            .find(|(_, name)| name.len() == 1 && name.as_bytes()[0] == c as u8)
        {
            return Some(*usage);
        }
    }
    // GDK keysyms for the non-printing keys.
    Some(match keyval {
        0xff0d => 0x28, // Return
        0xff1b => 0x29, // Escape
        0xff08 => 0x2a, // BackSpace
        0xff09 => 0x2b, // Tab
        0x0020 => 0x2c, // space
        0xffe5 => 0x39, // Caps_Lock
        0xff61 => 0x46, // Print
        0xff14 => 0x47, // Scroll_Lock
        0xff13 => 0x48, // Pause
        0xff63 => 0x49, // Insert
        0xff50 => 0x4a, // Home
        0xff55 => 0x4b, // Page_Up
        0xffff => 0x4c, // Delete
        0xff57 => 0x4d, // End
        0xff56 => 0x4e, // Page_Down
        0xff53 => 0x4f, // Right
        0xff51 => 0x50, // Left
        0xff54 => 0x51, // Down
        0xff52 => 0x52, // Up
        0xffbe..=0xffc9 => 0x3a + (keyval - 0xffbe) as u8, // F1-F12
        _ => return None,
    })
}
