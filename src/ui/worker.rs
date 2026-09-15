//! Device I/O on a background thread.
//!
//! Every hardware call happens here. The GTK main loop must never block on the
//! device: feature reports take tens of milliseconds and the mouse can vanish
//! mid-call.

use crate::hardware::{Device, Identity, Profile};
use std::sync::mpsc;
use std::thread;

pub enum Job {
    Connect,
    WriteProfile(Box<Profile>),
    SurfaceStart,
    SurfaceResult,
}

#[derive(Debug)]
pub enum Update {
    Connected(Identity),
    Disconnected(String),
    Applied,
    SurfaceStarted,
    Surface(u8),
}

#[derive(Clone)]
pub struct Worker {
    jobs: mpsc::Sender<Job>,
}

impl Worker {
    /// Spawn the device thread. Updates arrive on the returned receiver, which
    /// is consumed from the GTK main context.
    pub fn spawn() -> (Self, async_channel::Receiver<Update>) {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (up_tx, up_rx) = async_channel::unbounded::<Update>();

        thread::Builder::new()
            .name("castty-device".into())
            .spawn(move || run(job_rx, up_tx))
            .expect("spawning the device thread");

        (Worker { jobs: job_tx }, up_rx)
    }

    pub fn send(&self, job: Job) {
        // The thread only goes away at shutdown; a failed send is not worth
        // surfacing to the user.
        let _ = self.jobs.send(job);
    }
}

fn run(jobs: mpsc::Receiver<Job>, updates: async_channel::Sender<Update>) {
    let mut device: Option<Device> = None;

    while let Ok(job) = jobs.recv() {
        // Reconnect lazily: the mouse can be unplugged and replugged at any time.
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
