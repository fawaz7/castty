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
        assert!(
            content.width <= widgets::MEASURE + 1.0,
            "{page:?} content is wider than the measure: {content:?}"
        );
    }
}

/// With no hero, the content column is centred rather than left-aligned.
#[test]
fn the_about_page_centres_its_content() {
    let app = app_on(Page::About);
    let mut ui = simulate(&app);
    let content = bounds_of(&mut ui, "page-content");
    let centre = content.x + content.width / 2.0;
    assert!((centre - WIDTH / 2.0).abs() < 2.0, "About content is off-centre: {content:?}");
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
                (b.width - Hero::SMALL_WIDTH).abs() < 1.0
                    && (b.height - Hero::SMALL_HEIGHT).abs() < 1.0,
                "{page:?} small hero is not the fixed stage: {b:?}"
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

/// The Lighting page is the one people see first, and both of its cards
/// must be on screen at the default window size without scrolling.
#[test]
fn the_lighting_page_fits_the_default_window_without_scrolling() {
    let app = app_on(Page::Lighting);
    let mut ui = simulate(&app);
    let rainbow = ui.find("Rainbow").expect("the last field on the page").bounds();
    let footer = bounds_of(&mut ui, "footer");
    assert!(
        rainbow.y + rainbow.height < footer.y,
        "the Effect card runs off the bottom: {rainbow:?} vs footer {footer:?}"
    );
}

/// Not an assertion: a picture of every page, plus the states a first
/// render does not show, so a layout change can be looked at rather than
/// inferred. Rendered with the bundled Fira Sans, so text metrics differ
/// slightly from a system font, but geometry does not.
#[test]
fn write_a_snapshot_of_every_page() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/ui-snapshots");
    std::fs::create_dir_all(&dir).expect("snapshot directory");
    for page in Page::ALL {
        snapshot(&dir, &format!("{page:?}").to_lowercase(), app_on(page));
    }

    let mut split = app_on(Page::Lighting);
    split.lighting.update(pages::lighting::Message::ModeChanged(pages::lighting::Mode::Split));
    snapshot(&dir, "lighting-split", split);

    let mut paper = app_on(Page::Lighting);
    paper.settings.theme = theme::Named::Paper;
    snapshot(&dir, "lighting-paper", paper);

    let mut separate = app_on(Page::Sensor);
    separate.sensor.linked = false;
    snapshot(&dir, "sensor-separate", separate);

    let mut editing = app_on(Page::Macros);
    editing.macros.update(pages::macros::Message::New, &mut editing.library);
    snapshot(&dir, "macros-editing", editing);

    let mut confirming = app_on(Page::Profiles);
    confirming.profiles_page.update(&pages::profiles::Message::RestoreRequested);
    snapshot(&dir, "profiles-confirming", confirming);
}

fn snapshot(dir: &std::path::Path, name: &str, app: Castty) {
    let theme = app_theme(&app);
    let mut ui = simulate(&app);
    let shot = ui.snapshot(&theme).expect("render");
    write_png(&dir.join(format!("{name}.png")), &shot);
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
