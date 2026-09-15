//! Macros page.
//!
//! Macros belong to buttons -- the device has no separate macro library, each
//! button entry points at its own event list -- so this page is organised by
//! button. Recording one here also assigns it to that button.
//!
//! All macros in a profile share a single small storage area, so the capacity
//! readout is part of the page rather than a detail of the editor.

use super::macro_editor;
use crate::hardware::{keycode, Macro, Profile, BUTTONS};
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::rc::Rc;

/// A short description of what a macro types.
fn summarise(m: &Macro) -> String {
    let keys: Vec<String> = m
        .events
        .iter()
        .filter(|e| e.pressed)
        .map(|e| keycode::label(e.key))
        .collect();
    let total: u32 = m.events.iter().map(|e| e.delay_ms).sum();

    let mut text = keys.join(" ");
    if text.chars().count() > 40 {
        text = format!("{}\u{2026}", text.chars().take(39).collect::<String>());
    }
    if m.hold {
        format!("{text} — held while the button is down")
    } else {
        format!("{text} — {} events, {:.1} s", m.events.len(), total as f64 / 1000.0)
    }
}

pub struct MacrosPage {
    pub widget: adw::PreferencesPage,
    group: adw::PreferencesGroup,
    rows: Vec<adw::ActionRow>,
    edit: Vec<gtk::Button>,
    remove: Vec<gtk::Button>,
}

impl Default for MacrosPage {
    fn default() -> Self {
        Self::new()
    }
}

impl MacrosPage {
    pub fn new() -> Self {
        let page = adw::PreferencesPage::builder().build();
        let group = adw::PreferencesGroup::builder().title("Macros").build();

        let mut rows = Vec::new();
        let mut edit = Vec::new();
        let mut remove = Vec::new();
        for (i, button) in BUTTONS.iter().enumerate() {
            let row = adw::ActionRow::builder()
                .title(format!("{} — {}", i + 1, button.label()))
                .subtitle("No macro")
                .build();

            let remove_btn = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text("Remove this macro")
                .valign(gtk::Align::Center)
                .visible(false)
                .build();
            remove_btn.add_css_class("flat");

            let edit_btn = gtk::Button::builder()
                .label("Record")
                .valign(gtk::Align::Center)
                .build();

            let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            controls.append(&remove_btn);
            controls.append(&edit_btn);
            row.add_suffix(&controls);

            group.add(&row);
            rows.push(row);
            edit.push(edit_btn);
            remove.push(remove_btn);
        }
        page.add(&group);

        MacrosPage { widget: page, group, rows, edit, remove }
    }

    pub fn load(&self, profile: &Profile) {
        let macros = profile.macros();
        for (i, row) in self.rows.iter().enumerate() {
            match &macros[i] {
                Some(m) => {
                    row.set_subtitle(&summarise(m));
                    self.edit[i].set_label("Edit");
                    self.remove[i].set_visible(true);
                }
                None => {
                    row.set_subtitle("No macro");
                    self.edit[i].set_label("Record");
                    self.remove[i].set_visible(false);
                }
            }
        }
        let free = profile.macro_slots_free();
        let total = free + profile.macros().iter().flatten().map(|m| m.events.len() + 1).sum::<usize>();
        self.group.set_description(Some(&format!(
            "{} of {total} storage slots free. Every macro also uses one slot as a separator.",
            free
        )));
    }

    /// `provide` gives a button's current macro and the slots it may use;
    /// `apply` stores the result and is expected to repack and reload.
    pub fn connect_editing<P, A>(&self, window: &gtk::Window, provide: P, apply: A)
    where
        P: Fn(usize) -> (Option<Macro>, usize) + 'static,
        A: Fn(usize, Option<Macro>) + 'static,
    {
        let provide = Rc::new(provide);
        let apply = Rc::new(apply);

        for (i, button) in self.edit.iter().enumerate() {
            let window = window.clone();
            let provide = provide.clone();
            let apply = apply.clone();
            button.connect_clicked(move |_| {
                let (existing, slots) = provide(i);
                let apply = apply.clone();
                macro_editor::open(&window, existing, slots, move |result| apply(i, result));
            });
        }
        for (i, button) in self.remove.iter().enumerate() {
            let apply = apply.clone();
            button.connect_clicked(move |_| apply(i, None));
        }
    }
}
