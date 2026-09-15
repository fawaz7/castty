//! Button assignment page.
//!
//! The six entries are a fixed order in the profile blob. Assignments we can
//! express are offered as a list; anything captured that we cannot yet build
//! (a macro, an unrecognised type) is preserved and shown as the current value
//! rather than being silently replaced.

use crate::hardware::{keycode, ButtonAction, Profile, BUTTONS};
use gtk4 as gtk;
use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::{Cell, RefCell};
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

/// Sentinel row that opens the key-capture dialog instead of assigning. It is
/// always the last entry, so a key can be reassigned as often as you like.
/// Macros are recorded on their own page, not here.
const ASSIGN_KEY: &str = "Set a key\u{2026}";

/// How an action outside `CHOICES` is described back to the user.
fn describe(action: ButtonAction) -> Option<String> {
    match action {
        ButtonAction::Key(code) => Some(format!("Key: {}", keycode::label(code))),
        ButtonAction::Macro { events, hold: true, .. } => {
            Some(format!("Macro, hold ({events} events)"))
        }
        ButtonAction::Macro { events, .. } => Some(format!("Macro ({events} events)")),
        ButtonAction::Unknown(kind, param) => {
            Some(format!("Unrecognised (0x{kind:02x} 0x{param:02x})"))
        }
        _ => None,
    }
}

/// The dropdown for one row: the fixed choices, the current value when it is
/// not one of them, and always the "set a key" action last.
fn build_model(extra: Option<&str>) -> (gtk::StringList, Option<u32>, u32) {
    let mut labels: Vec<String> = CHOICES.iter().map(|(n, _)| (*n).to_string()).collect();
    let extra_index = extra.map(|text| {
        labels.push(text.to_string());
        labels.len() as u32 - 1
    });
    labels.push(ASSIGN_KEY.to_string());
    let assign_index = labels.len() as u32 - 1;
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    (gtk::StringList::new(&refs), extra_index, assign_index)
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
    /// Current action per row when it is not one of `CHOICES`.
    extra: Rc<RefCell<Vec<Option<ButtonAction>>>>,
    /// Selection to fall back to when the capture dialog is cancelled.
    previous: Rc<RefCell<Vec<u32>>>,
    /// Set while `load` populates, so it does not fire user-change logic.
    loading: Rc<Cell<bool>>,
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

        let count = rows.len();
        ButtonsPage {
            widget: page,
            rows,
            extra: Rc::new(RefCell::new(vec![None; count])),
            previous: Rc::new(RefCell::new(vec![0; count])),
            loading: Rc::new(Cell::new(false)),
        }
    }

    /// Wire the "set a key" entry. Needs the window to parent the dialog.
    pub fn connect_key_assignment<F: Fn() + Clone + 'static>(
        &self,
        window: &gtk::Window,
        changed: F,
    ) {
        for (i, row) in self.rows.iter().enumerate() {
            let window = window.clone();
            let extra = self.extra.clone();
            let previous = self.previous.clone();
            let loading = self.loading.clone();
            let changed = changed.clone();
            row.connect_selected_notify(move |r| {
                if loading.get() {
                    return;
                }
                let Some(list) = r.model().and_downcast::<gtk::StringList>() else {
                    return;
                };
                if list.string(r.selected()).as_deref() != Some(ASSIGN_KEY) {
                    // An ordinary choice: remember it as the fallback.
                    previous.borrow_mut()[i] = r.selected();
                    return;
                }

                // Clones for the dialog callback; the originals stay available
                // for the cancel path below.
                let dlg_extra = extra.clone();
                let dlg_previous = previous.clone();
                let dlg_loading = loading.clone();
                let changed = changed.clone();
                let row = r.clone();
                capture_key(&window, move |usage| {
                    let action = ButtonAction::Key(usage);
                    dlg_extra.borrow_mut()[i] = Some(action);
                    let text = describe(action).unwrap_or_default();
                    let (model, extra_index, _) = build_model(Some(&text));
                    dlg_loading.set(true);
                    row.set_model(Some(&model));
                    let selected = extra_index.unwrap_or(0);
                    row.set_selected(selected);
                    dlg_previous.borrow_mut()[i] = selected;
                    dlg_loading.set(false);
                    changed();
                });

                // Cancelling leaves the sentinel selected, which is not an
                // assignment; restore whatever was chosen before.
                let fallback = previous.borrow()[i];
                {
                    let loading_guard = loading.clone();
                    let row = r.clone();
                    glib::idle_add_local_once(move || {
                        if let Some(list) = row.model().and_downcast::<gtk::StringList>() {
                            if list.string(row.selected()).as_deref() == Some(ASSIGN_KEY) {
                                loading_guard.set(true);
                                row.set_selected(fallback);
                                loading_guard.set(false);
                            }
                        }
                    });
                }
            });
        }
    }

    pub fn load(&self, profile: &Profile) {
        self.loading.set(true);
        {
            let mut extra = self.extra.borrow_mut();
            let mut previous = self.previous.borrow_mut();
            for (i, row) in self.rows.iter().enumerate() {
                let action = profile.buttons[i];
                let text = describe(action);
                extra[i] = text.as_ref().map(|_| action);

                let (model, extra_index, _) = build_model(text.as_deref());
                row.set_model(Some(&model));

                let index = match extra_index {
                    Some(idx) => idx,
                    None => CHOICES
                        .iter()
                        .position(|(_, a)| *a == action)
                        .unwrap_or(0) as u32,
                };
                row.set_selected(index);
                previous[i] = index;
            }
        }
        self.loading.set(false);
    }

    pub fn store(&self, profile: &mut Profile) {
        let extra = self.extra.borrow();
        for (i, row) in self.rows.iter().enumerate() {
            let index = row.selected() as usize;
            profile.buttons[i] = match CHOICES.get(index) {
                Some((_, action)) => *action,
                // Past the fixed choices: either a captured key or a value we
                // preserved because we cannot build it. Never the sentinel,
                // which is restored to the previous selection on cancel.
                None => extra[i].unwrap_or(profile.buttons[i]),
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
