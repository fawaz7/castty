//! Theming.
//!
//! The palette is the app's own, never the desktop's, so it looks identical on
//! every distribution. Several are built in and the choice is persisted.

use iced::{Background, Border, Color, Shadow, Vector};
use serde::{Deserialize, Serialize};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Named {
    #[default]
    Graphite,
    Nordic,
    Indigo,
    Ember,
    Paper,
}

impl Named {
    pub const ALL: [Named; 5] = [
        Named::Graphite,
        Named::Nordic,
        Named::Indigo,
        Named::Ember,
        Named::Paper,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Named::Graphite => "Graphite",
            Named::Nordic => "Nordic",
            Named::Indigo => "Indigo",
            Named::Ember => "Ember",
            Named::Paper => "Paper",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            // Neutral dark; the accent carries all the colour.
            Named::Graphite => Palette {
                bg: rgb(18, 19, 23),
                surface: rgb(26, 28, 34),
                raised: rgb(34, 37, 44),
                border: rgb(48, 52, 61),
                text: rgb(233, 236, 241),
                dim: rgb(140, 148, 162),
                accent: rgb(96, 165, 250),
                on_accent: rgb(10, 12, 16),
                danger: rgb(239, 83, 80),
                dark: true,
            },
            Named::Nordic => Palette {
                bg: rgb(36, 41, 51),
                surface: rgb(46, 52, 64),
                raised: rgb(59, 66, 82),
                border: rgb(76, 86, 106),
                text: rgb(236, 239, 244),
                dim: rgb(143, 154, 173),
                accent: rgb(136, 192, 208),
                on_accent: rgb(30, 34, 42),
                danger: rgb(191, 97, 106),
                dark: true,
            },
            Named::Indigo => Palette {
                bg: rgb(23, 21, 34),
                surface: rgb(32, 29, 47),
                raised: rgb(42, 38, 62),
                border: rgb(58, 52, 86),
                text: rgb(232, 230, 243),
                dim: rgb(150, 143, 178),
                accent: rgb(167, 139, 250),
                on_accent: rgb(18, 16, 28),
                danger: rgb(244, 114, 130),
                dark: true,
            },
            Named::Ember => Palette {
                bg: rgb(26, 20, 18),
                surface: rgb(36, 28, 25),
                raised: rgb(48, 37, 33),
                border: rgb(68, 52, 46),
                text: rgb(243, 234, 229),
                dim: rgb(166, 148, 140),
                accent: rgb(251, 146, 60),
                on_accent: rgb(26, 16, 8),
                danger: rgb(239, 68, 68),
                dark: true,
            },
            // The one light theme, for anyone who works in daylight.
            Named::Paper => Palette {
                bg: rgb(246, 246, 244),
                surface: rgb(255, 255, 255),
                raised: rgb(238, 238, 235),
                border: rgb(216, 216, 211),
                text: rgb(28, 30, 34),
                dim: rgb(110, 116, 125),
                accent: rgb(37, 99, 235),
                on_accent: rgb(255, 255, 255),
                danger: rgb(200, 45, 45),
                dark: false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub raised: Color,
    pub border: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub danger: Color,
    pub dark: bool,
}

impl Palette {
    pub fn iced_theme(&self, name: &str) -> iced::Theme {
        iced::Theme::custom(
            name.to_string(),
            iced::theme::Palette {
                background: self.bg,
                text: self.text,
                primary: self.accent,
                success: self.accent,
                warning: rgb(234, 179, 8),
                danger: self.danger,
            },
        )
    }

    /// A raised panel.
    pub fn card(&self) -> iced::widget::container::Style {
        iced::widget::container::Style {
            background: Some(Background::Color(self.surface)),
            border: Border {
                color: self.border,
                width: 1.0,
                radius: 12.0.into(),
            },
            text_color: Some(self.text),
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, if self.dark { 0.28 } else { 0.08 }),
                offset: Vector::new(0.0, 2.0),
                blur_radius: 10.0,
            },
            ..Default::default()
        }
    }

    /// The recessed area behind the mouse preview.
    pub fn stage(&self) -> iced::widget::container::Style {
        iced::widget::container::Style {
            background: Some(Background::Color(if self.dark {
                Color {
                    r: self.bg.r * 0.72,
                    g: self.bg.g * 0.72,
                    b: self.bg.b * 0.72,
                    a: 1.0,
                }
            } else {
                self.raised
            })),
            border: Border {
                color: self.border,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    }

    /// The tab bar and footer: a shade apart from the page so they read as
    /// fixed chrome rather than content.
    pub fn chrome(&self) -> iced::widget::container::Style {
        iced::widget::container::Style {
            background: Some(Background::Color(if self.dark {
                Color {
                    r: self.bg.r * 0.8,
                    g: self.bg.g * 0.8,
                    b: self.bg.b * 0.8,
                    a: 1.0,
                }
            } else {
                self.raised
            })),
            ..Default::default()
        }
    }
}

/// An accent override. `Preset` keeps the theme's own.
///
/// A fixed set rather than free colour entry: an arbitrary accent can land
/// unreadably close to a surface colour, and there is no contrast check at
/// runtime to catch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Accent {
    #[default]
    Preset,
    Blue,
    Teal,
    Green,
    Amber,
    Orange,
    Rose,
    Violet,
    Slate,
}

impl Accent {
    pub const ALL: [Accent; 9] = [
        Accent::Preset,
        Accent::Blue,
        Accent::Teal,
        Accent::Green,
        Accent::Amber,
        Accent::Orange,
        Accent::Rose,
        Accent::Violet,
        Accent::Slate,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Accent::Preset => "Theme",
            Accent::Blue => "Blue",
            Accent::Teal => "Teal",
            Accent::Green => "Green",
            Accent::Amber => "Amber",
            Accent::Orange => "Orange",
            Accent::Rose => "Rose",
            Accent::Violet => "Violet",
            Accent::Slate => "Slate",
        }
    }

    pub fn colour(self, preset: Named) -> Color {
        match self {
            Accent::Preset => preset.palette().accent,
            Accent::Blue => rgb(96, 165, 250),
            Accent::Teal => rgb(45, 212, 191),
            Accent::Green => rgb(74, 222, 128),
            Accent::Amber => rgb(251, 191, 36),
            Accent::Orange => rgb(251, 146, 60),
            Accent::Rose => rgb(244, 114, 182),
            Accent::Violet => rgb(167, 139, 250),
            Accent::Slate => rgb(148, 163, 184),
        }
    }
}

/// The palette for a preset with an accent override applied.
pub fn resolve(named: Named, accent: Accent) -> Palette {
    let mut palette = named.palette();
    palette.accent = accent.colour(named);
    // Dark text on bright accents, light text on dark ones, so the Apply
    // button stays readable whatever is chosen.
    let luminance = 0.299 * palette.accent.r + 0.587 * palette.accent.g + 0.114 * palette.accent.b;
    palette.on_accent = if luminance > 0.6 {
        rgb(16, 18, 22)
    } else {
        rgb(255, 255, 255)
    };
    palette
}
