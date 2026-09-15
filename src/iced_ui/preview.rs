//! The live mouse preview.
//!
//! Draws the body, the two recoloured LED cut-outs, a halo in the LED colour,
//! and — on the buttons page — numbered callouts.

use super::art::Art;
use super::theme::Palette;
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::rc::Rc;

/// Where each button sits on the artwork, as a fraction of the image, in the
/// same order as `hardware::BUTTONS`. Measured off the product render.
const BUTTON_MARKS: [(f32, f32); 6] = [
    (0.35, 0.22),  // left
    (0.66, 0.22),  // right
    (0.51, 0.235), // wheel click — centre of the wheel, which spans 0.165-0.31
    (0.20, 0.36),  // side, front
    (0.20, 0.48),  // side, rear
    (0.51, 0.40),  // DPI — the button below the wheel
];

/// Breathing room around the artwork, so callouts near the edge are not clipped
/// and the halo has somewhere to fade out.
const INSET: f32 = 34.0;

pub struct Preview {
    pub art: Rc<Art>,
    /// Colours after the effect's brightness has been applied.
    pub wheel: (u8, u8, u8),
    pub logo: (u8, u8, u8),
    pub show_buttons: bool,
    pub palette: Palette,
}

impl<Message> canvas::Program<Message> for Preview {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let art_w = super::art::WIDTH as f32;
        let art_h = super::art::HEIGHT as f32;

        let usable = Size::new(
            (bounds.width - INSET * 2.0).max(1.0),
            (bounds.height - INSET * 2.0).max(1.0),
        );
        let scale = (usable.width / art_w).min(usable.height / art_h);
        let drawn = Size::new(art_w * scale, art_h * scale);
        let origin = Point::new(
            (bounds.width - drawn.width) / 2.0,
            (bounds.height - drawn.height) / 2.0,
        );

        // The halo is centred on the mouse and kept comfortably inside the
        // widget, so it fades to nothing rather than meeting an edge.
        draw_glow(
            &mut frame,
            Point::new(origin.x + drawn.width * 0.5, origin.y + drawn.height * 0.42),
            drawn.width.max(drawn.height) * 0.62,
            self.wheel,
            self.logo,
        );

        frame.with_save(|frame| {
            frame.translate(Vector::new(origin.x, origin.y));
            frame.scale(scale);

            let full = Rectangle { x: 0.0, y: 0.0, width: art_w, height: art_h };
            frame.draw_image(full, &self.art.body);
            // Bound to locals: only a reference converts into a canvas image.
            let wheel = self.art.wheel.tinted(self.wheel);
            let logo = self.art.logo.tinted(self.logo);
            frame.draw_image(self.art.wheel.bounds(), &wheel);
            frame.draw_image(self.art.logo.bounds(), &logo);
        });

        // Callouts are drawn outside the artwork transform so their size stays
        // constant however the window is resized.
        if self.show_buttons {
            draw_marks(&mut frame, origin, scale, &self.palette);
        }

        vec![frame.into_geometry()]
    }
}

/// Light spilling onto the surround, in the LED colour. Off means no glow.
fn draw_glow(frame: &mut Frame, centre: Point, radius: f32, wheel: (u8, u8, u8), logo: (u8, u8, u8)) {
    let mix = |a: u8, b: u8| f32::from(a.max(b)) / 255.0;
    let (r, g, b) = (
        mix(wheel.0, logo.0),
        mix(wheel.1, logo.1),
        mix(wheel.2, logo.2),
    );
    let strength = r.max(g).max(b);
    if strength <= 0.01 {
        return;
    }
    // Concentric rings: iced has no radial gradient, and a stack of translucent
    // circles gives the same falloff. Drawn outwards-in so alpha accumulates
    // towards the centre.
    const RINGS: usize = 28;
    for i in (0..RINGS).rev() {
        let t = (i + 1) as f32 / RINGS as f32;
        let alpha = 0.055 * strength * (1.0 - t).powf(1.4);
        frame.fill(
            &Path::circle(centre, radius * t),
            Color::from_rgba(r, g, b, alpha),
        );
    }
}

/// Numbered badges over each button, so the rows on the buttons page can be
/// matched to the physical mouse without guesswork.
fn draw_marks(frame: &mut Frame, origin: Point, scale: f32, palette: &Palette) {
    const R: f32 = 14.0;
    for (i, (fx, fy)) in BUTTON_MARKS.iter().enumerate() {
        let centre = Point::new(
            origin.x + fx * super::art::WIDTH as f32 * scale,
            origin.y + fy * super::art::HEIGHT as f32 * scale,
        );
        frame.fill(&Path::circle(centre, R), Color::from_rgba(0.0, 0.0, 0.0, 0.7));
        frame.stroke(
            &Path::circle(centre, R),
            Stroke::default().with_color(palette.accent).with_width(2.0),
        );
        frame.fill_text(Text {
            content: (i + 1).to_string(),
            position: centre,
            color: Color::WHITE,
            size: 14.0.into(),
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Text::default()
        });
    }
}
