use castty::hardware::Profile;
use std::fs;

fn factory() -> Profile {
    let raw = fs::read("research/captures/factory-default-p0.bin").expect("fixture");
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
    state.apply_to(&mut written, &library).unwrap();
    let stored = written.macro_events(written.buttons[4]);
    assert_eq!(stored, vec![MacroEvent { key: 0x05, pressed: true, delay_ms: 0 }]);
}

#[test]
fn buttons_round_trip_plain_actions() {
    let library = Library::default();
    let mut profile = factory();
    let mut state = buttons::State::from_profile(&profile, &library);
    state.slots[1] = buttons::Slot::Action(ButtonAction::Disabled);
    state.slots[5] = buttons::Slot::Action(ButtonAction::ProfileSwitch(0xf0));
    state.apply_to(&mut profile, &library).unwrap();

    let back = buttons::State::from_profile(&profile, &library);
    assert_eq!(back.slots[1], buttons::Slot::Action(ButtonAction::Disabled));
    assert_eq!(back.slots[5], buttons::Slot::Action(ButtonAction::ProfileSwitch(0xf0)));
    assert_eq!(BUTTONS.len(), 6);
}

/// A rename on the Macros page must follow the name into whichever slot
/// pointed at it, without touching the profile at all.
#[test]
fn sync_follows_a_rename_into_the_matching_slot() {
    let mut state =
        buttons::State { slots: std::array::from_fn(|_| buttons::Slot::Action(ButtonAction::Disabled)) };
    state.slots[2] = buttons::Slot::Library("Old".into());

    state.sync(&castty::iced_ui::pages::macros::Change::Renamed {
        from: "Old".into(),
        to: "New".into(),
    });

    assert_eq!(state.slots[2], buttons::Slot::Library("New".into()));
}

/// A macro deleted from the library is still on the mouse until the button is
/// reassigned, so its slot must fall back to "keep", not lose the assignment.
#[test]
fn sync_turns_a_deleted_macros_slot_into_keep() {
    let mut state =
        buttons::State { slots: std::array::from_fn(|_| buttons::Slot::Action(ButtonAction::Disabled)) };
    state.slots[4] = buttons::Slot::Library("Gone".into());

    state.sync(&castty::iced_ui::pages::macros::Change::Deleted("Gone".into()));

    assert_eq!(state.slots[4], buttons::Slot::Keep);
}

/// A button edit the user made but has not applied yet must survive a library
/// change to an unrelated macro -- syncing must not fall back to rebuilding
/// from the profile, which would revert it.
#[test]
fn sync_leaves_an_unapplied_edit_untouched() {
    let mut state =
        buttons::State { slots: std::array::from_fn(|_| buttons::Slot::Action(ButtonAction::Disabled)) };
    state.slots[0] = buttons::Slot::Action(ButtonAction::Mouse(0x02));

    state.sync(&castty::iced_ui::pages::macros::Change::Renamed {
        from: "Old".into(),
        to: "New".into(),
    });

    assert_eq!(state.slots[0], buttons::Slot::Action(ButtonAction::Mouse(0x02)));
}

/// A slot assigned to a different macro must not react to a change that
/// doesn't name it.
#[test]
fn sync_leaves_slots_for_other_macros_untouched() {
    let mut state =
        buttons::State { slots: std::array::from_fn(|_| buttons::Slot::Action(ButtonAction::Disabled)) };
    state.slots[1] = buttons::Slot::Library("Other".into());

    state.sync(&castty::iced_ui::pages::macros::Change::Renamed {
        from: "Old".into(),
        to: "New".into(),
    });
    state.sync(&castty::iced_ui::pages::macros::Change::Deleted("Unrelated".into()));

    assert_eq!(state.slots[1], buttons::Slot::Library("Other".into()));
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

/// The same invariant holds crossing back out of hold mode.
#[test]
fn switching_from_hold_clears_the_recording() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::TimingChanged(Timing::Hold), &mut library);
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    assert_eq!(state.draft_events(), 1);

    state.update(macros_page::Message::TimingChanged(Timing::Delay), &mut library);
    assert_eq!(state.draft_events(), 0, "a hold recording is not a timed recording");
}

/// Hold mode records the press, not the release, and never a delay: the keys
/// stay down for as long as the button does, which is not decided here.
#[test]
fn hold_recording_stores_only_presses_with_zero_delay() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::TimingChanged(Timing::Hold), &mut library);
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, false), &mut library);
    state.update(macros_page::Message::KeyPressed(0x05, true), &mut library);

    let events = state.editing.as_ref().unwrap().events.clone();
    assert_eq!(events.len(), 2, "the release must not be stored in hold mode");
    assert!(events.iter().all(|e| e.pressed && e.delay_ms == 0));
}

/// The recorder ignores keys entirely outside a recording -- there is no
/// reachable Stop to end one, so a stray keystroke must not join the draft.
#[test]
fn key_presses_are_ignored_when_not_recording() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    assert_eq!(state.draft_events(), 0);
}

/// The first key of a recording has nothing to be delayed after.
#[test]
fn the_first_recorded_event_has_zero_delay() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    // Seed a stale `last` from well before the recording starts, so the
    // assertion actually constrains RecordToggled's reset rather than just
    // observing a fresh Draft's default: without the reset, the first event's
    // delay would be computed against this timestamp instead of `None`.
    state.editing.as_mut().unwrap().last =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(5));
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);

    let events = state.editing.as_ref().unwrap().events.clone();
    assert_eq!(events[0].delay_ms, 0);
}

/// A release with no matching press (a focused widget ate the press, but
/// iced's `listen()` still surfaces the release) must not be recorded as an
/// orphan "up" event.
#[test]
fn a_release_with_no_matching_press_is_not_recorded() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, false), &mut library);
    assert_eq!(state.draft_events(), 0);
}

/// Auto-repeat sends a fresh press for every tick a key is held; only the
/// first is a keystroke, so holding "a" must not record it twenty times.
#[test]
fn held_key_auto_repeat_is_not_recorded_as_repeated_presses() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    assert_eq!(state.draft_events(), 1, "repeats of a still-held key must not be recorded");

    state.update(macros_page::Message::KeyPressed(0x04, false), &mut library);
    assert_eq!(state.draft_events(), 2);
}

/// The auto-repeat guard must not swallow a genuine second tap: releasing a
/// key has to clear it from `down` so pressing it again is recorded.
#[test]
fn a_key_tapped_twice_records_both_presses() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, false), &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    assert_eq!(state.draft_events(), 3, "both presses and the release between them must be recorded");
}

/// `Timing::None` promises every delay is zero; leaving `Delay` must zero the
/// recorded gaps rather than either keeping them or discarding the recording.
#[test]
fn switching_to_no_timing_zeroes_recorded_delays_but_keeps_the_events() {
    let mut library = Library::default();
    let mut state = macros_page::State {
        editing: Some(macros_page::Draft {
            name: "Test".into(),
            timing: Timing::Delay,
            events: vec![
                MacroEvent { key: 0x04, pressed: true, delay_ms: 0 },
                MacroEvent { key: 0x04, pressed: false, delay_ms: 120 },
            ],
            original: None,
            last: None,
            ..Default::default()
        }),
        ..Default::default()
    };

    state.update(macros_page::Message::TimingChanged(Timing::None), &mut library);

    let events = state.editing.as_ref().unwrap().events.clone();
    assert_eq!(events.len(), 2, "the events must survive the switch");
    assert!(events.iter().all(|e| e.delay_ms == 0));
}

/// A macro recorded but never named cannot be referred to or played back.
#[test]
fn saving_requires_a_name() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    state.update(macros_page::Message::NameChanged("   ".into()), &mut library);
    state.update(macros_page::Message::RecordToggled, &mut library);
    state.update(macros_page::Message::KeyPressed(0x04, true), &mut library);
    assert!(!state.update(macros_page::Message::Save, &mut library));
    assert!(library.macros.is_empty());
}

/// A named draft with nothing recorded has nothing to play back.
#[test]
fn saving_requires_events() {
    let mut library = Library::default();
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::New, &mut library);
    assert!(!state.update(macros_page::Message::Save, &mut library));
    assert!(library.macros.is_empty());
}

/// Renaming onto a name already in the library must not destroy that other
/// macro -- `Library::put` replaces by name, so the rename has to be rejected
/// before it reaches `put`, not cleaned up after.
#[test]
fn renaming_onto_an_existing_name_is_rejected() {
    let mut library = Library::default();
    library.put(NamedMacro {
        name: "One".into(),
        timing: Timing::None,
        events: vec![MacroEvent { key: 0x04, pressed: true, delay_ms: 0 }],
    });
    library.put(NamedMacro {
        name: "Two".into(),
        timing: Timing::None,
        events: vec![MacroEvent { key: 0x05, pressed: true, delay_ms: 0 }],
    });
    let mut state = macros_page::State::default();

    state.update(macros_page::Message::Edit("Two".into()), &mut library);
    state.update(macros_page::Message::NameChanged("One".into()), &mut library);
    assert!(!state.update(macros_page::Message::Save, &mut library));

    let one = library.find("One").expect("the existing macro must survive");
    assert_eq!(one.events, vec![MacroEvent { key: 0x04, pressed: true, delay_ms: 0 }]);
    assert!(library.find("Two").is_some(), "the macro being edited must not be lost either");
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

    let (used, total) = macros_page::slots_used(&profile);
    assert_eq!(total, 32);
    assert_eq!(used, 2, "one event plus its terminator");
}

use castty::iced_ui::pages::profiles;

/// The name field on the device is ten bytes of ASCII, not ten characters --
/// a non-ASCII character truncated by `chars().take(10)` could still be more
/// than ten bytes and get cut mid-encode.
#[test]
fn profile_names_are_capped_at_ten_bytes() {
    let mut list = vec![factory(), factory()];
    profiles::rename(&mut list, 0, "a very long name indeed");
    assert_eq!(list[0].name.len(), 10);
    assert_eq!(list[0].name, "a very lon");
}

/// Non-ASCII characters are dropped rather than counted toward the ten
/// bytes, so what is shown in the field is exactly what the device stores.
#[test]
fn non_ascii_characters_are_dropped_from_the_name() {
    let mut list = vec![factory()];
    profiles::rename(&mut list, 0, "café \u{1F600} racer");
    assert!(list[0].name.is_ascii());
    assert_eq!(list[0].name.len(), 10);
    assert_eq!(list[0].name, "caf  racer");
}

#[test]
fn renaming_leaves_other_profiles_alone() {
    let mut list = vec![factory(), factory()];
    let before = list[1].name.clone();
    profiles::rename(&mut list, 0, "Gaming");
    assert_eq!(list[0].name, "Gaming");
    assert_eq!(list[1].name, before);
}

/// An out-of-range index must be ignored rather than panicking.
#[test]
fn renaming_an_unknown_profile_is_ignored() {
    let mut list = vec![factory()];
    profiles::rename(&mut list, 9, "Nope");
    assert_ne!(list[0].name, "Nope");
}

/// Apply must write every profile marked dirty, not only whichever one is
/// currently on screen -- a rename on a slot you are not editing, or a
/// restore, has to survive the next Apply.
#[test]
fn dirty_profiles_includes_every_marked_slot_not_just_the_active_one() {
    let list = vec![factory(), factory(), factory()];
    let dirty = [false, true, true];
    let picked = profiles::dirty_profiles(&list, &dirty);
    assert_eq!(picked.len(), 2);
    assert_eq!(picked[0].0, 1);
    assert_eq!(picked[1].0, 2);
    assert_eq!(picked[0].1.index, list[1].index);
    assert_eq!(picked[1].1.index, list[2].index);
}

#[test]
fn dirty_profiles_is_empty_when_nothing_changed() {
    let list = vec![factory()];
    let dirty = [false];
    assert!(profiles::dirty_profiles(&list, &dirty).is_empty());
}

/// A restore has to mark all five profiles dirty, or the fix above has
/// nothing to work with -- the reset happens in memory, but Apply only ever
/// writes what is flagged.
#[test]
fn restoring_defaults_marks_every_profile_dirty() {
    let mut list = vec![factory(), factory(), factory()];
    let mut dirty = [false, false, false];
    profiles::restore_defaults(&mut list, &mut dirty);
    assert!(dirty.iter().all(|&d| d));
}

/// Arming the restore and then cancelling must leave it disarmed without
/// ever reporting that it should fire.
#[test]
fn arming_then_cancelling_the_restore_disarms_it() {
    let mut state = profiles::State::default();
    assert!(!state.update(&profiles::Message::RestoreRequested));
    assert!(state.confirming());
    assert!(!state.update(&profiles::Message::RestoreCancelled));
    assert!(!state.confirming());
}

/// Arming and then confirming is the only path that reports the restore
/// should actually happen.
#[test]
fn arming_then_confirming_the_restore_fires_once() {
    let mut state = profiles::State::default();
    state.update(&profiles::Message::RestoreRequested);
    assert!(state.update(&profiles::Message::RestoreConfirmed));
    assert!(!state.confirming(), "firing must also disarm it");
}

/// A confirm with no prior arming -- for example a leftover message after
/// the page was left and re-entered -- must never fire.
#[test]
fn confirming_without_arming_does_not_fire() {
    let mut state = profiles::State::default();
    assert!(!state.update(&profiles::Message::RestoreConfirmed));
}
