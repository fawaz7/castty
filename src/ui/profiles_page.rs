//! Profiles page: naming the five slots and restoring factory defaults.
//!
//! Selecting which profile you are editing lives in the header bar, because it
//! is context for every other page rather than a setting of its own.
//!
//! Note the device has no read path: these names and settings are what *we*
//! last wrote, not what the mouse currently holds. If something else has
//! written to the mouse, this view will be stale until the next apply.

use crate::config::PROFILE_COUNT;
use crate::hardware::Profile;
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The name field in the profile blob is ten bytes of ASCII.
const NAME_MAX: usize = 10;

type Hook = Rc<RefCell<Option<Box<dyn Fn()>>>>;

pub struct ProfilesPage {
    pub widget: adw::PreferencesPage,
    names: Vec<adw::EntryRow>,
    reset: gtk::Button,
    on_reset: Hook,
}

impl Default for ProfilesPage {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfilesPage {
    pub fn new() -> Self {
        let page = adw::PreferencesPage::builder().build();

        let group = adw::PreferencesGroup::builder()
            .title("Profile names")
            .description("Up to ten characters each, stored on the mouse.")
            .build();

        let names: Vec<adw::EntryRow> = (0..PROFILE_COUNT)
            .map(|i| {
                let row = adw::EntryRow::builder()
                    .title(format!("Profile {}", i + 1))
                    .build();
                row.set_max_width_chars(NAME_MAX as i32);
                group.add(&row);
                row
            })
            .collect();
        page.add(&group);

        let danger = adw::PreferencesGroup::builder()
            .title("Reset")
            .description(
                "Restores every profile to the factory settings captured from the vendor \
                 software. This overwrites all five profiles on the mouse.",
            )
            .build();
        let reset = gtk::Button::builder()
            .label("Restore factory defaults")
            .valign(gtk::Align::Center)
            .build();
        reset.add_css_class("destructive-action");
        let reset_row = adw::ActionRow::builder().title("All profiles").build();
        reset_row.add_suffix(&reset);
        danger.add(&reset_row);
        page.add(&danger);

        ProfilesPage {
            widget: page,
            names,
            reset,
            on_reset: Rc::new(RefCell::new(None)),
        }
    }

    pub fn load(&self, profiles: &[Profile]) {
        for (row, p) in self.names.iter().zip(profiles.iter()) {
            if row.text() != p.name {
                row.set_text(&p.name);
            }
        }
    }

    /// Write the edited names back. Only names live here; every other field
    /// belongs to the page that owns it.
    pub fn store(&self, profiles: &mut [Profile]) {
        for (row, p) in self.names.iter().zip(profiles.iter_mut()) {
            let text = row.text();
            p.name = text.chars().take(NAME_MAX).collect();
        }
    }

    pub fn connect_changed<F: Fn() + Clone + 'static>(&self, f: F) {
        for row in &self.names {
            let g = f.clone();
            row.connect_changed(move |_| g());
        }
    }

    pub fn connect_reset<F: Fn() + 'static>(&self, f: F) {
        *self.on_reset.borrow_mut() = Some(Box::new(f));
        let hook = self.on_reset.clone();
        self.reset.connect_clicked(move |_| {
            if let Some(f) = hook.borrow().as_ref() {
                f();
            }
        });
    }
}
