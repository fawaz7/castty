//! A self-contained HSV colour picker: saturation/value plane, hue strip and a
//! hex readout, all visible at once rather than behind a "custom" step.

use super::art::{hsv_to_rgb, rgb_to_hsv};
use super::theme::Palette;
use iced::widget::canvas::{self, gradient, Frame, Geometry, Path, Stroke};
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};

/// Height of the hue strip; the plane takes the rest.
const STRIP: f32 = 22.0;
const GAP: f32 = 10.0;

fn colour(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

pub struct ColourPicker {
    pub hsv: (f32, f32, f32),
    /// Greyed out when the colour has no effect -- LEDs off, or rainbow on.
    pub enabled: bool,
    pub palette: Palette,
}

/// Which control a drag started on, so it keeps receiving the drag even if the
/// pointer wanders outside it.
#[derive(Default)]
pub enum Dragging {
    #[default]
    None,
    Plane,
    Strip,
}

impl ColourPicker {
    fn plane_bounds(size: iced::Size) -> Rectangle {
        Rectangle {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: (size.height - STRIP - GAP).max(1.0),
        }
    }

    fn strip_bounds(size: iced::Size) -> Rectangle {
        Rectangle {
            x: 0.0,
            y: size.height - STRIP,
            width: size.width,
            height: STRIP,
        }
    }
}

impl canvas::Program<(f32, f32, f32)> for ColourPicker {
    type State = Dragging;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<(f32, f32, f32)>> {
        if !self.enabled {
            return None;
        }
        let position = cursor.position_in(bounds);

        let pick = |state: &Dragging, point: Point| -> Option<(f32, f32, f32)> {
            let plane = Self::plane_bounds(bounds.size());
            let strip = Self::strip_bounds(bounds.size());
            let (h, s, v) = self.hsv;
            match state {
                Dragging::Plane => Some((
                    h,
                    (point.x / plane.width).clamp(0.0, 1.0),
                    1.0 - (point.y / plane.height).clamp(0.0, 1.0),
                )),
                Dragging::Strip => Some((
                    (point.x / strip.width).clamp(0.0, 1.0) * 360.0,
                    s,
                    v,
                )),
                Dragging::None => None,
            }
        };

        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = position?;
                *state = if point.y >= Self::strip_bounds(bounds.size()).y {
                    Dragging::Strip
                } else {
                    Dragging::Plane
                };
                pick(state, point).map(canvas::Action::publish)
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if matches!(state, Dragging::None) {
                    return None;
                }
                // Use the raw position so a drag continues outside the widget.
                let point = cursor.position()? - iced::Vector::new(bounds.x, bounds.y);
                pick(state, Point::new(point.x, point.y)).map(canvas::Action::publish)
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                *state = Dragging::None;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let plane = Self::plane_bounds(bounds.size());
        let strip = Self::strip_bounds(bounds.size());
        let (hue, sat, val) = self.hsv;
        let dim = if self.enabled { 1.0 } else { 0.35 };

        // Saturation across, value down: two linear gradients, which is all
        // iced offers, and exactly how an HSV plane is built anyway.
        let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
        let across = gradient::Linear::new(
            Point::new(plane.x, plane.y),
            Point::new(plane.x + plane.width, plane.y),
        )
        .add_stop(0.0, Color { a: dim, ..Color::WHITE })
        .add_stop(1.0, Color { a: dim, ..colour(r, g, b) });
        frame.fill_rectangle(
            Point::new(plane.x, plane.y),
            plane.size(),
            across,
        );
        let down = gradient::Linear::new(
            Point::new(plane.x, plane.y),
            Point::new(plane.x, plane.y + plane.height),
        )
        .add_stop(0.0, Color::TRANSPARENT)
        .add_stop(1.0, Color { a: dim, ..Color::BLACK });
        frame.fill_rectangle(
            Point::new(plane.x, plane.y),
            plane.size(),
            down,
        );

        // Hue strip.
        let mut hues = gradient::Linear::new(
            Point::new(strip.x, strip.y),
            Point::new(strip.x + strip.width, strip.y),
        );
        for i in 0..=6 {
            let (r, g, b) = hsv_to_rgb(i as f32 * 60.0, 1.0, 1.0);
            hues = hues.add_stop(i as f32 / 6.0, Color { a: dim, ..colour(r, g, b) });
        }
        frame.fill_rectangle(
            Point::new(strip.x, strip.y),
            strip.size(),
            hues,
        );

        // Markers.
        let knob = Point::new(plane.x + sat * plane.width, plane.y + (1.0 - val) * plane.height);
        frame.stroke(
            &Path::circle(knob, 7.0),
            Stroke::default().with_color(Color { a: dim, ..Color::WHITE }).with_width(2.0),
        );
        frame.stroke(
            &Path::circle(knob, 8.5),
            Stroke::default()
                .with_color(Color::from_rgba(0.0, 0.0, 0.0, 0.6 * dim))
                .with_width(1.5),
        );

        let x = strip.x + hue / 360.0 * strip.width;
        frame.stroke(
            &Path::line(Point::new(x, strip.y), Point::new(x, strip.y + strip.height)),
            Stroke::default().with_color(Color { a: dim, ..Color::WHITE }).with_width(2.0),
        );
        frame.stroke_rectangle(
            Point::new(strip.x, strip.y),
            strip.size(),
            Stroke::default().with_color(self.palette.border).with_width(1.0),
        );

        vec![frame.into_geometry()]
    }
}

/// Convenience for the page: current colour as bytes.
pub fn to_rgb(hsv: (f32, f32, f32)) -> (u8, u8, u8) {
    hsv_to_rgb(hsv.0, hsv.1, hsv.2)
}

pub fn from_rgb(rgb: (u8, u8, u8)) -> (f32, f32, f32) {
    rgb_to_hsv(rgb.0, rgb.1, rgb.2)
}
