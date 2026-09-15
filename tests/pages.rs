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
