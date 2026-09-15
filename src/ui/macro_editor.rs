//! Recording and editing a macro.
//!
//! Storage is one small shared area -- 32 event slots for the whole profile,
//! and every macro also costs one slot for its terminator -- so the editor
//! shows what is left and refuses to record past it. Overflowing would corrupt
//! the profile rather than fail cleanly.

use crate::hardware::{keycode, Macro, MacroEvent};
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
    hold: bool,
    recording: bool,
    last: Option<Instant>,
    /// Keys physically down, so auto-repeat does not record duplicates.
    held: HashSet<u8>,
}

/// Open the editor. `slots_free` is the capacity available to *this* macro,
/// i.e. the profile's free slots plus whatever the existing macro occupies.
pub fn open<F: Fn(Option<Macro>) + 'static>(
    parent: &gtk::Window,
    existing: Option<Macro>,
    slots_free: usize,
    on_done: F,
) {
    let state = Rc::new(RefCell::new(State {
        events: existing.as_ref().map(|m| m.events.clone()).unwrap_or_default(),
        hold: existing.as_ref().is_some_and(|m| m.hold),
        recording: false,
        last: None,
        held: HashSet::new(),
    }));

    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Macro")
        .default_width(460)
        .default_height(520)
        .build();

    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);

    let hold_row = adw::SwitchRow::builder()
        .title("Hold mode")
        .subtitle("Keys stay down while the button is held, instead of replaying with timing")
        .active(state.borrow().hold)
        .build();

    let record = gtk::ToggleButton::with_label("Record");
    record.add_css_class("destructive-action");
    let clear = gtk::Button::with_label("Clear");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&record);
    buttons.append(&clear);

    let capacity = gtk::Label::new(None);
    capacity.add_css_class("dim-label");

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(&hold_row);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.append(&group);
    body.append(&buttons);
    body.append(&capacity);
    body.append(&scroller);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&body));
    window.set_content(Some(&view));

    // Redraw the event list and capacity readout from state.
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let list = list.clone();
        let capacity = capacity.clone();
        let save = save.clone();
        Rc::new(move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let s = state.borrow();
            for event in &s.events {
                let action = if event.pressed { "press" } else { "release" };
                let row = adw::ActionRow::builder()
                    .title(format!("{} {}", keycode::label(event.key), action))
                    .build();
                if !s.hold {
                    row.set_subtitle(&format!("{} ms", event.delay_ms));
                }
                list.append(&row);
            }
            let used = s.events.len() + usize::from(!s.events.is_empty());
            let over = used > slots_free;
            capacity.set_label(&format!(
                "{} of {} slots used{}",
                used,
                slots_free,
                if over { " — too many to save" } else { "" }
            ));
            save.set_sensitive(!over);
        })
    };
    refresh();

    // Key capture while recording.
    let keys = gtk::EventControllerKey::new();
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let slots = slots_free;
        keys.connect_key_pressed(move |_, key, _, _| {
            let mut s = state.borrow_mut();
            if !s.recording {
                return glib::Propagation::Proceed;
            }
            let Some(usage) = keycode::from_keyval(key.into_glib()) else {
                return glib::Propagation::Stop;
            };
            // Auto-repeat fires repeatedly while a key is down; record once.
            if !s.held.insert(usage) {
                return glib::Propagation::Stop;
            }
            if s.events.len() + 2 > slots {
                return glib::Propagation::Stop;
            }
            let now = Instant::now();
            let delay = if s.hold {
                0
            } else {
                s.last.map_or(0, |t| now.duration_since(t).as_millis() as u32)
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
            // Hold macros store only presses; the release happens when the
            // mouse button is let go.
            if s.hold {
                return;
            }
            let now = Instant::now();
            let delay = s.last.map_or(0, |t| now.duration_since(t).as_millis() as u32);
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

    clear.connect_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |_| {
            let mut s = state.borrow_mut();
            s.events.clear();
            s.last = None;
            drop(s);
            refresh();
        }
    });

    hold_row.connect_active_notify({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row| {
            let mut s = state.borrow_mut();
            s.hold = row.is_active();
            // The two modes store different things, so a recording made in one
            // is not valid in the other.
            s.events.clear();
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
        move |_| {
            let s = state.borrow();
            let result = if s.events.is_empty() {
                None
            } else {
                Some(Macro { events: s.events.clone(), hold: s.hold })
            };
            drop(s);
            on_done(result);
            window.close();
        }
    });

    window.present();
}
