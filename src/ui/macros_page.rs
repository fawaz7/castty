//! Macro library.
//!
//! Macros are recorded and named here, then assigned to buttons on the buttons
//! page. The device stores no macro names -- only a button's event list -- so
//! the library lives in our own config.

use super::macro_editor;
use crate::hardware::keycode;
use crate::macros::{Library, NamedMacro, Timing};
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// A short description of what a macro types.
fn summarise(m: &NamedMacro) -> String {
    let keys: Vec<String> = m
        .events
        .iter()
        .filter(|e| e.pressed)
        .map(|e| keycode::label(e.key))
        .collect();
    let mut text = keys.join(" ");
    if text.chars().count() > 36 {
        text = format!("{}\u{2026}", text.chars().take(35).collect::<String>());
    }
    match m.timing {
        Timing::Hold => format!("{text} — held while the button is down"),
        Timing::Delay => {
            let total: u32 = m.events.iter().map(|e| e.delay_ms).sum();
            format!("{text} — {} events, {:.1} s", m.events.len(), total as f64 / 1000.0)
        }
        Timing::None => format!("{text} — {} events", m.events.len()),
    }
}

type Hook = Rc<RefCell<Option<Box<dyn Fn()>>>>;

pub struct MacrosPage {
    pub widget: adw::PreferencesPage,
    group: adw::PreferencesGroup,
    rows: RefCell<Vec<adw::ActionRow>>,
    library: Rc<RefCell<Library>>,
    window: RefCell<Option<gtk::Window>>,
    on_change: Hook,
}

impl MacrosPage {
    pub fn new(library: Rc<RefCell<Library>>) -> Rc<Self> {
        let page = adw::PreferencesPage::builder().build();
        let group = adw::PreferencesGroup::builder()
            .title("Macros")
            .description("Record a macro here, then assign it to a button on the Buttons page.")
            .build();

        let new_button = gtk::Button::builder()
            .label("New macro")
            .valign(gtk::Align::Center)
            .build();
        new_button.add_css_class("suggested-action");
        group.set_header_suffix(Some(&new_button));
        page.add(&group);

        let me = Rc::new(MacrosPage {
            widget: page,
            group,
            rows: RefCell::new(Vec::new()),
            library,
            window: RefCell::new(None),
            on_change: Rc::new(RefCell::new(None)),
        });

        new_button.connect_clicked({
            let me = me.clone();
            move |_| {
                let draft = NamedMacro {
                    name: me.library.borrow().unused_name(),
                    timing: Timing::Delay,
                    events: Vec::new(),
                };
                me.edit(Some(draft));
            }
        });

        me
    }

    /// The dialogs need a parent; the window does not exist when the page is
    /// constructed, so it is supplied afterwards.
    pub fn set_window(&self, window: &gtk::Window) {
        *self.window.borrow_mut() = Some(window.clone());
    }

    pub fn connect_changed<F: Fn() + 'static>(&self, f: F) {
        *self.on_change.borrow_mut() = Some(Box::new(f));
    }

    fn notify(&self) {
        if let Some(f) = self.on_change.borrow().as_ref() {
            f();
        }
    }

    fn edit(self: &Rc<Self>, existing: Option<NamedMacro>) {
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let me = self.clone();
        let previous_name = existing.as_ref().map(|m| m.name.clone());
        macro_editor::open(&window, existing, move |result| {
            if let Some(macro_) = result {
                let mut library = me.library.borrow_mut();
                // Renaming should move the entry, not leave a copy behind.
                if let Some(old) = previous_name.as_deref() {
                    if old != macro_.name {
                        library.remove(old);
                    }
                }
                library.put(macro_);
                let _ = library.save();
                drop(library);
                me.refresh();
                me.notify();
            }
        });
    }

    /// Rebuild the list from the library.
    pub fn refresh(self: &Rc<Self>) {
        for row in self.rows.borrow_mut().drain(..) {
            self.group.remove(&row);
        }
        let entries = self.library.borrow().macros.clone();
        let mut rows = Vec::new();
        for entry in entries {
            let row = adw::ActionRow::builder()
                .title(&entry.name)
                .subtitle(summarise(&entry))
                .build();

            let delete = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text("Delete this macro")
                .valign(gtk::Align::Center)
                .build();
            delete.add_css_class("flat");
            let edit = gtk::Button::builder()
                .label("Edit")
                .valign(gtk::Align::Center)
                .build();

            delete.connect_clicked({
                let me = self.clone();
                let name = entry.name.clone();
                move |_| {
                    let Some(window) = me.window.borrow().clone() else {
                        return;
                    };
                    // Deleting also unassigns it from any button, so confirm.
                    let dialog = adw::MessageDialog::new(
                        Some(&window),
                        Some(&format!("Delete “{name}”?")),
                        Some(
                            "Any button using this macro will be left unassigned.                              This cannot be undone.",
                        ),
                    );
                    dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
                    dialog.set_response_appearance(
                        "delete",
                        adw::ResponseAppearance::Destructive,
                    );
                    dialog.set_default_response(Some("cancel"));
                    dialog.set_close_response("cancel");
                    dialog.connect_response(None, {
                        let me = me.clone();
                        let name = name.clone();
                        move |_, response| {
                            if response != "delete" {
                                return;
                            }
                            let mut library = me.library.borrow_mut();
                            library.remove(&name);
                            let _ = library.save();
                            drop(library);
                            me.refresh();
                            me.notify();
                        }
                    });
                    dialog.present();
                }
            });
            edit.connect_clicked({
                let me = self.clone();
                let entry = entry.clone();
                move |_| me.edit(Some(entry.clone()))
            });

            let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            controls.append(&delete);
            controls.append(&edit);
            row.add_suffix(&controls);

            self.group.add(&row);
            rows.push(row);
        }
        if rows.is_empty() {
            let empty = adw::ActionRow::builder()
                .title("No macros yet")
                .subtitle("Use “New macro” to record one")
                .build();
            self.group.add(&empty);
            rows.push(empty);
        }
        *self.rows.borrow_mut() = rows;
    }
}
