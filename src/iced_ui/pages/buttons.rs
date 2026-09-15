//! Buttons page.
//!
//! Rows are numbered to match the callouts on the hero: "side, front" is
//! ambiguous without a picture, and the two side buttons are stored in a
//! non-obvious order (front is 0x10, rear 0x08).

use super::super::theme::Palette;
use super::super::widgets::{self, GAP};
use crate::hardware::{ButtonAction, Profile, BUTTONS};
use crate::macros::{Library, Timing};
use iced::widget::{column, pick_list, text};
use iced::Element;
use std::sync::OnceLock;

/// Assignments the UI can construct directly, in dropdown order.
const CHOICES: [(&str, ButtonAction); 14] = [
    ("Left click", ButtonAction::Mouse(0x01)),
    ("Right click", ButtonAction::Mouse(0x02)),
    ("Middle click", ButtonAction::Mouse(0x04)),
    ("Side, front", ButtonAction::Mouse(0x10)),
    ("Side, rear", ButtonAction::Mouse(0x08)),
    ("Scroll up", ButtonAction::Scroll(1)),
    ("Scroll down", ButtonAction::Scroll(-1)),
    ("Profile up", ButtonAction::ProfileSwitch(0xf0)),
    ("Profile down", ButtonAction::ProfileSwitch(0xf2)),
    ("Profile cycle", ButtonAction::ProfileSwitch(0xf1)),
    ("DPI up", ButtonAction::DpiSwitch(0xf0)),
    ("DPI down", ButtonAction::DpiSwitch(0xf2)),
    ("DPI cycle", ButtonAction::DpiSwitch(0xf1)),
    ("Disabled", ButtonAction::Disabled),
];

const MACRO_PREFIX: &str = "Macro: ";
const KEEP_LABEL: &str = "Macro, not in library";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    /// One of the fixed actions.
    Action(ButtonAction),
    /// Run this library macro.
    Library(String),
    /// Keep whatever is already on the device. A macro deleted from the library
    /// stays on the mouse until that button is changed.
    Keep,
}

#[derive(Debug, Clone)]
pub enum Message {
    Changed(usize, Slot),
}

pub struct State {
    pub slots: [Slot; 6],
}

impl State {
    pub fn from_profile(profile: &Profile, library: &Library) -> Self {
        State {
            slots: std::array::from_fn(|i| match profile.buttons[i] {
                action @ ButtonAction::Macro { hold, .. } => {
                    let events = profile.macro_events(action);
                    match library
                        .macros
                        .iter()
                        .find(|m| m.events == events && (m.timing == Timing::Hold) == hold)
                    {
                        Some(found) => Slot::Library(found.name.clone()),
                        None => Slot::Keep,
                    }
                }
                other => Slot::Action(other),
            }),
        }
    }

    pub fn apply_to(&self, profile: &mut Profile, library: &Library) {
        // Read what is already stored before rewriting the shared macro area,
        // so a macro no longer in the library can be carried through.
        let existing = profile.macros();
        let mut macros: [Option<crate::hardware::Macro>; 6] = Default::default();
        for (i, slot) in self.slots.iter().enumerate() {
            match slot {
                Slot::Action(action) => profile.buttons[i] = *action,
                Slot::Library(name) => {
                    macros[i] = library.find(name).map(|m| m.to_device());
                }
                Slot::Keep => macros[i] = existing[i].clone(),
            }
        }
        // Macros share one area, so every assignment is packed together.
        let _ = profile.set_macros(&macros);
    }

    pub fn update(&mut self, message: Message) {
        let Message::Changed(index, slot) = message;
        if index < self.slots.len() {
            self.slots[index] = slot;
        }
    }
}

/// Row headings, numbered to match the callouts on the hero.
///
/// `widgets::field` borrows its label for the page's whole render lifetime, so
/// a `String` built fresh in `view` cannot supply it -- it would be dropped at
/// the end of the function while the returned `Element` still referenced it.
/// A `OnceLock` builds the six labels once and keeps them for the process's
/// life, which is fine because `BUTTONS` never changes at runtime.
fn headings() -> &'static [String; 6] {
    static HEADINGS: OnceLock<[String; 6]> = OnceLock::new();
    HEADINGS.get_or_init(|| std::array::from_fn(|i| format!("{}. {}", i + 1, BUTTONS[i].label())))
}

pub fn view<'a>(state: &'a State, library: &'a Library, palette: &Palette) -> Element<'a, Message> {
    let headings = headings();
    let mut rows = column![].spacing(GAP);

    for (i, _) in BUTTONS.iter().enumerate() {
        let mut labels: Vec<String> = CHOICES.iter().map(|(n, _)| (*n).to_string()).collect();
        for entry in &library.macros {
            labels.push(format!("{MACRO_PREFIX}{}", entry.name));
        }

        let selected = match &state.slots[i] {
            Slot::Action(action) => CHOICES
                .iter()
                .find(|(_, a)| a == action)
                .map(|(n, _)| (*n).to_string()),
            Slot::Library(name) => Some(format!("{MACRO_PREFIX}{name}")),
            // Shown as its own entry so it is visible rather than silently lost.
            Slot::Keep => {
                labels.push(KEEP_LABEL.to_string());
                Some(KEEP_LABEL.to_string())
            }
        };

        rows = rows.push(widgets::field(
            palette,
            &headings[i],
            None::<&str>,
            pick_list(labels, selected, move |chosen| {
                let slot = if let Some(name) = chosen.strip_prefix(MACRO_PREFIX) {
                    Slot::Library(name.to_string())
                } else if chosen == KEEP_LABEL {
                    Slot::Keep
                } else {
                    Slot::Action(
                        CHOICES
                            .iter()
                            .find(|(n, _)| *n == chosen)
                            .map(|(_, a)| *a)
                            .unwrap_or(ButtonAction::Disabled),
                    )
                };
                Message::Changed(i, slot)
            }),
        ));
    }

    let numbered = column![
        text("Numbers match the callouts on the mouse.").size(12.0).style({
            let dim = palette.dim;
            move |_t| text::Style { color: Some(dim) }
        }),
        rows,
    ]
    .spacing(GAP);

    column![
        widgets::card(
            palette,
            "Buttons",
            Some("Take care leaving yourself without a left click"),
            numbered,
        ),
        widgets::spacer(),
    ]
    .spacing(GAP)
    .into()
}
