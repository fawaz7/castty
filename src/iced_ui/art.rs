//! The mouse artwork, and recolouring it.
//!
//! Three pre-aligned layers: the body (which ships with its LEDs baked in
//! yellow-green) and cut-outs for the logo and scroll wheel. The cut-outs are
//! recoloured and drawn over the body, hiding the baked-in colour.
//!
//! Recolouring is weighted by how colourful each pixel already is. Forcing the
//! target's saturation onto every pixel tints the wheel's neutral dark housing
//! along with its LEDs, which looks wrong; scaling by the pixel's own saturation
//! leaves neutral pixels neutral so only the lit segments take the colour.

use iced::widget::image::Handle;
use iced::Rectangle;

const BODY: &[u8] = include_bytes!("../../resources/mouse.png");
const LOGO: &[u8] = include_bytes!("../../resources/logo.png");
const WHEEL: &[u8] = include_bytes!("../../resources/scroll_wheel.png");

/// Artwork is rendered at a fixed size and scaled by the canvas, so recolouring
/// costs the same regardless of window size.
pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 392;

/// A recolourable overlay, cropped to its opaque area so tinting stays cheap.
pub struct Layer {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    pub offset: (f32, f32),
}

impl Layer {
    /// Recolour and hand back something the canvas can draw.
    pub fn tinted(&self, (r, g, b): (u8, u8, u8)) -> Handle {
        let (th, ts, tv) = rgb_to_hsv(r, g, b);
        let mut out = self.rgba.clone();
        for px in out.chunks_exact_mut(4) {
            if px[3] == 0 {
                continue;
            }
            let (_, ps, pv) = rgb_to_hsv(px[0], px[1], px[2]);
            let (nr, ng, nb) = hsv_to_rgb(th, ps * ts, pv * (1.0 - ps + ps * tv));
            px[0] = nr;
            px[1] = ng;
            px[2] = nb;
        }
        Handle::from_rgba(self.width, self.height, out)
    }

    pub fn bounds(&self) -> Rectangle {
        Rectangle {
            x: self.offset.0,
            y: self.offset.1,
            width: self.width as f32,
            height: self.height as f32,
        }
    }
}

pub struct Art {
    pub body: Handle,
    pub wheel: Layer,
    pub logo: Layer,
}

impl Art {
    pub fn load() -> Self {
        let body = decode(BODY);
        Art {
            body: Handle::from_rgba(WIDTH, HEIGHT, body),
            wheel: layer(WHEEL),
            logo: layer(LOGO),
        }
    }
}

/// Decode a PNG and scale it to the fixed preview size.
fn decode(bytes: &[u8]) -> Vec<u8> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("bundled artwork must decode")
        .resize_exact(WIDTH, HEIGHT, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    img.into_raw()
}

/// Decode, then crop to the opaque area so only the lit region is recoloured.
fn layer(bytes: &[u8]) -> Layer {
    let full = decode(bytes);
    let (mut x0, mut y0, mut x1, mut y1) = (WIDTH, HEIGHT, 0u32, 0u32);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if full[((y * WIDTH + x) * 4 + 3) as usize] > 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    let (w, h) = (x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y0 + h {
        let start = ((y * WIDTH + x0) * 4) as usize;
        rgba.extend_from_slice(&full[start..start + (w * 4) as usize]);
    }
    Layer {
        rgba,
        width: w,
        height: h,
        offset: (x0 as f32, y0 as f32),
    }
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h.rem_euclid(360.0), s, max)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let f = |t: f32| ((t + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (f(r), f(g), f(b))
}
