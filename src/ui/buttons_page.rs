//! Button assignment page.
//!
//! The six entries are a fixed order in the profile blob. Assignments we can
//! express are offered as a list; anything captured that we cannot yet build
//! (a single keystroke, a macro, an unrecognised type) is preserved and shown
//! as the current value rather than being silently replaced.

use crate::hardware::{keycode, Button, ButtonAction, Profile, BUTTONS};
use gtk4 as gtk;
use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
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

/// Sentinel row that opens the key-capture dialog instead of assigning.
const ASSIGN_KEY: &str = "Single key\u{2026}";

/// How an action outside `CHOICES` is described back to the user.
fn describe(action: ButtonAction) -> Option<String> {
    match action {
        ButtonAction::Key(code) => Some(format!("Key: {}", keycode::label(code))),
        ButtonAction::Unknown(kind, param) => {
            Some(format!("Unrecognised (0x{kind:02x} 0x{param:02x})"))
        }
        _ => None,
    }
}

/// Modal that waits for a keypress and reports its HID usage code.
fn capture_key<F: Fn(u8) + 'static>(parent: &gtk::Window, on_key: F) {
    let dialog = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Assign a key")
        .default_width(320)
        .resizable(false)
        .build();

    let label = gtk::Label::builder()
        .label("Press the key to assign.\nEscape cancels.")
        .justify(gtk::Justification::Center)
        .margin_top(28)
        .margin_bottom(28)
        .margin_start(24)
        .margin_end(24)
        .build();
    dialog.set_child(Some(&label));

    let keys = gtk::EventControllerKey::new();
    {
        let dialog = dialog.clone();
        let label = label.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            let value = key.into_glib();
            if value == 0xff1b {
                dialog.close();
                return glib::Propagation::Stop;
            }
            match keycode::from_keyval(value) {
                Some(usage) => {
                    on_key(usage);
                    dialog.close();
                }
                // Modifiers and anything the device has no code for: say so
                // rather than storing something meaningless.
                None => label.set_label("That key can't be assigned.\nTry another, or Escape."),
            }
            glib::Propagation::Stop
        });
    }
    dialog.add_controller(keys);
    dialog.present();
}

pub struct ButtonsPage {
    pub widget: adw::PreferencesPage,
    rows: Vec<adw::ComboRow>,
    /// Action to keep for each row when it is showing a preserved value.
    preserved: Rc<RefCell<Vec<Option<ButtonAction>>>>,
    /// Set while `load` is populating, so it does not fire user-change logic.
    loading: Rc<std::cell::Cell<bool>>,
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
            loading: Rc::new(std::cell::Cell::new(false)),
        }
    }

    /// Wire the "Single key..." entry. Needs the window to parent the dialog.
    pub fn connect_key_assignment<F: Fn() + Clone + 'static>(
        &self,
        window: &gtk::Window,
        changed: F,
    ) {
        for (i, row) in self.rows.iter().enumerate() {
            let window = window.clone();
            let preserved = self.preserved.clone();
            let loading = self.loading.clone();
            let row_weak = row.downgrade();
            let changed = changed.clone();
            row.connect_selected_notify(move |r| {
                if loading.get() {
                    return;
                }
                let model = r.model();
                let Some(list) = model.and_downcast::<gtk::StringList>() else {
                    return;
                };
                if list.string(r.selected()).as_deref() != Some(ASSIGN_KEY) {
                    return;
                }
                let preserved = preserved.clone();
                let row_weak = row_weak.clone();
                let changed = changed.clone();
                capture_key(&window, move |usage| {
                    preserved.borrow_mut()[i] = Some(ButtonAction::Key(usage));
                    if let Some(row) = row_weak.upgrade() {
                        // Re-label the preserved entry to the captured key.
                        let mut labels: Vec<String> =
                            CHOICES.iter().map(|(n, _)| (*n).to_string()).collect();
                        labels.push(format!("Key: {}", keycode::label(usage)));
                        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                        row.set_model(Some(&gtk::StringList::new(&refs)));
                        row.set_selected(labels.len() as u32 - 1);
                    }
                    changed();
                });
            });
        }
    }

    pub fn load(&self, profile: &Profile) {
        self.loading.set(true);
        let mut preserved = self.preserved.borrow_mut();
        for (i, row) in self.rows.iter().enumerate() {
            let action = profile.buttons[i];
            let extra = describe(action);
            preserved[i] = extra.as_ref().map(|_| action);

            let mut labels: Vec<String> =
                CHOICES.iter().map(|(name, _)| (*name).to_string()).collect();
            match &extra {
                Some(text) => labels.push(text.clone()),
                None => labels.push(ASSIGN_KEY.to_string()),
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
        drop(preserved);
        self.loading.set(false);
    }

    pub fn store(&self, profile: &mut Profile) {
        let preserved = self.preserved.borrow();
        for (i, row) in self.rows.iter().enumerate() {
            let index = row.selected() as usize;
            profile.buttons[i] = match CHOICES.get(index) {
                Some((_, action)) => *action,
                // Past the end of the list is either a captured key or a value
                // we preserved because we cannot build it.
                None => preserved[i].unwrap_or(profile.buttons[i]),
            };
        }
    }

    pub fn connect_changed<F: Fn() + Clone + 'static>(&self, f: F) {
        let loading = self.loading.clone();
        for row in &self.rows {
            let g = f.clone();
            let loading = loading.clone();
            row.connect_selected_notify(move |_| {
                if !loading.get() {
                    g();
                }
            });
        }
    }
}
