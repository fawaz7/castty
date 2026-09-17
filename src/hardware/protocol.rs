//! Wire-protocol constants. Every value here is documented in `research/PROTOCOL.md`.

pub const VID: u16 = 0x22d4;
pub const PID: u16 = 0x1316;

/// Command/response report: 63 data bytes plus the report ID.
pub const REPORT_CMD: u8 = 0x60;
/// Profile blob report: 1040 data bytes plus the report ID.
pub const REPORT_PROFILE: u8 = 0x61;

pub const CMD_FRAME_LEN: usize = 64;
pub const PROFILE_FRAME_LEN: usize = 1041;

pub const CMD_IDENTIFY: u8 = 0x02;
pub const CMD_STATUS: u8 = 0x03;
pub const CMD_COMMIT: u8 = 0x04;
pub const CMD_SURFACE: u8 = 0x05;
/// Read a profile back out of flash. The index goes in `[5]` -- the same slot
/// the write and commit frames use -- and the reply is read on `REPORT_PROFILE`
/// because a profile does not fit in the 64-byte command report.
pub const CMD_PROFILE_READ: u8 = 0x07;
pub const CMD_PROFILE_WRITE: u8 = 0x08;

/// Byte `[1]` of a read reply: the ack the whole `0x60` channel answers with,
/// where a write frame carries `CMD_PROFILE_WRITE`. Everything from `[16]` on is
/// laid out exactly like a write frame.
///
/// There is deliberately no constant for byte `[0]`. It is not stable in a read
/// reply -- the same device has answered `0x00` in one session and `0x60` in
/// another -- so nothing may test it. See `profile::validate_read_reply`.
pub const RESPONSE_ACK: u8 = 0x01;

/// The commit frame doubles as profile select: byte 5 is the profile the mouse
/// switches to. Sending it as 0 silently moves the user off their profile.
pub const COMMIT_ACTIVE_PROFILE: usize = 5;

pub const SURFACE_START: u8 = 0x01;
pub const SURFACE_RESULT: u8 = 0x02;

/// Byte offsets inside a profile blob.
pub mod offset {
    pub const CMD: usize = 1;
    /// Where a one-byte command argument goes on the `0x60` report -- the
    /// surface analyzer's sub-command, for instance. The profile commands are
    /// the exception: their index sits at `PROFILE_INDEX`.
    pub const CMD_ARG: usize = 2;
    pub const PROFILE_INDEX: usize = 5;
    pub const TERMINATOR_FLAG: usize = 6;
    pub const PROFILE_INDEX_DUP: usize = 16;
    pub const NAME: usize = 17;
    /// Verified 10 from a capture ("testrename"); bounded above by the constant
    /// at [34], so it cannot exceed 17.
    pub const NAME_LEN: usize = 10;

    /// Constant `0x08` in every frame ever captured, read or write. Its purpose
    /// is unknown, which is precisely what makes it useful: nothing the user
    /// can change moves it, so it is a fixed shape a real profile must have.
    pub const CONSTANT: usize = 34;
    pub const CONSTANT_VALUE: u8 = 0x08;

    pub const POLLING: usize = 37;

    /// Six colour records, each `<R> <G> <B> <mode>`. The first two are the
    /// physical LEDs; the remaining four always move together.
    pub const LED_BASES: [usize; 6] = [39, 43, 49, 53, 57, 61];
    pub const LED_COUNT: usize = 6;
    /// Index into `LED_BASES` of the records that can be set independently.
    pub const LED_PHYSICAL: usize = 2;
    /// Which physical LED each of the first two records drives. Established by
    /// lighting one at a time and observing the mouse.
    pub const LED_WHEEL: usize = 0;
    pub const LED_LOGO: usize = 1;

    pub const ACTIVE_DPI_STEP: usize = 66;
    /// Three DPI steps; spacing is irregular because other fields sit between.
    pub const DPI_BASES: [usize; 3] = [68, 77, 98];
    pub const DPI_STEP_COUNT: usize = 102;
    /// The Castor stores exactly three steps and this byte has never held
    /// anything else, in any capture or any read.
    pub const DPI_STEP_COUNT_VALUE: u8 = 0x03;

    /// Lift-off distance, 1-31, matching the vendor slider's 31 steps 1:1.
    pub const LIFT_OFF: usize = 86;
    pub const LIFT_OFF_MIN: u8 = 1;
    pub const LIFT_OFF_MAX: u8 = 31;

    pub const ANGLE_SNAPPING: usize = 88;
    pub const ANGLE_TUNING: usize = 104;

    /// Six 7-byte entries, `<type> <param> 00 00 00 00 0f`, in a fixed order:
    /// left, right, wheel click, side front, side rear, DPI.
    pub const BUTTON_TABLE: usize = 117;
    pub const BUTTON_ENTRY_LEN: usize = 7;
    pub const BUTTON_COUNT: usize = 6;
    pub const BUTTON_TERMINATOR: usize = 6;

    /// Macro event storage. Pointers in button entries are relative to the
    /// profile payload at `[16]`, so an event block lives at `PAYLOAD + ptr`.
    pub const PAYLOAD: usize = 16;
    /// Where the vendor software starts allocating macro data.
    pub const MACRO_BASE_PTR: u16 = 800;
    pub const MACRO_EVENT_LEN: usize = 7;
    /// First byte of macro storage.
    pub const MACRO_BASE: usize = PAYLOAD + MACRO_BASE_PTR as usize;
    /// Slots available for macro data. Each macro costs its event count plus
    /// one for the terminator, and all macros in a profile share this.
    pub const MACRO_SLOTS: usize =
        (super::PROFILE_FRAME_LEN - MACRO_BASE) / MACRO_EVENT_LEN;

    /// Identify response fields (report 0x60, command 0x02).
    pub const ID_FIRMWARE: usize = 0x10;
    pub const ID_MCU: usize = 0x17;
    pub const ID_MCU_LEN: usize = 5;
    /// Surface analyzer result (report 0x60, command 0x05 sub 0x02).
    pub const SURFACE_VALUE: usize = 16;
}
