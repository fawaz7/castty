//! Application window: shell, live preview, profile selection, apply plumbing.

use super::buttons_page::ButtonsPage;
use super::dpi_page::DpiPage;
use super::led_page::LedPage;
use super::macros_page::MacrosPage;
use super::mouse_preview::MousePreview;
use super::profiles_page::ProfilesPage;
use super::worker::{Job, Update, Worker};
use crate::config::{self, PROFILE_COUNT};
use crate::hardware::Profile;
use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const APP_ID: &str = "net.castty.Castty";
const STYLE: &str = include_str!("../../resources/style.css");

pub fn run() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider.load_from_data(STYLE);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
    app.connect_activate(build);
    // Our own CLI arguments are handled in main(); don't let GTK parse them.
    app.run_with_args::<&str>(&[])
}

fn scroller(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .child(child)
        .build()
}

fn profile_labels(profiles: &[Profile]) -> Vec<String> {
    profiles
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if p.name.trim().is_empty() {
                format!("Profile {}", i + 1)
            } else {
                p.name.clone()
            }
        })
        .collect()
}

fn build(app: &adw::Application) {
    let profiles = Rc::new(RefCell::new(config::load_all()));
    let current = Rc::new(Cell::new(0usize));
    // Set while pushing state into the widgets. Loading a profile fires the
    // same "changed" signals a user edit does, and those handlers write back to
    // `profiles` -- without this guard that re-entrancy panics on the RefCell.
    let loading = Rc::new(Cell::new(false));
    let (worker, updates) = Worker::spawn();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Castty")
        .default_width(920)
        .default_height(660)
        .build();

    let header = adw::HeaderBar::new();

    // Which profile every page is editing. Applying also makes it the live one,
    // because the commit frame is the only profile-select mechanism there is.
    let selector = gtk::DropDown::from_strings(
        &profile_labels(&profiles.borrow())
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    selector.set_tooltip_text(Some("Profile being edited"));
    header.pack_start(&selector);

    let apply = gtk::Button::builder()
        .label("Apply")
        .tooltip_text("Write this profile to the mouse and switch to it")
        .sensitive(false)
        .build();
    apply.add_css_class("suggested-action");
    header.pack_end(&apply);

    let banner = adw::Banner::builder().revealed(false).build();

    let preview = Rc::new(MousePreview::new());
    let preview_panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview_panel.add_css_class("castty-stage");
    preview_panel.set_size_request(380, -1);
    preview_panel.append(&preview.widget);
    preview.widget.set_vexpand(true);

    let led_page = Rc::new(LedPage::new());
    let dpi_page = Rc::new(DpiPage::new());
    let buttons_page = Rc::new(ButtonsPage::new());
    let macros_page = Rc::new(MacrosPage::new());
    let profiles_page = Rc::new(ProfilesPage::new());

    let stack = adw::ViewStack::new();
    stack.add_titled_with_icon(
        &scroller(&led_page.widget),
        Some("lighting"),
        "Lighting",
        "preferences-color-symbolic",
    );
    stack.add_titled_with_icon(
        &scroller(&dpi_page.widget),
        Some("sensor"),
        "Sensor",
        "input-mouse-symbolic",
    );
    stack.add_titled_with_icon(
        &scroller(&buttons_page.widget),
        Some("buttons"),
        "Buttons",
        "input-touchpad-symbolic",
    );
    stack.add_titled_with_icon(
        &scroller(&macros_page.widget),
        Some("macros"),
        "Macros",
        "media-playback-start-symbolic",
    );
    stack.add_titled_with_icon(
        &scroller(&profiles_page.widget),
        Some("profiles"),
        "Profiles",
        "view-list-symbolic",
    );
    // Callouts are only meaningful next to the button rows.
    stack.connect_visible_child_name_notify({
        let preview = preview.clone();
        move |stack| {
            let on_buttons = stack.visible_child_name().as_deref() == Some("buttons");
            preview.set_show_buttons(on_buttons);
        }
    });

    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    header.set_title_widget(Some(&switcher));

    let split = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    split.append(&preview_panel);
    split.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    split.append(&stack);
    split.set_vexpand(true);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.append(&banner);
    column.append(&split);

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&column));

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&toasts));
    window.set_content(Some(&view));

    // Load the selected profile into every page.
    let refresh = {
        let profiles = profiles.clone();
        let current = current.clone();
        let led_page = led_page.clone();
        let dpi_page = dpi_page.clone();
        let buttons_page = buttons_page.clone();
        let macros_page = macros_page.clone();
        let profiles_page = profiles_page.clone();
        let preview = preview.clone();
        let loading = loading.clone();
        Rc::new(move || {
            loading.set(true);
            // Take a snapshot rather than holding the borrow across widget
            // updates, which would deadlock against any handler that writes back.
            let snapshot = profiles.borrow().clone();
            let p = &snapshot[current.get()];
            led_page.load(p);
            dpi_page.load(p);
            buttons_page.load(p);
            macros_page.load(p);
            profiles_page.load(&snapshot);
            preview.set_state(led_page.preview_state());
            loading.set(false);
        })
    };
    refresh();

    // Collect every page's edits into the selected profile.
    let collect = {
        let profiles = profiles.clone();
        let current = current.clone();
        let led_page = led_page.clone();
        let dpi_page = dpi_page.clone();
        let buttons_page = buttons_page.clone();
        let profiles_page = profiles_page.clone();
        let loading = loading.clone();
        Rc::new(move || {
            if loading.get() {
                return;
            }
            let mut all = profiles.borrow_mut();
            let index = current.get();
            led_page.store(&mut all[index]);
            dpi_page.store(&mut all[index]);
            buttons_page.store(&mut all[index]);
            profiles_page.store(&mut all);
            all[index].index = index as u8;
        })
    };

    let mark_dirty = {
        let apply = apply.clone();
        let preview = preview.clone();
        let led_page = led_page.clone();
        let loading = loading.clone();
        move || {
            if loading.get() {
                return;
            }
            apply.set_sensitive(true);
            preview.set_state(led_page.preview_state());
        }
    };
    // The key-capture dialog needs a parent window, so wire it once the window
    // exists rather than inside the page's constructor.
    buttons_page.connect_key_assignment(window.upcast_ref::<gtk::Window>(), mark_dirty.clone());

    macros_page.connect_editing(
        window.upcast_ref::<gtk::Window>(),
        {
            let profiles = profiles.clone();
            let current = current.clone();
            move |i| {
                let all = profiles.borrow();
                let p = &all[current.get()];
                let existing = p.macros()[i].clone();
                // This macro may reuse whatever it already occupies.
                let own = existing.as_ref().map_or(0, |m| m.events.len() + 1);
                (existing, p.macro_slots_free() + own)
            }
        },
        {
            let profiles = profiles.clone();
            let current = current.clone();
            let collect = collect.clone();
            let refresh = refresh.clone();
            let apply = apply.clone();
            let toasts = toasts.clone();
            move |i, result| {
                // Keep edits made on other rows before rewriting the profile.
                collect();
                let outcome = {
                    let mut all = profiles.borrow_mut();
                    let index = current.get();
                    let mut macros = all[index].macros();
                    macros[i] = result;
                    all[index].set_macros(&macros)
                };
                if let Err(e) = outcome {
                    toasts.add_toast(adw::Toast::new(&e.to_string()));
                    return;
                }
                refresh();
                apply.set_sensitive(true);
            }
        },
    );

    led_page.connect_changed(mark_dirty.clone());
    dpi_page.connect_changed(mark_dirty.clone());
    buttons_page.connect_changed(mark_dirty.clone());
    profiles_page.connect_changed({
        let apply = apply.clone();
        let selector = selector.clone();
        let profiles = profiles.clone();
        let collect = collect.clone();
        let loading = loading.clone();
        move || {
            if loading.get() {
                return;
            }
            apply.set_sensitive(true);
            // Keep the header list in step with renaming. Rebuilding the model
            // re-fires the selector, so guard it the same way.
            collect();
            let labels = profile_labels(&profiles.borrow());
            let selected = selector.selected();
            loading.set(true);
            selector.set_model(Some(&gtk::StringList::new(
                &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            )));
            selector.set_selected(selected);
            loading.set(false);
        }
    });

    // Switching profiles keeps unsaved edits in memory for the old one.
    selector.connect_selected_notify({
        let current = current.clone();
        let collect = collect.clone();
        let refresh = refresh.clone();
        let loading = loading.clone();
        move |sel| {
            if loading.get() {
                return;
            }
            let index = sel.selected() as usize;
            if index >= PROFILE_COUNT || index == current.get() {
                return;
            }
            collect();
            current.set(index);
            refresh();
        }
    });

    dpi_page.connect_surface_start({
        let worker = worker.clone();
        move || worker.send(Job::SurfaceStart)
    });
    dpi_page.connect_surface_result({
        let worker = worker.clone();
        move || worker.send(Job::SurfaceResult)
    });

    profiles_page.connect_reset({
        let profiles = profiles.clone();
        let refresh = refresh.clone();
        let apply = apply.clone();
        move || {
            {
                let mut all = profiles.borrow_mut();
                for (i, p) in all.iter_mut().enumerate() {
                    *p = config::factory_default(i);
                }
            }
            refresh();
            apply.set_sensitive(true);
        }
    });

    apply.connect_clicked({
        let profiles = profiles.clone();
        let current = current.clone();
        let collect = collect.clone();
        let worker = worker.clone();
        let apply_btn = apply.clone();
        move |_| {
            collect();
            let index = current.get();
            let profile = profiles.borrow()[index].clone();
            worker.send(Job::WriteProfile(Box::new(profile)));
            apply_btn.set_sensitive(false);
        }
    });

    glib::spawn_future_local({
        let banner = banner.clone();
        let toasts = toasts.clone();
        let profiles = profiles.clone();
        let current = current.clone();
        let apply = apply.clone();
        let dpi_page = dpi_page.clone();
        async move {
            while let Ok(update) = updates.recv().await {
                match update {
                    Update::Connected(_) => banner.set_revealed(false),
                    Update::Disconnected(msg) => {
                        banner.set_title(&msg);
                        banner.set_revealed(true);
                        apply.set_sensitive(true);
                        dpi_page.set_surface_result(None);
                    }
                    Update::Applied => {
                        banner.set_revealed(false);
                        let index = current.get();
                        match config::save(index, &profiles.borrow()[index]) {
                            Ok(()) => toasts.add_toast(adw::Toast::new("Applied")),
                            Err(e) => toasts
                                .add_toast(adw::Toast::new(&format!("Could not save: {e}"))),
                        }
                    }
                    Update::SurfaceStarted => {}
                    Update::Surface(v) => dpi_page.set_surface_result(Some(v)),
                }
            }
        }
    });

    worker.send(Job::Connect);
    window.present();
}
