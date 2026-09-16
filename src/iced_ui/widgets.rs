//! Shared building blocks, so the pages look like one application.
//!
//! Every measurement here is a multiple of [`UNIT`], and every text size
//! comes from [`size`]. Pages take their numbers from these two places
//! rather than choosing per call site, which is what keeps the hierarchy
//! between a page title, a card title and a field label legible.

use super::theme::Palette;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Border, Element, Length};
use std::borrow::Cow;

/// The base spacing unit. Nothing below is an arbitrary pixel count.
pub const UNIT: f32 = 4.0;
/// Between the fields inside a card, and between a card's head and body.
pub const GAP: f32 = 3.0 * UNIT;
/// Between cards, and between the hero and the content column.
pub const STACK: f32 = 4.0 * UNIT;
/// Inside a card, from its edge to its content.
pub const PAD: f32 = 5.0 * UNIT;
/// From the window edge to anything that is not chrome.
pub const PAGE_PAD: f32 = 6.0 * UNIT;
/// The widest a content column gets. Past this, label/control pairs drift
/// apart and a form stops reading as a form.
pub const MEASURE: f32 = 720.0;

/// The type scale: five steps, one per role. Sizes are `f32` because
/// `iced::Pixels` converts from `f32`, not from integers.
pub mod size {
    /// The page heading, one per page.
    pub const TITLE: f32 = 22.0;
    /// A card's title.
    pub const HEADING: f32 = 16.0;
    /// Field labels, list entries, primary buttons, and the controls.
    pub const BODY: f32 = 14.0;
    /// Secondary buttons and segmented controls.
    pub const LABEL: f32 = 13.0;
    /// Hints, subtitles, the status line.
    pub const CAPTION: f32 = 12.0;
}

/// Corner radius for panels and controls.
pub const RADIUS: f32 = 10.0;

/// A titled panel. Most of the UI is a stack of these.
///
/// `subtitle` accepts the same kinds of value as [`field`]'s hint: a
/// borrowed literal or an owned `String` built during the render. It used
/// to take `Option<&'a str>`, which could not hold a `format!` result and
/// pushed two readouts out of the place they belonged.
pub fn card<'a, M: 'a>(
    palette: &Palette,
    title: &'a str,
    subtitle: Option<impl Into<Cow<'a, str>>>,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let mut head = column![heading(title)].spacing(UNIT);
    if let Some(sub) = subtitle {
        head = head.push(caption(palette, sub.into()));
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
/// `hint` takes anything convertible to `Cow<'a, str>`: a `&'a str` literal
/// (the common case) or an owned `String` computed fresh each render (a live
/// reading, say). Callers never need a borrow-lifetime workaround for a value
/// that does not outlive the render.
pub fn field<'a, M: 'a>(
    palette: &Palette,
    label: &'a str,
    hint: Option<impl Into<Cow<'a, str>>>,
    control: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let mut left = column![text(label).size(size::BODY)].spacing(UNIT / 2.0);
    if let Some(hint) = hint {
        left = left.push(caption(palette, hint.into()));
    }
    row![left.width(Length::Fill), control.into()]
        .align_y(iced::Alignment::Center)
        .spacing(GAP)
        .into()
}

/// A card title: the second step of the scale, set a little heavier so it
/// separates from the field labels below it even at close sizes.
pub fn heading<'a>(content: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(content).size(size::HEADING).font(iced::Font {
        weight: iced::font::Weight::Medium,
        ..iced::Font::DEFAULT
    })
}

/// Secondary text in the palette's dimmed colour, at caption size.
pub fn caption<'a, M: 'a>(palette: &Palette, content: impl text::IntoFragment<'a>) -> Element<'a, M> {
    muted(palette, content, size::CAPTION)
}

/// Secondary text in the palette's dimmed colour, at any size.
pub fn muted<'a, M: 'a>(
    palette: &Palette,
    content: impl text::IntoFragment<'a>,
    size: f32,
) -> Element<'a, M> {
    let colour = palette.dim;
    text(content)
        .size(size)
        .style(move |_t| text::Style { color: Some(colour) })
        .into()
}

/// A segmented control: a row of mutually exclusive buttons.
pub fn segmented<'a, M: Clone + 'a>(
    palette: &Palette,
    options: Vec<(&'a str, M)>,
    selected: usize,
) -> Element<'a, M> {
    let style = *palette;
    let mut bar = row![].spacing(UNIT);
    for (index, (label, message)) in options.into_iter().enumerate() {
        let chosen = index == selected;
        bar = bar.push(
            button(text(label).size(size::LABEL))
                .padding([1.5 * UNIT, 3.5 * UNIT])
                .style(move |_t, status| segment(&style, status, chosen))
                .on_press(message),
        );
    }
    container(bar).padding(UNIT * 0.75).style(move |_t| container::Style {
        background: Some(iced::Background::Color(style.raised)),
        border: Border { radius: RADIUS.into(), ..Default::default() },
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
        border: Border { radius: (RADIUS - 2.0).into(), ..Default::default() },
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
        border: Border { radius: RADIUS.into(), ..Default::default() },
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
            radius: RADIUS.into(),
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
        border: Border { radius: RADIUS.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Pushes whatever follows it in a `row!` to the right-hand edge.
///
/// Deliberately horizontal only. A single `spacer()` that filled the
/// *vertical* axis was once used here inside a row, where it inflated the
/// tab bar to the full window height; naming the axis makes that mistake
/// impossible to write. Columns that scroll do not want a trailing pusher at
/// all: inside a `scrollable` there is nothing to push against.
pub fn push_right<'a, M: 'a>() -> Element<'a, M> {
    Space::new().width(Length::Fill).into()
}

/// A one-pixel horizontal line in the palette's border colour. Used to
/// separate the chrome (tab bar, footer) from the page, since a container
/// border cannot be applied to one side only.
pub fn hairline<'a, M: 'a>(palette: &Palette) -> Element<'a, M> {
    let colour = palette.border;
    container(Space::new().width(Length::Fill).height(1.0))
        .style(move |_t| container::Style {
            background: Some(iced::Background::Color(colour)),
            ..Default::default()
        })
        .into()
}

pub fn lighten(colour: iced::Color, amount: f32) -> iced::Color {
    iced::Color {
        r: (colour.r + amount).min(1.0),
        g: (colour.g + amount).min(1.0),
        b: (colour.b + amount).min(1.0),
        a: colour.a,
    }
}
