//! Decoding and encoding of the 1041-byte profile blob.
//!
//! Only a fraction of the blob is understood. Rather than synthesise a frame
//! from scratch, a `Profile` keeps the bytes it was decoded from and patches
//! just the known fields back in on encode. Unknown regions -- including the
//! presumed macro storage -- therefore survive a read/modify/write untouched,
//! which matters for a device whose firmware we cannot inspect.

use super::protocol::{offset, PROFILE_FRAME_LEN, REPORT_PROFILE, CMD_PROFILE_WRITE};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    BadLength(usize),
    NotAProfileFrame { report: u8, cmd: u8 },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::BadLength(n) => {
                write!(f, "expected {PROFILE_FRAME_LEN} bytes, got {n}")
            }
            DecodeError::NotAProfileFrame { report, cmd } => {
                write!(f, "not a profile frame (report 0x{report:02x} cmd 0x{cmd:02x})")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueError {
    AngleSnapping(u8),
    AngleTuning(i8),
    Dpi(u16),
    LiftOff(u8),
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueError::AngleSnapping(v) => write!(f, "angle snapping {v} out of range 0..=15"),
            ValueError::AngleTuning(v) => write!(f, "angle tuning {v} out of range -30..=30"),
            ValueError::Dpi(v) => write!(f, "DPI {v} out of range {DPI_MIN}..={DPI_MAX}"),
            ValueError::LiftOff(v) => write!(
                f,
                "lift-off {v} out of range {}..={}",
                offset::LIFT_OFF_MIN,
                offset::LIFT_OFF_MAX
            ),
        }
    }
}

impl std::error::Error for ValueError {}

/// Verified in capture across 400..9150. The wider bound is the Castor's rated
/// maximum; values outside the verified range are accepted but untested.
pub const DPI_MIN: u16 = 100;
pub const DPI_MAX: u16 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingRate {
    Hz1000,
    Hz500,
    Hz250,
    Hz125,
}

impl PollingRate {
    /// The stored byte is a divisor of 1000 Hz. Inferred from the order the
    /// vendor GUI stepped through its rate list -- see PROTOCOL.md.
    pub fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            1 => PollingRate::Hz1000,
            2 => PollingRate::Hz500,
            4 => PollingRate::Hz250,
            8 => PollingRate::Hz125,
            _ => return None,
        })
    }

    pub fn to_byte(self) -> u8 {
        match self {
            PollingRate::Hz1000 => 1,
            PollingRate::Hz500 => 2,
            PollingRate::Hz250 => 4,
            PollingRate::Hz125 => 8,
        }
    }

    pub fn hz(self) -> u16 {
        1000 / self.to_byte() as u16
    }
}

/// The base animation. The mode byte's low nibble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Solid,
    Blinking,
    Pulsating,
    Breathing,
}

pub const EFFECTS: [Effect; 4] =
    [Effect::Solid, Effect::Blinking, Effect::Pulsating, Effect::Breathing];

impl Effect {
    fn from_nibble(n: u8) -> Option<Self> {
        Some(match n {
            1 => Effect::Solid,
            2 => Effect::Blinking,
            3 => Effect::Pulsating,
            4 => Effect::Breathing,
            _ => return None,
        })
    }

    fn nibble(self) -> u8 {
        match self {
            Effect::Solid => 1,
            Effect::Blinking => 2,
            Effect::Pulsating => 3,
            Effect::Breathing => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Effect::Solid => "Solid",
            Effect::Blinking => "Blinking",
            Effect::Pulsating => "Pulsating",
            Effect::Breathing => "Breathing",
        }
    }
}

/// The LED mode byte is two fields, not an enum: the low nibble selects the
/// animation and the high nibble turns on colour cycling. `0x14` is therefore
/// "breathing, with rainbow", which is how the two combine in hardware.
///
/// `Unknown` preserves any byte that does not fit, so an unrecognised value
/// survives a read/modify/write instead of being silently rewritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedMode {
    Set { effect: Effect, rainbow: bool },
    Unknown(u8),
}

impl LedMode {
    pub const fn new(effect: Effect, rainbow: bool) -> Self {
        LedMode::Set { effect, rainbow }
    }

    pub fn from_byte(b: u8) -> Self {
        match (b >> 4, Effect::from_nibble(b & 0x0f)) {
            (0, Some(effect)) => LedMode::Set { effect, rainbow: false },
            (1, Some(effect)) => LedMode::Set { effect, rainbow: true },
            _ => LedMode::Unknown(b),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            LedMode::Set { effect, rainbow } => {
                if rainbow {
                    0x10 | effect.nibble()
                } else {
                    effect.nibble()
                }
            }
            LedMode::Unknown(b) => b,
        }
    }

    pub fn effect(self) -> Effect {
        match self {
            LedMode::Set { effect, .. } => effect,
            LedMode::Unknown(_) => Effect::Solid,
        }
    }

    pub fn rainbow(self) -> bool {
        matches!(self, LedMode::Set { rainbow: true, .. })
    }

    pub fn with_effect(self, effect: Effect) -> Self {
        LedMode::Set { effect, rainbow: self.rainbow() }
    }

    pub fn with_rainbow(self, rainbow: bool) -> Self {
        LedMode::Set { effect: self.effect(), rainbow }
    }

    pub fn label(self) -> String {
        match self {
            LedMode::Set { effect, rainbow: false } => effect.label().to_string(),
            LedMode::Set { effect, rainbow: true } => format!("{} + Rainbow", effect.label()),
            LedMode::Unknown(b) => format!("Unknown (0x{b:02x})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Led {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub mode: LedMode,
}

/// Which physical button a table entry drives. The order is fixed by the
/// hardware; it is not the bitmask order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    WheelClick,
    SideFront,
    SideRear,
    Dpi,
}

pub const BUTTONS: [Button; offset::BUTTON_COUNT] = [
    Button::Left,
    Button::Right,
    Button::WheelClick,
    Button::SideFront,
    Button::SideRear,
    Button::Dpi,
];

impl Button {
    pub fn label(self) -> &'static str {
        match self {
            Button::Left => "Left",
            Button::Right => "Right",
            Button::WheelClick => "Wheel click",
            Button::SideFront => "Side, front",
            Button::SideRear => "Side, rear",
            Button::Dpi => "DPI",
        }
    }

    /// The button's own standard action, as the factory profile assigns it.
    pub fn default_action(self) -> ButtonAction {
        match self {
            Button::Left => ButtonAction::Mouse(0x01),
            Button::Right => ButtonAction::Mouse(0x02),
            Button::WheelClick => ButtonAction::Mouse(0x04),
            Button::SideFront => ButtonAction::Mouse(0x10),
            Button::SideRear => ButtonAction::Mouse(0x08),
            Button::Dpi => ButtonAction::DpiSwitch(0xf1),
        }
    }
}

/// What a button does. Stored as `<type> <param>`; `Unknown` preserves anything
/// we cannot interpret so it survives a read/modify/write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    /// Standard mouse button; param is the button bitmask.
    Mouse(u8),
    /// Scroll; param is a signed direction, +1 up and -1 down.
    Scroll(i8),
    /// Single keystroke; param is a HID usage code.
    Key(u8),
    /// Cycle profiles. This is why the mouse can switch profiles unaided.
    ProfileSwitch(u8),
    /// Cycle DPI steps.
    DpiSwitch(u8),
    Disabled,
    Unknown(u8, u8),
}

impl ButtonAction {
    pub fn from_bytes(kind: u8, param: u8) -> Self {
        match kind {
            0x00 => ButtonAction::Mouse(param),
            0x01 => ButtonAction::Scroll(param as i8),
            0x02 => ButtonAction::Key(param),
            0x08 => ButtonAction::ProfileSwitch(param),
            0x09 => ButtonAction::DpiSwitch(param),
            0xff => ButtonAction::Disabled,
            other => ButtonAction::Unknown(other, param),
        }
    }

    pub fn to_bytes(self) -> (u8, u8) {
        match self {
            ButtonAction::Mouse(p) => (0x00, p),
            ButtonAction::Scroll(d) => (0x01, d as u8),
            ButtonAction::Key(k) => (0x02, k),
            ButtonAction::ProfileSwitch(p) => (0x08, p),
            ButtonAction::DpiSwitch(p) => (0x09, p),
            ButtonAction::Disabled => (0xff, 0x00),
            ButtonAction::Unknown(t, p) => (t, p),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiStep {
    pub x: u16,
    pub y: u16,
}

impl DpiStep {
    pub fn linked(dpi: u16) -> Self {
        DpiStep { x: dpi, y: dpi }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    raw: Vec<u8>,
    pub index: u8,
    pub name: String,
    pub polling: Option<PollingRate>,
    pub leds: [Led; offset::LED_COUNT],
    pub dpi: [DpiStep; 3],
    pub dpi_step_count: u8,
    pub active_dpi_step: u8,
    pub angle_snapping: u8,
    pub angle_tuning: i8,
    /// 1-31. The vendor app's "pointer speed" control is deliberately absent:
    /// it writes nothing to the device and is an OS setting, not a mouse one.
    pub lift_off: u8,
    pub buttons: [ButtonAction; offset::BUTTON_COUNT],
}

fn le16(raw: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([raw[at], raw[at + 1]])
}

impl Profile {
    pub fn decode(raw: &[u8]) -> Result<Self, DecodeError> {
        if raw.len() != PROFILE_FRAME_LEN {
            return Err(DecodeError::BadLength(raw.len()));
        }
        if raw[0] != REPORT_PROFILE || raw[offset::CMD] != CMD_PROFILE_WRITE {
            return Err(DecodeError::NotAProfileFrame { report: raw[0], cmd: raw[offset::CMD] });
        }

        let name_bytes = &raw[offset::NAME..offset::NAME + offset::NAME_LEN];
        let name = String::from_utf8_lossy(name_bytes)
            .trim_end_matches('\0')
            .to_string();

        let mut leds = [Led { r: 0, g: 0, b: 0, mode: LedMode::new(Effect::Solid, false) }; offset::LED_COUNT];
        for (i, base) in offset::LED_BASES.iter().enumerate() {
            leds[i] = Led {
                r: raw[*base],
                g: raw[base + 1],
                b: raw[base + 2],
                mode: LedMode::from_byte(raw[base + 3]),
            };
        }

        let mut dpi = [DpiStep { x: 0, y: 0 }; 3];
        for (i, base) in offset::DPI_BASES.iter().enumerate() {
            dpi[i] = DpiStep { x: le16(raw, *base), y: le16(raw, base + 2) };
        }

        Ok(Profile {
            raw: raw.to_vec(),
            index: raw[offset::PROFILE_INDEX],
            name,
            polling: PollingRate::from_byte(raw[offset::POLLING]),
            leds,
            dpi,
            dpi_step_count: raw[offset::DPI_STEP_COUNT],
            active_dpi_step: raw[offset::ACTIVE_DPI_STEP],
            angle_snapping: raw[offset::ANGLE_SNAPPING],
            angle_tuning: raw[offset::ANGLE_TUNING] as i8,
            lift_off: raw[offset::LIFT_OFF],
            buttons: std::array::from_fn(|i| {
                let base = offset::BUTTON_TABLE + i * offset::BUTTON_ENTRY_LEN;
                ButtonAction::from_bytes(raw[base], raw[base + 1])
            }),
        })
    }

    /// Patch the known fields back into the bytes this profile was decoded
    /// from. Unknown regions are carried through untouched.
    pub fn encode(&self) -> Result<Vec<u8>, ValueError> {
        if self.angle_snapping > 15 {
            return Err(ValueError::AngleSnapping(self.angle_snapping));
        }
        if !(-30..=30).contains(&self.angle_tuning) {
            return Err(ValueError::AngleTuning(self.angle_tuning));
        }
        if !(offset::LIFT_OFF_MIN..=offset::LIFT_OFF_MAX).contains(&self.lift_off) {
            return Err(ValueError::LiftOff(self.lift_off));
        }
        for step in &self.dpi {
            for v in [step.x, step.y] {
                if !(DPI_MIN..=DPI_MAX).contains(&v) {
                    return Err(ValueError::Dpi(v));
                }
            }
        }

        let mut out = self.raw.clone();
        out[0] = REPORT_PROFILE;
        out[offset::CMD] = CMD_PROFILE_WRITE;
        out[offset::PROFILE_INDEX] = self.index;
        out[offset::PROFILE_INDEX_DUP] = self.index;

        let name = self.name.as_bytes();
        for i in 0..offset::NAME_LEN {
            out[offset::NAME + i] = name.get(i).copied().unwrap_or(0);
        }

        if let Some(p) = self.polling {
            out[offset::POLLING] = p.to_byte();
        }

        for (i, base) in offset::LED_BASES.iter().enumerate() {
            let led = self.leds[i];
            out[*base] = led.r;
            out[base + 1] = led.g;
            out[base + 2] = led.b;
            out[base + 3] = led.mode.to_byte();
        }

        for (i, base) in offset::DPI_BASES.iter().enumerate() {
            out[*base..*base + 2].copy_from_slice(&self.dpi[i].x.to_le_bytes());
            out[base + 2..base + 4].copy_from_slice(&self.dpi[i].y.to_le_bytes());
        }

        out[offset::DPI_STEP_COUNT] = self.dpi_step_count;
        out[offset::ACTIVE_DPI_STEP] = self.active_dpi_step;
        out[offset::ANGLE_SNAPPING] = self.angle_snapping;
        out[offset::ANGLE_TUNING] = self.angle_tuning as u8;
        out[offset::LIFT_OFF] = self.lift_off;

        for (i, action) in self.buttons.iter().enumerate() {
            let base = offset::BUTTON_TABLE + i * offset::BUTTON_ENTRY_LEN;
            let (kind, param) = action.to_bytes();
            out[base] = kind;
            out[base + 1] = param;
        }
        Ok(out)
    }

    /// The two records that map to physical LEDs.
    pub fn physical_leds(&self) -> &[Led] {
        &self.leds[..offset::LED_PHYSICAL]
    }

    pub fn wheel(&self) -> Led {
        self.leds[offset::LED_WHEEL]
    }

    pub fn logo(&self) -> Led {
        self.leds[offset::LED_LOGO]
    }

    pub fn set_wheel_colour(&mut self, r: u8, g: u8, b: u8) {
        let led = &mut self.leds[offset::LED_WHEEL];
        (led.r, led.g, led.b) = (r, g, b);
    }

    pub fn set_logo_colour(&mut self, r: u8, g: u8, b: u8) {
        let led = &mut self.leds[offset::LED_LOGO];
        (led.r, led.g, led.b) = (r, g, b);
    }

    /// Set every colour record to one colour, which is what the vendor app does.
    pub fn set_all_colours(&mut self, r: u8, g: u8, b: u8) {
        for led in self.leds.iter_mut() {
            led.r = r;
            led.g = g;
            led.b = b;
        }
    }

    /// The effect is global on this device: all six mode bytes move together.
    pub fn set_mode(&mut self, mode: LedMode) {
        for led in self.leds.iter_mut() {
            led.mode = mode;
        }
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
}
