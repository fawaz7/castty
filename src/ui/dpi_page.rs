//! Sensor page: the three DPI steps, polling rate, and the two angle settings.
//!
//! The mouse stores exactly three DPI steps and its DPI button cycles between
//! them, so they are presented as three equal slots rather than a list you can
//! grow.
//!
//! X and Y are stored separately and can genuinely differ -- captured with
//! X=850 / Y=3200. Nothing on the device records whether they are linked; the
//! vendor software's "link" checkbox is purely a UI convenience, so the same
//! switch here is local state and writes no flag.

use crate::hardware::{surface_score, DpiStep, PollingRate, Profile};
use gtk4 as gtk;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Callback fired when the user asks for a surface measurement.
type AnalyzeHook = Rc<RefCell<Option<Box<dyn Fn()>>>>;
use libadwaita as adw;
use adw::prelude::*;

/// Every DPI value seen in capture was a multiple of 50.
const DPI_STEP_GRANULARITY: f64 = 50.0;
const DPI_MIN: f64 = 100.0;
const DPI_MAX: f64 = 10_000.0;

/// The vendor tool measures for a fixed window and reads the result itself.
const MEASURE_SECONDS: u32 = 10;

const RATES: [PollingRate; 4] = [
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
];

pub struct DpiPage {
    pub widget: adw::PreferencesPage,
    steps_x: [adw::SpinRow; 3],
    steps_y: [adw::SpinRow; 3],
    link: adw::SwitchRow,
    polling: adw::ComboRow,
    snapping: adw::SpinRow,
    tuning: adw::SpinRow,
    lift_off: adw::SpinRow,
    analyze: gtk::Button,
    surface_row: adw::ActionRow,
    on_start: AnalyzeHook,
    on_result: AnalyzeHook,
    /// Seconds left in the running measurement, or None when idle.
    countdown: Rc<Cell<Option<u32>>>,
}

impl Default for DpiPage {
    fn default() -> Self {
        Self::new()
    }
}

impl DpiPage {
    pub fn new() -> Self {
        let page = adw::PreferencesPage::builder().build();

        let dpi_group = adw::PreferencesGroup::builder()
            .title("DPI steps")
            .description("The DPI button on the mouse cycles between these three.")
            .build();

        let link = adw::SwitchRow::builder()
            .title("Link X and Y")
            .subtitle("Use one value per step for both axes")
            .active(true)
            .build();
        dpi_group.add(&link);

        let mut xs = Vec::with_capacity(3);
        let mut ys = Vec::with_capacity(3);
        for i in 0..3 {
            let x = adw::SpinRow::with_range(DPI_MIN, DPI_MAX, DPI_STEP_GRANULARITY);
            x.set_title(&format!("Step {}", i + 1));
            dpi_group.add(&x);
            let y = adw::SpinRow::with_range(DPI_MIN, DPI_MAX, DPI_STEP_GRANULARITY);
            y.set_title(&format!("Step {} — Y", i + 1));
            y.set_visible(false);
            dpi_group.add(&y);
            xs.push(x);
            ys.push(y);
        }
        let steps_x: [adw::SpinRow; 3] = xs.try_into().expect("three steps");
        let steps_y: [adw::SpinRow; 3] = ys.try_into().expect("three steps");
        page.add(&dpi_group);

        let sensor_group = adw::PreferencesGroup::builder().title("Sensor").build();

        let labels: Vec<String> = RATES.iter().map(|r| format!("{} Hz", r.hz())).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let polling = adw::ComboRow::builder()
            .title("Polling rate")
            .subtitle("How often the mouse reports its position")
            .model(&gtk::StringList::new(&refs))
            .build();
        sensor_group.add(&polling);

        let snapping = adw::SpinRow::with_range(0.0, 15.0, 1.0);
        snapping.set_title("Angle snapping");
        snapping.set_subtitle("Straightens near-straight movement. 0 disables it");
        sensor_group.add(&snapping);

        let tuning = adw::SpinRow::with_range(-30.0, 30.0, 1.0);
        tuning.set_title("Angle tuning");
        tuning.set_subtitle("Rotates the sensor axis, in degrees");
        sensor_group.add(&tuning);

        let lift_off = adw::SpinRow::with_range(1.0, 31.0, 1.0);
        lift_off.set_title("Lift-off distance");
        lift_off.set_subtitle("How far the mouse can be raised before it stops tracking");
        sensor_group.add(&lift_off);

        // The analyzer is a device command, not a stored setting, so it sits in
        // its own group rather than among the fields Apply writes.
        // Mionix's S.Q.A.T. measures data loss between successive sensor images.
        // It needs the mouse moved across the surface while running, so starting
        // and reading are two steps -- reading straight away measures nothing.
        let surface_group = adw::PreferencesGroup::builder()
            .title("Surface analyzer")
            .description(
                "Measures how well the sensor reads the surface under it. Start the test, \
                 move the mouse over as much of the surface as you can, then show the result.",
            )
            .build();
        let analyze = gtk::Button::builder()
            .label("Start")
            .valign(gtk::Align::Center)
            .build();
        let surface_row = adw::ActionRow::builder()
            .title("Surface quality")
            .subtitle("Not measured yet")
            .build();
        surface_row.add_suffix(&analyze);
        surface_row.set_activatable_widget(Some(&analyze));
        surface_group.add(&surface_row);

        page.add(&sensor_group);
        page.add(&surface_group);

        let me = DpiPage {
            widget: page,
            steps_x,
            steps_y,
            link,
            polling,
            snapping,
            tuning,
            lift_off,
            analyze,
            surface_row,
            on_start: Rc::new(RefCell::new(None)),
            on_result: Rc::new(RefCell::new(None)),
            countdown: Rc::new(Cell::new(None)),
        };
        me.wire();
        me
    }

    fn wire(&self) {
        {
            let on_start = self.on_start.clone();
            let on_result = self.on_result.clone();
            let row = self.surface_row.clone();
            let countdown = self.countdown.clone();
            self.analyze.connect_clicked(move |btn| {
                btn.set_sensitive(false);
                countdown.set(Some(MEASURE_SECONDS));
                row.set_subtitle(&format!(
                    "Move the mouse over the surface\u{2026} {MEASURE_SECONDS}"
                ));
                if let Some(f) = on_start.borrow().as_ref() {
                    f();
                }

                // The vendor tool runs a fixed ~10 s window and then reads the
                // result itself, so do the same rather than asking the user to
                // press a second button.
                let row = row.clone();
                let countdown = countdown.clone();
                let on_result = on_result.clone();
                glib::timeout_add_seconds_local(1, move || {
                    let Some(left) = countdown.get() else {
                        return glib::ControlFlow::Break;
                    };
                    let left = left.saturating_sub(1);
                    if left == 0 {
                        countdown.set(None);
                        row.set_subtitle("Reading result\u{2026}");
                        if let Some(f) = on_result.borrow().as_ref() {
                            f();
                        }
                        glib::ControlFlow::Break
                    } else {
                        countdown.set(Some(left));
                        row.set_subtitle(&format!(
                            "Move the mouse over the surface\u{2026} {left}"
                        ));
                        glib::ControlFlow::Continue
                    }
                });
            });
        }
    }

    pub fn load(&self, profile: &Profile) {
        for ((x, y), step) in self
            .steps_x
            .iter()
            .zip(self.steps_y.iter())
            .zip(profile.dpi.iter())
        {
            x.set_value(step.x as f64);
            y.set_value(step.y as f64);
        }
        // Infer the link state from the data, since the device stores no flag.
        let linked = profile.dpi.iter().all(|s| s.x == s.y);
        self.link.set_active(linked);
        for y in &self.steps_y {
            y.set_visible(!linked);
        }
        let index = profile
            .polling
            .and_then(|p| RATES.iter().position(|r| *r == p))
            .unwrap_or(RATES.len() - 1);
        self.polling.set_selected(index as u32);
        self.snapping.set_value(profile.angle_snapping as f64);
        self.tuning.set_value(profile.angle_tuning as f64);
        self.lift_off.set_value(profile.lift_off as f64);
    }

    pub fn store(&self, profile: &mut Profile) {
        let linked = self.link.is_active();
        for ((x, y), step) in self
            .steps_x
            .iter()
            .zip(self.steps_y.iter())
            .zip(profile.dpi.iter_mut())
        {
            let xv = x.value().round() as u16;
            let yv = if linked { xv } else { y.value().round() as u16 };
            *step = DpiStep { x: xv, y: yv };
        }
        if let Some(rate) = RATES.get(self.polling.selected() as usize) {
            profile.polling = Some(*rate);
        }
        profile.angle_snapping = self.snapping.value().round() as u8;
        profile.angle_tuning = self.tuning.value().round() as i8;
        profile.lift_off = self.lift_off.value().round() as u8;
    }

    /// The page does no device I/O itself; the window routes these to the worker.
    pub fn connect_surface_start<F: Fn() + 'static>(&self, f: F) {
        *self.on_start.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_surface_result<F: Fn() + 'static>(&self, f: F) {
        *self.on_result.borrow_mut() = Some(Box::new(f));
    }

    /// Show a completed measurement, or clear it if the read failed.
    pub fn set_surface_result(&self, value: Option<u8>) {
        self.analyze.set_sensitive(true);
        self.countdown.set(None);
        self.surface_row.set_subtitle(&match value {
            Some(0) => "0 / 10 — no reading. Did the mouse move?".to_string(),
            Some(v) => {
                let score = surface_score(v);
                let verdict = if score >= 8 {
                    " — excellent"
                } else if score >= 6 {
                    " — good"
                } else if score >= 4 {
                    " — usable"
                } else {
                    " — poor tracking"
                };
                let _ = v;
                format!("{score} / 10{verdict}")
            }
            None => "Measurement failed".to_string(),
        });
    }

    pub fn connect_changed<F: Fn() + Clone + 'static>(&self, f: F) {
        for row in self.steps_x.iter().chain(self.steps_y.iter()) {
            let g = f.clone();
            row.connect_value_notify(move |_| g());
        }
        let l = f.clone();
        self.link.connect_active_notify(move |_| l());
        let a = f.clone();
        self.polling.connect_selected_notify(move |_| a());
        let b = f.clone();
        self.snapping.connect_value_notify(move |_| b());
        let c = f.clone();
        self.tuning.connect_value_notify(move |_| c());
        self.lift_off.connect_value_notify(move |_| f());
    }
}
