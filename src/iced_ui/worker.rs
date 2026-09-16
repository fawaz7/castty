//! Device I/O on a background thread, bridged to iced's event loop.
//!
//! Same reasoning as the GTK build: feature reports take tens of milliseconds
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
    /// the user has selected.
    WriteProfiles { profiles: Vec<(usize, Profile)>, active: u8 },
    SurfaceStart,
    SurfaceResult,
}

#[derive(Debug, Clone)]
pub enum Update {
    Connected(Identity),
    Disconnected(String),
    /// Indices of the profiles actually written to the device, so the caller
    /// persists exactly those to disk -- not whatever happens to be current
    /// by the time this reply arrives.
    Applied(Vec<usize>),
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

/// Write every profile in turn, then commit whichever one the user actually
/// has selected. Each `write_profile` call ends in its own commit to its own
/// index, so a plain loop over several profiles would leave the mouse
/// switched to the last one written rather than the active one -- the
/// explicit final `commit(active)` is what fixes that.
fn write_profiles(dev: &Device, profiles: &[(usize, Profile)], active: u8) -> Result<Update, Error> {
    // Nothing to write means nothing to commit either. Unreachable today --
    // the UI disables Apply when nothing is dirty -- but that guard lives in
    // the UI; the worker should not depend on it to avoid a bare commit.
    if profiles.is_empty() {
        return Ok(Update::Applied(Vec::new()));
    }
    for (_, profile) in profiles {
        dev.write_profile(profile)?;
    }
    dev.commit(active)?;
    Ok(Update::Applied(profiles.iter().map(|(i, _)| *i).collect()))
}
