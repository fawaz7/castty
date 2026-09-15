//! A self-contained HSV colour picker: saturation/value plane, hue strip and a
//! hex field, all visible at once.
//!
//! GTK's built-in colour dialog hides the wheel behind a "custom" step, which is
//! awkward when picking a colour *is* the task.

use gtk4 as gtk;
use gtk::cairo;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let f = |t: f64| ((t + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (f(r), f(g), f(b))
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
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

type Callbacks = Rc<RefCell<Vec<Box<dyn Fn(u8, u8, u8)>>>>;

pub struct ColourPicker {
    pub widget: gtk::Box,
    hsv: Rc<Cell<(f64, f64, f64)>>,
    hex: gtk::Entry,
    plane: gtk::DrawingArea,
    strip: gtk::DrawingArea,
    on_change: Callbacks,
    /// Set while pushing state in, so programmatic updates don't echo back out.
    quiet: Rc<Cell<bool>>,
}

impl ColourPicker {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let hsv = Rc::new(Cell::new((0.0, 1.0, 1.0)));
        let on_change: Callbacks = Rc::new(RefCell::new(Vec::new()));
        let quiet = Rc::new(Cell::new(false));

        let plane = gtk::DrawingArea::builder().content_height(150).build();
        plane.add_css_class("castty-picker");
        let strip = gtk::DrawingArea::builder().content_height(22).build();
        strip.add_css_class("castty-picker");

        let hex = gtk::Entry::builder()
            .max_length(7)
            .placeholder_text("#RRGGBB")
            .build();

        widget.append(&plane);
        widget.append(&strip);
        widget.append(&hex);

        let me = ColourPicker { widget, hsv, hex, plane, strip, on_change, quiet };
        me.wire_drawing();
        me.wire_input();
        me
    }

    fn wire_drawing(&self) {
        let hsv = self.hsv.clone();
        self.plane.set_draw_func(move |_, cr, w, h| {
            let (hue, s, v) = hsv.get();
            let (w, h) = (w as f64, h as f64);
            let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);

            // saturation: white -> full hue, left to right
            let grad = cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
            grad.add_color_stop_rgb(0.0, 1.0, 1.0, 1.0);
            grad.add_color_stop_rgb(1.0, r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
            let _ = cr.set_source(&grad);
            let _ = cr.paint();

            // value: transparent -> black, top to bottom
            let shade = cairo::LinearGradient::new(0.0, 0.0, 0.0, h);
            shade.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
            shade.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 1.0);
            let _ = cr.set_source(&shade);
            let _ = cr.paint();

            // marker
            let (x, y) = (s * w, (1.0 - v) * h);
            cr.set_line_width(2.0);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.arc(x, y, 6.0, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.arc(x, y, 7.5, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
        });

        let hsv = self.hsv.clone();
        self.strip.set_draw_func(move |_, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            let grad = cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
            for i in 0..=6 {
                let (r, g, b) = hsv_to_rgb(i as f64 * 60.0, 1.0, 1.0);
                grad.add_color_stop_rgb(
                    i as f64 / 6.0,
                    r as f64 / 255.0,
                    g as f64 / 255.0,
                    b as f64 / 255.0,
                );
            }
            let _ = cr.set_source(&grad);
            let _ = cr.paint();

            let x = hsv.get().0 / 360.0 * w;
            cr.set_line_width(2.0);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(x, 0.0);
            cr.line_to(x, h);
            let _ = cr.stroke();
        });
    }

    fn wire_input(&self) {
        // saturation / value plane
        let drag = gtk::GestureDrag::new();
        {
            let me = self.handle();
            let area = self.plane.clone();
            let pick = move |x: f64, y: f64| {
                let (w, h) = (area.width() as f64, area.height() as f64);
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let (hue, _, _) = me.hsv.get();
                let s = (x / w).clamp(0.0, 1.0);
                let v = 1.0 - (y / h).clamp(0.0, 1.0);
                me.hsv.set((hue, s, v));
                me.refresh(true);
            };
            let p1 = pick.clone();
            drag.connect_drag_begin(move |_, x, y| p1(x, y));
            let g = drag.clone();
            drag.connect_drag_update(move |_, dx, dy| {
                if let Some((sx, sy)) = g.start_point() {
                    pick(sx + dx, sy + dy);
                }
            });
        }
        self.plane.add_controller(drag);

        // hue strip
        let drag = gtk::GestureDrag::new();
        {
            let me = self.handle();
            let area = self.strip.clone();
            let pick = move |x: f64| {
                let w = area.width() as f64;
                if w <= 0.0 {
                    return;
                }
                let (_, s, v) = me.hsv.get();
                me.hsv.set(((x / w).clamp(0.0, 1.0) * 360.0, s, v));
                me.refresh(true);
            };
            let p1 = pick.clone();
            drag.connect_drag_begin(move |_, x, _| p1(x));
            let g = drag.clone();
            drag.connect_drag_update(move |_, dx, _| {
                if let Some((sx, _)) = g.start_point() {
                    pick(sx + dx);
                }
            });
        }
        self.strip.add_controller(drag);

        // hex entry
        let me = self.handle();
        self.hex.connect_activate(move |entry| {
            let text = entry.text();
            let t = text.trim().trim_start_matches('#');
            if t.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&t[0..2], 16),
                    u8::from_str_radix(&t[2..4], 16),
                    u8::from_str_radix(&t[4..6], 16),
                ) {
                    me.hsv.set(rgb_to_hsv(r, g, b));
                    me.refresh(true);
                }
            }
        });
    }

    /// Cheap clonable view of the shared state, for use inside closures.
    fn handle(&self) -> Handle {
        Handle {
            hsv: self.hsv.clone(),
            hex: self.hex.clone(),
            plane: self.plane.clone(),
            strip: self.strip.clone(),
            on_change: self.on_change.clone(),
            quiet: self.quiet.clone(),
        }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        let (h, s, v) = self.hsv.get();
        hsv_to_rgb(h, s, v)
    }

    pub fn set_rgb(&self, r: u8, g: u8, b: u8) {
        self.quiet.set(true);
        self.hsv.set(rgb_to_hsv(r, g, b));
        self.handle().refresh(false);
        self.quiet.set(false);
    }

    /// Listeners are additive; several parts of the UI observe the picker.
    pub fn connect_changed<F: Fn(u8, u8, u8) + 'static>(&self, f: F) {
        self.on_change.borrow_mut().push(Box::new(f));
    }
}

impl Default for ColourPicker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct Handle {
    hsv: Rc<Cell<(f64, f64, f64)>>,
    hex: gtk::Entry,
    plane: gtk::DrawingArea,
    strip: gtk::DrawingArea,
    on_change: Callbacks,
    quiet: Rc<Cell<bool>>,
}

impl Handle {
    fn refresh(&self, notify: bool) {
        let (h, s, v) = self.hsv.get();
        let (r, g, b) = hsv_to_rgb(h, s, v);
        self.hex.set_text(&format!("#{r:02x}{g:02x}{b:02x}"));
        self.plane.queue_draw();
        self.strip.queue_draw();
        if notify && !self.quiet.get() {
            for cb in self.on_change.borrow().iter() {
                cb(r, g, b);
            }
        }
    }
}
