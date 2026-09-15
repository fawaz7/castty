//! Choosing which macro a button runs.
//!
//! A picker rather than entries in the button's function list: the library can
//! grow without the function list growing with it.

use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::rc::Rc;

/// `on_done` gets the chosen name, or `None` to clear the assignment.
/// It is not called at all if the dialog is cancelled.
pub fn open<F: Fn(Option<String>) + 'static>(
    parent: &gtk::Window,
    names: Vec<String>,
    current: Option<String>,
    on_done: F,
) {
    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Choose a macro")
        .default_width(420)
        .default_height(380)
        .build();

    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label("Cancel");
    header.pack_start(&cancel);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_margin_start(12);
    body.set_margin_end(12);

    let on_done = Rc::new(on_done);

    if names.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("document-edit-symbolic")
            .title("No macros yet")
            .description("Record one on the Macros page, then come back to assign it.")
            .vexpand(true)
            .build();
        body.append(&empty);
    } else {
        let group = adw::PreferencesGroup::new();
        for name in &names {
            let row = adw::ActionRow::builder()
                .title(name)
                .activatable(true)
                .build();
            if current.as_deref() == Some(name.as_str()) {
                let tick = gtk::Image::from_icon_name("object-select-symbolic");
                row.add_suffix(&tick);
            }
            row.connect_activated({
                let window = window.clone();
                let on_done = on_done.clone();
                let name = name.clone();
                move |_| {
                    on_done(Some(name.clone()));
                    window.close();
                }
            });
            group.add(&row);
        }
        let scroller = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&group)
            .build();
        body.append(&scroller);
    }

    if current.is_some() {
        let clear = gtk::Button::with_label("Remove macro from this button");
        clear.add_css_class("destructive-action");
        clear.connect_clicked({
            let window = window.clone();
            let on_done = on_done.clone();
            move |_| {
                on_done(None);
                window.close();
            }
        });
        body.append(&clear);
    }

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&body));
    window.set_content(Some(&view));

    cancel.connect_clicked({
        let window = window.clone();
        move |_| window.close()
    });
    window.present();
}
