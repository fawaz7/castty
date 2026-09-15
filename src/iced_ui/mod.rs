//! The iced front end.
//!
//! Everything below `hardware`, `config` and `macros` is unchanged — those
//! layers never depended on a toolkit.

pub mod about;
pub mod art;
pub mod colour_picker;
pub mod lighting;
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
use widgets::GAP;

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length, Subscription, Task};
use std::rc::Rc;
use std::time::Instant;

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

    /// The preview only earns its space where the mouse itself is the subject.
    fn shows_preview(self) -> bool {
        matches!(self, Page::Lighting | Page::Buttons)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    PageSelected(Page),
    ProfileSelected(usize),
    Apply,
    Device(worker::Update),
    Lighting(lighting::Message),
    About(about::Message),
    Tick,
}

pub struct Castty {
    profiles: Vec<Profile>,
    current: usize,
    #[allow(dead_code)]
    library: Library,
    page: Page,
    dirty: bool,
    status: String,
    settings: Settings,
    worker: worker::Handle,
    art: Rc<art::Art>,
    lighting: lighting::State,
    started: Instant,
}

impl Castty {
    fn new() -> (Self, Task<Message>) {
        let (worker, _) = worker::spawn();
        worker.send(worker::Job::Connect);
        let profiles = config::load_all();
        let lighting = lighting::State::from_profile(&profiles[0]);
        (
            Castty {
                lighting,
                art: Rc::new(art::Art::load()),
                started: Instant::now(),
                profiles,
                current: 0,
                library: Library::load(),
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
        self.settings.theme.palette()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PageSelected(page) => self.page = page,
            Message::ProfileSelected(index) => {
                if index < self.profiles.len() {
                    self.current = index;
                    self.lighting = lighting::State::from_profile(&self.profiles[index]);
                }
            }
            Message::Lighting(message) => {
                self.lighting.update(message);
                self.dirty = true;
            }
            Message::About(about::Message::ThemeChanged(named)) => {
                self.settings.theme = named;
                self.settings.save();
            }
            Message::About(about::Message::OpenGithub) => {
                // Best effort; a missing opener is not worth an error dialog.
                let _ = std::process::Command::new("xdg-open")
                    .arg(about::GITHUB)
                    .spawn();
            }
            Message::Apply => {
                self.lighting.apply_to(&mut self.profiles[self.current]);
                let profile = self.profiles[self.current].clone();
                self.worker.send(worker::Job::WriteProfile(Box::new(profile)));
                self.dirty = false;
            }
            Message::Tick => {}
            Message::Device(update) => match update {
                worker::Update::Connected(id) => {
                    self.status = format!("Connected — firmware {:x}.{:02x}", id.firmware >> 8, id.firmware & 0xff);
                }
                worker::Update::Disconnected(why) => self.status = why,
                worker::Update::Applied => {
                    let _ = config::save(self.current, &self.profiles[self.current]);
                    self.status = "Saved to the mouse".into();
                }
                worker::Update::Surface(value) => {
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
        if self.page.shows_preview() && self.lighting.animated() {
            Subscription::batch([
                device,
                iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::Tick),
            ])
        } else {
            device
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let palette = self.palette();

        let content: Element<'_, Message> = match self.page {
            Page::Lighting => lighting::view(&self.lighting, &palette).map(Message::Lighting),
            Page::About => about::view(self.settings.theme, &palette).map(Message::About),
            other => widgets::card(
                &palette,
                other.label(),
                Some("Coming next"),
                text("").size(1.0),
            ),
        };

        let page_body: Element<'_, Message> = if self.page.shows_preview() {
            let seconds = self.started.elapsed().as_secs_f32();
            let (wheel, logo) = self.lighting.lit(seconds);
            let stage_style = palette;
            let stage = container(
                iced::widget::canvas(preview::Preview {
                    art: self.art.clone(),
                    wheel,
                    logo,
                    show_buttons: self.page == Page::Buttons,
                    palette,
                })
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .width(Length::FillPortion(4))
            .height(Length::Fill)
            .style(move |_t| stage_style.stage());

            row![stage, container(content).width(Length::FillPortion(5))]
                .spacing(GAP)
                .height(Length::Fill)
                .into()
        } else {
            content
        };

        let scrolled = scrollable(container(page_body).padding(GAP)).height(Length::Fill);

        row![self.sidebar(&palette), column![self.header(&palette), scrolled, self.footer(&palette)]]
            .height(Length::Fill)
            .into()
    }

    fn sidebar(&self, palette: &Palette) -> Element<'_, Message> {
        let style = *palette;
        let items = Page::ALL.iter().fold(column![].spacing(4), |acc, page| {
            let selected = *page == self.page;
            acc.push(
                button(text(page.label()).size(14.0))
                    .width(Length::Fill)
                    .padding([9, 14])
                    .style(move |_t, status| nav_style(&style, status, selected))
                    .on_press(Message::PageSelected(*page)),
            )
        });

        container(
            column![
                container(text("castty").size(19.0)).padding([4, 14]),
                Space::new().height(12),
                items,
                widgets::spacer(),
            ]
            .spacing(2)
            .padding(12),
        )
        .width(Length::Fixed(190.0))
        .height(Length::Fill)
        .style(move |_t| style.sidebar())
        .into()
    }

    fn header(&self, palette: &Palette) -> Element<'_, Message> {
        let style = *palette;
        let dim = palette.dim;
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
                text("Profile").size(13.0).style(move |_t| text::Style { color: Some(dim) }),
                iced::widget::pick_list(names.clone(), selected, move |chosen| {
                    let index = names.iter().position(|n| *n == chosen).unwrap_or(0);
                    Message::ProfileSelected(index)
                }),
                widgets::spacer(),
                button(text("Apply").size(14.0))
                    .padding([9, 22])
                    .style(move |_t, status| widgets::primary(&style, status))
                    .on_press_maybe(self.dirty.then_some(Message::Apply)),
            ]
            .align_y(iced::Alignment::Center)
            .spacing(12),
        )
        .padding([12.0, GAP])
        .into()
    }

    fn footer(&self, palette: &Palette) -> Element<'_, Message> {
        let dim = palette.dim;
        container(
            text(&self.status)
                .size(12.0)
                .style(move |_t| text::Style { color: Some(dim) }),
        )
        .padding([8.0, GAP])
        .into()
    }
}

fn nav_style(palette: &Palette, status: button::Status, selected: bool) -> button::Style {
    let background = if selected {
        palette.surface
    } else if matches!(status, button::Status::Hovered) {
        palette.raised
    } else {
        iced::Color::TRANSPARENT
    };
    button::Style {
        background: Some(iced::Background::Color(background)),
        text_color: if selected { palette.text } else { palette.dim },
        border: iced::Border { radius: 9.0.into(), ..Default::default() },
        ..Default::default()
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
