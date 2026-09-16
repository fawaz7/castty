//! Shared building blocks, so the pages look like one application.

use super::theme::Palette;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Border, Element, Length};

pub const GAP: f32 = 14.0;
pub const PAD: f32 = 18.0;

/// A titled panel. Most of the UI is a stack of these.
pub fn card<'a, M: 'a>(
    palette: &Palette,
    title: &'a str,
    subtitle: Option<&'a str>,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let mut head = column![text(title).size(15.0)].spacing(3);
    if let Some(sub) = subtitle {
        let dim = palette.dim;
        head = head.push(text(sub).size(12.0).style(move |_t| text::Style { color: Some(dim) }));
    }
    let style = *palette;
    container(column![head, body.into()].spacing(GAP))
        .padding(PAD)
        .width(Length::Fill)
        .style(move |_t| style.card())
        .into()
}

/// One labelled control on its own line.
///
/// `hint` takes anything convertible to `Cow<'a, str>` — a `&'a str` literal
/// (the common case) or an owned `String` computed fresh each render (e.g. a
/// live reading) both work, so callers never need a borrow-lifetime
/// workaround for a value that doesn't outlive the render.
pub fn field<'a, M: 'a>(
    palette: &Palette,
    label: &'a str,
    hint: Option<impl Into<std::borrow::Cow<'a, str>>>,
    control: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let dim = palette.dim;
    let mut left = column![text(label).size(14.0)].spacing(2);
    if let Some(hint) = hint {
        let hint: std::borrow::Cow<'a, str> = hint.into();
        left = left.push(text(hint).size(11.0).style(move |_t| text::Style { color: Some(dim) }));
    }
    row![left.width(Length::Fill), control.into()]
        .align_y(iced::Alignment::Center)
        .spacing(GAP)
        .into()
}

/// A segmented control: a row of mutually exclusive buttons.
pub fn segmented<'a, M: Clone + 'a>(
    palette: &Palette,
    options: Vec<(&'a str, M)>,
    selected: usize,
) -> Element<'a, M> {
    let style = *palette;
    let mut bar = row![].spacing(4);
    for (index, (label, message)) in options.into_iter().enumerate() {
        let chosen = index == selected;
        bar = bar.push(
            button(text(label).size(13.0))
                .padding([7, 15])
                .style(move |_t, status| segment(&style, status, chosen))
                .on_press(message),
        );
    }
    container(bar).padding(3).style(move |_t| container::Style {
        background: Some(iced::Background::Color(style.raised)),
        border: Border { radius: 10.0.into(), ..Default::default() },
        ..Default::default()
    })
    .into()
}

pub fn segment(palette: &Palette, status: button::Status, selected: bool) -> button::Style {
    let background = if selected {
        palette.accent
    } else if matches!(status, button::Status::Hovered) {
        palette.surface
    } else {
        iced::Color::TRANSPARENT
    };
    button::Style {
        background: Some(iced::Background::Color(background)),
        text_color: if selected { palette.on_accent } else { palette.text },
        border: Border { radius: 8.0.into(), ..Default::default() },
        ..Default::default()
    }
}

pub fn primary(palette: &Palette, status: button::Status) -> button::Style {
    let enabled = !matches!(status, button::Status::Disabled);
    let background = match status {
        button::Status::Hovered => lighten(palette.accent, 0.08),
        button::Status::Disabled => palette.raised,
        _ => palette.accent,
    };
    button::Style {
        background: Some(iced::Background::Color(background)),
        text_color: if enabled { palette.on_accent } else { palette.dim },
        border: Border { radius: 9.0.into(), ..Default::default() },
        ..Default::default()
    }
}

pub fn subtle(palette: &Palette, status: button::Status) -> button::Style {
    button::Style {
        background: Some(iced::Background::Color(match status {
            button::Status::Hovered => palette.raised,
            _ => iced::Color::TRANSPARENT,
        })),
        text_color: palette.text,
        border: Border {
            color: palette.border,
            width: 1.0,
            radius: 9.0.into(),
        },
        ..Default::default()
    }
}

pub fn destructive(palette: &Palette, status: button::Status) -> button::Style {
    button::Style {
        background: Some(iced::Background::Color(match status {
            button::Status::Hovered => lighten(palette.danger, 0.08),
            _ => palette.danger,
        })),
        text_color: iced::Color::WHITE,
        border: Border { radius: 9.0.into(), ..Default::default() },
        ..Default::default()
    }
}

pub fn dim<'a>(palette: &Palette, content: impl text::IntoFragment<'a>, size: f32) -> Element<'a, ()> {
    let colour = palette.dim;
    text(content)
        .size(size)
        .style(move |_t| text::Style { color: Some(colour) })
        .into()
}

/// Pushes whatever follows it in a `row!` to the right-hand edge.
///
/// Deliberately horizontal only. A single `spacer()` that filled the
/// *vertical* axis was once used here inside a row, where it inflated the
/// tab bar to the full window height; naming the axis makes that mistake
/// impossible to write. Columns that scroll do not want a trailing pusher at
/// all -- inside a `scrollable` there is nothing to push against.
pub fn push_right<'a, M: 'a>() -> Element<'a, M> {
    Space::new().width(Length::Fill).into()
}

pub fn lighten(colour: iced::Color, amount: f32) -> iced::Color {
    iced::Color {
        r: (colour.r + amount).min(1.0),
        g: (colour.g + amount).min(1.0),
        b: (colour.b + amount).min(1.0),
        a: colour.a,
    }
}
