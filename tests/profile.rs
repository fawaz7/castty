//! Tests run against blobs captured from the vendor application on real
//! hardware, so a passing encode is a frame the device has actually accepted.

use castty::hardware::{
    validate_read_reply, DpiStep, Effect, Led, LedMode, PollingRate, Profile, ReadError, DPI_MAX,
    DPI_MIN,
};
use std::fs;

fn fixture(name: &str) -> Vec<u8> {
    fs::read(format!("research/captures/{name}.bin"))
        .unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
}

#[test]
fn decodes_captured_profile() {
    let p = Profile::decode(&fixture("profile1-blue")).unwrap();
    assert_eq!(p.index, 0);
    assert_eq!(p.name, "Profile1");
    assert_eq!(p.dpi_step_count, 3);
    // the capture left both physical LEDs blue, solid
    assert_eq!(p.leds[0], Led { r: 0, g: 0, b: 255, mode: LedMode::new(Effect::Solid, false) });
    assert_eq!(p.leds[1], Led { r: 0, g: 0, b: 255, mode: LedMode::new(Effect::Solid, false) });
}

#[test]
fn round_trips_every_fixture_byte_exact() {
    for name in [
        "profile1-red", "profile1-green", "profile1-blue",
        "factory-default-p0", "factory-default-p1", "factory-default-p2",
        "factory-default-p3", "factory-default-p4",
    ] {
        let raw = fixture(name);
        let encoded = Profile::decode(&raw).unwrap().encode().unwrap();
        assert_eq!(encoded, raw, "round-trip changed bytes for {name}");
    }
}

/// The strongest check available without hardware: take the frame the vendor
/// app sent for red, recolour the two physical LEDs green, and require the
/// result to equal the frame it actually sent for green.
#[test]
fn recolouring_reproduces_the_captured_transition() {
    let mut p = Profile::decode(&fixture("profile1-red")).unwrap();
    for led in p.leds.iter_mut().take(2) {
        led.r = 0x00;
        led.g = 0xff;
        led.b = 0x00;
    }
    assert_eq!(p.encode().unwrap(), fixture("profile1-green"));
}

#[test]
fn factory_defaults_match_documented_values() {
    let p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    assert_eq!(p.dpi[0], DpiStep::linked(3000));
    assert_eq!(p.leds[0], Led { r: 201, g: 255, b: 0, mode: LedMode::new(Effect::Solid, false) });
    assert_eq!(p.polling, Some(PollingRate::Hz1000));
    assert_eq!(p.angle_snapping, 0);
    assert_eq!(p.angle_tuning, 0);
}

#[test]
fn angle_tuning_is_twos_complement() {
    let mut p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    for (value, byte) in [(-30i8, 0xe2u8), (-15, 0xf1), (0, 0x00), (15, 0x0f), (30, 0x1e)] {
        p.angle_tuning = value;
        assert_eq!(p.encode().unwrap()[104], byte, "angle tuning {value}");
    }
}

#[test]
fn polling_rates_map_to_divisors() {
    for (rate, hz, byte) in [
        (PollingRate::Hz1000, 1000, 1u8),
        (PollingRate::Hz500, 500, 2),
        (PollingRate::Hz250, 250, 4),
        (PollingRate::Hz125, 125, 8),
    ] {
        assert_eq!(rate.hz(), hz);
        assert_eq!(rate.to_byte(), byte);
        assert_eq!(PollingRate::from_byte(byte), Some(rate));
    }
}

#[test]
fn rejects_out_of_range_values() {
    let base = Profile::decode(&fixture("factory-default-p0")).unwrap();

    let mut p = base.clone();
    p.angle_snapping = 16;
    assert!(p.encode().is_err());

    let mut p = base.clone();
    p.angle_tuning = 31;
    assert!(p.encode().is_err());

    let mut p = base.clone();
    p.dpi[0] = DpiStep::linked(20_000);
    assert!(p.encode().is_err());
}

/// The macro region is unmapped; a read/modify/write must not disturb it.
#[test]
fn preserves_unmapped_macro_region() {
    let raw = fixture("profile1-blue");
    let mut p = Profile::decode(&raw).unwrap();
    p.set_all_colours(1, 2, 3);
    p.set_mode(LedMode::new(Effect::Blinking, false));
    let out = p.encode().unwrap();
    assert_eq!(&out[160..], &raw[160..]);
}

/// The name field is 10 bytes, not the 8 originally assumed -- a capture where
/// a profile was renamed to "testrename" is what caught it.
#[test]
fn decodes_full_length_profile_name() {
    let p = Profile::decode(&fixture("profile2-renamed")).unwrap();
    assert_eq!(p.name, "testrename");
    assert_eq!(p.encode().unwrap(), fixture("profile2-renamed"));
}

#[test]
fn renaming_round_trips() {
    let mut p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    for name in ["a", "Profile1", "testrename"] {
        p.name = name.to_string();
        let encoded = p.encode().unwrap();
        assert_eq!(Profile::decode(&encoded).unwrap().name, name);
    }
}

/// The logo and scroll wheel are independently settable; the capture that
/// proves it had them at different colours in the same frame.
#[test]
fn physical_leds_are_independent() {
    let p = Profile::decode(&fixture("profile1-split-leds")).unwrap();
    assert_ne!(
        (p.leds[0].r, p.leds[0].g, p.leds[0].b),
        (p.leds[1].r, p.leds[1].g, p.leds[1].b),
        "fixture should have the two LEDs at different colours"
    );
    assert_eq!(p.physical_leds().len(), 2);
}

/// The mode byte is two fields: low nibble selects the animation, high nibble
/// turns on rainbow. Established by observing the mouse -- 0x14 runs breathing
/// *and* colour cycling at once.
#[test]
fn mode_byte_splits_into_effect_and_rainbow() {
    for (effect, nibble) in [
        (Effect::Solid, 0x01u8),
        (Effect::Blinking, 0x02),
        (Effect::Pulsating, 0x03),
        (Effect::Breathing, 0x04),
    ] {
        let plain = LedMode::new(effect, false);
        assert_eq!(plain.to_byte(), nibble);
        assert_eq!(LedMode::from_byte(nibble), plain);

        let rainbow = LedMode::new(effect, true);
        assert_eq!(rainbow.to_byte(), 0x10 | nibble);
        assert_eq!(LedMode::from_byte(0x10 | nibble), rainbow);
        assert!(rainbow.rainbow());
        assert_eq!(rainbow.effect(), effect);
    }
    // bytes that fit neither field survive a round trip rather than being clamped
    assert_eq!(LedMode::from_byte(0x7f), LedMode::Unknown(0x7f));
    assert_eq!(LedMode::Unknown(0x7f).to_byte(), 0x7f);
}

/// The capture that identified which record drives which LED: the scroll wheel
/// was set to red on its own, leaving the logo dark.
#[test]
fn wheel_and_logo_map_to_the_right_records() {
    let p = Profile::decode(&fixture("profile1-split-leds")).unwrap();
    assert_eq!((p.wheel().r, p.wheel().g, p.wheel().b), (0, 0, 0));
    assert_eq!((p.logo().r, p.logo().g, p.logo().b), (255, 0, 0));

    let mut p2 = p.clone();
    p2.set_wheel_colour(1, 2, 3);
    p2.set_logo_colour(4, 5, 6);
    let out = p2.encode().unwrap();
    assert_eq!(&out[39..42], &[1, 2, 3], "wheel lives at [39]");
    assert_eq!(&out[43..46], &[4, 5, 6], "logo lives at [43]");
}

/// The three DPI slots are not evenly spaced -- other fields sit between them --
/// so pin the offsets rather than trusting a stride.
#[test]
fn dpi_steps_write_to_their_captured_offsets() {
    let mut p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    p.dpi = [
        DpiStep::linked(1000),
        DpiStep::linked(2000),
        DpiStep::linked(3000),
    ];
    let out = p.encode().unwrap();
    for (offset, value) in [(68usize, 1000u16), (77, 2000), (98, 3000)] {
        assert_eq!(
            u16::from_le_bytes([out[offset], out[offset + 1]]),
            value,
            "X at [{offset}]"
        );
        assert_eq!(
            u16::from_le_bytes([out[offset + 2], out[offset + 3]]),
            value,
            "Y at [{}]",
            offset + 2
        );
    }
    assert_eq!(Profile::decode(&out).unwrap().dpi, p.dpi);
}

/// Angle snapping is 0-15 and angle tuning is a signed -30..=30; both are
/// rejected outside those ranges rather than silently wrapping into the blob.
#[test]
fn sensor_settings_round_trip() {
    let mut p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    p.angle_snapping = 15;
    p.angle_tuning = -30;
    p.polling = Some(PollingRate::Hz125);
    let out = p.encode().unwrap();
    let back = Profile::decode(&out).unwrap();
    assert_eq!(back.angle_snapping, 15);
    assert_eq!(back.angle_tuning, -30);
    assert_eq!(back.polling, Some(PollingRate::Hz125));
}

/// Lift-off distance maps 1:1 onto the vendor slider's 31 steps. The vendor
/// app's "pointer speed" control writes nothing to the device -- it is an OS
/// setting -- so it has no field here by design.
#[test]
fn lift_off_round_trips_and_is_range_checked() {
    let mut p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    for value in [1u8, 15, 31] {
        p.lift_off = value;
        let out = p.encode().unwrap();
        assert_eq!(out[86], value, "lift-off lives at [86]");
        assert_eq!(Profile::decode(&out).unwrap().lift_off, value);
    }
    p.lift_off = 0;
    assert!(p.encode().is_err(), "0 is below the slider's range");
    p.lift_off = 32;
    assert!(p.encode().is_err(), "32 is above the slider's range");
}

/// The surface score curve was fitted to the vendor tool's own output: the same
/// measurement was run across five surfaces while capturing the raw byte behind
/// each displayed score.
#[test]
fn surface_score_matches_the_vendor_tool() {
    use castty::hardware::surface_score;
    for (raw, expected) in [(0u8, 0u8), (13, 2), (28, 5), (38, 7), (39, 7), (40, 7)] {
        assert_eq!(surface_score(raw), expected, "raw {raw}");
    }
    // never report outside Mionix's 1-10 scale, whatever the device sends
    assert_eq!(surface_score(255), 10);
}

/// The button table decode was settled by reassigning five buttons in the
/// vendor software at once and matching each entry to what was set.
#[test]
fn button_table_matches_the_captured_assignments() {
    use castty::hardware::{Button, ButtonAction, BUTTONS};
    let p = Profile::decode(&fixture("profile2-buttons")).unwrap();

    let expected = [
        (Button::Left, ButtonAction::Mouse(0x01)),   // untouched
        (Button::Right, ButtonAction::Disabled),     // set to "disable button"
        (Button::WheelClick, ButtonAction::Key(0x1a)), // single key: w
        (Button::SideFront, ButtonAction::Scroll(1)),  // scroll up
        (Button::SideRear, ButtonAction::Scroll(-1)),  // scroll down
        (Button::Dpi, ButtonAction::ProfileSwitch(0xf1)), // profile switch
    ];
    for (i, (button, action)) in expected.iter().enumerate() {
        assert_eq!(BUTTONS[i], *button);
        assert_eq!(p.buttons[i], *action, "{}", button.label());
    }
    assert_eq!(p.encode().unwrap(), fixture("profile2-buttons"));
}

#[test]
fn button_actions_round_trip_through_bytes() {
    use castty::hardware::ButtonAction;
    for action in [
        ButtonAction::Mouse(0x04),
        ButtonAction::Scroll(1),
        ButtonAction::Scroll(-1),
        ButtonAction::Key(0x1a),
        ButtonAction::Macro { ptr: 800, events: 6, hold: false },
        ButtonAction::Macro { ptr: 849, events: 1, hold: true },
        ButtonAction::ProfileSwitch(0xf0),
        ButtonAction::ProfileSwitch(0xf1),
        ButtonAction::ProfileSwitch(0xf2),
        ButtonAction::DpiSwitch(0xf1),
        ButtonAction::Disabled,
        ButtonAction::Unknown(0x42, 0x99),
    ] {
        assert_eq!(ButtonAction::from_entry(&action.to_entry()), action);
    }
}

/// Factory assignments, including the DPI button's non-obvious 0x10/0x08 order
/// for the two side buttons.
#[test]
fn factory_buttons_are_the_defaults() {
    use castty::hardware::BUTTONS;
    let p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    for (i, button) in BUTTONS.iter().enumerate() {
        assert_eq!(p.buttons[i], button.default_action(), "{}", button.label());
    }
}

/// The device stores standard HID usage codes: assigning `w` in the vendor
/// software wrote 0x1a, which is exactly what the HID table specifies.
#[test]
fn hid_keycodes_match_the_standard_table() {
    use castty::hardware::keycode;
    assert_eq!(keycode::from_keyval('w' as u32), Some(0x1a));
    assert_eq!(keycode::label(0x1a), "W");

    assert_eq!(keycode::from_keyval('a' as u32), Some(0x04));
    assert_eq!(keycode::from_keyval('z' as u32), Some(0x1d));
    assert_eq!(keycode::from_keyval('1' as u32), Some(0x1e));
    assert_eq!(keycode::from_keyval('0' as u32), Some(0x27));
    assert_eq!(keycode::from_keyval(0xffbe), Some(0x3a)); // F1
    assert_eq!(keycode::label(0x3a), "F1");
    assert_eq!(keycode::label(0x45), "F12");
    assert_eq!(keycode::from_keyval(0x0020), Some(0x2c)); // space
    assert_eq!(keycode::label(0x2c), "Space");

    // capitals map to the same key; the device stores keys, not characters
    assert_eq!(keycode::from_keyval('W' as u32), keycode::from_keyval('w' as u32));
    // unmapped keys are refused rather than stored as something meaningless
    assert_eq!(keycode::from_keyval(0xffe1), None); // Shift_L
}

/// Macros were captured by recording `a b c` on one button and `a`-`f` on
/// another, which pins the event size, the press/release pairing, and the fact
/// that the pointer is relative to the payload at [16] rather than the blob.
#[test]
fn macros_decode_from_the_captured_profile() {
    use castty::hardware::ButtonAction;
    let p = Profile::decode(&fixture("profile1-macros")).unwrap();

    let front = p.buttons[3];
    let rear = p.buttons[4];
    assert_eq!(front, ButtonAction::Macro { ptr: 800, events: 6, hold: false });
    assert_eq!(rear, ButtonAction::Macro { ptr: 849, events: 12, hold: false });

    // three keys, each recorded as a press and a release
    let a = p.macro_events(front);
    assert_eq!(a.len(), 6);
    let typed: Vec<char> = a
        .iter()
        .filter(|e| e.pressed)
        .map(|e| (b'a' + e.key - 0x04) as char)
        .collect();
    assert_eq!(typed, ['a', 'b', 'c']);
    assert!(a.iter().all(|e| e.delay_ms == 0), "recorded with delays off");

    let b = p.macro_events(rear);
    assert_eq!(b.len(), 12);
    let typed: Vec<char> = b
        .iter()
        .filter(|e| e.pressed)
        .map(|e| (b'a' + e.key - 0x04) as char)
        .collect();
    assert_eq!(typed, ['a', 'b', 'c', 'd', 'e', 'f']);

    // macros are packed with a 7-byte terminator between them
    assert_eq!(849, 800 + 7 * (6 + 1));
    assert_eq!(p.encode().unwrap(), fixture("profile1-macros"));
}

#[test]
fn macro_events_round_trip() {
    use castty::hardware::MacroEvent;
    for event in [
        MacroEvent { key: 0x04, pressed: true, delay_ms: 0 },
        MacroEvent { key: 0x1a, pressed: false, delay_ms: 2175 },
        MacroEvent { key: 0x05, pressed: true, delay_ms: MacroEvent::MAX_DELAY_MS },
    ] {
        assert_eq!(MacroEvent::from_bytes(&event.to_bytes()), Some(event));
    }
    // the inter-macro terminator is not an event
    assert_eq!(MacroEvent::from_bytes(&[0u8; 7]), None);
}

/// Delays are milliseconds since the previous event, verified against the
/// numbers the vendor editor displayed for the same recording: `a` down after
/// 2175 ms and held 119 ms, `b` down after 2701 ms and held 111 ms.
#[test]
fn macro_delays_are_milliseconds() {
    let p = Profile::decode(&fixture("profile1-macro-timing")).unwrap();

    let front = p.buttons[3];
    let events = p.macro_events(front);
    let timings: Vec<u32> = events.iter().map(|e| e.delay_ms).collect();
    assert_eq!(timings, vec![2175, 119, 2701, 111, 3927, 93]);

    // presses carry the gap the user paused for; releases carry the key's hold
    // time, which is why every other value is around a tenth of a second
    for pair in events.chunks(2) {
        assert!(pair[0].pressed && !pair[1].pressed);
        assert!(pair[0].delay_ms > pair[1].delay_ms);
    }
    assert_eq!(p.encode().unwrap(), fixture("profile1-macro-timing"));
}

/// "Record hold" makes a macro that holds its keys while the button is held:
/// one press event, no release, no timing.
#[test]
fn hold_macros_are_flagged_in_the_button_entry() {
    use castty::hardware::ButtonAction;
    let p = Profile::decode(&fixture("profile1-macro-timing")).unwrap();
    let rear = p.buttons[4];
    assert_eq!(rear, ButtonAction::Macro { ptr: 849, events: 1, hold: true });

    let events = p.macro_events(rear);
    assert_eq!(events.len(), 1);
    assert!(events[0].pressed, "hold macros record only the press");
    assert_eq!(events[0].delay_ms, 0);

    // the hold flag lives in byte 1 of the entry, which is 0 for a timed macro
    assert_eq!(rear.to_entry()[1], 0xfe);
    assert_eq!(p.buttons[3].to_entry()[1], 0x00);
}

/// Repacking must reproduce the vendor's own layout: sequential from the base
/// pointer, with a terminator between macros.
#[test]
fn repacking_macros_reproduces_the_vendor_layout() {
    use castty::hardware::{ButtonAction, Macro};
    let original = Profile::decode(&fixture("profile1-macros")).unwrap();
    let macros = original.macros();

    let mut rebuilt = Profile::decode(&fixture("factory-default-p0")).unwrap();
    rebuilt.set_macros(&macros).unwrap();

    assert_eq!(rebuilt.buttons[3], ButtonAction::Macro { ptr: 800, events: 6, hold: false });
    assert_eq!(rebuilt.buttons[4], ButtonAction::Macro { ptr: 849, events: 12, hold: false });
    assert_eq!(rebuilt.macro_events(rebuilt.buttons[3]), original.macro_events(original.buttons[3]));
    assert_eq!(rebuilt.macro_events(rebuilt.buttons[4]), original.macro_events(original.buttons[4]));

    // the byte-level layout should match the vendor's exactly
    let a = rebuilt.encode().unwrap();
    let b = original.encode().unwrap();
    assert_eq!(&a[816..], &b[816..], "macro storage differs from the vendor layout");

    // dropping a macro must not leave the button pointing into freed storage
    let mut cleared = rebuilt.clone();
    let mut without = macros.clone();
    without[3] = None;
    cleared.set_macros(&without).unwrap();
    assert!(!matches!(cleared.buttons[3], ButtonAction::Macro { .. }));
    // and the survivor moves to the base
    assert_eq!(cleared.buttons[4], ButtonAction::Macro { ptr: 800, events: 12, hold: false });
    let _ = Macro { events: vec![], hold: false };
}

/// Storage is shared and small, so overflow has to be refused rather than
/// silently corrupting the profile.
#[test]
fn macro_capacity_is_enforced() {
    use castty::hardware::{Macro, MacroEvent};
    let mut p = Profile::decode(&fixture("factory-default-p0")).unwrap();
    assert_eq!(p.macro_slots_free(), 32);

    let event = MacroEvent { key: 0x04, pressed: true, delay_ms: 0 };
    let fits = Macro { events: vec![event; 31], hold: false };
    let mut slots: [Option<Macro>; 6] = Default::default();
    slots[3] = Some(fits);
    p.set_macros(&slots).unwrap(); // 31 events + terminator = 32
    assert_eq!(p.macro_slots_free(), 0);

    let too_big = Macro { events: vec![event; 32], hold: false };
    let mut slots: [Option<Macro>; 6] = Default::default();
    slots[3] = Some(too_big);
    assert!(p.set_macros(&slots).is_err(), "33 slots must not fit in 32");
}

// ---------------------------------------------------------------------------
// The read path (0x60/0x07). `read-0x07-profile0..4` are real replies the
// device gave when each of its five profiles was read back out of flash.
// ---------------------------------------------------------------------------

/// Nothing is adopted that the validator has not passed, so the validator
/// accepting a genuine read is the first thing that has to hold. All five, so
/// this covers the factory-default profiles and the two carrying settings
/// castty had written.
#[test]
fn the_validator_accepts_every_real_read_reply() {
    for index in 0..5 {
        let raw = fixture(&format!("read-0x07-profile{index}"));
        assert_eq!(
            validate_read_reply(&raw),
            Ok(()),
            "profile {index} came off real hardware and must be trusted"
        );
    }
}

/// The hazard this whole path is designed around: for a few seconds after the
/// mouse is plugged in, a profile read answers with 1041 zero bytes **and
/// reports success**. Adopting that and writing it back would erase the
/// profile's DPI, button mapping and the unmapped 880-byte macro region on a
/// device that is out of production. A zeroed buffer must never validate.
#[test]
fn the_validator_rejects_the_warm_up_answer() {
    let zeros = vec![0u8; 1041];
    assert_eq!(validate_read_reply(&zeros), Err(ReadError::WarmUp));
}

/// A short buffer is not a profile either, and must be caught before anything
/// indexes into it.
#[test]
fn the_validator_rejects_a_short_buffer() {
    assert_eq!(validate_read_reply(&[0u8; 64]), Err(ReadError::BadLength(64)));
    assert_eq!(validate_read_reply(&[]), Err(ReadError::BadLength(0)));
}

/// Zeroing any one of the structural bytes has to be enough on its own -- a
/// buffer that is *mostly* right is the dangerous case, since the all-zero
/// check cannot see it.
#[test]
fn the_validator_rejects_a_partly_zeroed_reply() {
    let good = fixture("read-0x07-profile2");
    for (at, expected) in [(34usize, 0x08u8), (102, 0x03)] {
        let mut raw = good.clone();
        raw[at] = 0;
        assert_eq!(
            validate_read_reply(&raw),
            Err(ReadError::Constant { at, expected, found: 0 }),
            "a zeroed [{at}] must not pass"
        );
    }
    // and a DPI slot the hardware has never been seen to accept
    let mut raw = good.clone();
    raw[68..70].copy_from_slice(&0u16.to_le_bytes());
    assert_eq!(validate_read_reply(&raw), Err(ReadError::Dpi { step: 0, value: 0 }));

    // a write frame is not a read reply: [1] carries the 0x08 command where a
    // reply carries the 0x01 ack
    assert!(validate_read_reply(&fixture("factory-default-p2")).is_err());
}

/// **Every DPI this application will write must survive being read back.** A
/// validated range narrower than the range `encode` accepts is a trap: the
/// profile writes fine, and then every later read of it fails, burns the retry
/// budget, and falls back to the config file for good. The first version of the
/// validator used the 400-9150 figure `PROTOCOL.md` records as "confirmed
/// values", which is the range *seen in captures* rather than a hardware limit
/// -- while castty's own DPI slider goes from 100 to 10000. No fixture could
/// catch it: all five read captures carry values comfortably inside both ranges.
///
/// The same bug exists in libratbag's driver for this device.
#[test]
fn every_dpi_the_encoder_accepts_reads_back() {
    let mut p = Profile::decode_response(&fixture("read-0x07-profile2")).unwrap();
    for value in [DPI_MIN, 150, 400, 3000, 9150, 9200, DPI_MAX] {
        p.dpi = [DpiStep::linked(value); 3];
        let written = p
            .encode()
            .unwrap_or_else(|e| panic!("the encoder accepts DPI {value}: {e}"));

        // What the device hands back for that profile is the same payload
        // under the reply framing -- the ack at [1] where a write frame carries
        // the 0x08 command.
        let mut as_read = written.clone();
        as_read[1] = 0x01;
        assert_eq!(
            validate_read_reply(&as_read),
            Ok(()),
            "DPI {value} can be written but not read back"
        );
        assert_eq!(
            Profile::decode_response(&as_read).unwrap().dpi[0],
            DpiStep::linked(value)
        );
    }
}

/// The validator's job is to reject garbage, not to police values -- but zero
/// is garbage, and rejecting it is all the warm-up buffer needs.
#[test]
fn a_zero_dpi_is_still_refused() {
    let mut raw = fixture("read-0x07-profile2");
    for (step, base) in [(0usize, 68usize), (1, 77), (2, 98)] {
        let mut one = raw.clone();
        one[base..base + 2].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(validate_read_reply(&one), Err(ReadError::Dpi { step, value: 0 }));
    }
    // and the Y axis of a slot, not just the X
    raw[70..72].copy_from_slice(&0u16.to_le_bytes());
    assert_eq!(validate_read_reply(&raw), Err(ReadError::Dpi { step: 0, value: 0 }));
}

/// Byte `[0]` of a read reply carries no reliable value. The same device, read
/// with the same tool, answered `0x00` in one session and `0x60` in another --
/// stable within a session, different across power states. Every fixture here
/// shows `0x00` only because they were all captured in one sitting shortly
/// after a replug, and an earlier version of the validator turned that accident
/// into a constant and refused every read on a device that had been up a while.
/// So a reply must validate whatever `[0]` says.
#[test]
fn the_validator_ignores_the_unstable_first_byte() {
    for index in 0..5 {
        let mut raw = fixture(&format!("read-0x07-profile{index}"));
        assert_eq!(raw[0], 0x00, "the fixtures were captured with [0] = 0x00");
        for observed in [0x00u8, 0x60, 0x61, 0xff] {
            raw[0] = observed;
            assert_eq!(
                validate_read_reply(&raw),
                Ok(()),
                "profile {index} must validate with [0] = 0x{observed:02x}"
            );
            let p = Profile::decode_response(&raw)
                .unwrap_or_else(|e| panic!("[0] = 0x{observed:02x} must still decode: {e}"));
            assert_eq!(p.index, index as u8, "the index still comes from [16]");
            assert_eq!(p.name, format!("Profile{}", index + 1));
        }
    }
}

/// What does identify a reply: the ack at `[1]`. A write frame carries the
/// `0x08` command there, which is the only header byte separating the two
/// framings now that `[0]` is untrusted.
#[test]
fn the_validator_still_requires_the_ack_at_byte_one() {
    let mut raw = fixture("read-0x07-profile2");
    raw[1] = 0x08;
    assert_eq!(
        validate_read_reply(&raw),
        Err(ReadError::Constant { at: 1, expected: 0x01, found: 0x08 })
    );
    assert!(Profile::decode_response(&raw).is_err());
}

/// Decoding must not depend on the framing. A read reply differs from a write
/// frame only in the two-byte header (`00 01` where a write leads `61 08`),
/// with the profile index at `[16]` rather than `[5]`; everything from `[16]`
/// on is identical. Profiles 2-4 were still at factory defaults when they were
/// read, so the captured reply and the captured write frame are the same
/// profile seen through the two framings -- and re-encoding either must give
/// back the write frame.
#[test]
fn a_read_reply_decodes_to_the_same_profile_as_the_write_frame() {
    for index in 2..5 {
        let reply = Profile::decode_response(&fixture(&format!("read-0x07-profile{index}")))
            .unwrap_or_else(|e| panic!("profile {index} read reply must decode: {e}"));
        let written = Profile::decode(&fixture(&format!("factory-default-p{index}"))).unwrap();

        assert_eq!(reply.index, index as u8, "the index comes from [16] in a reply");
        assert_eq!(reply.index, written.index);
        assert_eq!(reply.name, written.name);
        assert_eq!(reply.leds, written.leds);
        assert_eq!(reply.dpi, written.dpi);
        assert_eq!(reply.dpi_step_count, written.dpi_step_count);
        assert_eq!(reply.active_dpi_step, written.active_dpi_step);
        assert_eq!(reply.polling, written.polling);
        assert_eq!(reply.angle_snapping, written.angle_snapping);
        assert_eq!(reply.angle_tuning, written.angle_tuning);
        assert_eq!(reply.lift_off, written.lift_off);
        assert_eq!(reply.buttons, written.buttons);

        // The strongest form of the same claim: a profile read off the device
        // re-encodes to exactly the frame the vendor app sent for it, macro
        // region and all. Anything read can therefore be written back.
        assert_eq!(
            reply.encode().unwrap(),
            fixture(&format!("factory-default-p{index}")),
            "profile {index} must re-encode to the captured write frame"
        );
    }
}

/// Profiles 0 and 1 were not at factory defaults when they were read, so they
/// have no matching write fixture -- but they still have to decode to the
/// settings the bytes plainly carry, and survive a re-encode unchanged apart
/// from the header the framing owns.
#[test]
fn a_read_reply_round_trips_through_encode() {
    for index in 0..5 {
        let raw = fixture(&format!("read-0x07-profile{index}"));
        let p = Profile::decode_response(&raw).unwrap();
        let encoded = p.encode().unwrap();

        // [0], [1] and [5] are the framing; everything else must be untouched.
        assert_eq!(&encoded[6..], &raw[6..], "profile {index} payload changed on re-encode");
        assert_eq!(encoded[0], 0x61, "re-encoded as a write frame");
        assert_eq!(encoded[1], 0x08);
        assert_eq!(encoded[5], index as u8);
        assert_eq!(Profile::decode(&encoded).unwrap().index, index as u8);
    }
}

/// The name each profile was read back with, straight from the device.
#[test]
fn read_replies_carry_the_profile_names_on_the_device() {
    for index in 0..5 {
        let p = Profile::decode_response(&fixture(&format!("read-0x07-profile{index}"))).unwrap();
        assert_eq!(p.name, format!("Profile{}", index + 1));
        assert_eq!(p.dpi_step_count, 3);
    }
}

/// Both switch types take the same three direction parameters, captured by
/// assigning each in turn in the vendor software.
#[test]
fn switch_directions_are_symmetric() {
    use castty::hardware::ButtonAction;
    for (up, down, roll) in [
        (
            ButtonAction::ProfileSwitch(0xf0),
            ButtonAction::ProfileSwitch(0xf2),
            ButtonAction::ProfileSwitch(0xf1),
        ),
        (
            ButtonAction::DpiSwitch(0xf0),
            ButtonAction::DpiSwitch(0xf2),
            ButtonAction::DpiSwitch(0xf1),
        ),
    ] {
        for action in [up, down, roll] {
            assert_eq!(ButtonAction::from_entry(&action.to_entry()), action);
        }
    }
}
