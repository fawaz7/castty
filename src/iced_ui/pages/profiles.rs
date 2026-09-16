//! Profiles page: naming the five slots, choosing the active one, and
//! restoring factory defaults.
//!
//! The device has no read path, so these are the settings we last wrote, not
//! what the mouse currently holds.

use super::super::theme::Palette;
use super::super::widgets::{self, GAP};
use crate::config;
use crate::hardware::Profile;
use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Length};

/// The name field in the profile blob is ten bytes of ASCII (see
/// `Profile::encode`, which writes `name.as_bytes()` into a ten-byte field).
/// Truncating by `chars()` alone can still overrun that if the name holds a
/// multi-byte character, so non-ASCII characters are dropped first -- what
/// is shown here must be exactly what gets written to the device.
const NAME_MAX: usize = 10;

#[derive(Debug, Clone)]
pub enum Message {
    NameChanged(usize, String),
    Select(usize),
    /// First click on "Restore factory defaults": arms the confirmation
    /// rather than acting immediately.
    RestoreRequested,
    /// Second click, only reachable once armed: actually overwrites the
    /// profiles.
    RestoreConfirmed,
    RestoreCancelled,
}

/// Rename a profile, truncated to what the device can store.
pub fn rename(profiles: &mut [Profile], index: usize, name: &str) {
    if let Some(profile) = profiles.get_mut(index) {
        profile.name = name.chars().filter(char::is_ascii).take(NAME_MAX).collect();
    }
}

/// Reset every profile to its factory default and mark all five dirty.
///
/// The mark matters as much as the reset: Apply only ever writes profiles
/// flagged dirty, and a restore that touches all five in memory but leaves
/// only the active one flagged would silently discard the other four on the
/// next Apply -- which is exactly what the confirm dialog promises it will
/// not do.
pub fn restore_defaults(profiles: &mut [Profile], dirty: &mut [bool]) {
    for (i, profile) in profiles.iter_mut().enumerate() {
        *profile = config::factory_default(i);
        if let Some(flag) = dirty.get_mut(i) {
            *flag = true;
        }
    }
}

/// The profiles Apply should actually send: every one marked dirty, in slot
/// order. Kept as a free function, rather than inlined where `Castty` builds
/// the write job, so "Apply writes every dirty profile, not only the active
/// one" is a claim a test can check without a live device.
pub fn dirty_profiles(profiles: &[Profile], dirty: &[bool]) -> Vec<Profile> {
    profiles
        .iter()
        .zip(dirty)
        .filter(|(_, &d)| d)
        .map(|(p, _)| p.clone())
        .collect()
}

/// Local page state: just the restore confirmation, since names and the
/// active slot live on the profiles themselves.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// Set by `RestoreRequested`; cleared by anything else -- including a
    /// second look at the name field or the row picker -- so a stray click
    /// elsewhere never leaves the destructive action armed.
    confirming: bool,
}

impl State {
    pub fn confirming(&self) -> bool {
        self.confirming
    }

    /// Any message but `RestoreRequested` disarms; only `RestoreConfirmed`
    /// while armed returns true, which is the caller's signal to actually
    /// perform the restore.
    pub fn update(&mut self, message: &Message) -> bool {
        match message {
            Message::RestoreRequested => {
                self.confirming = true;
                false
            }
            Message::RestoreConfirmed => {
                let fire = self.confirming;
                self.confirming = false;
                fire
            }
            _ => {
                self.confirming = false;
                false
            }
        }
    }

    /// Disarm from outside a `Message::Profiles` -- switching the active
    /// profile through the top-bar picker takes this path instead.
    pub fn disarm(&mut self) {
        self.confirming = false;
    }
}

pub fn view<'a>(
    profiles: &'a [Profile],
    current: usize,
    confirming: bool,
    palette: &Palette,
) -> Element<'a, Message> {
    let style = *palette;
    let dim = palette.dim;

    let mut rows = column![].spacing(GAP);
    for (i, profile) in profiles.iter().enumerate() {
        let active = i == current;
        rows = rows.push(
            row![
                text(format!("{}", i + 1)).size(13.0).width(Length::Fixed(20.0)),
                text_input("Profile name", &profile.name)
                    .on_input(move |name| Message::NameChanged(i, name))
                    .width(Length::Fixed(200.0)),
                text(if active { "active" } else { "" })
                    .size(12.0)
                    .style(move |_t| text::Style { color: Some(dim) })
                    .width(Length::Fill),
                button(text(if active { "Editing" } else { "Edit" }).size(13.0))
                    .padding([7.0, 14.0])
                    .style(move |_t, status| widgets::segment(&style, status, active))
                    .on_press(Message::Select(i)),
            ]
            .spacing(10.0)
            .align_y(iced::Alignment::Center),
        );
    }

    // A single button that fires on the first click is one stray click away
    // from wiping every saved profile, so it is replaced with an explicit
    // warning and a second confirming button once armed.
    let restore: Element<'_, Message> = if confirming {
        let danger = palette.danger;
        column![
            text("This overwrites the names and settings of all five profiles saved on this device. This cannot be undone.")
                .size(12.0)
                .style(move |_t| text::Style { color: Some(danger) }),
            row![
                button(text("Overwrite all profiles").size(13.0))
                    .padding([8.0, 16.0])
                    .style(move |_t, status| widgets::destructive(&style, status))
                    .on_press(Message::RestoreConfirmed),
                button(text("Cancel").size(13.0))
                    .padding([8.0, 16.0])
                    .style(move |_t, status| widgets::subtle(&style, status))
                    .on_press(Message::RestoreCancelled),
            ]
            .spacing(10.0),
        ]
        .spacing(10.0)
        .into()
    } else {
        button(text("Restore factory defaults").size(13.0))
            .padding([8.0, 16.0])
            .style(move |_t, status| widgets::destructive(&style, status))
            .on_press(Message::RestoreRequested)
            .into()
    };

    column![
        widgets::card(
            palette,
            "Profiles",
            Some("Up to ten characters each, stored on the mouse"),
            rows,
        ),
        widgets::card(
            palette,
            "Reset",
            Some("Overwrites all five profiles with the captured factory settings"),
            restore,
        ),
        widgets::spacer(),
    ]
    .spacing(GAP)
    .into()
}
