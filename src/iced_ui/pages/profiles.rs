//! Profiles page: naming the five slots, choosing the active one, and
//! restoring factory defaults.
//!
//! The device has no read path, so these are the settings we last wrote, not
//! what the mouse currently holds.

use super::super::theme::Palette;
use super::super::widgets::{self, GAP};
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
