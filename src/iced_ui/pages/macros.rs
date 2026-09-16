//! Macro library.
//!
//! The device stores no macro names -- only a button's event list -- so the
//! library lives in our config. All macros in a profile share one 224-byte
//! area, so the capacity readout belongs on the page, before recording starts.

use super::super::theme::Palette;
use super::super::widgets::{self, GAP};
use crate::hardware::protocol::offset::MACRO_SLOTS;
use crate::hardware::{keycode, MacroEvent, Profile};
use crate::macros::{Library, NamedMacro, Timing};
use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Length};
use std::collections::HashSet;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub name: String,
    pub timing: Timing,
    pub events: Vec<MacroEvent>,
    /// The name this draft is replacing, so a rename moves rather than copies.
    pub original: Option<String>,
    pub last: Option<Instant>,
    /// Keys currently held down, by HID usage. A `KeyReleased` reaching us
    /// with no matching entry here was never ours to record -- typically a
    /// focused widget (the name field) consumed the press but iced's
    /// `listen()` still surfaces the release -- so it must not be stored as
    /// an orphan "up" event.
    pub down: HashSet<u8>,
}

/// What changed in the library on the last update, beyond "it changed and
/// needs saving" -- enough for a caller that also tracks button assignments
/// by name to keep them in step without re-deriving everything from the
/// profile.
#[derive(Debug, Clone)]
pub enum Change {
    Renamed { from: String, to: String },
    Deleted(String),
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
    /// Set when Save is rejected, so the editor can say why.
    pub error: Option<String>,
    /// What the last update did to the library, for a caller that needs to
    /// react to exactly that change. Consumed with `take()`.
    pub last_change: Option<Change>,
}

impl State {
    /// Events in the open draft, for tests and the capacity readout.
    pub fn draft_events(&self) -> usize {
        self.editing.as_ref().map_or(0, |d| d.events.len())
    }

    /// Returns true when the library changed and should be saved.
    pub fn update(&mut self, message: Message, library: &mut Library) -> bool {
        self.error = None;
        self.last_change = None;
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
                        down: HashSet::new(),
                    });
                }
                self.recording = false;
            }
            Message::Delete(name) => {
                library.remove(&name);
                // The macro stays on the device until its button is reassigned,
                // so a button pointed at it must fall back to "keep", not lose
                // its assignment silently.
                self.last_change = Some(Change::Deleted(name));
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
                    } else if draft.timing == Timing::Delay && timing != Timing::Delay {
                        // `Timing::None` promises every delay is zero. Leaving
                        // Delay must honour that instead of silently writing
                        // the recorded gaps to a mode that claims not to have
                        // any -- the events themselves are still a perfectly
                        // good recording, so keep them.
                        for event in &mut draft.events {
                            event.delay_ms = 0;
                        }
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
                        draft.down.clear();
                    }
                }
            }
            Message::KeyPressed(key, pressed) => {
                if !self.recording {
                    return false;
                }
                let Some(draft) = &mut self.editing else {
                    return false;
                };
                if pressed {
                    draft.down.insert(key);
                } else if !draft.down.remove(&key) {
                    // No press for this key was recorded -- most likely a
                    // focused widget (the name field) ate the press but iced
                    // still surfaces the release. Storing it would leave an
                    // orphan "up" event with a real, meaningless delay.
                    return false;
                }
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
            Message::Save => {
                let Some(draft) = self.editing.clone() else {
                    return false;
                };
                let name = draft.name.trim().to_string();
                if name.is_empty() || draft.events.is_empty() {
                    return false;
                }
                let renamed = draft.original.as_deref() != Some(name.as_str());
                // A rename (or a new macro) landing on a name already in the
                // library would otherwise get silently overwritten by `put`
                // below -- reject it instead of destroying that other macro.
                if renamed && library.find(&name).is_some() {
                    self.error = Some(format!("A macro named \"{name}\" already exists"));
                    return false;
                }
                if renamed {
                    if let Some(old) = draft.original.as_deref() {
                        library.remove(old);
                        self.last_change = Some(Change::Renamed { from: old.to_string(), to: name.clone() });
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
pub fn slots_used(profile: &Profile) -> (usize, usize) {
    (MACRO_SLOTS - profile.macro_slots_free(), MACRO_SLOTS)
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
    let (used, total) = slots_used(profile);

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

        let mut editor = column![
            widgets::field(
                palette,
                "Name",
                None::<&str>,
                text_input("Macro name", &draft.name)
                    .on_input(Message::NameChanged)
                    .width(Length::Fixed(220.0)),
            ),
            widgets::field(palette, "Timing", None::<&str>, timing),
        ]
        .spacing(GAP);

        if let Some(error) = &state.error {
            let danger = palette.danger;
            editor = editor.push(
                text(error.clone()).size(12.0).style(move |_t| text::Style { color: Some(danger) }),
            );
        }

        let editor = editor
            .push(
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
            )
            .push(
                text(if state.recording {
                    "Recording: type the keys you want"
                } else {
                    "Press Record, then type"
                })
                .size(12.0)
                .style(move |_t| text::Style { color: Some(dim) }),
            )
            .push(events);

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
