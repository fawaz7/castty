//! Device I/O on a background thread, bridged to iced's event loop.
//!
//! Same reasoning as the GTK build: feature reports take tens of milliseconds
//! and the mouse can vanish mid-call, so nothing touches the device from the
//! UI thread. Results arrive as a subscription.

use crate::hardware::{Device, Identity, Profile};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::thread;

#[derive(Debug)]
pub enum Job {
    Connect,
    WriteProfile(Box<Profile>),
    SurfaceStart,
    SurfaceResult,
}

#[derive(Debug, Clone)]
pub enum Update {
    Connected(Identity),
    Disconnected(String),
    Applied,
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
            Job::WriteProfile(p) => dev.write_profile(&p).map(|()| Update::Applied),
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
