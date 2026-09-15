//! Recording and editing a named macro.
//!
//! Mirrors the vendor flow: name it, choose how timing is captured, record,
//! stop. Assigning it to a button happens on the buttons page.

use crate::hardware::{keycode, MacroEvent};
use crate::macros::{NamedMacro, Timing};
use gtk4 as gtk;
use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

struct State {
    events: Vec<MacroEvent>,
    timing: Timing,
    recording: bool,
    last: Option<Instant>,
    /// Keys physically down, so auto-repeat does not record duplicates.
    held: HashSet<u8>,
}

/// Open the editor for a new or existing macro. `on_done` receives the result,
/// or `None` if cancelled.
pub fn open<F: Fn(Option<NamedMacro>) + 'static>(
    parent: &gtk::Window,
    existing: Option<NamedMacro>,
    on_done: F,
) {
    let state = Rc::new(RefCell::new(State {
        events: existing.as_ref().map(|m| m.events.clone()).unwrap_or_default(),
        timing: existing.as_ref().map_or(Timing::Delay, |m| m.timing),
        recording: false,
        last: None,
        held: HashSet::new(),
    }));

    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(if existing.is_some() { "Edit macro" } else { "New macro" })
        .default_width(480)
        .default_height(560)
        .build();

    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);

    let name = adw::EntryRow::builder().title("Name").build();
    name.set_text(&existing.as_ref().map(|m| m.name.clone()).unwrap_or_default());

    let timing_row = adw::ComboRow::builder()
        .title("Timing")
        .model(&gtk::StringList::new(
            &Timing::ALL.iter().map(|t| t.label()).collect::<Vec<_>>(),
        ))
        .build();
    timing_row.set_selected(
        Timing::ALL
            .iter()
            .position(|t| *t == state.borrow().timing)
            .unwrap_or(0) as u32,
    );

    let settings = adw::PreferencesGroup::new();
    settings.add(&name);
    settings.add(&timing_row);

    let record = gtk::ToggleButton::with_label("Record");
    record.add_css_class("destructive-action");
    let status = gtk::Label::new(Some("Not recording"));
    status.add_css_class("dim-label");
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 6);
    controls.set_halign(gtk::Align::Center);
    controls.append(&record);
    controls.append(&status);

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();

    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.append(&settings);
    body.append(&controls);
    body.append(&scroller);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&body));
    window.set_content(Some(&view));

    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let list = list.clone();
        let status = status.clone();
        Rc::new(move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let s = state.borrow();
            for event in &s.events {
                let row = adw::ActionRow::builder()
                    .title(format!(
                        "{} {}",
                        keycode::label(event.key),
                        if event.pressed { "down" } else { "up" }
                    ))
                    .build();
                if s.timing == Timing::Delay {
                    row.set_subtitle(&format!("{} ms", event.delay_ms));
                }
                list.append(&row);
            }
            status.set_label(&if s.recording {
                format!("Recording — {} events", s.events.len())
            } else if s.events.is_empty() {
                "Not recording".to_string()
            } else {
                format!("{} events", s.events.len())
            });
        })
    };
    refresh();

    let keys = gtk::EventControllerKey::new();
    {
        let state = state.clone();
        let refresh = refresh.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            let mut s = state.borrow_mut();
            if !s.recording {
                return glib::Propagation::Proceed;
            }
            let Some(usage) = keycode::from_keyval(key.into_glib()) else {
                return glib::Propagation::Stop;
            };
            // Auto-repeat fires while a key is down; record the press once.
            if !s.held.insert(usage) {
                return glib::Propagation::Stop;
            }
            let now = Instant::now();
            let delay = match s.timing {
                Timing::Delay => s.last.map_or(0, |t| now.duration_since(t).as_millis() as u32),
                _ => 0,
            };
            s.last = Some(now);
            s.events.push(MacroEvent { key: usage, pressed: true, delay_ms: delay });
            drop(s);
            refresh();
            glib::Propagation::Stop
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        keys.connect_key_released(move |_, key, _, _| {
            let mut s = state.borrow_mut();
            if !s.recording {
                return;
            }
            let Some(usage) = keycode::from_keyval(key.into_glib()) else {
                return;
            };
            s.held.remove(&usage);
            // Hold macros store presses only; the key is released when the
            // mouse button is let go.
            if s.timing == Timing::Hold {
                return;
            }
            let now = Instant::now();
            let delay = match s.timing {
                Timing::Delay => s.last.map_or(0, |t| now.duration_since(t).as_millis() as u32),
                _ => 0,
            };
            s.last = Some(now);
            s.events.push(MacroEvent { key: usage, pressed: false, delay_ms: delay });
            drop(s);
            refresh();
        });
    }
    window.add_controller(keys);

    record.connect_toggled({
        let state = state.clone();
        let refresh = refresh.clone();
        move |btn| {
            let mut s = state.borrow_mut();
            s.recording = btn.is_active();
            if s.recording {
                s.events.clear();
                s.last = None;
                s.held.clear();
                btn.set_label("Stop");
            } else {
                btn.set_label("Record");
            }
            drop(s);
            refresh();
        }
    });

    timing_row.connect_selected_notify({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row| {
            let mut s = state.borrow_mut();
            let new = Timing::ALL
                .get(row.selected() as usize)
                .copied()
                .unwrap_or_default();
            // Hold records something structurally different, so a recording
            // made in one mode is not valid in the other.
            if (s.timing == Timing::Hold) != (new == Timing::Hold) {
                s.events.clear();
            }
            s.timing = new;
            drop(s);
            refresh();
        }
    });

    cancel.connect_clicked({
        let window = window.clone();
        move |_| window.close()
    });

    save.connect_clicked({
        let state = state.clone();
        let window = window.clone();
        let name = name.clone();
        move |_| {
            let s = state.borrow();
            let text = name.text().trim().to_string();
            if text.is_empty() || s.events.is_empty() {
                // A macro with no name or no events cannot be referred to or
                // played back; say so rather than storing something useless.
                name.add_css_class("error");
                return;
            }
            let result = NamedMacro {
                name: text,
                timing: s.timing,
                events: s.events.clone(),
            };
            drop(s);
            on_done(Some(result));
            window.close();
        }
    });

    window.present();
}
