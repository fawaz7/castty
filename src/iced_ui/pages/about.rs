//! About and appearance.

use super::super::theme::{Accent, Named, Palette};
use super::super::widgets::{self, size, STACK, UNIT};
use iced::widget::{button, column, row, text};
use iced::Element;

pub const AUTHOR: &str = "Fawaz Alghzawi";
pub const GITHUB: &str = "https://github.com/fawaz7/castty";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub enum Message {
    ThemeChanged(Named),
    AccentChanged(Accent),
    OpenGithub,
}

pub fn view<'a>(current: Named, accent: Accent, palette: &Palette) -> Element<'a, Message> {
    let swatches = Named::ALL
        .iter()
        .fold(row![].spacing(2.0 * UNIT), |acc, named| {
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
                        .spacing(UNIT * 0.75),
                        text(named.label()).size(size::CAPTION),
                    ]
                    .spacing(1.5 * UNIT)
                    .align_x(iced::Alignment::Center),
                )
                .padding(2.5 * UNIT)
                .style(move |_t, status| widgets::segment(&style, status, chosen))
                .on_press(Message::ThemeChanged(*named)),
            )
        });

    let accents = Accent::ALL.iter().fold(row![].spacing(1.5 * UNIT), |acc, option| {
        let chosen = *option == accent;
        let colour = option.colour(current);
        let style = *palette;
        acc.push(
            button(swatch(colour))
                .padding(1.5 * UNIT)
                .style(move |_t, status| widgets::segment(&style, status, chosen))
                .on_press(Message::AccentChanged(*option)),
        )
    });

    let appearance = widgets::card(
        palette,
        "Appearance",
        Some("Chosen here, not taken from the desktop, so it looks the same everywhere"),
        column![
            widgets::field(palette, "Theme", None::<&str>, swatches),
            widgets::field(palette, "Accent", Some("Used for selection and the Apply button"), accents),
        ]
        .spacing(3.0 * UNIT),
    );

    let about_body = column![
        row![
            widgets::muted(palette, "Built by", size::BODY),
            text(AUTHOR).size(size::BODY),
            widgets::muted(palette, "·", size::BODY),
            widgets::muted(palette, format!("v{VERSION}"), size::BODY),
        ]
        .spacing(1.5 * UNIT),
        button(text(GITHUB).size(size::LABEL))
            .padding([1.5 * UNIT, 3.0 * UNIT])
            .style({
                let style = *palette;
                move |_t, status| widgets::subtle(&style, status)
            })
            .on_press(Message::OpenGithub),
        widgets::caption(
            palette,
            "The Castor shipped without Linux software and without a published protocol. \
             This one was reverse engineered from captures of the Windows application and \
             verified against the hardware. Everything derived along the way -- the protocol, \
             the raw captures and the capture rig -- is published in research/ for anyone to \
             use. Free software under the GPL-3.0; the mouse artwork remains Mionix's.",
        ),
    ]
    .spacing(2.5 * UNIT);

    column![
        widgets::card(
            palette,
            "castty",
            Some("Configuration for the Mionix Castor on Linux"),
            about_body,
        ),
        appearance,
    ]
    .spacing(STACK)
    .into()
}

fn swatch<'a, M: 'a>(colour: iced::Color) -> Element<'a, M> {
    iced::widget::container(iced::widget::Space::new().width(4.0 * UNIT).height(4.0 * UNIT))
        .style(move |_t| iced::widget::container::Style {
            background: Some(iced::Background::Color(colour)),
            border: iced::Border { radius: UNIT.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
}
