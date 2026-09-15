//! Locating and talking to the Castor's configuration interface.
//!
//! The device exposes two HID interfaces; only interface 1 carries the vendor
//! collection. hidraw numbering is not stable across reboots or replugs, so the
//! node is always resolved through sysfs rather than hardcoded.

use super::profile::Profile;
use super::protocol::*;
use std::fmt;
use std::fs;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub enum Error {
    NotFound,
    Io(io::Error),
    Value(super::profile::ValueError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => write!(
                f,
                "Mionix Castor ({VID:04x}:{PID:04x}) configuration interface not found. \
                 Is it plugged in, and is the udev rule installed?"
            ),
            Error::Io(e) if e.kind() == io::ErrorKind::PermissionDenied => write!(
                f,
                "permission denied opening the device: install the udev rule \
                 granting uaccess on {VID:04x}:{PID:04x}"
            ),
            Error::Io(e) => write!(f, "{e}"),
            Error::Value(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<super::profile::ValueError> for Error {
    fn from(e: super::profile::ValueError) -> Self {
        Error::Value(e)
    }
}

const IOC_WRITE_READ: u64 = 3;

const fn hid_ioc(nr: u8, size: usize) -> u64 {
    (IOC_WRITE_READ << 30) | ((size as u64) << 16) | ((b'H' as u64) << 8) | nr as u64
}

fn hidioc_set_feature(size: usize) -> u64 {
    hid_ioc(0x06, size)
}

fn hidioc_get_feature(size: usize) -> u64 {
    hid_ioc(0x07, size)
}

/// Convert a raw surface-analyzer reading into Mionix's 1-10 score.
///
/// Derived by running the vendor tool across five surfaces and capturing the
/// raw byte behind each displayed score:
///
/// | raw | vendor score |
/// |-----|--------------|
/// |   0 |            0 |
/// |  13 |            2 |
/// |  28 |            5 |
/// |  38 |            7 |
/// |  39 |            7 |
///
/// The vendor UI shows the score multiplied by ten, which is why a raw ~40 is
/// presented as "70" and looked like a discrepancy.
///
/// The top of the range is **unverified**: no surface tested scored above 7, so
/// the slope is fitted from the lower two thirds and extrapolates above that.
pub fn surface_score(raw: u8) -> u8 {
    (f64::from(raw) * 0.18).round().clamp(0.0, 10.0) as u8
}

/// Firmware version and MCU string from the identify command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub firmware: u16,
    pub mcu: String,
}

pub struct Device {
    file: fs::File,
    path: PathBuf,
}

impl Device {
    /// Walk sysfs for the hidraw node backing interface 1 of the Castor.
    pub fn find_node() -> Result<PathBuf, Error> {
        let want = format!("HID_ID=0003:0000{:04X}:0000{:04X}", VID, PID);
        for entry in fs::read_dir("/sys/class/hidraw")? {
            let entry = entry?;
            let uevent = entry.path().join("device/uevent");
            let Ok(text) = fs::read_to_string(&uevent) else { continue };
            // interface 0 is the boot mouse; interface 1 has the vendor collection
            if text.contains(&want) && text.contains("input1") {
                return Ok(Path::new("/dev").join(entry.file_name()));
            }
        }
        Err(Error::NotFound)
    }

    pub fn open() -> Result<Self, Error> {
        let path = Self::find_node()?;
        Self::open_path(&path)
    }

    pub fn open_path(path: &Path) -> Result<Self, Error> {
        let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Device { file, path: path.to_path_buf() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }

    fn set_feature(&self, buf: &[u8]) -> Result<(), Error> {
        let rc = unsafe {
            libc::ioctl(self.fd(), hidioc_set_feature(buf.len()), buf.as_ptr())
        };
        if rc < 0 {
            return Err(Error::Io(io::Error::last_os_error()));
        }
        Ok(())
    }

    fn get_feature(&self, report_id: u8, len: usize) -> Result<Vec<u8>, Error> {
        let mut buf = vec![0u8; len];
        buf[0] = report_id;
        let rc = unsafe {
            libc::ioctl(self.fd(), hidioc_get_feature(len), buf.as_mut_ptr())
        };
        if rc < 0 {
            return Err(Error::Io(io::Error::last_os_error()));
        }
        Ok(buf)
    }

    /// Send a bare command on the 0x60 report.
    fn command(&self, cmd: u8, arg: Option<u8>) -> Result<(), Error> {
        let mut frame = vec![0u8; CMD_FRAME_LEN];
        frame[0] = REPORT_CMD;
        frame[offset::CMD] = cmd;
        if let Some(a) = arg {
            frame[2] = a;
        }
        self.set_feature(&frame)
    }

    /// The 0x60 channel is request/response: a bare GET with no preceding SET
    /// returns zeros, which is why probing the device cold looks dead.
    fn request(&self, cmd: u8, arg: Option<u8>) -> Result<Vec<u8>, Error> {
        self.command(cmd, arg)?;
        thread::sleep(Duration::from_millis(50));
        self.get_feature(REPORT_CMD, CMD_FRAME_LEN)
    }

    pub fn identify(&self) -> Result<Identity, Error> {
        let r = self.request(CMD_IDENTIFY, None)?;
        let firmware = u16::from_le_bytes([r[offset::ID_FIRMWARE], r[offset::ID_FIRMWARE + 1]]);
        let mcu = String::from_utf8_lossy(
            &r[offset::ID_MCU..offset::ID_MCU + offset::ID_MCU_LEN],
        )
        .trim_end_matches('\0')
        .to_string();
        Ok(Identity { firmware, mcu })
    }

    /// Begin a surface measurement.
    ///
    /// The vendor tool asks the user to move the mouse around the surface while
    /// this runs and only then reads the result, so starting and reading are
    /// separate calls -- reading immediately measures nothing.
    pub fn surface_start(&self) -> Result<(), Error> {
        self.command(CMD_SURFACE, Some(SURFACE_START))
    }

    /// Read the result of a measurement started earlier.
    pub fn surface_result(&self) -> Result<u8, Error> {
        let r = self.request(CMD_SURFACE, Some(SURFACE_RESULT))?;
        Ok(r[offset::SURFACE_VALUE])
    }

    /// Write one profile and commit it.
    ///
    /// The vendor app rewrites all five profiles on every apply; writing only
    /// the one that changed is sufficient and has been verified on hardware.
    pub fn write_profile(&self, profile: &Profile) -> Result<(), Error> {
        let blob = profile.encode()?;
        self.set_feature(&blob)?;
        thread::sleep(Duration::from_millis(50));

        let mut term = vec![0u8; PROFILE_FRAME_LEN];
        term[0] = REPORT_PROFILE;
        term[offset::CMD] = CMD_PROFILE_WRITE;
        term[offset::PROFILE_INDEX] = profile.index;
        term[offset::TERMINATOR_FLAG] = 0x01;
        self.set_feature(&term)?;
        thread::sleep(Duration::from_millis(50));

        self.commit(profile.index)
    }

    /// Commit pending writes and make `active_profile` the live one.
    ///
    /// Byte 5 of this frame is the profile the mouse switches to -- this is the
    /// only profile-select mechanism there is.
    pub fn commit(&self, active_profile: u8) -> Result<(), Error> {
        let mut frame = vec![0u8; CMD_FRAME_LEN];
        frame[0] = REPORT_CMD;
        frame[offset::CMD] = CMD_COMMIT;
        frame[COMMIT_ACTIVE_PROFILE] = active_profile;
        self.set_feature(&frame)
    }
}
