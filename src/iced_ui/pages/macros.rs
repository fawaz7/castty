//! Macro library.
//!
//! The device stores no macro names -- only a button's event list -- so the
//! library lives in our config. All macros in a profile share one 224-byte
//! area, so the capacity readout belongs on the page, before recording starts.

use super::super::theme::Palette;
use super::super::widgets::{self, GAP};
use crate::hardware::{keycode, MacroEvent, Profile};
use crate::macros::{Library, NamedMacro, Timing};
use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Length};
use std::time::Instant;

/// Slots in the shared macro area; each macro costs its events plus one.
const SLOTS: usize = 32;

#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub name: String,
    pub timing: Timing,
    pub events: Vec<MacroEvent>,
    /// The name this draft is replacing, so a rename moves rather than copies.
    pub original: Option<String>,
    pub last: Option<Instant>,
}

#[derive(Debug, Clone)]
pub enum Message {
    New,
    Edit(String),
    Delete(String),
    NameChanged(String),
    TimingChanged(Timing),
    RecordToggled,
    /// A key went down or up while recording.
    KeyPressed(u8, bool),
    Save,
    Cancel,
}

#[derive(Debug, Default)]
pub struct State {
    pub editing: Option<Draft>,
    pub recording: bool,
}

impl State {
    /// Events in the open draft, for tests and the capacity readout.
    pub fn draft_events(&self) -> usize {
        self.editing.as_ref().map_or(0, |d| d.events.len())
    }

    /// Returns true when the library changed and should be saved.
    pub fn update(&mut self, message: Message, library: &mut Library) -> bool {
        match message {
            Message::New => {
                self.editing = Some(Draft {
                    name: library.unused_name(),
                    timing: Timing::Delay,
                    ..Draft::default()
                });
                self.recording = false;
            }
            Message::Edit(name) => {
                if let Some(found) = library.find(&name) {
                    self.editing = Some(Draft {
                        name: found.name.clone(),
                        timing: found.timing,
                        events: found.events.clone(),
                        original: Some(found.name.clone()),
                        last: None,
                    });
                }
                self.recording = false;
            }
            Message::Delete(name) => {
                library.remove(&name);
                return true;
            }
            Message::NameChanged(name) => {
                if let Some(draft) = &mut self.editing {
                    draft.name = name;
                }
            }
            Message::TimingChanged(timing) => {
                if let Some(draft) = &mut self.editing {
                    // Hold records presses only, so a timed recording is not
                    // valid in hold mode and vice versa.
                    if (draft.timing == Timing::Hold) != (timing == Timing::Hold) {
                        draft.events.clear();
                    }
                    draft.timing = timing;
                }
            }
            Message::RecordToggled => {
                self.recording = !self.recording;
                if self.recording {
                    if let Some(draft) = &mut self.editing {
                        draft.events.clear();
                        draft.last = None;
                    }
                }
            }
            Message::KeyPressed(key, pressed) => {
                if !self.recording {
                    return false;
                }
                if let Some(draft) = &mut self.editing {
                    // Hold macros store only the press; the key is released
                    // when the mouse button is let go.
                    if draft.timing == Timing::Hold && !pressed {
                        return false;
                    }
                    let now = Instant::now();
                    let delay = match draft.timing {
                        Timing::Delay => draft
                            .last
                            .map_or(0, |t| now.duration_since(t).as_millis() as u32),
                        _ => 0,
                    };
                    draft.last = Some(now);
                    draft.events.push(MacroEvent { key, pressed, delay_ms: delay });
                }
            }
            Message::Save => {
                let Some(draft) = self.editing.clone() else {
                    return false;
                };
                let name = draft.name.trim().to_string();
                if name.is_empty() || draft.events.is_empty() {
                    return false;
                }
                if let Some(old) = draft.original.as_deref() {
                    if old != name {
                        library.remove(old);
                    }
                }
                library.put(NamedMacro { name, timing: draft.timing, events: draft.events });
                self.editing = None;
                self.recording = false;
                return true;
            }
            Message::Cancel => {
                self.editing = None;
                self.recording = false;
            }
        }
        false
    }
}

/// Slots used by the macros currently on this profile, and the total available.
pub fn slots_used(_library: &Library, profile: &Profile) -> (usize, usize) {
    let used: usize = profile
        .macros()
        .iter()
        .flatten()
        .map(|m| m.events.len() + 1)
        .sum();
    (used, SLOTS)
}

fn summarise(entry: &NamedMacro) -> String {
    let keys: Vec<String> = entry
        .events
        .iter()
        .filter(|e| e.pressed)
        .map(|e| keycode::label(e.key))
        .collect();
    let mut text = keys.join(" ");
    if text.chars().count() > 36 {
        text = format!("{}\u{2026}", text.chars().take(35).collect::<String>());
    }
    match entry.timing {
        Timing::Hold => format!("{text}, held while the button is down"),
        Timing::Delay => {
            let total: u32 = entry.events.iter().map(|e| e.delay_ms).sum();
            format!("{text}: {} events, {:.1} s", entry.events.len(), total as f64 / 1000.0)
        }
        Timing::None => format!("{text}: {} events", entry.events.len()),
    }
}

pub fn view<'a>(
    state: &'a State,
    library: &'a Library,
    profile: &Profile,
    palette: &Palette,
) -> Element<'a, Message> {
    let style = *palette;
    let dim = palette.dim;
    let (used, total) = slots_used(library, profile);

    if let Some(draft) = &state.editing {
        let timing = widgets::segmented(
            palette,
            vec![
                ("No timing", Message::TimingChanged(Timing::None)),
                ("Record delay", Message::TimingChanged(Timing::Delay)),
                ("Record hold", Message::TimingChanged(Timing::Hold)),
            ],
            match draft.timing {
                Timing::None => 0,
                Timing::Delay => 1,
                Timing::Hold => 2,
            },
        );

        let mut events = column![].spacing(4.0);
        for event in &draft.events {
            let line = if draft.timing == Timing::Delay {
                format!(
                    "{} {}   {} ms",
                    keycode::label(event.key),
                    if event.pressed { "down" } else { "up" },
                    event.delay_ms
                )
            } else {
                format!(
                    "{} {}",
                    keycode::label(event.key),
                    if event.pressed { "down" } else { "up" }
                )
            };
            events = events.push(text(line).size(12.0));
        }

        let editor = column![
            widgets::field(
                palette,
                "Name",
                None::<&str>,
                text_input("Macro name", &draft.name)
                    .on_input(Message::NameChanged)
                    .width(Length::Fixed(220.0)),
            ),
            widgets::field(palette, "Timing", None::<&str>, timing),
            row![
                button(text(if state.recording { "Stop" } else { "Record" }).size(14.0))
                    .padding([8.0, 18.0])
                    .style(move |_t, status| widgets::subtle(&style, status))
                    .on_press(Message::RecordToggled),
                button(text("Save").size(14.0))
                    .padding([8.0, 18.0])
                    .style(move |_t, status| widgets::primary(&style, status))
                    .on_press(Message::Save),
                button(text("Cancel").size(14.0))
                    .padding([8.0, 18.0])
                    .style(move |_t, status| widgets::subtle(&style, status))
                    .on_press(Message::Cancel),
            ]
            .spacing(8.0),
            text(if state.recording {
                "Recording: type the keys you want"
            } else {
                "Press Record, then type"
            })
            .size(12.0)
            .style(move |_t| text::Style { color: Some(dim) }),
            events,
        ]
        .spacing(GAP);

        return column![
            widgets::card(palette, "Edit macro", None, editor),
            widgets::spacer(),
        ]
        .spacing(GAP)
        .into();
    }

    let mut list = column![].spacing(GAP);
    if library.macros.is_empty() {
        list = list.push(
            text("No macros yet. Record one, then assign it on the Buttons page.")
                .size(13.0)
                .style(move |_t| text::Style { color: Some(dim) }),
        );
    }
    for entry in &library.macros {
        let name = entry.name.clone();
        let edit_name = name.clone();
        list = list.push(
            row![
                column![
                    text(entry.name.clone()).size(14.0),
                    text(summarise(entry))
                        .size(12.0)
                        .style(move |_t| text::Style { color: Some(dim) }),
                ]
                .spacing(3.0)
                .width(Length::Fill),
                button(text("Edit").size(13.0))
                    .padding([7.0, 14.0])
                    .style(move |_t, status| widgets::subtle(&style, status))
                    .on_press(Message::Edit(edit_name.clone())),
                button(text("Delete").size(13.0))
                    .padding([7.0, 14.0])
                    .style(move |_t, status| widgets::destructive(&style, status))
                    .on_press(Message::Delete(name.clone())),
            ]
            .spacing(8.0)
            .align_y(iced::Alignment::Center),
        );
    }

    let new_button = button(text("New macro").size(14.0))
        .padding([8.0, 18.0])
        .style(move |_t, status| widgets::primary(&style, status))
        .on_press(Message::New);

    // The capacity line depends on `profile`, which view borrows for only the
    // call's own scope, so it cannot supply `card`'s `&'a str` subtitle -- it
    // is folded into the body instead, where an owned `text` is fine.
    let capacity = text(format!(
        "{used} of {total} storage slots used on this profile. Each macro also uses one as a separator."
    ))
    .size(12.0)
    .style(move |_t| text::Style { color: Some(dim) });

    column![
        widgets::card(
            palette,
            "Macros",
            None,
            column![capacity, new_button, list].spacing(GAP),
        ),
        widgets::spacer(),
    ]
    .spacing(GAP)
    .into()
}
