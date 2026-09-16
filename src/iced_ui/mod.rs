//! The iced front end.
//!
//! Everything below `hardware`, `config` and `macros` is unchanged — those
//! layers never depended on a toolkit.

pub mod art;
pub mod colour_picker;
pub mod pages;
pub mod preview;
pub mod settings;
pub mod theme;
pub mod widgets;
pub mod worker;

use crate::config;
use crate::hardware::Profile;
use crate::macros::Library;
use settings::Settings;
use theme::Palette;

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length, Subscription, Task};
use std::rc::Rc;
use std::time::Instant;

/// How much room the mouse gets on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hero {
    /// Drives the page height; the mouse is the subject.
    Large,
    /// A fixed compact render that never competes with the content beside it.
    Small,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Lighting,
    Sensor,
    Buttons,
    Macros,
    Profiles,
    About,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::Lighting,
        Page::Sensor,
        Page::Buttons,
        Page::Macros,
        Page::Profiles,
        Page::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Page::Lighting => "Lighting",
            Page::Sensor => "Sensor",
            Page::Buttons => "Buttons",
            Page::Macros => "Macros",
            Page::Profiles => "Profiles",
            Page::About => "About",
        }
    }

    /// The mouse leads where it is the subject, shrinks where it is context,
    /// and goes away where it would be decoration.
    fn hero(self) -> Hero {
        match self {
            Page::Lighting | Page::Buttons => Hero::Large,
            Page::Sensor | Page::Macros | Page::Profiles => Hero::Small,
            Page::About => Hero::None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    PageSelected(Page),
    ProfileSelected(usize),
    Apply,
    Device(worker::Update),
    Lighting(pages::lighting::Message),
    Sensor(pages::sensor::Message),
    Buttons(pages::buttons::Message),
    Macros(pages::macros::Message),
    About(pages::about::Message),
    Tick,
}

pub struct Castty {
    profiles: Vec<Profile>,
    current: usize,
    library: Library,
    page: Page,
    dirty: bool,
    status: String,
    settings: Settings,
    worker: worker::Handle,
    art: Rc<art::Art>,
    lighting: pages::lighting::State,
    sensor: pages::sensor::State,
    buttons: pages::buttons::State,
    macros: pages::macros::State,
    started: Instant,
}

impl Castty {
    fn new() -> (Self, Task<Message>) {
        let (worker, _) = worker::spawn();
        worker.send(worker::Job::Connect);
        let profiles = config::load_all();
        let lighting = pages::lighting::State::from_profile(&profiles[0]);
        let sensor = pages::sensor::State::from_profile(&profiles[0]);
        let library = Library::load();
        let buttons = pages::buttons::State::from_profile(&profiles[0], &library);
        (
            Castty {
                lighting,
                sensor,
                buttons,
                macros: pages::macros::State::default(),
                art: Rc::new(art::Art::load()),
                started: Instant::now(),
                profiles,
                current: 0,
                library,
                page: Page::Lighting,
                dirty: false,
                status: "Looking for the mouse\u{2026}".into(),
                settings: Settings::load(),
                worker,
            },
            Task::none(),
        )
    }

    fn palette(&self) -> Palette {
        theme::resolve(self.settings.theme, self.settings.accent)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PageSelected(page) => {
                // The subscription that feeds the recorder stays live on
                // every other page too; leaving Macros mid-recording must
                // stop it, since there is no reachable Stop button once the
                // page is gone.
                self.macros.recording = false;
                self.page = page;
            }
            Message::ProfileSelected(index) => {
                if index < self.profiles.len() {
                    self.current = index;
                    self.lighting = pages::lighting::State::from_profile(&self.profiles[index]);
                    self.sensor = pages::sensor::State::from_profile(&self.profiles[index]);
                    self.buttons =
                        pages::buttons::State::from_profile(&self.profiles[index], &self.library);
                }
            }
            Message::Lighting(message) => {
                self.lighting.update(message);
                self.dirty = true;
            }
            Message::Buttons(message) => {
                self.buttons.update(message);
                self.dirty = true;
            }
            Message::Macros(message) => {
                if self.macros.update(message, &mut self.library) {
                    let _ = self.library.save();
                    // Re-resolve only the slots the change actually touches,
                    // rather than rebuilding from the profile: that would
                    // discard button edits the user made but has not applied
                    // yet, while `dirty` stayed true and the next Apply wrote
                    // the reverted assignment to flash.
                    match self.macros.last_change.take() {
                        Some(pages::macros::Change::Renamed { from, to }) => {
                            for slot in &mut self.buttons.slots {
                                if *slot == pages::buttons::Slot::Library(from.clone()) {
                                    *slot = pages::buttons::Slot::Library(to.clone());
                                }
                            }
                        }
                        Some(pages::macros::Change::Deleted(name)) => {
                            // The macro stays on the device until the button is
                            // reassigned, so this becomes "keep", not "none".
                            for slot in &mut self.buttons.slots {
                                if *slot == pages::buttons::Slot::Library(name.clone()) {
                                    *slot = pages::buttons::Slot::Keep;
                                }
                            }
                        }
                        None => {}
                    }
                }
            }
            Message::Sensor(message) => {
                let wants_device = self.sensor.update(message);
                self.dirty = true;
                if wants_device {
                    // Starting sends the start command; the end of the window
                    // asks for the result.
                    let job = if self.sensor.measuring() {
                        worker::Job::SurfaceStart
                    } else {
                        worker::Job::SurfaceResult
                    };
                    self.worker.send(job);
                }
            }
            Message::About(pages::about::Message::ThemeChanged(named)) => {
                self.settings.theme = named;
                self.settings.save();
            }
            Message::About(pages::about::Message::AccentChanged(accent)) => {
                self.settings.accent = accent;
                self.settings.save();
            }
            Message::About(pages::about::Message::OpenGithub) => {
                // Best effort; a missing opener is not worth an error dialog.
                let _ = std::process::Command::new("xdg-open")
                    .arg(pages::about::GITHUB)
                    .spawn();
            }
            Message::Apply => {
                self.lighting.apply_to(&mut self.profiles[self.current]);
                self.sensor.apply_to(&mut self.profiles[self.current]);
                match self.buttons.apply_to(&mut self.profiles[self.current], &self.library) {
                    Ok(()) => {
                        let profile = self.profiles[self.current].clone();
                        self.worker.send(worker::Job::WriteProfile(Box::new(profile)));
                        self.dirty = false;
                    }
                    // The macro area is full; nothing is sent to the mouse, so
                    // the write is not silently reported as saved.
                    Err(error) => self.status = format!("Not saved: {error}"),
                }
            }
            Message::Tick => {}
            Message::Device(update) => match update {
                worker::Update::Connected(id) => {
                    self.status = format!("Connected, firmware {:x}.{:02x}", id.firmware >> 8, id.firmware & 0xff);
                }
                worker::Update::Disconnected(why) => self.status = why,
                worker::Update::Applied => {
                    let _ = config::save(self.current, &self.profiles[self.current]);
                    self.status = "Saved to the mouse".into();
                }
                worker::Update::SurfaceStarted => {
                    self.status = "Measuring the surface\u{2026}".into();
                }
                worker::Update::Surface(value) => {
                    self.sensor.update(pages::sensor::Message::SurfaceResult(value));
                    self.status = format!(
                        "Surface quality {}/10",
                        crate::hardware::surface_score(value)
                    );
                }
            },
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let device = worker::subscription().map(Message::Device);
        let mut subs = vec![device];
        if self.page.hero() != Hero::None && self.lighting.animated() {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::Tick),
            );
        }
        if self.sensor.measuring() {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(1))
                    .map(|_| Message::Sensor(pages::sensor::Message::Tick)),
            );
        }
        if self.macros.recording {
            // iced 0.14 exposes only `listen()`; there is no on_key_press or
            // on_key_release helper.
            subs.push(iced::keyboard::listen().filter_map(|event| match event {
                iced::keyboard::Event::KeyPressed { key, .. } => key_to_message(key, true),
                iced::keyboard::Event::KeyReleased { key, .. } => key_to_message(key, false),
                _ => None,
            }));
        }
        Subscription::batch(subs)
    }

    fn tab_bar(&self, palette: &Palette) -> Element<'_, Message> {
        let style = *palette;
        let tabs = Page::ALL.iter().fold(row![].spacing(2.0), |acc, page| {
            let selected = *page == self.page;
            acc.push(
                button(text(page.label()).size(14.0))
                    .padding([8.0, 16.0])
                    .style(move |_t, status| widgets::segment(&style, status, selected))
                    .on_press(Message::PageSelected(*page)),
            )
        });

        let names: Vec<String> = self
            .profiles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if p.name.trim().is_empty() {
                    format!("Profile {}", i + 1)
                } else {
                    p.name.clone()
                }
            })
            .collect();
        let selected = names.get(self.current).cloned();

        container(
            row![
                text("castty").size(18.0),
                Space::new().width(Length::Fixed(18.0)),
                tabs,
                widgets::spacer(),
                iced::widget::pick_list(names.clone(), selected, move |chosen| {
                    let index = names.iter().position(|n| *n == chosen).unwrap_or(0);
                    Message::ProfileSelected(index)
                }),
                button(text("Apply").size(14.0))
                    .padding([9.0, 22.0])
                    .style(move |_t, status| widgets::primary(&style, status))
                    .on_press_maybe(self.dirty.then_some(Message::Apply)),
            ]
            .align_y(iced::Alignment::Center)
            .spacing(10.0),
        )
        .padding([12.0, widgets::GAP])
        .into()
    }

    fn hero(&self, palette: &Palette) -> Element<'_, Message> {
        let seconds = self.started.elapsed().as_secs_f32();
        let (wheel, logo) = self.lighting.lit(seconds);
        let style = *palette;
        container(
            iced::widget::canvas(preview::Preview {
                art: self.art.clone(),
                wheel,
                logo,
                show_buttons: self.page == Page::Buttons,
                palette: *palette,
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .style(move |_t| style.stage())
        .into()
    }

    fn view(&self) -> Element<'_, Message> {
        let palette = self.palette();

        let content: Element<'_, Message> = match self.page {
            Page::Lighting => pages::lighting::view(&self.lighting, &palette).map(Message::Lighting),
            Page::Sensor => pages::sensor::view(&self.sensor, &palette).map(Message::Sensor),
            Page::Buttons => {
                pages::buttons::view(&self.buttons, &self.library, &palette).map(Message::Buttons)
            }
            Page::Macros => pages::macros::view(
                &self.macros,
                &self.library,
                &self.profiles[self.current],
                &palette,
            )
            .map(Message::Macros),
            Page::About => {
                pages::about::view(self.settings.theme, self.settings.accent, &palette)
                    .map(Message::About)
            }
            other => widgets::card(&palette, other.label(), Some("Coming next"), text("").size(1.0)),
        };

        let body: Element<'_, Message> = match self.page.hero() {
            Hero::Large => row![
                container(self.hero(&palette)).width(Length::FillPortion(5)).height(Length::Fill),
                container(content).width(Length::FillPortion(5)),
            ]
            .spacing(widgets::GAP)
            .height(Length::Fill)
            .into(),
            Hero::Small => column![
                container(self.hero(&palette))
                    .width(Length::Fill)
                    .height(Length::Fixed(150.0)),
                content,
            ]
            .spacing(widgets::GAP)
            .into(),
            Hero::None => content,
        };

        column![
            self.tab_bar(&palette),
            scrollable(container(body).padding(widgets::GAP)).height(Length::Fill),
            self.footer(&palette),
        ]
        .into()
    }

    fn footer(&self, palette: &Palette) -> Element<'_, Message> {
        let dim = palette.dim;
        container(
            text(&self.status)
                .size(12.0)
                .style(move |_t| text::Style { color: Some(dim) }),
        )
        .padding([8.0, widgets::GAP])
        .into()
    }
}

// Free functions rather than methods or closures: `view` borrows from state, and
// only a fn item is higher-ranked enough over that lifetime to satisfy iced.
fn view(state: &Castty) -> Element<'_, Message> {
    state.view()
}

fn update(state: &mut Castty, message: Message) -> Task<Message> {
    state.update(message)
}

fn subscription(state: &Castty) -> Subscription<Message> {
    state.subscription()
}

/// Translate a keyboard event into a macro event, ignoring keys the device has
/// no code for rather than storing something meaningless.
///
/// Named keys are translated to the GDK keysym numbers `keycode::from_keyval`
/// already understands, so that table stays the single source of truth for
/// which keys the device can store -- this only has to know how iced spells
/// the same keys.
fn key_to_message(key: iced::keyboard::Key, pressed: bool) -> Option<Message> {
    use iced::keyboard::key::Named;
    use iced::keyboard::Key;

    let named = match key {
        Key::Character(ref c) => c.chars().next().map(|c| c as u32),
        Key::Named(Named::Space) => Some(0x0020),
        Key::Named(Named::Enter) => Some(0xff0d),
        Key::Named(Named::Tab) => Some(0xff09),
        Key::Named(Named::Backspace) => Some(0xff08),
        Key::Named(Named::Escape) => Some(0xff1b),
        Key::Named(Named::CapsLock) => Some(0xffe5),
        Key::Named(Named::PrintScreen) => Some(0xff61),
        Key::Named(Named::ScrollLock) => Some(0xff14),
        Key::Named(Named::Pause) => Some(0xff13),
        Key::Named(Named::Insert) => Some(0xff63),
        Key::Named(Named::Home) => Some(0xff50),
        Key::Named(Named::PageUp) => Some(0xff55),
        Key::Named(Named::Delete) => Some(0xffff),
        Key::Named(Named::End) => Some(0xff57),
        Key::Named(Named::PageDown) => Some(0xff56),
        Key::Named(Named::ArrowRight) => Some(0xff53),
        Key::Named(Named::ArrowLeft) => Some(0xff51),
        Key::Named(Named::ArrowDown) => Some(0xff54),
        Key::Named(Named::ArrowUp) => Some(0xff52),
        Key::Named(Named::F1) => Some(0xffbe),
        Key::Named(Named::F2) => Some(0xffbf),
        Key::Named(Named::F3) => Some(0xffc0),
        Key::Named(Named::F4) => Some(0xffc1),
        Key::Named(Named::F5) => Some(0xffc2),
        Key::Named(Named::F6) => Some(0xffc3),
        Key::Named(Named::F7) => Some(0xffc4),
        Key::Named(Named::F8) => Some(0xffc5),
        Key::Named(Named::F9) => Some(0xffc6),
        Key::Named(Named::F10) => Some(0xffc7),
        Key::Named(Named::F11) => Some(0xffc8),
        Key::Named(Named::F12) => Some(0xffc9),
        _ => None,
    }?;
    crate::hardware::keycode::from_keyval(named)
        .map(|usage| Message::Macros(pages::macros::Message::KeyPressed(usage, pressed)))
}

fn app_theme(state: &Castty) -> iced::Theme {
    state.palette().iced_theme(state.settings.theme.label())
}

pub fn run() -> iced::Result {
    iced::application(Castty::new, update, view)
        .title("Castty")
        .subscription(subscription)
        .theme(app_theme)
        .window_size((1040.0, 700.0))
        .run()
}
