//! Headless layout checks.
//!
//! The state tests in `tests/` and the wiring tests next door can pass while
//! two of six pages render nothing: they assert what the widgets *hold*, not
//! where they *are*. These build every page's real `view()` against a fixed
//! window, walk the resulting layout, and assert the few invariants that
//! distinguish a drawn page from a blank one. They also write a PNG of every
//! page to `target/ui-snapshots/` so the result can be looked at, which no
//! assertion replaces.
//!
//! Rendering uses tiny-skia (pinned in `.cargo/config.toml`), so no GPU or
//! display is needed.

use super::tests::test_app;
use super::*;
use iced_test::selector;
use iced_test::Simulator;

const WIDTH: f32 = 1040.0;
const HEIGHT: f32 = 700.0;

fn app_on(page: Page) -> Castty {
    let (mut app, _jobs) = test_app();
    app.page = page;
    app
}

fn simulate(app: &Castty) -> Simulator<'_, Message> {
    Simulator::with_size(
        iced_test::core::Settings::default(),
        iced::Size::new(WIDTH, HEIGHT),
        app.view(),
    )
}

fn bounds_of(ui: &mut Simulator<'_, Message>, id: &'static str) -> iced::Rectangle {
    ui.find(selector::id(id))
        .unwrap_or_else(|e| panic!("no widget with id {id:?}: {e}"))
        .bounds()
}

/// A page whose content area collapsed is the failure that motivated this
/// module. Every page must lay its content out with real width and height.
#[test]
fn every_page_lays_out_a_visible_content_area() {
    for page in Page::ALL {
        let app = app_on(page);
        let mut ui = simulate(&app);
        let content = bounds_of(&mut ui, "page-content");
        assert!(
            content.width > 300.0 && content.height > 200.0,
            "{page:?} content collapsed to {content:?}"
        );
        assert!(
            content.y < HEIGHT * 0.4,
            "{page:?} content starts too far down the window: {content:?}"
        );
    }
}

/// The hero is the app's signature element. Large must be large, small must
/// be small, and About must not have one.
#[test]
fn the_hero_measures_what_its_size_says() {
    for page in Page::ALL {
        let app = app_on(page);
        let mut ui = simulate(&app);
        let hero = ui.find(selector::id("hero")).ok().map(|t| t.bounds());
        match (page.hero(), hero) {
            (Hero::Large, Some(b)) => assert!(
                b.width >= 320.0 && b.height >= 392.0,
                "{page:?} large hero is smaller than the artwork: {b:?}"
            ),
            (Hero::Small, Some(b)) => assert!(
                (120.0..=360.0).contains(&b.height) && b.width >= 160.0,
                "{page:?} small hero is not small: {b:?}"
            ),
            (Hero::None, None) => {}
            (size, b) => panic!("{page:?} expects {size:?} hero, laid out {b:?}"),
        }
    }
}

/// The tab bar is chrome: a fixed strip pinned to the top, never a band that
/// grows with the window.
#[test]
fn the_tab_bar_is_a_fixed_strip_at_the_top() {
    let app = app_on(Page::Lighting);
    let mut ui = simulate(&app);
    let bar = bounds_of(&mut ui, "tab-bar");
    assert_eq!(bar.y, 0.0, "tab bar must sit at the top: {bar:?}");
    assert!(bar.height <= 72.0, "tab bar is {} tall; it is chrome, not content", bar.height);
    assert!((bar.width - WIDTH).abs() < 1.0, "tab bar must span the window: {bar:?}");
}

/// Apply and the profile picker are global and belong at the right edge.
#[test]
fn apply_sits_at_the_right_edge_of_the_tab_bar() {
    let app = app_on(Page::Lighting);
    let mut ui = simulate(&app);
    let apply = ui.find("Apply").expect("an Apply button").bounds();
    let bar = bounds_of(&mut ui, "tab-bar");
    assert!(apply.x > WIDTH * 0.8, "Apply is not at the right edge: {apply:?}");
    assert!(
        apply.y >= bar.y && apply.y + apply.height <= bar.y + bar.height,
        "Apply must sit inside the tab bar: {apply:?} vs {bar:?}"
    );
}

/// The footer is the only place errors surface, so it must be on screen.
#[test]
fn the_footer_hugs_the_bottom_of_the_window() {
    let app = app_on(Page::Sensor);
    let mut ui = simulate(&app);
    let footer = bounds_of(&mut ui, "footer");
    assert!(footer.height <= 40.0, "footer is {} tall", footer.height);
    assert!(
        (footer.y + footer.height - HEIGHT).abs() < 1.0,
        "footer must touch the bottom edge: {footer:?}"
    );
}

/// Not an assertion: a picture of every page, so a layout change can be
/// looked at rather than inferred. Rendered with the bundled Fira Sans, so
/// text metrics differ slightly from a system font, but geometry does not.
#[test]
fn write_a_snapshot_of_every_page() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/ui-snapshots");
    std::fs::create_dir_all(&dir).expect("snapshot directory");
    for page in Page::ALL {
        let app = app_on(page);
        let theme = app_theme(&app);
        let mut ui = simulate(&app);
        let shot = ui.snapshot(&theme).expect("render");
        write_png(&dir.join(format!("{page:?}.png").to_lowercase()), &shot);
    }
}

/// `Snapshot` only exposes its pixels through a compare-or-create, so the
/// stale file is removed first to make it always create. The renderer's
/// name is appended to the file stem by iced.
fn write_png(path: &std::path::Path, snapshot: &iced_test::simulator::Snapshot) {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    let _ = std::fs::remove_file(path.with_file_name(format!("{stem}-tiny-skia.png")));
    snapshot
        .matches_image(path)
        .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}
