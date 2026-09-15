//! Button assignment page.
//!
//! The six entries are a fixed order in the profile blob. Standard assignments
//! are offered directly; macros go through a picker so the function list does
//! not grow with the library. Anything captured that we cannot build (an
//! unrecognised type) is preserved and shown as the current value rather than
//! being silently replaced.

use super::macro_picker;
use crate::hardware::{keycode, ButtonAction, Profile, BUTTONS};
use crate::macros::{Library, Timing};
use gtk4 as gtk;
use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Assignments the UI can construct directly, in dropdown order.
const CHOICES: [(&str, ButtonAction); 14] = [
    ("Left click", ButtonAction::Mouse(0x01)),
    ("Right click", ButtonAction::Mouse(0x02)),
    ("Middle click", ButtonAction::Mouse(0x04)),
    ("Side, front", ButtonAction::Mouse(0x10)),
    ("Side, rear", ButtonAction::Mouse(0x08)),
    ("Scroll up", ButtonAction::Scroll(1)),
    ("Scroll down", ButtonAction::Scroll(-1)),
    // Both switches take the same three directions: f0 up, f2 down, f1 cycles.
    ("Profile up", ButtonAction::ProfileSwitch(0xf0)),
    ("Profile down", ButtonAction::ProfileSwitch(0xf2)),
    ("Profile cycle", ButtonAction::ProfileSwitch(0xf1)),
    ("DPI up", ButtonAction::DpiSwitch(0xf0)),
    ("DPI down", ButtonAction::DpiSwitch(0xf2)),
    ("DPI cycle", ButtonAction::DpiSwitch(0xf1)),
    ("Disabled", ButtonAction::Disabled),
];

/// Sentinel rows that open a dialog rather than assigning. Always last, so a
/// binding can be changed as often as you like.
const ASSIGN_MACRO: &str = "Macro\u{2026}";
const ASSIGN_KEY: &str = "Set a key\u{2026}";

/// How an action outside `CHOICES` is described back to the user.
fn describe(action: ButtonAction, macro_name: Option<&str>) -> Option<String> {
    match action {
        ButtonAction::Key(code) => Some(format!("Key: {}", keycode::label(code))),
        ButtonAction::Macro { events, .. } => Some(match macro_name {
            Some(name) => format!("Macro: {name}"),
            // On the device a macro is just events; if none in the library
            // matches, say so rather than naming it wrongly.
            None => format!("Macro, not in library ({events} events)"),
        }),
        ButtonAction::Unknown(kind, param) => {
            Some(format!("Unrecognised (0x{kind:02x} 0x{param:02x})"))
        }
        _ => None,
    }
}

/// The dropdown for one row: fixed choices, the current value when it is none
/// of them, then the two dialog actions.
fn build_model(extra: Option<&str>) -> (gtk::StringList, Option<u32>) {
    let mut labels: Vec<String> = CHOICES.iter().map(|(n, _)| (*n).to_string()).collect();
    let extra_index = extra.map(|text| {
        labels.push(text.to_string());
        labels.len() as u32 - 1
    });
    labels.push(ASSIGN_MACRO.to_string());
    labels.push(ASSIGN_KEY.to_string());
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    (gtk::StringList::new(&refs), extra_index)
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
                None => label.set_label("That key can't be assigned.\nTry another, or Escape."),
            }
            glib::Propagation::Stop
        });
    }
    dialog.add_controller(keys);
    dialog.present();
}

/// What a button row says about macros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroSlot {
    /// Run this library macro.
    Library(String),
    /// Keep whatever is already on the device. A macro deleted from the library
    /// stays on the mouse until the user changes that button, so the events are
    /// carried through rather than wiped.
    Keep,
    None,
}

pub struct ButtonsPage {
    pub widget: adw::PreferencesPage,
    rows: Vec<adw::ComboRow>,
    /// Current action per row when it is not one of `CHOICES`.
    extra: Rc<RefCell<Vec<Option<ButtonAction>>>>,
    /// Library macro assigned to each row, by name.
    chosen: Rc<RefCell<Vec<Option<String>>>>,
    /// Rows showing a macro that is on the device but not in the library.
    keep: Rc<RefCell<Vec<bool>>>,
    /// Selection to fall back to when a dialog is cancelled.
    previous: Rc<RefCell<Vec<u32>>>,
    /// Set while populating, so it does not fire user-change logic.
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
            chosen: Rc::new(RefCell::new(vec![None; count])),
            keep: Rc::new(RefCell::new(vec![false; count])),
            previous: Rc::new(RefCell::new(vec![0; count])),
            loading: Rc::new(Cell::new(false)),
        }
    }

    /// Re-select a row, showing `extra` as its current value.
    fn show(&self, index: usize, extra: Option<&str>) {
        let (model, extra_index) = build_model(extra);
        self.loading.set(true);
        self.rows[index].set_model(Some(&model));
        let selected = extra_index.unwrap_or(0);
        self.rows[index].set_selected(selected);
        self.previous.borrow_mut()[index] = selected;
        self.loading.set(false);
    }

    /// Wire both dialogs. `library` is read when the picker opens, so macros
    /// recorded after this call are still offered.
    pub fn connect_dialogs<F: Fn() + Clone + 'static>(
        self: &Rc<Self>,
        window: &gtk::Window,
        library: Rc<RefCell<Library>>,
        changed: F,
    ) {
        for i in 0..self.rows.len() {
            let me = self.clone();
            let window = window.clone();
            let library = library.clone();
            let changed = changed.clone();
            self.rows[i].connect_selected_notify(move |r| {
                if me.loading.get() {
                    return;
                }
                let Some(list) = r.model().and_downcast::<gtk::StringList>() else {
                    return;
                };
                let label = list.string(r.selected()).map(|s| s.to_string());
                match label.as_deref() {
                    Some(ASSIGN_MACRO) => {
                        // Sentinels are actions, not values: restore first.
                        let fallback = me.previous.borrow()[i];
                        me.loading.set(true);
                        r.set_selected(fallback);
                        me.loading.set(false);

                        let names: Vec<String> =
                            library.borrow().macros.iter().map(|m| m.name.clone()).collect();
                        let current = me.chosen.borrow()[i].clone();
                        let me2 = me.clone();
                        let changed = changed.clone();
                        macro_picker::open(&window, names, current, move |picked| {
                            match picked {
                                Some(name) => {
                                    me2.extra.borrow_mut()[i] = None;
                                    me2.keep.borrow_mut()[i] = false;
                                    me2.show(i, Some(&format!("Macro: {name}")));
                                    me2.chosen.borrow_mut()[i] = Some(name);
                                }
                                None => {
                                    me2.chosen.borrow_mut()[i] = None;
                                    me2.extra.borrow_mut()[i] = None;
                                    me2.keep.borrow_mut()[i] = false;
                                    me2.show(i, None);
                                }
                            }
                            changed();
                        });
                    }
                    Some(ASSIGN_KEY) => {
                        let fallback = me.previous.borrow()[i];
                        me.loading.set(true);
                        r.set_selected(fallback);
                        me.loading.set(false);

                        let me2 = me.clone();
                        let changed = changed.clone();
                        capture_key(&window, move |usage| {
                            me2.chosen.borrow_mut()[i] = None;
                            me2.keep.borrow_mut()[i] = false;
                            me2.extra.borrow_mut()[i] = Some(ButtonAction::Key(usage));
                            me2.show(i, Some(&format!("Key: {}", keycode::label(usage))));
                            changed();
                        });
                    }
                    _ => {
                        // An ordinary choice replaces whatever was there.
                        me.previous.borrow_mut()[i] = r.selected();
                        me.chosen.borrow_mut()[i] = None;
                        me.keep.borrow_mut()[i] = false;
                        changed();
                    }
                }
            });
        }
    }

    pub fn load(&self, profile: &Profile, library: &Library) {
        self.loading.set(true);
        {
            let mut extra = self.extra.borrow_mut();
            let mut chosen = self.chosen.borrow_mut();
            let mut keep = self.keep.borrow_mut();
            let mut previous = self.previous.borrow_mut();
            for (i, row) in self.rows.iter().enumerate() {
                let action = profile.buttons[i];

                // A macro on the device is only an event list, so recognise it
                // by matching those events against the library.
                let name = match action {
                    ButtonAction::Macro { hold, .. } => {
                        let events = profile.macro_events(action);
                        library
                            .macros
                            .iter()
                            .find(|m| {
                                m.events == events && (m.timing == Timing::Hold) == hold
                            })
                            .map(|m| m.name.clone())
                    }
                    _ => None,
                };
                chosen[i] = name.clone();
                keep[i] = matches!(action, ButtonAction::Macro { .. }) && name.is_none();

                let text = describe(action, name.as_deref());
                extra[i] = match action {
                    ButtonAction::Macro { .. } => None,
                    _ => text.as_ref().map(|_| action),
                };

                let (model, extra_index) = build_model(text.as_deref());
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

    /// What each button wants doing about macros. Resolving these into device
    /// storage is the window's job, because macros share one area.
    pub fn macro_assignments(&self) -> [MacroSlot; BUTTONS.len()] {
        let chosen = self.chosen.borrow();
        let keep = self.keep.borrow();
        std::array::from_fn(|i| match (&chosen[i], keep[i]) {
            (Some(name), _) => MacroSlot::Library(name.clone()),
            (None, true) => MacroSlot::Keep,
            (None, false) => MacroSlot::None,
        })
    }

    /// Write the non-macro assignments; macro rows are left to the caller.
    pub fn store(&self, profile: &mut Profile) {
        let extra = self.extra.borrow();
        let chosen = self.chosen.borrow();
        let keep = self.keep.borrow();
        for (i, row) in self.rows.iter().enumerate() {
            if chosen[i].is_some() || keep[i] {
                continue;
            }
            let index = row.selected() as usize;
            profile.buttons[i] = match CHOICES.get(index) {
                Some((_, action)) => *action,
                // Past the fixed choices: a captured key, or a value preserved
                // because we cannot build it.
                None => extra[i].unwrap_or(profile.buttons[i]),
            };
        }
    }
}
