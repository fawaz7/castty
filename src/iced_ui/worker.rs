//! Device I/O on a background thread, bridged to iced's event loop.
//!
//! The reasoning has not changed with the toolkit: feature reports take tens of milliseconds
//! and the mouse can vanish mid-call, so nothing touches the device from the
//! UI thread. Results arrive as a subscription.

use crate::hardware::{Device, Error, Identity, Profile};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::thread;

#[derive(Debug)]
pub enum Job {
    Connect,
    /// Every profile that needs to reach flash, each tagged with the slot it
    /// belongs to, plus which slot the mouse should end up switched to.
    /// `Device::write_profile` commits to its own index as it writes, so
    /// without an explicit final commit the mouse would end up active on
    /// whichever profile happened to be written last rather than the one
    /// the user has selected. Always sent with at least one profile in it --
    /// selecting a different profile with nothing else dirty goes through
    /// `Switch` instead, since a write batch with nothing in it must not
    /// touch the device at all (see `write_profiles`).
    WriteProfiles { profiles: Vec<(usize, Profile)>, active: u8 },
    /// Just the commit: the user selected a different profile but changed
    /// nothing else, so there is nothing to write, only a live switch. The
    /// commit frame is the device's only profile-select mechanism, so this
    /// is the one way Selecting a profile can ever reach the mouse on its
    /// own.
    Switch { active: u8 },
    SurfaceStart,
    SurfaceResult,
}

#[derive(Debug, Clone)]
pub enum Update {
    Connected(Identity),
    Disconnected(String),
    /// The profiles actually written (by host slot, not `Profile.index` --
    /// that byte is decoded off the wire and not to be trusted as an array
    /// index) and the slot the device ended up committed to. Both are handed
    /// back explicitly rather than read off `current` when this reply
    /// arrives, since `current` may have moved on by then.
    Applied { indices: Vec<usize>, active: usize },
    /// A switch-only Apply went through: nothing was written, but the
    /// device is now committed to this slot.
    Switched(usize),
    /// The analyzer's measurement window has been armed on the mouse. Distinct
    /// from `Applied`: nothing was written to the device or the config file --
    /// the project only writes on an explicit Apply.
    SurfaceStarted,
    Surface(u8),
}

#[derive(Debug, Clone)]
pub struct Handle {
    jobs: mpsc::Sender<Job>,
}

impl Handle {
    pub fn send(&self, job: Job) {
        // The thread lives as long as the process; a failed send is not worth
        // surfacing to the user.
        let _ = self.jobs.send(job);
    }

    /// A handle backed by a plain channel with no worker thread reading it,
    /// so a test can inspect the `Job` `Castty::update` sends without
    /// spawning a thread that tries to open real hardware.
    #[cfg(test)]
    pub(crate) fn for_test(jobs: mpsc::Sender<Job>) -> Self {
        Handle { jobs }
    }
}

/// Updates are broadcast through one process-wide channel so the subscription
/// can be rebuilt (iced may drop and recreate it) without losing the worker.
static UPDATES: OnceLock<async_channel::Receiver<Update>> = OnceLock::new();

pub fn spawn() -> (Handle, async_channel::Receiver<Update>) {
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let (up_tx, up_rx) = async_channel::unbounded::<Update>();

    thread::Builder::new()
        .name("castty-device".into())
        .spawn(move || run(job_rx, up_tx))
        .expect("spawning the device thread");

    let _ = UPDATES.set(up_rx.clone());
    (Handle { jobs: job_tx }, up_rx)
}

pub fn subscription() -> iced::Subscription<Update> {
    iced::Subscription::run(updates)
}

/// A plain function, not a closure: iced identifies a subscription by its
/// builder, so it must be a fn pointer. The receiver comes from the static
/// rather than being captured.
fn updates() -> impl iced::futures::Stream<Item = Update> {
    iced::futures::stream::unfold(UPDATES.get().cloned(), |rx| async move {
        let rx = rx?;
        match rx.recv().await {
            Ok(update) => Some((update, Some(rx))),
            Err(_) => None,
        }
    })
}

fn run(jobs: mpsc::Receiver<Job>, updates: async_channel::Sender<Update>) {
    let mut device: Option<Device> = None;

    while let Ok(job) = jobs.recv() {
        // Reconnect lazily: the mouse can be unplugged at any time.
        if device.is_none() {
            match Device::open() {
                Ok(d) => device = Some(d),
                Err(e) => {
                    let _ = updates.send_blocking(Update::Disconnected(e.to_string()));
                    continue;
                }
            }
        }
        let dev = device.as_ref().expect("device present");

        let result = match job {
            Job::Connect => dev.identify().map(Update::Connected),
            Job::WriteProfiles { profiles, active } => write_profiles(dev, &profiles, active),
            Job::Switch { active } => dev.commit(active).map(|()| Update::Switched(active as usize)),
            Job::SurfaceStart => dev.surface_start().map(|()| Update::SurfaceStarted),
            Job::SurfaceResult => dev.surface_result().map(Update::Surface),
        };

        match result {
            Ok(update) => {
                let _ = updates.send_blocking(update);
            }
            Err(e) => {
                // Drop the handle so the next job reopens it; a failure here is
                // usually the device going away.
                device = None;
                let _ = updates.send_blocking(Update::Disconnected(e.to_string()));
            }
        }
    }
}

/// The device operations `write_profiles` needs, factored out so a test can
/// supply a recording fake instead of talking to real hardware -- there is
/// no mock `Device`; it opens a real hidraw node. `Device` already exposes
/// both methods publicly, so this impl is a thin forwarding block, not new
/// behaviour.
pub trait ProfileSink {
    fn write_profile(&self, profile: &Profile) -> Result<(), Error>;
    fn commit(&self, active: u8) -> Result<(), Error>;
}

impl ProfileSink for Device {
    fn write_profile(&self, profile: &Profile) -> Result<(), Error> {
        Device::write_profile(self, profile)
    }

    fn commit(&self, active: u8) -> Result<(), Error> {
        Device::commit(self, active)
    }
}

/// Write every profile in turn, then commit whichever one the user actually
/// has selected. Each `write_profile` call ends in its own commit to its own
/// index, so a plain loop over several profiles would leave the mouse
/// switched to the last one written rather than the active one -- the
/// explicit final `commit(active)` is what fixes that.
fn write_profiles(
    dev: &impl ProfileSink,
    profiles: &[(usize, Profile)],
    active: u8,
) -> Result<Update, Error> {
    // Nothing to write means nothing to commit either. `WriteProfiles` is
    // only ever sent with something in it -- a selection with nothing else
    // dirty goes through `Job::Switch` instead -- but the guard lives here
    // too, so this function's own contract does not depend on callers
    // getting that right.
    if profiles.is_empty() {
        return Ok(Update::Applied { indices: Vec::new(), active: active as usize });
    }
    for (_, profile) in profiles {
        dev.write_profile(profile)?;
    }
    dev.commit(active)?;
    Ok(Update::Applied {
        indices: profiles.iter().map(|(i, _)| *i).collect(),
        active: active as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, PartialEq)]
    enum Call {
        Write(u8),
        Commit(u8),
    }

    /// Records what it was asked to do, in order, instead of touching real
    /// hardware. `write_profile` is keyed off `Profile.index` here purely as
    /// a convenient, test-chosen tag on each fixture -- not a stand-in for
    /// the host slot, which `write_profiles` never asks this trait for.
    #[derive(Default)]
    struct RecordingSink {
        calls: RefCell<Vec<Call>>,
        /// If set, the write at this position (0-based, among writes only)
        /// fails instead of succeeding, so a test can simulate a mid-batch
        /// disconnect.
        fail_at: Option<usize>,
    }

    impl ProfileSink for RecordingSink {
        fn write_profile(&self, profile: &Profile) -> Result<(), Error> {
            let writes_so_far =
                self.calls.borrow().iter().filter(|c| matches!(c, Call::Write(_))).count();
            if self.fail_at == Some(writes_so_far) {
                return Err(Error::NotFound);
            }
            self.calls.borrow_mut().push(Call::Write(profile.index));
            Ok(())
        }

        fn commit(&self, active: u8) -> Result<(), Error> {
            self.calls.borrow_mut().push(Call::Commit(active));
            Ok(())
        }
    }

    /// A profile fixture tagged with a device-index byte chosen purely to be
    /// recognisable in a test assertion -- deliberately not equal to the
    /// host position it will be paired with, so a test that checked
    /// `Update::Applied`'s indices against this value instead of the real
    /// position would be caught.
    fn tagged(index: u8) -> Profile {
        let mut p = crate::config::factory_default(0);
        p.index = index;
        p
    }

    #[test]
    fn writes_every_profile_in_order_then_commits_once_last() {
        let sink = RecordingSink::default();
        let profiles = vec![(1, tagged(99)), (3, tagged(77))];

        let update = write_profiles(&sink, &profiles, 2).expect("no failure was set up");

        assert_eq!(
            sink.calls.borrow().as_slice(),
            &[Call::Write(99), Call::Write(77), Call::Commit(2)],
            "every write before the one and only commit, in order"
        );
        match update {
            Update::Applied { indices, active } => {
                assert_eq!(indices, vec![1, 3], "indices are host positions, not Profile.index");
                assert_eq!(active, 2);
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_batch_makes_no_calls_at_all_not_even_a_commit() {
        let sink = RecordingSink::default();

        let update = write_profiles(&sink, &[], 3).expect("empty batch cannot fail");

        assert!(sink.calls.borrow().is_empty(), "not even a bare commit");
        match update {
            Update::Applied { indices, active } => {
                assert!(indices.is_empty());
                assert_eq!(active, 3);
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }

    #[test]
    fn a_failed_write_stops_the_sequence_and_never_commits() {
        let sink = RecordingSink { fail_at: Some(1), ..Default::default() };
        let profiles = vec![(0, tagged(10)), (1, tagged(20)), (2, tagged(30))];

        let result = write_profiles(&sink, &profiles, 5);

        assert!(result.is_err(), "the failure must reach the caller");
        assert_eq!(
            sink.calls.borrow().as_slice(),
            &[Call::Write(10)],
            "must stop at the first failure and reach neither the later writes nor the commit"
        );
    }
}
