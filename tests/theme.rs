use castty::iced_ui::theme::{Accent, Named};

/// Every preset must define a full palette; a missing colour would render as
/// black or transparent rather than failing loudly.
#[test]
fn every_preset_defines_a_complete_palette() {
    for named in Named::ALL {
        let p = named.palette();
        for (label, colour) in [
            ("bg", p.bg), ("surface", p.surface), ("raised", p.raised),
            ("border", p.border), ("text", p.text), ("dim", p.dim),
            ("accent", p.accent), ("on_accent", p.on_accent), ("danger", p.danger),
        ] {
            assert!(colour.a > 0.0, "{} has transparent {label}", named.label());
        }
        // Text must be readable against the background.
        let contrast = (p.text.r - p.bg.r).abs() + (p.text.g - p.bg.g).abs() + (p.text.b - p.bg.b).abs();
        assert!(contrast > 1.0, "{} has low text contrast", named.label());
    }
}

/// The override replaces only the accent; everything else stays the preset's.
#[test]
fn accent_override_replaces_only_the_accent() {
    let base = Named::Graphite.palette();
    let overridden = castty::iced_ui::theme::resolve(Named::Graphite, Accent::Rose);

    assert_ne!(overridden.accent, base.accent);
    assert_eq!(overridden.bg, base.bg);
    assert_eq!(overridden.surface, base.surface);
    assert_eq!(overridden.text, base.text);
}

/// "Preset" means leave the theme's own accent alone.
#[test]
fn preset_accent_is_the_identity() {
    for named in Named::ALL {
        let base = named.palette();
        let resolved = castty::iced_ui::theme::resolve(named, Accent::Preset);
        assert_eq!(resolved.accent, base.accent, "{}", named.label());
    }
}

/// Every accent must be legible on every preset's surfaces.
#[test]
fn accents_contrast_with_every_preset_surface() {
    for named in Named::ALL {
        for accent in Accent::ALL {
            let p = castty::iced_ui::theme::resolve(named, accent);
            let diff = (p.accent.r - p.surface.r).abs()
                + (p.accent.g - p.surface.g).abs()
                + (p.accent.b - p.surface.b).abs();
            assert!(
                diff > 0.35,
                "{} + {} leaves the accent too close to the surface",
                named.label(),
                accent.label()
            );
        }
    }
}
