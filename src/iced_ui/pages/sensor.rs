//! Sensor page: DPI steps, polling rate, angle settings, lift-off, and the
//! surface analyzer.
//!
//! X and Y DPI are genuinely independent on this device; the link toggle is
//! presentation only, because nothing on the mouse records whether they are
//! linked. It is inferred from whether the axes match.

use super::super::theme::Palette;
use super::super::widgets::{self, GAP};
use crate::hardware::{DpiStep, PollingRate, Profile};
use iced::widget::{button, column, pick_list, row, slider, text};
use iced::{Element, Length};

/// The vendor tool measures for a fixed window and reads the result itself.
const MEASURE_SECONDS: u8 = 10;
/// Every DPI value seen in capture was a multiple of 50.
const DPI_STEP: u16 = 50;
const DPI_MIN: u16 = 100;
const DPI_MAX: u16 = 10_000;

const RATES: [PollingRate; 4] = [
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
];

#[derive(Debug, Clone)]
pub enum Message {
    DpiChanged(usize, u16),
    DpiYChanged(usize, u16),
    LinkToggled(bool),
    PollingChanged(PollingRate),
    SnappingChanged(u8),
    TuningChanged(i32),
    LiftOffChanged(u8),
    AnalyzeStarted,
    Tick,
    SurfaceResult(u8),
}

pub struct State {
    pub dpi: [u16; 3],
    pub dpi_y: [u16; 3],
    pub linked: bool,
    pub polling: PollingRate,
    pub snapping: u8,
    /// i32 rather than i8: iced's slider requires `T: From<u8>` and i8 has
    /// no such impl. Narrowed to i8 on write.
    pub tuning: i32,
    pub lift_off: u8,
    pub surface: Option<u8>,
    pub countdown: Option<u8>,
}

impl State {
    pub fn from_profile(profile: &Profile) -> Self {
        State {
            dpi: [profile.dpi[0].x, profile.dpi[1].x, profile.dpi[2].x],
            dpi_y: [profile.dpi[0].y, profile.dpi[1].y, profile.dpi[2].y],
            linked: profile.dpi.iter().all(|s| s.x == s.y),
            polling: profile.polling.unwrap_or(PollingRate::Hz1000),
            snapping: profile.angle_snapping,
            tuning: i32::from(profile.angle_tuning),
            lift_off: profile.lift_off,
            surface: None,
            countdown: None,
        }
    }

    pub fn apply_to(&self, profile: &mut Profile) {
        for i in 0..3 {
            let x = self.dpi[i].clamp(DPI_MIN, DPI_MAX);
            let y = if self.linked { x } else { self.dpi_y[i].clamp(DPI_MIN, DPI_MAX) };
            profile.dpi[i] = DpiStep { x, y };
        }
        profile.polling = Some(self.polling);
        profile.angle_snapping = self.snapping.min(15);
        profile.angle_tuning = self.tuning.clamp(-30, 30) as i8;
        profile.lift_off = self.lift_off.clamp(1, 31);
    }

    /// Returns true when the caller should talk to the device: either to start
    /// a measurement, or to read one whose window has just closed.
    pub fn update(&mut self, message: Message) -> bool {
        match message {
            Message::DpiChanged(i, value) => {
                self.dpi[i] = value;
                if self.linked {
                    self.dpi_y[i] = value;
                }
            }
            Message::DpiYChanged(i, value) => self.dpi_y[i] = value,
            Message::LinkToggled(on) => {
                self.linked = on;
                if on {
                    self.dpi_y = self.dpi;
                }
            }
            Message::PollingChanged(rate) => self.polling = rate,
            Message::SnappingChanged(value) => self.snapping = value,
            Message::TuningChanged(value) => self.tuning = value,
            Message::LiftOffChanged(value) => self.lift_off = value,
            Message::AnalyzeStarted => {
                if self.countdown.is_some() {
                    return false;
                }
                self.surface = None;
                self.countdown = Some(MEASURE_SECONDS);
                return true;
            }
            Message::Tick => {
                if let Some(left) = self.countdown {
                    let left = left.saturating_sub(1);
                    if left == 0 {
                        self.countdown = None;
                        return true;
                    }
                    self.countdown = Some(left);
                }
            }
            Message::SurfaceResult(value) => {
                self.surface = Some(value);
                self.countdown = None;
            }
        }
        false
    }

    pub fn measuring(&self) -> bool {
        self.countdown.is_some()
    }
}

pub fn view<'a>(state: &'a State, palette: &Palette) -> Element<'a, Message> {
    let mut steps = column![].spacing(GAP);
    for i in 0..3 {
        let value = state.dpi[i];
        steps = steps.push(widgets::field(
            palette,
            match i {
                0 => "Step 1",
                1 => "Step 2",
                _ => "Step 3",
            },
            Some("The DPI button cycles between these three"),
            row![
                text(format!("{value}")).size(13.0),
                slider(DPI_MIN..=DPI_MAX, value, move |v| Message::DpiChanged(i, v)).step(DPI_STEP),
            ]
            .spacing(10.0)
            .align_y(iced::Alignment::Center)
            .width(Length::Fixed(260.0)),
        ));
        if !state.linked {
            let y = state.dpi_y[i];
            steps = steps.push(widgets::field(
                palette,
                "    Y axis",
                None::<&str>,
                row![
                    text(format!("{y}")).size(13.0),
                    slider(DPI_MIN..=DPI_MAX, y, move |v| Message::DpiYChanged(i, v)).step(DPI_STEP),
                ]
                .spacing(10.0)
                .align_y(iced::Alignment::Center)
                .width(Length::Fixed(260.0)),
            ));
        }
    }

    let link = widgets::segmented(
        palette,
        vec![
            ("Linked", Message::LinkToggled(true)),
            ("Separate X/Y", Message::LinkToggled(false)),
        ],
        usize::from(!state.linked),
    );

    let dpi_card = widgets::card(
        palette,
        "DPI",
        Some("Three steps, stored on the mouse"),
        column![link, steps].spacing(GAP),
    );

    let sensor_card = widgets::card(
        palette,
        "Sensor",
        None,
        column![
            widgets::field(
                palette,
                "Polling rate",
                Some("How often the mouse reports its position"),
                pick_list(
                    RATES.map(|r| format!("{} Hz", r.hz())).to_vec(),
                    Some(format!("{} Hz", state.polling.hz())),
                    |chosen| {
                        let hz: u16 = chosen.trim_end_matches(" Hz").parse().unwrap_or(1000);
                        Message::PollingChanged(
                            RATES.into_iter().find(|r| r.hz() == hz).unwrap_or(PollingRate::Hz1000),
                        )
                    },
                ),
            ),
            widgets::field(
                palette,
                "Angle snapping",
                Some("Straightens near-straight movement; 0 disables it"),
                row![
                    text(format!("{}", state.snapping)).size(13.0),
                    slider(0..=15u8, state.snapping, Message::SnappingChanged),
                ]
                .spacing(10.0)
                .align_y(iced::Alignment::Center)
                .width(Length::Fixed(220.0)),
            ),
            widgets::field(
                palette,
                "Angle tuning",
                Some("Rotates the sensor axis, in degrees"),
                row![
                    text(format!("{}", state.tuning)).size(13.0),
                    slider(-30..=30i32, state.tuning, Message::TuningChanged),
                ]
                .spacing(10.0)
                .align_y(iced::Alignment::Center)
                .width(Length::Fixed(220.0)),
            ),
            widgets::field(
                palette,
                "Lift-off distance",
                Some("How far the mouse can rise before it stops tracking"),
                row![
                    text(format!("{}", state.lift_off)).size(13.0),
                    slider(1..=31u8, state.lift_off, Message::LiftOffChanged),
                ]
                .spacing(10.0)
                .align_y(iced::Alignment::Center)
                .width(Length::Fixed(220.0)),
            ),
        ]
        .spacing(GAP),
    );

    let style = *palette;
    let reading = match (state.countdown, state.surface) {
        (Some(left), _) => format!("Move the mouse over the surface\u{2026} {left}"),
        (None, Some(0)) => "No reading. Did the mouse move?".to_string(),
        (None, Some(value)) => {
            let score = crate::hardware::surface_score(value);
            let verdict = if score >= 8 {
                "excellent"
            } else if score >= 6 {
                "good"
            } else if score >= 4 {
                "usable"
            } else {
                "poor tracking"
            };
            format!("{score} / 10 — {verdict}")
        }
        (None, None) => "Not measured yet".to_string(),
    };

    let surface_card = widgets::card(
        palette,
        "Surface analyzer",
        Some("Measures how well the sensor reads the surface under it"),
        widgets::field(
            palette,
            "Surface quality",
            Some(reading),
            button(text("Start").size(14.0))
                .padding([8.0, 18.0])
                .style(move |_t, status| widgets::subtle(&style, status))
                .on_press_maybe((!state.measuring()).then_some(Message::AnalyzeStarted)),
        ),
    );

    column![dpi_card, sensor_card, surface_card, widgets::spacer()]
        .spacing(GAP)
        .into()
}
