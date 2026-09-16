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
    Profiles(pages::profiles::Message),
    About(pages::about::Message),
    Tick,
}

pub struct Castty {
    profiles: Vec<Profile>,
    current: usize,
    library: Library,
    page: Page,
    /// Per profile, not one flag for the lot: a rename or a restore can dirty
    /// a slot other than `current`, and Apply has to write all of them, not
    /// just whichever one the widgets on screen belong to.
    dirty: [bool; config::PROFILE_COUNT],
    /// The slot the device was last told (successfully) to make active, or
    /// `None` before the first Apply this run. Selecting a different profile
    /// sets no dirty flag of its own, so without this Apply would have
    /// nothing to notice a plain switch by -- the commit frame is the
    /// device's only profile-select mechanism, and this is what tells Apply
    /// a commit is owed even with nothing to write.
    device_active: Option<usize>,
    status: String,
    settings: Settings,
    worker: worker::Handle,
    art: Rc<art::Art>,
    lighting: pages::lighting::State,
    sensor: pages::sensor::State,
    buttons: pages::buttons::State,
    macros: pages::macros::State,
    profiles_page: pages::profiles::State,
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
                profiles_page: pages::profiles::State::default(),
                art: Rc::new(art::Art::load()),
                started: Instant::now(),
                profiles,
                current: 0,
                library,
                page: Page::Lighting,
                dirty: [false; config::PROFILE_COUNT],
                // There is no read path, so which profile the device is
                // actually sitting on is unknown at launch -- `None`, not a
                // guessed slot, is the honest seed. It costs one commit the
                // first time Apply runs (always enabled, always sent), which
                // is the right trade against silently agreeing with a guess
                // that might be wrong and leaving the mouse on some other
                // profile with no way back short of a detour through
                // selecting elsewhere and back.
                device_active: None,
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

    /// Flush the on-screen widgets into the profile they belong to -- the
    /// same sequence `Message::Apply` runs, minus the device write. Both an
    /// Apply and a switch away from the current profile need the in-memory
    /// profile to actually hold what is on screen before doing anything else
    /// with it; leaving this to `Apply` alone meant switching profiles first
    /// silently discarded whatever the widgets held.
    fn flush_current(&mut self) -> Result<(), crate::hardware::profile::ValueError> {
        self.lighting.apply_to(&mut self.profiles[self.current]);
        self.sensor.apply_to(&mut self.profiles[self.current]);
        self.buttons.apply_to(&mut self.profiles[self.current], &self.library)
    }

    /// What to call a slot in status text and the top-bar picker alike, so
    /// the two cannot describe the same profile differently.
    fn profile_label(&self, index: usize) -> String {
        match self.profiles.get(index) {
            Some(p) if !p.name.trim().is_empty() => p.name.clone(),
            _ => format!("Profile {}", index + 1),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PageSelected(page) => {
                // The subscription that feeds the recorder stays live on
                // every other page too; leaving Macros mid-recording must
                // stop it, since there is no reachable Stop button once the
                // page is gone.
                self.macros.recording = false;
                // Leaving Profiles with the restore armed must not leave it
                // primed for a stray click on return.
                self.profiles_page.disarm();
                self.page = page;
            }
            Message::ProfileSelected(index) => {
                if index < self.profiles.len() {
                    // Selecting a profile is never the confirming click the
                    // restore prompt is waiting for, whether or not the
                    // switch itself goes on to succeed.
                    self.profiles_page.disarm();
                    // Flush the outgoing profile's widgets into it before
                    // switching -- otherwise the edit vanishes the moment the
                    // widgets are rebuilt from the profile being switched to,
                    // while its dirty flag stays set and Apply later writes
                    // the unedited profile to flash for nothing.
                    match self.flush_current() {
                        Ok(()) => {
                            self.current = index;
                            self.lighting = pages::lighting::State::from_profile(&self.profiles[index]);
                            self.sensor = pages::sensor::State::from_profile(&self.profiles[index]);
                            self.buttons = pages::buttons::State::from_profile(
                                &self.profiles[index],
                                &self.library,
                            );
                        }
                        // The macro area on the outgoing profile is full;
                        // switching now would discard the edit that
                        // overflowed it, so stay on it instead.
                        Err(error) => self.status = format!("Not saved: {error}"),
                    }
                }
            }
            Message::Lighting(message) => {
                self.lighting.update(message);
                self.dirty[self.current] = true;
            }
            Message::Buttons(message) => {
                self.buttons.update(message);
                self.dirty[self.current] = true;
            }
            Message::Macros(message) => {
                if self.macros.update(message, &mut self.library) {
                    let _ = self.library.save();
                    // Re-resolve only the slots the change actually touches,
                    // rather than rebuilding from the profile: that would
                    // discard button edits the user made but has not applied
                    // yet, while `dirty` stayed true and the next Apply wrote
                    // the reverted assignment to flash.
                    if let Some(change) = self.macros.last_change.take() {
                        self.buttons.sync(&change);
                    }
                }
            }
            Message::Profiles(msg) => {
                // Decides arm/disarm/fire; the page-specific effects below
                // still need to run for `NameChanged` and `Select` regardless
                // of what this returns.
                let restore_now = self.profiles_page.update(&msg);
                match msg {
                    pages::profiles::Message::NameChanged(index, name) => {
                        pages::profiles::rename(&mut self.profiles, index, &name);
                        if index < self.profiles.len() {
                            self.dirty[index] = true;
                        }
                    }
                    pages::profiles::Message::Select(index) => {
                        return self.update(Message::ProfileSelected(index));
                    }
                    pages::profiles::Message::RestoreRequested
                    | pages::profiles::Message::RestoreCancelled => {}
                    pages::profiles::Message::RestoreConfirmed => {
                        if restore_now {
                            pages::profiles::restore_defaults(&mut self.profiles, &mut self.dirty);
                            self.lighting =
                                pages::lighting::State::from_profile(&self.profiles[self.current]);
                            self.sensor =
                                pages::sensor::State::from_profile(&self.profiles[self.current]);
                            self.buttons = pages::buttons::State::from_profile(
                                &self.profiles[self.current],
                                &self.library,
                            );
                        }
                    }
                }
            }
            Message::Sensor(message) => {
                let wants_device = self.sensor.update(message);
                self.dirty[self.current] = true;
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
            Message::Apply => match self.flush_current() {
                Ok(()) => {
                    // Every dirty profile, not only the active one -- a
                    // rename on a slot you are not editing, or a restore,
                    // must not be silently dropped on the next Apply. `dirty`
                    // itself is left alone here and only cleared per index as
                    // each write is confirmed (`Update::Applied`), so a write
                    // that fails partway through a batch leaves the rest
                    // marked dirty and retryable instead of silently dropped.
                    let profiles = pages::profiles::dirty_profiles(&self.profiles, &self.dirty);
                    if !profiles.is_empty() {
                        self.worker.send(worker::Job::WriteProfiles {
                            profiles,
                            active: self.current as u8,
                        });
                    } else if self.device_active != Some(self.current) {
                        // Selecting a profile sets no dirty flag of its own,
                        // so a switch with nothing else changed still has to
                        // reach the device -- the commit frame is the only
                        // way the mouse ever finds out which profile is live.
                        self.worker.send(worker::Job::Switch { active: self.current as u8 });
                    }
                    // Otherwise there is nothing to do: Apply is disabled in
                    // that case, so this arm exists only as a defensive
                    // no-op rather than sending a meaningless job.
                }
                // The macro area is full; nothing is sent to the mouse, so
                // the write is not silently reported as saved.
                Err(error) => self.status = format!("Not saved: {error}"),
            },
            Message::Tick => {}
            Message::Device(update) => match update {
                worker::Update::Connected(id) => {
                    self.status = format!("Connected, firmware {:x}.{:02x}", id.firmware >> 8, id.firmware & 0xff);
                }
                worker::Update::Disconnected(why) => self.status = why,
                worker::Update::Applied { indices, active } => {
                    // Persist exactly the slots the worker actually wrote --
                    // not whichever profile happens to be `current` by the
                    // time this reply lands, which may have changed since
                    // Apply was pressed -- and only now clear their dirty
                    // flags, so a write that failed partway through (a
                    // disconnect mid-Apply) leaves the rest of the batch
                    // dirty and retryable rather than silently dropped.
                    for &index in &indices {
                        let _ = config::save(index, &self.profiles[index]);
                        if let Some(flag) = self.dirty.get_mut(index) {
                            *flag = false;
                        }
                    }
                    // An empty `indices` means `write_profiles`' own
                    // empty-batch guard fired: no commit was issued, so
                    // `device_active` must not move either, or this would
                    // record a switch that never reached the device. Not
                    // reachable through the UI today -- `WriteProfiles` is
                    // only ever sent non-empty; a switch-only Apply goes
                    // through `Job::Switch` instead.
                    if !indices.is_empty() {
                        self.device_active = Some(active);
                        self.status = "Saved to the mouse".into();
                    }
                }
                worker::Update::Switched(active) => {
                    self.device_active = Some(active);
                    self.status = format!("Now using {}", self.profile_label(active));
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

        let names: Vec<String> =
            (0..self.profiles.len()).map(|i| self.profile_label(i)).collect();
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
                    .on_press_maybe(
                        can_apply(&self.dirty, self.current, self.device_active)
                            .then_some(Message::Apply),
                    ),
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
            Page::Profiles => pages::profiles::view(
                &self.profiles,
                self.current,
                self.profiles_page.confirming(),
                &palette,
            )
            .map(Message::Profiles),
            Page::About => {
                pages::about::view(self.settings.theme, self.settings.accent, &palette)
                    .map(Message::About)
            }
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

/// Whether Apply has anything to do: a dirty profile to write, or a
/// selection that has not yet reached the device. `device_active` being
/// `None` (nothing committed yet this run) must count as "differs", not
/// "matches" -- otherwise the very first Apply after launch could be
/// skipped, leaving the mouse on whatever profile it already happened to be
/// sitting on, which this app has no way to read and confirm.
fn can_apply(dirty: &[bool], current: usize, device_active: Option<usize>) -> bool {
    dirty.iter().any(|d| *d) || device_active != Some(current)
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

// `Castty`'s fields and `update` are private, so exercising the Apply/switch
// wiring means testing from inside this module rather than from
// `tests/pages.rs` -- an external test cannot construct a `Castty` at all,
// let alone reach into `dirty` or `profiles_page` to check it. A prior round
// covered the pure helpers (`dirty_profiles`, `restore_defaults`,
// `profiles::State`) from outside; this covers the wiring that calls them,
// which turned out to matter -- reverting the fixes those helpers exist for
// left the outside tests green.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::MacroEvent;
    use crate::macros::{NamedMacro, Timing};
    use std::sync::mpsc;

    fn test_profiles() -> Vec<Profile> {
        (0..config::PROFILE_COUNT).map(config::factory_default).collect()
    }

    /// A `Castty` wired to a plain channel instead of a live worker thread,
    /// so a test can inspect the `Job` sent by `update` without touching
    /// real hardware, and without `Castty::new`'s side effects (spawning the
    /// worker thread, reading the real settings and macro library from disk).
    fn test_app() -> (Castty, mpsc::Receiver<worker::Job>) {
        let profiles = test_profiles();
        let library = Library::default();
        let lighting = pages::lighting::State::from_profile(&profiles[0]);
        let sensor = pages::sensor::State::from_profile(&profiles[0]);
        let buttons = pages::buttons::State::from_profile(&profiles[0], &library);
        let (tx, rx) = mpsc::channel();
        let app = Castty {
            lighting,
            sensor,
            buttons,
            macros: pages::macros::State::default(),
            profiles_page: pages::profiles::State::default(),
            art: Rc::new(art::Art::load()),
            started: Instant::now(),
            profiles,
            current: 0,
            library,
            page: Page::Lighting,
            dirty: [false; config::PROFILE_COUNT],
            device_active: None,
            status: String::new(),
            settings: Settings::default(),
            worker: worker::Handle::for_test(tx),
        };
        (app, rx)
    }

    /// The bug fix-round-1 existed for: Apply must send every dirty profile,
    /// not only whichever one is on screen.
    #[test]
    fn apply_sends_every_dirty_profile_not_only_the_active_one() {
        let (mut app, jobs) = test_app();
        app.current = 2;
        app.dirty[1] = true;
        app.dirty[3] = true;

        let _ = app.update(Message::Apply);

        let job = jobs.try_recv().expect("Apply must send a job");
        let worker::Job::WriteProfiles { profiles, active } = job else {
            panic!("expected WriteProfiles");
        };
        let indices: Vec<usize> = profiles.iter().map(|(i, _)| *i).collect();
        assert_eq!(indices, vec![1, 3], "only the dirty slots, not slot 0");
        assert_eq!(active, 2, "the mouse must end up switched to the current profile");
    }

    /// Selecting a profile sets no dirty flag of its own, so with nothing
    /// else edited Apply has nothing in `dirty` to notice -- but the commit
    /// frame is the device's only profile-select mechanism, so the switch
    /// still has to reach it. This must go out as `Job::Switch`, not a
    /// `WriteProfiles` with an empty batch: the two look the same to the
    /// UI's "is anything dirty" check, but only one of them is allowed to
    /// touch the device with nothing to write.
    #[test]
    fn selecting_a_profile_with_nothing_else_dirty_sends_a_switch_only_job() {
        let (mut app, jobs) = test_app();
        let _ = app.update(Message::ProfileSelected(4));

        let _ = app.update(Message::Apply);

        let job = jobs.try_recv().expect("Apply must send a job when only the selection changed");
        match job {
            worker::Job::Switch { active } => assert_eq!(active, 4),
            other => panic!("expected a switch-only job, not {other:?}"),
        }
    }

    /// With nothing edited and the selection already matching what was last
    /// committed, Apply has genuinely nothing to do -- the button is
    /// disabled in that state, but the handler itself must not send a job
    /// just because it was asked to. `device_active` has to be `Some` here:
    /// the freshly-seeded `None` from `test_app` counts as "differs" (see
    /// the `can_apply` tests below), so this test sets it explicitly to
    /// simulate a run where an Apply has already landed once.
    #[test]
    fn apply_with_nothing_to_do_sends_no_job() {
        let (mut app, jobs) = test_app();
        app.device_active = Some(app.current);

        let _ = app.update(Message::Apply);

        assert!(jobs.try_recv().is_err(), "nothing changed, so nothing should be sent");
    }

    /// `Update::Applied` must persist exactly the indices it is handed, not
    /// whatever is `current` when the reply lands -- and only then clear
    /// their dirty flags, so a failed write elsewhere in the batch stays
    /// retryable.
    #[test]
    fn applied_persists_exactly_the_indices_it_was_handed() {
        // config::save resolves its path through XDG_CONFIG_HOME; point it
        // at a throwaway directory for this one test so it cannot touch the
        // developer's real saved profiles.
        let dir = std::env::temp_dir().join(format!("castty-test-{}", std::process::id()));
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: this test does not run concurrently with any other test
        // that reads or writes XDG_CONFIG_HOME -- it is the only one in this
        // binary that touches it, and the previous value is restored below.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };

        let (mut app, _jobs) = test_app();
        app.current = 0;
        app.dirty[1] = true;
        app.dirty[3] = true;

        let _ = app.update(Message::Device(worker::Update::Applied {
            indices: vec![1, 3],
            active: 2,
        }));

        assert!(config::state_path(1).exists());
        assert!(config::state_path(3).exists());
        assert!(!config::state_path(0).exists(), "only the handed indices are persisted");
        assert!(!app.dirty[1], "a confirmed write clears its own dirty flag");
        assert!(!app.dirty[3]);
        assert_eq!(app.device_active, Some(2), "the committed slot, not whatever `current` is now");

        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: see above.
        unsafe {
            match previous {
                Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }

    /// `write_profiles`' empty-batch guard means an empty `Applied` reply
    /// never actually reached a commit -- `device_active` recording a
    /// switch that never happened would put it out of step with the real
    /// device on the strength of a job that touched nothing.
    #[test]
    fn an_empty_applied_reply_leaves_device_active_unchanged() {
        let (mut app, _jobs) = test_app();
        app.device_active = Some(0);

        let _ = app.update(Message::Device(worker::Update::Applied { indices: vec![], active: 4 }));

        assert_eq!(app.device_active, Some(0));
    }

    /// A write that fails must not have already thrown away the chance to
    /// retry -- `dirty` is cleared per index in `Update::Applied`, never on
    /// send, so a `Disconnected` reply (or simply never replying) leaves the
    /// edit retryable rather than silently lost.
    #[test]
    fn dirty_survives_a_send_until_the_write_is_confirmed() {
        let (mut app, _jobs) = test_app();
        app.dirty[1] = true;

        let _ = app.update(Message::Apply);

        assert!(app.dirty[1], "dirty must not clear just because a job was sent");
    }

    /// Switching profiles must not throw away an edit that has not been
    /// applied yet -- it has to land in the outgoing profile first, exactly
    /// as pressing Apply would, or it vanishes the moment the widgets are
    /// rebuilt from the profile being switched to.
    #[test]
    fn switching_profiles_flushes_pending_edits_into_the_outgoing_profile() {
        let (mut app, _jobs) = test_app();
        let _ = app.update(Message::Lighting(pages::lighting::Message::ModeChanged(
            pages::lighting::Mode::Off,
        )));

        let _ = app.update(Message::ProfileSelected(1));

        let wheel = app.profiles[0].wheel();
        assert_eq!(
            (wheel.r, wheel.g, wheel.b),
            (0, 0, 0),
            "the edit must survive the switch instead of vanishing with the widgets"
        );
        assert_eq!(app.current, 1);
    }

    /// If flushing the outgoing profile fails (the macro area overflows),
    /// the switch must not happen either -- otherwise the edit that
    /// overflowed it is lost and the user is looking at a different profile
    /// with no idea why nothing landed.
    #[test]
    fn a_macro_capacity_error_on_switch_keeps_the_user_on_the_current_profile() {
        let (mut app, _jobs) = test_app();
        let events: Vec<MacroEvent> = (0..33)
            .map(|i| MacroEvent { key: 0x04, pressed: i % 2 == 0, delay_ms: 0 })
            .collect();
        app.library.put(NamedMacro { name: "Huge".into(), timing: Timing::None, events });
        app.buttons.slots[0] = pages::buttons::Slot::Library("Huge".into());

        let _ = app.update(Message::ProfileSelected(1));

        assert_eq!(app.current, 0, "must not switch away from an edit that failed to flush");
        assert!(app.status.starts_with("Not saved"));
    }

    /// Selecting a profile is not the confirming click the restore prompt is
    /// waiting for -- it must disarm a pending restore, the same way leaving
    /// the Profiles page does.
    #[test]
    fn selecting_a_profile_disarms_a_pending_restore() {
        let (mut app, _jobs) = test_app();
        let _ = app.update(Message::Profiles(pages::profiles::Message::RestoreRequested));
        assert!(app.profiles_page.confirming());

        let _ = app.update(Message::ProfileSelected(1));

        assert!(!app.profiles_page.confirming());
    }

    /// The disarm must not depend on the switch actually succeeding -- an
    /// aborted switch (the macro-capacity error above) still has to clear a
    /// pending restore, or the prompt is left armed by an action that looks
    /// to the user like it did nothing.
    #[test]
    fn selecting_a_profile_disarms_a_pending_restore_even_when_the_switch_is_refused() {
        let (mut app, _jobs) = test_app();
        let events: Vec<MacroEvent> = (0..33)
            .map(|i| MacroEvent { key: 0x04, pressed: i % 2 == 0, delay_ms: 0 })
            .collect();
        app.library.put(NamedMacro { name: "Huge".into(), timing: Timing::None, events });
        app.buttons.slots[0] = pages::buttons::Slot::Library("Huge".into());
        let _ = app.update(Message::Profiles(pages::profiles::Message::RestoreRequested));
        assert!(app.profiles_page.confirming());

        let _ = app.update(Message::ProfileSelected(1));

        assert_eq!(app.current, 0, "sanity: the switch must indeed have been refused");
        assert!(!app.profiles_page.confirming(), "a refused switch must still disarm");
    }

    /// `Update::Switched` is the reply to a switch-only Apply: nothing was
    /// written, but the device now knows which profile is live, and the
    /// status has to say so rather than claim a save that did not happen.
    #[test]
    fn update_switched_records_the_new_device_active_slot() {
        let (mut app, _jobs) = test_app();

        let _ = app.update(Message::Device(worker::Update::Switched(3)));

        assert_eq!(app.device_active, Some(3));
        assert!(!app.status.to_lowercase().contains("saved"));
    }

    // `can_apply` is the entire user-visible half of the switch-only-Apply
    // work: it is what actually enables the button in `view()`, which
    // nothing else here exercises. All four combinations of (nothing dirty
    // / something dirty) x (selection matches device_active / differs),
    // plus the `None`-seed case fix round 4 exists for.

    #[test]
    fn can_apply_is_false_when_nothing_dirty_and_selection_matches_device_active() {
        let dirty = [false; config::PROFILE_COUNT];
        assert!(!can_apply(&dirty, 1, Some(1)));
    }

    #[test]
    fn can_apply_is_true_when_something_dirty_even_if_selection_matches_device_active() {
        let mut dirty = [false; config::PROFILE_COUNT];
        dirty[0] = true;
        assert!(can_apply(&dirty, 1, Some(1)));
    }

    #[test]
    fn can_apply_is_true_when_selection_differs_even_with_nothing_dirty() {
        let dirty = [false; config::PROFILE_COUNT];
        assert!(can_apply(&dirty, 2, Some(1)));
    }

    #[test]
    fn can_apply_is_true_when_both_dirty_and_selection_differ() {
        let mut dirty = [false; config::PROFILE_COUNT];
        dirty[0] = true;
        assert!(can_apply(&dirty, 2, Some(1)));
    }

    /// The `None` seed (nothing committed yet this run) must count as
    /// "differs", not "matches" -- even when the untouched default
    /// `current == 0` would look equal to a wrongly-chosen `Some(0)` seed.
    #[test]
    fn can_apply_is_true_with_nothing_dirty_when_device_active_is_still_unknown() {
        let dirty = [false; config::PROFILE_COUNT];
        assert!(can_apply(&dirty, 0, None));
    }
}
