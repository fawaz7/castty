# castty UI — design brief and redesign handoff

**Status:** the iced UI is functionally complete and hardware-correct, but the
layout is visibly broken. This document is the handoff for a redesign pass.

**Audience:** whoever picks up the redesign next. Assume they know Rust and
have not seen this codebase.

**Read alongside:** `PROTOCOL.md` (the wire protocol — authoritative),
`CLAUDE.md` (project charter), and
`docs/superpowers/specs/2026-09-16-castty-iced-ui-design.md` (the spec this UI
was built from).

---

## 1. What the app is

`castty` configures the **Mionix Castor** gaming mouse on Linux. The device
shipped with Windows-only software and no published protocol; the protocol here
was reverse engineered from captures of the vendor app and verified against real
hardware.

The UI is a single window with six pages. It is not a dashboard and not a
utility panel — the brief was "a brand new experience that's modern, clean, and
polished", explicitly not inheriting the earlier GTK design.

---

## 2. Hardware facts that constrain the UI

These are not preferences. They are properties of the device, and the interface
has to be honest about them.

| Fact | Consequence for the UI |
|---|---|
| **There is no read path.** The host cannot ask the mouse what it is currently set to. | Every value on screen is *what we last wrote*, not what the device holds. Never phrase anything as "current settings". State is persisted to `~/.config/castty/profileN.bin`. |
| **Writes go to flash.** | No live-apply. Edits are local until an explicit **Apply**. A write per slider drag would wear the flash for nothing. |
| **The commit frame is the only profile-select mechanism.** Byte 5 of a commit says which profile the mouse switches to. | Selecting a profile cannot take effect without a commit. Writing several profiles in a row ends on whichever was written last, so a multi-write must end with a commit for the slot the user actually selected. |
| **Five profiles**, each with its own settings *and* its own macros. | The profile selector is global (top bar), not per-page. Switching profiles reloads every page's state. |
| **Profile names are ten bytes of ASCII.** | Truncate to ten ASCII bytes and drop non-ASCII, so the field shows exactly what the device will store. Not ten *characters*. |
| **Macro storage is 32 slots per profile**, shared across all six buttons; each macro costs its event count plus one terminator. | The capacity readout belongs on the Macros page, visible before recording starts. Exceeding it must fail loudly, not silently drop half an Apply. |
| **The mouse stores a macro as a bare event list with no name.** | Names live only in our config. A macro on the device that is not in the local library still works, so it must be preserved, not wiped. This is the `Slot::Keep` case on the Buttons page. |
| **The blue LED channel is much dimmer than red.** `1d00ff` reads as magenta on the hardware. | Documented as hardware behaviour and deliberately *not* corrected in software — correcting it would mean writing different bytes than the user picked. The preview should show what the user chose, and the caveat belongs in the README, not in a silent colour transform. |
| **Effect speed is fixed in firmware. Per-LED effects are impossible** (the mode byte is global). Host-driven animation is unsafe (every commit writes flash). | Do not offer speed sliders, per-LED effect pickers, or custom effects. See "Ruled out" in `CLAUDE.md`. |
| The device can be unplugged at any moment. | Every hardware call returns a `Result` the UI renders as a disconnected state. Device I/O is on a worker thread; the UI thread never blocks on it. |

### Value ranges

- **DPI:** three steps, each X and Y, linked or separate.
- **Polling rate:** 1000 / 500 / 250 / 125 Hz (stored as a divisor 1/2/4/8).
- **Lift-off distance:** 1–31.
- **Angle snapping:** 0–15.
- **Angle tuning:** −30 to +30 (signed).
- **Surface analyzer:** device returns a raw value; `score = round(raw × 0.18)`,
  displayed as `score`/10.
- **Buttons:** six, in device order — left, right, wheel click, side front,
  side rear, DPI.
- **LED effects:** solid, blinking, pulsating, breathing; rainbow is an
  independent flag on top of any of them.

---

## 3. Current state

### File map

```
src/iced_ui/
  mod.rs            app state, Message enum, update, view, tab bar, footer,
                    keyboard subscription, worker wiring
  theme.rs          Named themes, Accent overrides, Palette, resolve()
  settings.rs       persisted { theme, accent }
  widgets.rs        card, field, segmented, segment, primary, subtle,
                    destructive, dim, spacer, lighten, GAP, PAD
  art.rs            mouse artwork load + saturation-weighted LED tinting
  preview.rs        the hero canvas — mouse, glow, button callouts
  colour_picker.rs  HSV canvas picker
  worker.rs         device thread, Job/Update messages, ProfileSink seam
  pages/
    lighting.rs  sensor.rs  buttons.rs  macros.rs  profiles.rs  about.rs
```

Hardware and persistence layers (`src/hardware/`, `src/config.rs`,
`src/macros.rs`) are **toolkit-independent and must not be modified by UI
work**. They survived the GTK→iced rewrite untouched, which is what made the
rewrite tractable. Keep that property.

### What works

All six pages are functionally complete and hardware-correct. 89 tests pass,
covering the protocol, the theme system, every page's state transitions, and
the Apply/switch wiring. The device round trip is verified: settings persist
across a replug, including which profile is active.

### The old GTK front end

`src/ui/` still exists and `cargo run` still launches it. Replacing it with the
iced build is the last remaining task (drop `src/ui/`, the `gtk4` and
`libadwaita` deps, and `resources/style.css`, which is GTK-only).

---

## 4. What is broken

Three defects, all in layout. They were never caught because **tests assert
state and reviews read diffs — neither can see that a page is blank.** Fixing
the process gap matters as much as fixing the bugs; see §10.

### 4.1 Lighting and Buttons render nothing at all

Only the tab bar draws. The entire page body is empty.

**Cause.** Those two are exactly the `Hero::Large` pages. In `mod.rs::view`:

```rust
Hero::Large => row![
    container(self.hero(&palette)).width(Length::FillPortion(5)).height(Length::Fill),
    container(content).width(Length::FillPortion(5)),
]
.height(Length::Fill)
```

…and the whole body is then wrapped in `scrollable(...)`. A `scrollable` hands
its child **unbounded** vertical space, so `Length::Fill` resolves against
infinity and collapses to zero. `Hero::Small` uses `Length::Fixed(150.0)`
instead, which is why Sensor, Macros and Profiles survive.

**Rule to carry forward:** never put `Length::Fill` on the vertical axis inside
a vertical `scrollable`. Either bound the scrollable's child, or put the
scrollable around only the part that actually scrolls.

### 4.2 A huge dead band above and below the tab bar

Roughly 200px of emptiness on either side of the tabs, pushing all content far
down the window. The profile picker and Apply button also sit jammed against
the tabs instead of at the right edge.

**Cause.** `widgets::spacer()` is:

```rust
Space::new().height(Length::Fill)
```

That is correct for the page columns that use it as a trailing pusher. But
`tab_bar` uses the same helper inside a `row!`, where pushing things apart
requires `width(Length::Fill)`. Setting *height* to Fill in a row inflates the
row to the full window height, and `align_y(Center)` then centres the tab
contents in that enormous box.

**Fix shape:** two helpers, not one — a horizontal pusher (`width(Fill)`) for
rows and a vertical one (`height(Fill)`) for columns. One ambiguous `spacer()`
used in both axes is the bug.

### 4.3 Proportions are wrong

Even where it renders, it does not look designed:

- The `Hero::Small` band is a 150px strip containing a very small mouse, mostly
  empty. It reads as a placeholder, not a feature.
- The hero artwork is 320×392 and is being drawn far below its natural size.
- Cards run the full window width with no max measure, so on a wide window the
  label/control pairs are separated by a lot of nothing.
- There is no visual hierarchy between the page title, card titles and field
  labels — they are all close in size and weight.

This is a specification failure, not an implementation one. The spec said
"hero shrinks, content leads" without saying what either should measure.

---

## 5. Intended design

These were agreed at the start and still hold. The redesign should honour the
intent, not the current code.

### Chosen direction

- **Hero product page.** The mouse is the centrepiece, not a decorative icon.
  It should read as a physical object sitting in the window.
- **Hero shrinks, content leads.** On lighting-focused pages the mouse is large
  and the controls sit beside it. On data-heavy pages it shrinks to a band and
  the controls take the page. On About it disappears entirely.
- **Top tab bar** for page navigation — not a sidebar.
- **Both** a theme picker and an accent override.

### The hero

`preview.rs` paints, in order: a glow behind the mouse derived from the *actual
LED colour* (never a themed accent — a themed glow would light the mouse in
whatever hue the desktop happened to use), the mouse artwork tinted at the wheel
and logo positions, and — on the Buttons page only — six numbered callouts.

The glow must be painted **before** the image transform and across the full
allocation, or it clips to a rectangle. Text drawing leaves a current point, so
callout paths need an explicit reset before the next stroke; both of these were
bugs in the GTK version and the notes exist so they are not rediscovered.

Button callout positions are fractional coordinates into the artwork, in
`preview.rs::BUTTON_MARKS`, ordered to match the device's button order. The
numbering was wrong in the GTK app and is worth checking against the physical
mouse after any change.

### Interaction model

- Edits are local; **Apply** writes. Apply is enabled when any profile is dirty
  *or* when the selected profile differs from what was last committed.
- The profile selector and Apply live in the top bar, since both are global.
- The footer is a single status line: connection state, what was just written,
  or an error. It is the only place errors surface, so it must stay legible.
- Destructive actions confirm in place rather than in a dialog. "Restore
  defaults" is two-click and its copy must state the real blast radius (all five
  profiles) and that nothing reaches the device until Apply.

---

## 6. Design guidelines

### Spacing and measure

Current tokens are `GAP = 14.0` and `PAD = 18.0`. They are reasonable; the
problem is that nothing constrains *measure*.

- Give the content column a **maximum width** (something like 680–760px) and
  centre it, so label/control pairs stay readable on a wide window.
- Establish a vertical rhythm as multiples of a base unit rather than ad-hoc
  numbers. Pick one and apply it to card padding, gaps between cards, and the
  gap between a field label and its control.
- The tab bar should be a fixed, modest height — it is chrome, not content.

### Typography

Define an explicit scale rather than sprinkling `.size()` calls. Something like:
page title, card title, body, label, caption. Right now sizes are chosen
per-call-site (`14.0`, `18.0`, `12.0`) with no system, which is why hierarchy
reads flat.

Note `iced::Pixels` is `From<f32>`, not `From<u16>` — sizes must be written
`14.0`, not `14`.

### Colour

- The palette is **the app's own, never the desktop's**, so it looks identical
  on every distribution.
- Colour carries meaning in exactly three places: the accent (selection and
  primary action), `danger` (destructive), and the LED preview (the user's
  actual chosen colour). Everything else is neutral.
- The LED preview colour must never be substituted with a theme colour.

### Components

`widgets.rs` provides `card`, `field`, `segmented`/`segment`, `primary`,
`subtle`, `destructive`, `dim`, `spacer`. Two notes for the redesign:

- `field`'s hint is `Option<impl Into<Cow<'a, str>>>` (accepts owned strings),
  but `card`'s subtitle is still `Option<&'a str>` — a concrete borrow that
  **cannot** take `Some(&format!(...))`. That asymmetry has already cost time
  twice. Make them consistent.
- A bare `None` no longer infers for `field`'s hint; call sites need
  `None::<&str>`.

---

## 7. Theming

Five built-in themes, each a full `Palette`:

| Theme | Character |
|---|---|
| **Graphite** (default) | Neutral dark; the accent carries all the colour. |
| **Nordic** | Cool blue-grey dark. |
| **Indigo** | Violet-leaning dark. |
| **Ember** | Warm brown-orange dark. |
| **Paper** | The one light theme. |

`Palette` fields: `bg`, `surface`, `raised`, `border`, `text`, `dim`, `accent`,
`on_accent`, `danger`, `dark`.

**Accent override** is independent of the theme: `Preset` (use the theme's own
accent) plus Blue, Teal, Green, Amber, Orange, Rose, Violet, Slate.
`resolve(named, accent) -> Palette` combines them, recomputing `on_accent` for
contrast. Both choices persist in `~/.config/castty/settings.json`.

The user explicitly disliked a yellow accent. Amber/Orange exist as *choices*;
do not make anything yellow the default.

Theme and accent pickers live on the About page, under an "Appearance" card.

---

## 8. Animation

### LED preview — timings measured on real hardware

These are not invented; they were timed against the device with a stopwatch and
are the reason the preview looks like the mouse. Constants live in
`pages/lighting.rs`:

| Effect | Period | Shape |
|---|---|---|
| **Blinking** | `1.154 s` | Square: full brightness for half the period, 0.05 for the other half. |
| **Pulsating** | `1.58 s` | A **heartbeat**, not a sine — two short blackouts (`0.18 s` beat, `0.12 s` gap, dipping to `0.36`) then held lit for the remainder. |
| **Breathing** | `6.0 s` | Sine, eased: `0.08 + 0.92 · p²` where `p` is the half-sine. The square makes the fade perceptually even. |
| **Rainbow** | `5.0 s` | Hue sweeps a full 360° over the period, applied on top of whatever brightness the effect produced. |

`State::animated()` reports whether a redraw subscription is needed — it is true
when rainbow is on or the effect is anything but solid. **Only subscribe to
frame updates when something is actually animating**; a constant redraw on a
static page is wasted power.

On a real profile switch the hardware blacks the LEDs for about 300 ms before
the new colour appears. The preview does not currently model this; it could, and
it would make the switch feel more truthful.

### UI motion

There is none today, and that is a gap rather than a decision. Suggested
restraint if motion is added:

- Page transitions: either nothing, or a short cross-fade. Never slide the hero.
- The hero resizing between Large and Small is the one transition genuinely
  worth animating, since it is the app's signature move.
- Never animate a value the user is dragging.
- Respect the idea that this is a configuration tool — motion should confirm,
  not entertain.

---

## 9. iced 0.14 constraints

Hard-won, all of these cost time:

- **`Length::Fill` on the vertical axis inside a vertical `scrollable`
  collapses to zero.** This is bug 4.1. The single most important item here.
- `Pixels: From<f32>`, not `From<u16>`.
- `view` / `update` / `subscription` / `theme` must be **free `fn` items**, not
  closures — only a fn item is higher-ranked enough over the borrow from state.
- `slider` requires `T: Copy + From<u8> + PartialOrd + Into<f64> + FromPrimitive`.
  `i8` fails, so the signed angle-tuning slider uses `i32` and narrows on apply.
- **Only linear gradients.** There is no radial gradient, which is why the hero
  glow is drawn as concentric rings (`preview.rs`, 28 rings with a falloff).
- `Handle::from_rgba`; `frame.draw_image` takes `&Handle`.
- `iced::keyboard::listen()` — there is no `on_key_press`/`on_key_release`.
  `listen()` yields only `Status::Ignored` events, and **`text_input` captures
  key presses when focused but not key releases**, which is why the macro
  recorder tracks which keys are physically down rather than trusting pairing.
- `pick_list` options are owned; building a `Vec<String>` per frame is a real
  per-frame allocation on a page that redraws on every animation tick.

---

## 10. Testing the UI — the gap to close

**This is the most important section.** Six review rounds and 89 tests did not
notice that two of six pages render nothing.

What the current tests cover: protocol encode/decode against captured frames,
page state transitions, the Apply/switch wiring, the theme system. What they
cannot cover: anything about layout.

Two things would have caught this, in order of value:

1. **Headless layout assertions.** iced's `advanced` feature is already enabled.
   Build each page's view, compute the layout tree against fixed window bounds,
   and assert invariants: every page's body has non-zero width *and* height; the
   tab bar's height is below a sane bound; the profile picker's x position is in
   the right-hand portion of the window. Both bugs 4.1 and 4.2 are exactly these
   assertions. Worth investigating whether iced 0.14 exposes a null renderer for
   this; if not, extracting layout decisions into pure functions that return
   intended measurements is a weaker but real substitute.

2. **A screenshot in the loop.** Whoever implements should look at the window
   after every layout change. If that can be automated in the environment
   (a nested compositor plus a capture tool), automate it; otherwise it is a
   manual step that must not be skipped when an agent reports it "cannot
   automate a GUI". A reported blocker on visual verification is a reason to do
   it by hand, not to drop it.

Do not treat a green test suite as evidence the UI is correct. It is evidence
the logic is correct.

---

## 11. Copy rules

- Sentence case. No exclamation marks. **No em dashes** — this has been
  violated repeatedly and each slip got through a review.
- Say what the device does, not what the widget does.
- Never claim something reached the mouse unless it did. "Saved to the mouse"
  only after the write is confirmed; a switch says "Now using X", not "Saved".
- Where a value is what we last wrote rather than what the device holds, the
  copy should not imply otherwise.

---

## 12. Known limitations to carry forward

- **Macros cannot record modifier keys.** `Ctrl+C` records only the `C`.
  `keycode::from_keyval` has no arms for HID usages `0xe0`–`0xe7`, and nothing
  has verified the device accepts modifier usages inside a macro event — that
  needs another vendor-app capture session. The GTK front end has the identical
  gap. This is most of what people record macros for, so it is worth fixing
  properly rather than papering over.
- **`worker::write_profiles` ordering is verified by inspection**, not by a test
  against real hardware sequencing. There is now a `ProfileSink` trait seam and
  a recording fake covering call order, but no end-to-end hardware test exists.
- **`device_active` is an assumption.** Since there is no read path, the app
  cannot know which profile the mouse is on at launch. It is seeded `None` so
  the first Apply always commits.
- The production seed in `Castty::new()` is not itself covered by a test; only
  the test double's copy of it is.

---

## 13. Suggested order for the redesign

1. Fix 4.1 and 4.2 first — they are small and unblock actually *seeing* the app.
2. Put the layout assertions from §10 in place before touching proportions, so
   the redesign has a net under it.
3. Then redesign: measure, scale, hierarchy, the hero's two sizes.
4. Then motion, if any.
5. Then the GTK removal (drop `src/ui/`, `gtk4`, `libadwaita`,
   `resources/style.css`; point `src/main.rs` at `iced_ui::run`).

Leave the hardware layer alone throughout.
