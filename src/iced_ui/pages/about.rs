//! About and appearance.

use super::super::theme::{Accent, Named, Palette};
use super::super::widgets::{self, GAP};
use iced::widget::{button, column, row, text};
use iced::{Element, Length};

pub const AUTHOR: &str = "Fawaz Alghzawi";
pub const GITHUB: &str = "https://github.com/fawaz7";

#[derive(Debug, Clone)]
pub enum Message {
    ThemeChanged(Named),
    AccentChanged(Accent),
    OpenGithub,
}

pub fn view<'a>(current: Named, accent: Accent, palette: &Palette) -> Element<'a, Message> {
    let dim = palette.dim;
    let swatches = Named::ALL
        .iter()
        .fold(row![].spacing(8), |acc, named| {
            let chosen = *named == current;
            let colours = named.palette();
            let style = *palette;
            acc.push(
                button(
                    column![
                        // A miniature of the theme, so the choice is visible
                        // rather than a name you have to try.
                        row![
                            swatch(colours.bg),
                            swatch(colours.surface),
                            swatch(colours.accent),
                        ]
                        .spacing(3),
                        text(named.label()).size(12.0),
                    ]
                    .spacing(7)
                    .align_x(iced::Alignment::Center),
                )
                .padding(10)
                .style(move |_t, status| widgets::segment(&style, status, chosen))
                .on_press(Message::ThemeChanged(*named)),
            )
        });

    let accents = Accent::ALL.iter().fold(row![].spacing(6.0), |acc, option| {
        let chosen = *option == accent;
        let colour = option.colour(current);
        let style = *palette;
        acc.push(
            button(swatch(colour))
                .padding(6.0)
                .style(move |_t, status| widgets::segment(&style, status, chosen))
                .on_press(Message::AccentChanged(*option)),
        )
    });

    let appearance = widgets::card(
        palette,
        "Appearance",
        Some("Chosen here, not taken from the desktop, so it looks the same everywhere"),
        column![
            swatches,
            text("Accent").size(13.0).style({
                let dim = palette.dim;
                move |_t| text::Style { color: Some(dim) }
            }),
            accents,
        ]
        .spacing(12.0),
    );

    let about_body = column![
        text("castty").size(26.0),
        text("Configuration for the Mionix Castor on Linux.")
            .size(13.0)
            .style(move |_t| text::Style { color: Some(dim) }),
        row![
            text("Built by").size(13.0).style(move |_t| text::Style { color: Some(dim) }),
            text(AUTHOR).size(13.0),
        ]
        .spacing(6),
        button(text(GITHUB).size(13.0))
            .padding([7, 12])
            .style({
                let style = *palette;
                move |_t, status| widgets::subtle(&style, status)
            })
            .on_press(Message::OpenGithub),
        text(
            "The Castor shipped without Linux software and without a published protocol. \
             This one was reverse engineered from captures of the Windows application and \
             verified against the hardware; PROTOCOL.md in the repository documents all of it."
        )
        .size(12.0)
        .style(move |_t| text::Style { color: Some(dim) }),
    ]
    .spacing(10);

    column![
        widgets::card(palette, "About", None, about_body),
        appearance,
        widgets::spacer(),
    ]
    .spacing(GAP)
    .width(Length::Fill)
    .into()
}

fn swatch<'a, M: 'a>(colour: iced::Color) -> Element<'a, M> {
    iced::widget::container(iced::widget::Space::new().width(16).height(16))
        .style(move |_t| iced::widget::container::Style {
            background: Some(iced::Background::Color(colour)),
            border: iced::Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
}
