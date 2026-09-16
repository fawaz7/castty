//! Lighting page state and view.
//!
//! The two LEDs take independent colours but share one effect, because the
//! hardware reads a single mode byte — verified by writing different modes to
//! each and watching both follow the first.

use super::super::colour_picker::{self, ColourPicker};
use super::super::theme::Palette;
use crate::hardware::{Effect, LedMode, Profile, EFFECTS};
use super::super::widgets::{self, size, GAP, STACK};
use iced::widget::{canvas, checkbox, column, pick_list, row};
use iced::{Element, Length};

/// Measured on hardware against a stopwatch, not guessed. Re-measure rather
/// than rounding these off.
const BLINK_S: f32 = 1.154;
const PULSE_S: f32 = 1.58;
const PULSE_BEAT_S: f32 = 0.18;
const PULSE_GAP_S: f32 = 0.12;
const PULSE_DARK: f32 = 0.36;
const BREATHE_S: f32 = 6.0;
const RAINBOW_S: f32 = 5.0;

/// Tall enough to pick a colour precisely, short enough that the Colour and
/// Effect cards both fit beside the hero at the default window height.
const PICKER_HEIGHT: f32 = 170.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Unified,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Wheel,
    Logo,
}

#[derive(Debug, Clone)]
pub enum Message {
    ModeChanged(Mode),
    TargetChanged(Target),
    ColourChanged((f32, f32, f32)),
    EffectChanged(Effect),
    RainbowToggled(bool),
}

pub struct State {
    pub mode: Mode,
    pub target: Target,
    pub hsv: (f32, f32, f32),
    pub wheel: (u8, u8, u8),
    pub logo: (u8, u8, u8),
    pub effect: Effect,
    pub rainbow: bool,
}

impl State {
    pub fn from_profile(profile: &Profile) -> Self {
        let w = profile.wheel();
        let l = profile.logo();
        let wheel = (w.r, w.g, w.b);
        let logo = (l.r, l.g, l.b);
        let mode = if wheel == (0, 0, 0) && logo == (0, 0, 0) {
            Mode::Off
        } else if wheel == logo {
            Mode::Unified
        } else {
            Mode::Split
        };
        let led_mode = profile.leds[0].mode;
        State {
            mode,
            target: Target::Wheel,
            hsv: colour_picker::from_rgb(wheel),
            wheel,
            logo,
            effect: led_mode.effect(),
            rainbow: led_mode.rainbow(),
        }
    }

    pub fn apply_to(&self, profile: &mut Profile) {
        let (wheel, logo) = match self.mode {
            Mode::Off => ((0, 0, 0), (0, 0, 0)),
            Mode::Unified => {
                let c = colour_picker::to_rgb(self.hsv);
                (c, c)
            }
            Mode::Split => (self.wheel, self.logo),
        };
        profile.set_wheel_colour(wheel.0, wheel.1, wheel.2);
        profile.set_logo_colour(logo.0, logo.1, logo.2);
        // The four non-physical records are inert on this device, but the
        // vendor keeps them in step with the wheel, so we do too.
        for led in profile.leds.iter_mut().skip(2) {
            (led.r, led.g, led.b) = wheel;
        }
        profile.set_mode(LedMode::new(self.effect, self.rainbow));
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::ModeChanged(mode) => {
                self.mode = mode;
                if mode == Mode::Split {
                    self.hsv = colour_picker::from_rgb(self.current());
                }
            }
            Message::TargetChanged(target) => {
                self.target = target;
                self.hsv = colour_picker::from_rgb(self.current());
            }
            Message::ColourChanged(hsv) => {
                self.hsv = hsv;
                let rgb = colour_picker::to_rgb(hsv);
                match self.mode {
                    Mode::Off => {}
                    Mode::Unified => {
                        self.wheel = rgb;
                        self.logo = rgb;
                    }
                    Mode::Split => match self.target {
                        Target::Wheel => self.wheel = rgb,
                        Target::Logo => self.logo = rgb,
                    },
                }
            }
            Message::EffectChanged(effect) => self.effect = effect,
            Message::RainbowToggled(on) => self.rainbow = on,
        }
    }

    fn current(&self) -> (u8, u8, u8) {
        match self.target {
            Target::Wheel => self.wheel,
            Target::Logo => self.logo,
        }
    }

    /// Whether the preview needs repainting continuously.
    pub fn animated(&self) -> bool {
        self.rainbow || self.effect != Effect::Solid
    }

    /// Colours as the mouse would show them at this instant.
    pub fn lit(&self, seconds: f32) -> ((u8, u8, u8), (u8, u8, u8)) {
        let (wheel, logo) = match self.mode {
            Mode::Off => return ((0, 0, 0), (0, 0, 0)),
            Mode::Unified => {
                let c = colour_picker::to_rgb(self.hsv);
                (c, c)
            }
            Mode::Split => (self.wheel, self.logo),
        };
        let brightness = match self.effect {
            Effect::Solid => 1.0,
            Effect::Blinking => {
                if (seconds / BLINK_S).fract() < 0.5 {
                    1.0
                } else {
                    0.05
                }
            }
            // Measured shape: two quick blackouts, then held lit -- a heartbeat.
            Effect::Pulsating => {
                let phase = (seconds / PULSE_S).fract() * PULSE_S;
                beat(phase).or_else(|| beat(phase - PULSE_BEAT_S - PULSE_GAP_S)).unwrap_or(1.0)
            }
            Effect::Breathing => {
                let p = (seconds / BREATHE_S * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                0.08 + 0.92 * p * p
            }
        };
        let hue = self.rainbow.then(|| (seconds / RAINBOW_S * 360.0) % 360.0);
        (shade(wheel, brightness, hue), shade(logo, brightness, hue))
    }
}

fn beat(phase: f32) -> Option<f32> {
    if !(0.0..PULSE_BEAT_S).contains(&phase) {
        return None;
    }
    let x = phase / PULSE_BEAT_S;
    let edge = (1.0 - PULSE_DARK) / 2.0;
    Some(if x < edge {
        1.0 - x / edge
    } else if x < 1.0 - edge {
        0.0
    } else {
        (x - (1.0 - edge)) / edge
    })
}

fn shade(rgb: (u8, u8, u8), brightness: f32, hue: Option<f32>) -> (u8, u8, u8) {
    let (h, s, v) = super::super::art::rgb_to_hsv(rgb.0, rgb.1, rgb.2);
    match hue {
        // Rainbow cycles regardless of the stored colour, but an LED set to
        // black stays off.
        Some(cycled) if v > 0.0 => super::super::art::hsv_to_rgb(cycled, 1.0, brightness),
        Some(_) => (0, 0, 0),
        None => super::super::art::hsv_to_rgb(h, s, v * brightness),
    }
}

pub fn view<'a>(state: &'a State, palette: &Palette) -> Element<'a, Message> {
    let usable = state.mode != Mode::Off && !state.rainbow;

    let selector = widgets::segmented(
        palette,
        vec![
            ("Off", Message::ModeChanged(Mode::Off)),
            ("Unified", Message::ModeChanged(Mode::Unified)),
            ("Split", Message::ModeChanged(Mode::Split)),
        ],
        match state.mode {
            Mode::Off => 0,
            Mode::Unified => 1,
            Mode::Split => 2,
        },
    );

    // The mode selector, the LED selector (Split only) and the hex readout
    // share one line so the picker below gets the room, and the whole page
    // fits the default window without scrolling.
    let mut selectors = row![selector].spacing(GAP).align_y(iced::Alignment::Center);
    if state.mode == Mode::Split {
        selectors = selectors.push(widgets::segmented(
            palette,
            vec![
                ("Scroll wheel", Message::TargetChanged(Target::Wheel)),
                ("Logo", Message::TargetChanged(Target::Logo)),
            ],
            usize::from(state.target == Target::Logo),
        ));
    }
    let (r, g, b) = colour_picker::to_rgb(state.hsv);
    selectors = selectors
        .push(widgets::push_right())
        .push(widgets::muted(palette, format!("#{r:02x}{g:02x}{b:02x}"), size::LABEL));

    let colour_body = column![
        selectors,
        Element::from(
            canvas(ColourPicker {
                hsv: state.hsv,
                enabled: usable,
                palette: *palette,
            })
            .width(Length::Fill)
            .height(Length::Fixed(PICKER_HEIGHT)),
        )
        .map(Message::ColourChanged),
    ]
    .spacing(GAP);

    let effect_body = column![
        widgets::field(
            palette,
            "Animation",
            None::<&str>,
            pick_list(
                EFFECTS.map(|e| e.label()).to_vec(),
                Some(state.effect.label()),
                |label| Message::EffectChanged(
                    EFFECTS.into_iter().find(|e| e.label() == label).unwrap_or(Effect::Solid)
                ),
            )
            .text_size(size::BODY),
        ),
        widgets::field(
            palette,
            "Rainbow",
            Some("Cycles the spectrum; the chosen colour is not used"),
            checkbox(state.rainbow).on_toggle(Message::RainbowToggled),
        ),
    ]
    .spacing(GAP);

    column![
        widgets::card(
            palette,
            "Colour",
            Some(match state.mode {
                Mode::Off => "Both LEDs are off",
                Mode::Unified => "One colour for the wheel and the logo",
                Mode::Split => "Each LED set separately",
            }),
            colour_body,
        ),
        widgets::card(
            palette,
            "Effect",
            Some("Applies to both LEDs; the mouse has no per-LED effect"),
            effect_body,
        ),
    ]
    .spacing(STACK)
    .into()
}
