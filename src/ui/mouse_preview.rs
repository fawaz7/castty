//! Live rendering of the mouse with its LEDs in the chosen colours.
//!
//! Three pre-aligned 800x980 layers: the body (which ships with the LEDs baked
//! in yellow-green) and cut-outs for the logo and scroll wheel. The cut-outs are
//! recoloured and composited over the body, which hides the baked-in colour.
//!
//! Recolouring is weighted by how colourful each pixel already is. Forcing the
//! target's saturation onto every pixel tints the scroll wheel's neutral dark
//! housing along with its LEDs, which looks wrong; scaling by the pixel's own
//! saturation leaves neutral pixels neutral so only the lit segments take the
//! colour. Brightness is scaled by the target's value in proportion to the same
//! weight, which makes black mean "off" while keeping the unlit housing visible.

use super::colour_picker::{hsv_to_rgb, rgb_to_hsv};
use crate::hardware::{Effect, LedMode};
use gtk4 as gtk;
use gtk::gdk_pixbuf::{InterpType, Pixbuf};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

/// Last tint result, keyed by the colours that produced it.
type TintCache = Rc<RefCell<Option<((u8, u8, u8), (u8, u8, u8), Pixbuf, Pixbuf)>>>;

const BODY: &[u8] = include_bytes!("../../resources/mouse.png");
const LOGO: &[u8] = include_bytes!("../../resources/logo.png");
const WHEEL: &[u8] = include_bytes!("../../resources/scroll_wheel.png");

const PREVIEW_W: i32 = 320;
const PREVIEW_H: i32 = 392;

/// Where each button sits on the artwork, as a fraction of the image, in the
/// same order as `hardware::BUTTONS`. Read off the product render directly.
const BUTTON_MARKS: [(f64, f64); 6] = [
    (0.35, 0.22),  // left
    (0.66, 0.22),  // right
    (0.51, 0.235), // wheel click -- centre of the wheel, which spans 0.165-0.31
    (0.20, 0.36),  // side, front
    (0.20, 0.48),  // side, rear
    (0.51, 0.40),  // DPI -- the button below the wheel, spanning 0.345-0.45
];

fn load(bytes: &'static [u8]) -> Option<Pixbuf> {
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(bytes));
    Pixbuf::from_stream(&stream, None::<&gio::Cancellable>).ok()
}

/// A recolourable overlay, cropped to its opaque area so tinting stays cheap.
struct Layer {
    pixbuf: Pixbuf,
    x: f64,
    y: f64,
}

impl Layer {
    fn new(src: &Pixbuf) -> Option<Self> {
        let scaled = src.scale_simple(PREVIEW_W, PREVIEW_H, InterpType::Bilinear)?;
        let (x0, y0, x1, y1) = alpha_bounds(&scaled)?;
        let sub = scaled.new_subpixbuf(x0, y0, x1 - x0, y1 - y0);
        Some(Layer { pixbuf: sub, x: x0 as f64, y: y0 as f64 })
    }

    fn tinted(&self, rgb: (u8, u8, u8)) -> Option<Pixbuf> {
        let out = self.pixbuf.copy()?;
        let (th, ts, tv) = rgb_to_hsv(rgb.0, rgb.1, rgb.2);
        let n = out.n_channels() as usize;
        let stride = out.rowstride() as usize;
        let (w, h) = (out.width() as usize, out.height() as usize);
        // SAFETY: we hold the only reference to this freshly copied pixbuf.
        let pixels = unsafe { out.pixels() };
        for y in 0..h {
            for x in 0..w {
                let i = y * stride + x * n;
                if n == 4 && pixels[i + 3] == 0 {
                    continue;
                }
                let (_, ps, pv) = rgb_to_hsv(pixels[i], pixels[i + 1], pixels[i + 2]);
                let (r, g, b) = hsv_to_rgb(th, ps * ts, pv * (1.0 - ps + ps * tv));
                pixels[i] = r;
                pixels[i + 1] = g;
                pixels[i + 2] = b;
            }
        }
        Some(out)
    }
}

fn alpha_bounds(p: &Pixbuf) -> Option<(i32, i32, i32, i32)> {
    if p.n_channels() != 4 {
        return Some((0, 0, p.width(), p.height()));
    }
    let stride = p.rowstride() as usize;
    let (w, h) = (p.width() as usize, p.height() as usize);
    let pixels = unsafe { p.pixels() };
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if pixels[y * stride + x * 4 + 3] > 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as i32, y0 as i32, x1 as i32, y1 as i32))
}

#[derive(Clone, Copy)]
pub struct PreviewState {
    pub wheel: (u8, u8, u8),
    pub logo: (u8, u8, u8),
    pub mode: LedMode,
}

impl Default for PreviewState {
    fn default() -> Self {
        PreviewState { wheel: (0, 0, 0), logo: (0, 0, 0), mode: LedMode::new(Effect::Solid, false) }
    }
}

pub struct MousePreview {
    pub widget: gtk::DrawingArea,
    state: Rc<Cell<PreviewState>>,
    /// Numbered callouts, shown while the buttons page is visible.
    show_buttons: Rc<Cell<bool>>,
}

/// Cycle lengths in seconds, **measured on the hardware** by counting cycles
/// against a stopwatch -- not guessed. The preview exists to show what the mouse
/// will actually do, so if these drift from the device the preview is worse than
/// useless. Re-measure rather than round them off.
const BREATHING_PERIOD_S: f64 = 6.0; // 5 cycles in 30 s
const PULSATING_PERIOD_S: f64 = 1.58; // 19 cycles in 30 s
/// Pulsating is a heartbeat: **two** dark beats in quick succession, then the
/// LEDs are held lit for the rest of the cycle. One beat is a blackout.
const PULSATING_BEAT_S: f64 = 0.18;
/// Lit gap between the two beats of one heartbeat.
const PULSATING_BEAT_GAP_S: f64 = 0.12;
/// Proportion of a single beat held fully dark, rather than just touching zero
/// on the way past.
const PULSATING_DARK_HOLD: f64 = 0.36;
const BLINKING_PERIOD_S: f64 = 1.154; // 26 cycles in 30 s
const RAINBOW_PERIOD_S: f64 = 5.0; // full hue loop timed at ~5 s

/// Brightness multiplier for the animated effects, and a hue offset when rainbow
/// is on.
fn animate(mode: LedMode, t: f64) -> (f64, Option<f64>) {
    let brightness = match mode.effect() {
        Effect::Solid => 1.0,
        Effect::Blinking => {
            if (t / BLINKING_PERIOD_S).fract() < 0.5 {
                1.0
            } else {
                0.05
            }
        }
        Effect::Pulsating => {
            // Measured shape, not a sine: a quick dip to dark and back taking
            // well under half a second, then held lit for the rest of the cycle.
            // Seconds into the current heartbeat.
            let phase = (t / PULSATING_PERIOD_S).fract() * PULSATING_PERIOD_S;
            let beat = |start: f64| -> Option<f64> {
                if phase < start || phase >= start + PULSATING_BEAT_S {
                    return None;
                }
                let x = (phase - start) / PULSATING_BEAT_S;
                let edge = (1.0 - PULSATING_DARK_HOLD) / 2.0;
                Some(if x < edge {
                    1.0 - x / edge
                } else if x < 1.0 - edge {
                    0.0
                } else {
                    (x - (1.0 - edge)) / edge
                })
            };
            beat(0.0)
                .or_else(|| beat(PULSATING_BEAT_S + PULSATING_BEAT_GAP_S))
                .unwrap_or(1.0)
        }
        Effect::Breathing => {
            let phase = (t / BREATHING_PERIOD_S * std::f64::consts::TAU).sin() * 0.5 + 0.5;
            0.08 + 0.92 * phase * phase
        }
    };
    // Rainbow is an independent flag, so it combines with any base effect --
    // "breathing + rainbow" is a real hardware state, not a separate mode.
    let hue = mode.rainbow().then(|| (t / RAINBOW_PERIOD_S * 360.0) % 360.0);
    (brightness, hue)
}

fn apply_effect(rgb: (u8, u8, u8), brightness: f64, hue: Option<f64>) -> (u8, u8, u8) {
    let (h, s, v) = rgb_to_hsv(rgb.0, rgb.1, rgb.2);
    match hue {
        // Rainbow cycles hue regardless of the stored colour, but an LED set to
        // black stays off.
        Some(cycled) if v > 0.0 => hsv_to_rgb(cycled, 1.0, brightness),
        Some(_) => (0, 0, 0),
        None => hsv_to_rgb(h, s, v * brightness),
    }
}

impl MousePreview {
    pub fn new() -> Self {
        let widget = gtk::DrawingArea::builder()
            .content_width(PREVIEW_W)
            .content_height(PREVIEW_H)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();

        let state = Rc::new(Cell::new(PreviewState::default()));
        let show_buttons = Rc::new(Cell::new(false));
        let body = load(BODY).and_then(|p| p.scale_simple(PREVIEW_W, PREVIEW_H, InterpType::Bilinear));
        let logo = load(LOGO).and_then(|p| Layer::new(&p));
        let wheel = load(WHEEL).and_then(|p| Layer::new(&p));
        let start = Instant::now();
        let cache: TintCache = Rc::new(RefCell::new(None));

        {
            let state = state.clone();
            let cache = cache.clone();
            let show_buttons = show_buttons.clone();
            widget.set_draw_func(move |_, cr, _, _| {
                let s = state.get();
                let t = start.elapsed().as_secs_f64();
                let (brightness, hue) = animate(s.mode, t);
                let wheel_rgb = apply_effect(s.wheel, brightness, hue);
                let logo_rgb = apply_effect(s.logo, brightness, hue);

                if let Some(body) = &body {
                    cr.set_source_pixbuf(body, 0.0, 0.0);
                    let _ = cr.paint();
                }

                let mut cached = cache.borrow_mut();
                let fresh = match cached.as_ref() {
                    Some((w, l, _, _)) => *w != wheel_rgb || *l != logo_rgb,
                    None => true,
                };
                if fresh {
                    if let (Some(wl), Some(ll)) = (&wheel, &logo) {
                        if let (Some(wp), Some(lp)) = (wl.tinted(wheel_rgb), ll.tinted(logo_rgb)) {
                            *cached = Some((wheel_rgb, logo_rgb, wp, lp));
                        }
                    }
                }
                if let (Some((_, _, wp, lp)), Some(wl), Some(ll)) =
                    (cached.as_ref(), &wheel, &logo)
                {
                    cr.set_source_pixbuf(wp, wl.x, wl.y);
                    let _ = cr.paint();
                    cr.set_source_pixbuf(lp, ll.x, ll.y);
                    let _ = cr.paint();
                }

                if show_buttons.get() {
                    draw_button_marks(cr);
                }
            });
        }

        // Only redraw while an animated effect is selected.
        {
            let state = state.clone();
            widget.add_tick_callback(move |area, _| {
                let m = state.get().mode;
                let animated = m.rainbow() || m.effect() != Effect::Solid;
                if animated {
                    area.queue_draw();
                }
                glib::ControlFlow::Continue
            });
        }

        MousePreview { widget, state, show_buttons }
    }

    /// Show numbered callouts matching the rows on the buttons page.
    pub fn set_show_buttons(&self, show: bool) {
        if self.show_buttons.get() != show {
            self.show_buttons.set(show);
            self.widget.queue_draw();
        }
    }

    pub fn set_state(&self, s: PreviewState) {
        self.state.set(s);
        self.widget.queue_draw();
    }
}

/// Numbered badges over each button, so the page's rows can be matched to the
/// physical mouse without guesswork.
fn draw_button_marks(cr: &gtk::cairo::Context) {
    const R: f64 = 13.0;
    cr.select_font_face(
        "sans-serif",
        gtk::cairo::FontSlant::Normal,
        gtk::cairo::FontWeight::Bold,
    );
    cr.set_font_size(15.0);

    for (i, (fx, fy)) in BUTTON_MARKS.iter().enumerate() {
        let (x, y) = (fx * PREVIEW_W as f64, fy * PREVIEW_H as f64);

        cr.arc(x, y, R, 0.0, std::f64::consts::TAU);
        cr.set_source_rgba(0.09, 0.09, 0.11, 0.92);
        let _ = cr.fill_preserve();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        cr.set_line_width(2.0);
        let _ = cr.stroke();

        let label = (i + 1).to_string();
        if let Ok(ext) = cr.text_extents(&label) {
            cr.move_to(x - ext.width() / 2.0 - ext.x_bearing(), y + ext.height() / 2.0);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            let _ = cr.show_text(&label);
        }
    }
}

impl Default for MousePreview {
    fn default() -> Self {
        Self::new()
    }
}
