//! Lighting page.
//!
//! The two LEDs (`[39]` scroll wheel, `[43]` logo) are independent, but most
//! people want them the same, so the page offers Off / Unified / Split rather
//! than making per-LED editing the only path.
//!
//! The effect is global -- all six colour records carry one mode byte -- and
//! Color Shift cycles hue on its own, so the colour controls are disabled while
//! it is selected rather than left there implying they do something.

use super::colour_picker::ColourPicker;
use super::mouse_preview::PreviewState;
use crate::hardware::{Effect, LedMode, Profile, EFFECTS};
use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lighting {
    Off,
    Unified,
    Split,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Wheel,
    Logo,
}

fn effect_blurb(effect: Effect) -> &'static str {
    match effect {
        Effect::Solid => "A steady colour.",
        Effect::Blinking => "Switches on and off sharply.",
        Effect::Pulsating => "Fades quickly in and out.",
        Effect::Breathing => "Fades slowly in and out.",
    }
}

struct Swatch {
    button: gtk::ToggleButton,
    area: gtk::DrawingArea,
    rgb: Rc<Cell<(u8, u8, u8)>>,
}

impl Swatch {
    fn new(label: &str) -> Self {
        let rgb = Rc::new(Cell::new((0u8, 0u8, 0u8)));
        let area = gtk::DrawingArea::builder().content_width(26).content_height(26).build();
        area.add_css_class("castty-swatch");
        {
            let rgb = rgb.clone();
            area.set_draw_func(move |_, cr, w, h| {
                let (r, g, b) = rgb.get();
                cr.set_source_rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
                cr.rectangle(0.0, 0.0, w as f64, h as f64);
                let _ = cr.fill();
            });
        }
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        inner.append(&area);
        inner.append(&gtk::Label::new(Some(label)));
        let button = gtk::ToggleButton::builder().child(&inner).build();
        Swatch { button, area, rgb }
    }

    fn set(&self, rgb: (u8, u8, u8)) {
        self.rgb.set(rgb);
        self.area.queue_draw();
    }

    fn get(&self) -> (u8, u8, u8) {
        self.rgb.get()
    }
}

pub struct LedPage {
    pub widget: adw::PreferencesPage,
    picker: Rc<ColourPicker>,
    picker_group: adw::PreferencesGroup,
    effect: adw::ComboRow,
    rainbow: adw::SwitchRow,
    lighting: Rc<Cell<Lighting>>,
    target: Rc<Cell<Target>>,
    wheel: Rc<Swatch>,
    logo: Rc<Swatch>,
    split_row: gtk::Box,
    off_btn: gtk::ToggleButton,
    unified_btn: gtk::ToggleButton,
    split_btn: gtk::ToggleButton,
}

impl Default for LedPage {
    fn default() -> Self {
        Self::new()
    }
}

impl LedPage {
    pub fn new() -> Self {
        let page = adw::PreferencesPage::builder().build();

        // --- mode selector -------------------------------------------------
        let off_btn = gtk::ToggleButton::with_label("Off");
        let unified_btn = gtk::ToggleButton::with_label("Unified");
        let split_btn = gtk::ToggleButton::with_label("Split");
        unified_btn.set_group(Some(&off_btn));
        split_btn.set_group(Some(&off_btn));
        unified_btn.set_active(true);

        let selector = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        selector.add_css_class("linked");
        selector.set_halign(gtk::Align::Center);
        for b in [&off_btn, &unified_btn, &split_btn] {
            b.set_hexpand(true);
            selector.append(b);
        }

        let wheel = Rc::new(Swatch::new("Scroll wheel"));
        let logo = Rc::new(Swatch::new("Logo"));
        logo.button.set_group(Some(&wheel.button));
        wheel.button.set_active(true);

        let split_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        split_row.set_halign(gtk::Align::Center);
        split_row.append(&wheel.button);
        split_row.append(&logo.button);
        split_row.set_visible(false);

        let picker = Rc::new(ColourPicker::new());

        let holder = gtk::Box::new(gtk::Orientation::Vertical, 12);
        holder.set_margin_top(4);
        holder.set_margin_bottom(4);
        holder.append(&selector);
        holder.append(&split_row);
        holder.append(&picker.widget);

        let picker_group = adw::PreferencesGroup::builder().title("Lighting").build();
        let row = adw::PreferencesRow::builder().activatable(false).build();
        row.set_child(Some(&holder));
        picker_group.add(&row);
        page.add(&picker_group);

        // --- effect --------------------------------------------------------
        let refs: Vec<&str> = EFFECTS.iter().map(|e| e.label()).collect();
        let effect = adw::ComboRow::builder()
            .title("Effect")
            .subtitle(effect_blurb(Effect::Solid))
            .model(&gtk::StringList::new(&refs))
            .build();
        // Rainbow is an independent flag in the mode byte's high nibble, so it
        // layers onto any effect rather than being one of them.
        let rainbow = adw::SwitchRow::builder()
            .title("Rainbow")
            .subtitle("Cycle through the spectrum; the chosen colour is not used")
            .build();
        let effect_group = adw::PreferencesGroup::builder().build();
        effect_group.add(&effect);
        effect_group.add(&rainbow);
        page.add(&effect_group);

        let me = LedPage {
            widget: page,
            picker,
            picker_group,
            effect,
            rainbow,
            lighting: Rc::new(Cell::new(Lighting::Unified)),
            target: Rc::new(Cell::new(Target::Wheel)),
            wheel,
            logo,
            split_row,
            off_btn,
            unified_btn,
            split_btn,
        };
        me.wire();
        me
    }

    fn wire(&self) {
        // picker -> swatches
        {
            let lighting = self.lighting.clone();
            let target = self.target.clone();
            let wheel = self.wheel.clone();
            let logo = self.logo.clone();
            self.picker.connect_changed(move |r, g, b| match lighting.get() {
                Lighting::Off => {}
                Lighting::Unified => {
                    wheel.set((r, g, b));
                    logo.set((r, g, b));
                }
                Lighting::Split => match target.get() {
                    Target::Wheel => wheel.set((r, g, b)),
                    Target::Logo => logo.set((r, g, b)),
                },
            });
        }

        // lighting mode
        let setup = |btn: &gtk::ToggleButton, value: Lighting, me: &LedPage| {
            let lighting = me.lighting.clone();
            let split_row = me.split_row.clone();
            let picker = me.picker.clone();
            let wheel = me.wheel.clone();
            let rainbow = me.rainbow.clone();
            let group = me.picker_group.clone();
            btn.connect_toggled(move |b| {
                if !b.is_active() {
                    return;
                }
                lighting.set(value);
                split_row.set_visible(value == Lighting::Split);
                if value == Lighting::Split {
                    let c = wheel.get();
                    picker.set_rgb(c.0, c.1, c.2);
                }
                update_sensitivity(&group, &picker, value, rainbow.is_active());
            });
        };
        setup(&self.off_btn, Lighting::Off, self);
        setup(&self.unified_btn, Lighting::Unified, self);
        setup(&self.split_btn, Lighting::Split, self);

        // split target
        {
            let target = self.target.clone();
            let picker = self.picker.clone();
            let wheel = self.wheel.clone();
            self.wheel.button.connect_toggled(move |b| {
                if b.is_active() {
                    target.set(Target::Wheel);
                    let c = wheel.get();
                    picker.set_rgb(c.0, c.1, c.2);
                }
            });
        }
        {
            let target = self.target.clone();
            let picker = self.picker.clone();
            let logo = self.logo.clone();
            self.logo.button.connect_toggled(move |b| {
                if b.is_active() {
                    target.set(Target::Logo);
                    let c = logo.get();
                    picker.set_rgb(c.0, c.1, c.2);
                }
            });
        }

        // effect only changes the blurb; rainbow is what disables the colour
        self.effect.connect_selected_notify(move |row| {
            let effect = EFFECTS.get(row.selected() as usize).copied().unwrap_or(Effect::Solid);
            row.set_subtitle(effect_blurb(effect));
        });
        {
            let picker = self.picker.clone();
            let lighting = self.lighting.clone();
            let group = self.picker_group.clone();
            self.rainbow.connect_active_notify(move |sw| {
                update_sensitivity(&group, &picker, lighting.get(), sw.is_active());
            });
        }
    }

    pub fn load(&self, profile: &Profile) {
        let w = profile.wheel();
        let l = profile.logo();
        self.wheel.set((w.r, w.g, w.b));
        self.logo.set((l.r, l.g, l.b));

        let dark = |c: (u8, u8, u8)| c == (0, 0, 0);
        let mode = if dark((w.r, w.g, w.b)) && dark((l.r, l.g, l.b)) {
            Lighting::Off
        } else if (w.r, w.g, w.b) == (l.r, l.g, l.b) {
            Lighting::Unified
        } else {
            Lighting::Split
        };
        match mode {
            Lighting::Off => self.off_btn.set_active(true),
            Lighting::Unified => self.unified_btn.set_active(true),
            Lighting::Split => self.split_btn.set_active(true),
        }
        self.picker.set_rgb(w.r, w.g, w.b);

        let led_mode = profile.leds[0].mode;
        let index = EFFECTS.iter().position(|e| *e == led_mode.effect()).unwrap_or(0);
        self.effect.set_selected(index as u32);
        self.rainbow.set_active(led_mode.rainbow());
    }

    pub fn store(&self, profile: &mut Profile) {
        let (w, l) = match self.lighting.get() {
            Lighting::Off => ((0, 0, 0), (0, 0, 0)),
            Lighting::Unified => {
                let c = self.picker.rgb();
                (c, c)
            }
            Lighting::Split => (self.wheel.get(), self.logo.get()),
        };
        profile.set_wheel_colour(w.0, w.1, w.2);
        profile.set_logo_colour(l.0, l.1, l.2);
        // The four non-physical records are inert on this device -- verified by
        // setting them to a colour no LED used and finding it never appeared --
        // but the vendor keeps them in step with the wheel, so we do too. It
        // costs nothing and keeps our frames byte-identical to theirs.
        for led in profile.leds.iter_mut().skip(2) {
            (led.r, led.g, led.b) = w;
        }
        profile.set_mode(self.selected_mode());
    }

    pub fn selected_mode(&self) -> LedMode {
        let effect = EFFECTS
            .get(self.effect.selected() as usize)
            .copied()
            .unwrap_or(Effect::Solid);
        LedMode::new(effect, self.rainbow.is_active())
    }

    pub fn preview_state(&self) -> PreviewState {
        let (wheel, logo) = match self.lighting.get() {
            Lighting::Off => ((0, 0, 0), (0, 0, 0)),
            Lighting::Unified => {
                let c = self.picker.rgb();
                (c, c)
            }
            Lighting::Split => (self.wheel.get(), self.logo.get()),
        };
        PreviewState { wheel, logo, mode: self.selected_mode() }
    }

    pub fn connect_changed<F: Fn() + Clone + 'static>(&self, f: F) {
        let a = f.clone();
        self.picker.connect_changed(move |_, _, _| a());
        let b = f.clone();
        self.effect.connect_selected_notify(move |_| b());
        let r = f.clone();
        self.rainbow.connect_active_notify(move |_| r());
        for btn in [&self.off_btn, &self.unified_btn, &self.split_btn] {
            let c = f.clone();
            btn.connect_toggled(move |_| c());
        }
        let d = f.clone();
        self.wheel.button.connect_toggled(move |_| d());
        self.logo.button.connect_toggled(move |_| f());
    }
}

/// Colour is meaningless when the LEDs are off or the effect ignores it.
fn update_sensitivity(
    group: &adw::PreferencesGroup,
    picker: &ColourPicker,
    lighting: Lighting,
    rainbow: bool,
) {
    picker.widget.set_sensitive(lighting != Lighting::Off && !rainbow);
    group.set_description(match (lighting, rainbow) {
        (Lighting::Off, _) => Some("Both LEDs are off."),
        (_, true) => Some("Rainbow cycles the spectrum; the colour below is ignored."),
        (Lighting::Split, false) => Some("Pick a colour for each LED separately."),
        _ => Some("One colour for both LEDs."),
    });
}
