use castty::hardware::Profile;
use std::fs;

fn factory() -> Profile {
    let raw = fs::read("captures/factory-default-p0.bin").expect("fixture");
    Profile::decode(&raw).expect("decode")
}

/// Off / Unified / Split is inferred from the colours, because the device
/// stores no such setting -- it stores two colours and nothing else.
#[test]
fn lighting_mode_is_inferred_from_the_colours() {
    use castty::iced_ui::pages::lighting::{Mode, State};

    let mut profile = factory();

    profile.set_wheel_colour(0, 0, 0);
    profile.set_logo_colour(0, 0, 0);
    assert_eq!(State::from_profile(&profile).mode, Mode::Off);

    profile.set_wheel_colour(255, 0, 0);
    profile.set_logo_colour(255, 0, 0);
    assert_eq!(State::from_profile(&profile).mode, Mode::Unified);

    profile.set_wheel_colour(255, 0, 0);
    profile.set_logo_colour(0, 0, 255);
    assert_eq!(State::from_profile(&profile).mode, Mode::Split);
}

use castty::hardware::{DpiStep, PollingRate};
use castty::iced_ui::pages::sensor;

/// The link toggle is presentation only -- the device stores no such flag --
/// so it must be inferred from whether the axes actually match.
#[test]
fn sensor_infers_the_link_from_the_data() {
    let mut profile = factory();
    profile.dpi = [DpiStep::linked(800), DpiStep::linked(1600), DpiStep::linked(3200)];
    assert!(sensor::State::from_profile(&profile).linked);

    profile.dpi[0] = DpiStep { x: 850, y: 3200 };
    assert!(!sensor::State::from_profile(&profile).linked);
}

/// Linked means Y follows X on write, whatever Y happened to hold.
#[test]
fn linked_axes_write_the_same_value() {
    let mut profile = factory();
    let mut state = sensor::State::from_profile(&profile);
    state.linked = true;
    state.dpi = [800, 1600, 3200];
    state.dpi_y = [1, 2, 3];
    state.apply_to(&mut profile);

    for (i, expected) in [800u16, 1600, 3200].into_iter().enumerate() {
        assert_eq!(profile.dpi[i], DpiStep::linked(expected), "step {i}");
    }
}

#[test]
fn unlinked_axes_write_separately() {
    let mut profile = factory();
    let mut state = sensor::State::from_profile(&profile);
    state.linked = false;
    state.dpi = [800, 1600, 3200];
    state.dpi_y = [400, 1600, 6400];
    state.apply_to(&mut profile);

    assert_eq!(profile.dpi[0], DpiStep { x: 800, y: 400 });
    assert_eq!(profile.dpi[2], DpiStep { x: 3200, y: 6400 });
}

/// Ranges come from the hardware: snapping 0-15, tuning -30..=30, lift-off 1-31.
#[test]
fn sensor_values_round_trip_through_a_profile() {
    let mut profile = factory();
    let mut state = sensor::State::from_profile(&profile);
    state.polling = PollingRate::Hz125;
    state.snapping = 15;
    state.tuning = -30;
    state.lift_off = 31;
    state.apply_to(&mut profile);

    let back = sensor::State::from_profile(&profile);
    assert_eq!(back.polling, PollingRate::Hz125);
    assert_eq!(back.snapping, 15);
    assert_eq!(back.tuning, -30);
    assert_eq!(back.lift_off, 31);
}

/// The analyzer counts down and then asks for the reading; the page must not
/// report a result before the window is over.
#[test]
fn analyzer_counts_down_before_reading() {
    let profile = factory();
    let mut state = sensor::State::from_profile(&profile);

    assert!(state.update(sensor::Message::AnalyzeStarted));
    assert_eq!(state.countdown, Some(10));

    for expected in (1..10).rev() {
        assert!(!state.update(sensor::Message::Tick));
        assert_eq!(state.countdown, Some(expected));
    }
    // The last tick ends the window and asks for the reading.
    assert!(state.update(sensor::Message::Tick));
    assert_eq!(state.countdown, None);
}
