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

/// Pressing Start again mid-countdown must not reset the window or re-send
/// the start command -- the view disabling the button is presentation only,
/// this is the state-level backstop.
#[test]
fn analyzer_ignores_a_second_start_mid_countdown() {
    let profile = factory();
    let mut state = sensor::State::from_profile(&profile);

    assert!(state.update(sensor::Message::AnalyzeStarted));
    assert_eq!(state.countdown, Some(10));

    assert!(!state.update(sensor::Message::Tick));
    assert_eq!(state.countdown, Some(9));

    assert!(!state.update(sensor::Message::AnalyzeStarted));
    assert_eq!(state.countdown, Some(9));
}

use castty::hardware::{ButtonAction, MacroEvent, BUTTONS};
use castty::iced_ui::pages::buttons;
use castty::macros::{Library, NamedMacro, Timing};

fn library_with(name: &str) -> Library {
    let mut library = Library::default();
    library.put(NamedMacro {
        name: name.to_string(),
        timing: Timing::None,
        events: vec![MacroEvent { key: 0x04, pressed: true, delay_ms: 0 }],
    });
    library
}

/// A macro on the device is only an event list, so it is recognised by matching
/// those events against the library.
#[test]
fn buttons_recognise_a_library_macro_by_its_events() {
    let library = library_with("Reload");
    let mut profile = factory();
    let mut macros: [Option<castty::hardware::Macro>; 6] = Default::default();
    macros[3] = Some(library.find("Reload").unwrap().to_device());
    profile.set_macros(&macros).unwrap();

    let state = buttons::State::from_profile(&profile, &library);
    assert_eq!(state.slots[3], buttons::Slot::Library("Reload".into()));
}

/// A macro that is not in the library stays on the mouse; it must be carried
/// through rather than wiped.
#[test]
fn buttons_keep_a_macro_that_is_not_in_the_library() {
    let library = Library::default();
    let mut profile = factory();
    let mut macros: [Option<castty::hardware::Macro>; 6] = Default::default();
    macros[4] = Some(castty::hardware::Macro {
        events: vec![MacroEvent { key: 0x05, pressed: true, delay_ms: 0 }],
        hold: false,
    });
    profile.set_macros(&macros).unwrap();

    let state = buttons::State::from_profile(&profile, &library);
    assert_eq!(state.slots[4], buttons::Slot::Keep);

    // Applying must not clear it.
    let mut written = profile.clone();
    state.apply_to(&mut written, &library);
    assert!(matches!(written.buttons[4], ButtonAction::Macro { .. }));
}

#[test]
fn buttons_round_trip_plain_actions() {
    let library = Library::default();
    let mut profile = factory();
    let mut state = buttons::State::from_profile(&profile, &library);
    state.slots[1] = buttons::Slot::Action(ButtonAction::Disabled);
    state.slots[5] = buttons::Slot::Action(ButtonAction::ProfileSwitch(0xf0));
    state.apply_to(&mut profile, &library);

    let back = buttons::State::from_profile(&profile, &library);
    assert_eq!(back.slots[1], buttons::Slot::Action(ButtonAction::Disabled));
    assert_eq!(back.slots[5], buttons::Slot::Action(ButtonAction::ProfileSwitch(0xf0)));
    assert_eq!(BUTTONS.len(), 6);
}

use castty::iced_ui::pages::macros as macros_page;

/// Saving a draft under a new name must move the entry, not leave a copy.
#[test]
fn renaming_a_macro_moves_it() {
    let mut library = library_with("Old");
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::Edit("Old".into()), &mut library);
    state.update(macros_page::Message::NameChanged("New".into()), &mut library);
    assert!(state.update(macros_page::Message::Save, &mut library));

    assert!(library.find("Old").is_none(), "the old name should be gone");
    assert!(library.find("New").is_some(), "the new name should exist");
}

/// Hold macros store presses only, so switching modes invalidates a recording.
#[test]
fn switching_to_hold_clears_the_recording() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, false), &mut library);
    assert_eq!(state.draft_events(), 2);

    state.update(macros_page::Message::TimingChanged(Timing::Hold), &mut library);
    assert_eq!(state.draft_events(), 0, "a timed recording is not a hold recording");
}

/// A macro with no name or no events cannot be referred to or played back.
#[test]
fn saving_requires_a_name_and_events() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::NameChanged(String::new()), &mut library);
    assert!(!state.update(macros_page::Message::Save, &mut library));
    assert!(library.macros.is_empty());
}

/// Storage is shared across the profile: 32 slots, each macro costing its
/// events plus a terminator.
#[test]
fn capacity_counts_terminators() {
    let library = library_with("One");
    let mut profile = factory();
    let mut macros: [Option<castty::hardware::Macro>; 6] = Default::default();
    macros[0] = Some(library.find("One").unwrap().to_device());
    profile.set_macros(&macros).unwrap();

    let (used, total) = macros_page::slots_used(&library, &profile);
    assert_eq!(total, 32);
    assert_eq!(used, 2, "one event plus its terminator");
}
