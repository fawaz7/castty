//! Button assignment page.
//!
//! The six entries are a fixed order in the profile blob. Assignments we can
//! express are offered as a list; anything captured that we cannot yet build
//! (a single keystroke, a macro, an unrecognised type) is preserved and shown
//! as the current value rather than being silently replaced.

use crate::hardware::{Button, ButtonAction, Profile, BUTTONS};
use gtk4 as gtk;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Assignments the UI can construct, in dropdown order.
const CHOICES: [(&str, ButtonAction); 10] = [
    ("Left click", ButtonAction::Mouse(0x01)),
    ("Right click", ButtonAction::Mouse(0x02)),
    ("Middle click", ButtonAction::Mouse(0x04)),
    ("Side, front", ButtonAction::Mouse(0x10)),
    ("Side, rear", ButtonAction::Mouse(0x08)),
    ("Scroll up", ButtonAction::Scroll(1)),
    ("Scroll down", ButtonAction::Scroll(-1)),
    ("Next profile", ButtonAction::ProfileSwitch(0xf1)),
    ("Next DPI step", ButtonAction::DpiSwitch(0xf1)),
    ("Disabled", ButtonAction::Disabled),
];

/// How an action we cannot build is described back to the user.
fn describe(action: ButtonAction) -> Option<String> {
    match action {
        ButtonAction::Key(code) => Some(format!("Key (HID 0x{code:02x})")),
        ButtonAction::Unknown(kind, param) => {
            Some(format!("Unrecognised (0x{kind:02x} 0x{param:02x})"))
        }
        _ => None,
    }
}

pub struct ButtonsPage {
    pub widget: adw::PreferencesPage,
    rows: Vec<adw::ComboRow>,
    /// Action to keep for each row when it is showing a preserved value.
    preserved: Rc<RefCell<Vec<Option<ButtonAction>>>>,
}

impl Default for ButtonsPage {
    fn default() -> Self {
        Self::new()
    }
}

impl ButtonsPage {
    pub fn new() -> Self {
        let page = adw::PreferencesPage::builder().build();
        let group = adw::PreferencesGroup::builder()
            .title("Buttons")
            .description(
                "Numbers match the callouts on the mouse. Assignments are stored per profile; \
                 take care leaving yourself without a left click, as you may need another \
                 pointing device to undo it.",
            )
            .build();

        // Numbers match the callouts drawn on the preview, so a row can be tied
        // to a physical button without having to guess which is which.
        let rows: Vec<adw::ComboRow> = BUTTONS
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let row = adw::ComboRow::builder()
                    .title(format!("{} — {}", i + 1, b.label()))
                    .build();
                group.add(&row);
                row
            })
            .collect();
        page.add(&group);

        ButtonsPage {
            widget: page,
            rows,
            preserved: Rc::new(RefCell::new(vec![None; BUTTONS.len()])),
        }
    }

    pub fn load(&self, profile: &Profile) {
        let mut preserved = self.preserved.borrow_mut();
        for (i, row) in self.rows.iter().enumerate() {
            let action = profile.buttons[i];
            let extra = describe(action);
            preserved[i] = extra.as_ref().map(|_| action);

            let mut labels: Vec<String> =
                CHOICES.iter().map(|(name, _)| (*name).to_string()).collect();
            if let Some(text) = &extra {
                labels.push(text.clone());
            }
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            row.set_model(Some(&gtk::StringList::new(&refs)));

            let index = match extra {
                // A preserved value is always the last entry.
                Some(_) => labels.len() - 1,
                None => CHOICES
                    .iter()
                    .position(|(_, a)| *a == action)
                    .unwrap_or_else(|| {
                        CHOICES
                            .iter()
                            .position(|(_, a)| *a == Button::Left.default_action())
                            .unwrap_or(0)
                    }),
            };
            row.set_selected(index as u32);
        }
    }

    pub fn store(&self, profile: &mut Profile) {
        let preserved = self.preserved.borrow();
        for (i, row) in self.rows.iter().enumerate() {
            let index = row.selected() as usize;
            profile.buttons[i] = match CHOICES.get(index) {
                Some((_, action)) => *action,
                // Past the end of the list means the preserved entry.
                None => preserved[i].unwrap_or(profile.buttons[i]),
            };
        }
    }

    pub fn connect_changed<F: Fn() + Clone + 'static>(&self, f: F) {
        for row in &self.rows {
            let g = f.clone();
            row.connect_selected_notify(move |_| g());
        }
    }
}
