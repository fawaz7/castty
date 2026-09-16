<div align="center">

<img src="packaging/icons/hicolor/128x128/apps/io.github.fawaz7.castty.png" width="96" alt="castty">

# castty

**Configure the Mionix Castor on Linux.**

Lighting, DPI, polling rate, button mapping, macros and the surface analyzer — all of it, natively,
with no Wine and no Windows.

[![status](https://img.shields.io/badge/status-stable-brightgreen)](https://github.com/fawaz7/castty)
[![licence](https://img.shields.io/badge/licence-GPL--3.0-blue)](LICENSE)
[![protocol](https://img.shields.io/badge/protocol-documented-orange)](research/PROTOCOL.md)
[![platform](https://img.shields.io/badge/platform-Linux-lightgrey)](#requirements)

</div>

---

## Why this exists

I bought a Mionix Castor about ten years ago. It is still the best mouse I have ever used, and it has
been my daily driver that entire time.

When I moved from Windows to Linux, I lost control of it. Mionix never shipped Linux software, the
company is gone, the mouse has been out of production for years, and no popular tool supports it —
not OpenRGB, not libratbag, not anything I could find. The mouse kept working, but whatever settings
it had were frozen in whatever state Windows last left them.

So I took the protocol apart. I ran the original Mionix Castor software under Wine, recorded every
function as I used it, read the traffic, and mapped it byte by byte until the whole format was
understood. AI was a real help in the grind of it — diffing captures, spotting patterns, writing the
Rust — but the mouse on the desk and the bytes on the wire are what decided every question.

Then I built this app on top of it.

**All of it is here.** The protocol, the raw captures, the capture rig, the decoder — in
[`research/`](research/), documented, and free for anyone to use however they like. If you want to
add Castor support to OpenRGB or libratbag instead of using this app, please do; that's a better
outcome than everyone deriving it separately.

---

## What works

Everything the original Windows software could do, verified against real hardware:

| | |
|---|---|
| 🎨 **Lighting** | Per-LED colour — scroll wheel and logo set independently |
| ✨ **Effects** | Solid, blinking, pulsating, breathing — plus rainbow, combinable with any of them |
| 🎯 **DPI** | Three steps, independent X and Y |
| ⚡ **Polling rate** | 125 / 250 / 500 / 1000 Hz |
| 🖱️ **Sensor** | Angle snapping, angle tuning, lift-off distance |
| 🔘 **Buttons** | Full remapping, including profile switching and single-key assignment |
| 📼 **Macros** | Record, edit, per-event timing, and hold mode |
| 👤 **Profiles** | All five, with editable names |
| 📊 **Surface analyzer** | Mionix S.Q.A.T., same scoring as the vendor tool |

The GUI and a scriptable CLI are the same binary.

---

## Screens

<div align="center">

<img src="docs/screenshots/lighting.png" width="90%" alt="Lighting page — colour picker, effect and live mouse preview">

<em>Lighting — the preview lights the wheel and logo as you pick, before anything is written</em>

</div>

<table>
<tr>
<td width="50%"><img src="docs/screenshots/buttons.png" alt="Buttons page"><br><em>Buttons — numbered to match the callouts on the mouse</em></td>
<td width="50%"><img src="docs/screenshots/sensor.png" alt="Sensor page"><br><em>Sensor — DPI steps, polling rate, angle tuning, lift-off</em></td>
</tr>
<tr>
<td width="50%"><img src="docs/screenshots/macros.png" alt="Macros page"><br><em>Macros — record, retime, or hold</em></td>
<td width="50%"><img src="docs/screenshots/profiles.png" alt="Profiles page"><br><em>Profiles — all five, with editable names</em></td>
</tr>
<tr>
<td colspan="2"><img src="docs/screenshots/about.png" alt="About page"><br><em>About — five themes and nine accents, none of them borrowed from your desktop</em></td>
</tr>
</table>

The interface is drawn with [iced](https://iced.rs) and uses none of the desktop's theming, so it
looks and behaves the same on every distribution and on both Wayland and X11. It ships its own
palettes and accents, switchable in **About**.

> Those images are generated, not staged: `cargo test --lib layout_tests` builds every page's real
> widget tree in a headless renderer and writes it to `target/ui-snapshots/` — no display, and no
> mouse plugged in.

---

## Requirements

- Linux with `hidraw` (any modern kernel)
- Rust 1.88 or newer
- A Wayland or X11 session — GPU rendering via Vulkan or OpenGL where a driver exists, software
  rendering where it doesn't
- A **Mionix Castor**, USB ID `22d4:1316`

> The Castor **PRO** (`22d4:1320`/`1321`) is a different device and is *not* supported. See
> [Contributing](#contributing) if you have one.

---

## Install

### 1. Device access

`/dev/hidraw*` is root-only by default, so the rule below grants access to whoever is logged in at
the console. No group membership, nothing else on the system affected:

```sh
sudo install -m644 packaging/60-mionix-castor.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=hidraw
```

Replug the mouse, or reboot, if it was already connected.

### 2. Build and run

```sh
git clone https://github.com/fawaz7/castty
cd castty
cargo build --release
./target/release/castty
```

### 3. Desktop entry (optional)

Only needed to launch castty from your application menu rather than a terminal:

```sh
sudo install -Dm755 target/release/castty /usr/local/bin/castty
sudo install -Dm644 packaging/io.github.fawaz7.castty.desktop \
    /usr/share/applications/io.github.fawaz7.castty.desktop
sudo cp -r packaging/icons/hicolor /usr/share/icons/
sudo update-desktop-database /usr/share/applications
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
```

The last two refresh the desktop and icon caches; both are safe to skip if the commands aren't on
your system.

---

## Command line

Launching with no arguments opens the GUI. With arguments it's a CLI, which is handy for scripting
and for startup services:

```sh
castty info                      # device identity and stored profile
castty led ff6600                # set both LEDs
castty mode breathing rainbow    # effect, optionally with rainbow
castty dpi 1 1600                # set a DPI step
castty surface                   # run the surface analyzer
castty reset                     # restore factory defaults
castty -p 2 led 00ff00           # act on profile 2
castty help                      # full usage
```

---

## Two things worth knowing

### The mouse cannot be read back

No command returns the device's stored settings. This is a property of the hardware, not a gap in the
reverse engineering — the Windows software has exactly the same limitation and keeps its own state
file.

castty therefore shows what *it* last wrote, persisted in `~/.config/castty/`. If you configure the
mouse from another machine or from the Windows software, castty won't know, and will show stale
values until you next press Apply.

### LED colours are approximate

The mouse's blue emitter is much dimmer than its red, so a colour with even a little red in it looks
markedly warmer on the LED than in the picker, and full blue reads as a dark navy.

This is ordinary RGB LED behaviour, and the vendor software behaves identically. castty writes the
value you picked, unchanged — a correction curve would mean sending different bytes than the colour
you chose, and would make these frames diverge from the vendor's for no real gain.

Settings are written on an explicit **Apply**, never live as you drag a colour: writes go to the
mouse's flash, and a write per drag event would be wear for nothing.

---

## The protocol

[**`research/`**](research/) is the reverse-engineering half of this project, kept deliberately
separate so it can be used without the app:

- [`research/PROTOCOL.md`](research/PROTOCOL.md) — the full wire format. Every report, every offset,
  every decoded field with its confidence level, and the things that were tested and **ruled out** so
  nobody repeats them.
- [`research/captures/`](research/captures/) — the actual frames the vendor software sent. Every claim
  in the protocol document is checkable against the bytes it came from, and the test suite asserts
  against them directly.
- [`research/tools/`](research/tools/) — the capture rig: the `LD_PRELOAD` ioctl shim, the Wine
  launcher, the capture decoder, and an 88-line standalone Python LED writer.

[`research/README.md`](research/README.md) explains the method end to end and how to reproduce a
capture yourself.

---

## Contributing

**I only own a Castor.** That's the honest limit of this project — I can't test what I can't hold.

If you have another Mionix mouse — a Naos, an Avior, a Castor PRO — and you want it supported, the
capture rig in [`research/`](research/) is exactly what you need and it is documented for that
purpose. The protocol is likely a close relative across the range; the Castor PRO in particular
shares a software lineage. Adding a device should mostly be a matter of new captures and a byte map.

Also welcome:

- Packaging (AUR, Flatpak, Nix, .deb)
- Testing on other distributions and compositors
- The few remaining unknowns in `research/PROTOCOL.md`
- Porting the protocol into OpenRGB, libratbag or a kernel driver — genuinely, please

### Working on it

```sh
cargo test                       # 103 tests, no hardware or display needed
cargo clippy --all-targets       # lint
cargo test --lib layout_tests    # renders every page to target/ui-snapshots/
cargo run                        # launch the GUI
cargo run -- info                # CLI, needs the mouse
```

The test suite runs entirely against the captured frames, so you can work on the protocol layer
without a mouse plugged in. Layout tests render the real widget tree headlessly, so a page that
renders nothing fails a test rather than waiting for someone to notice.

---

## Licence

**GPL-3.0.** This is free software: you can redistribute it and/or modify it under the terms of the
GNU General Public License as published by the Free Software Foundation. See [`LICENSE`](LICENSE).

Two notes, both in [`NOTICE.md`](NOTICE.md):

- The mouse render and the Mionix logo shown inside the app are **not** covered by the GPLv3 grant;
  all rights in those images remain with Mionix.
- The licence lets you fork, modify and redistribute freely, and nothing narrows that. But if you
  build something derivative from this — especially a derivative app, or significant reuse of the GUI
  or hardware layer — please keep a visible credit to **Fawaz Alghzawi** and a link to this repo in
  your README, About screen or credits. It's a small ask that goes a long way for an independent,
  unpaid side project.

Not affiliated with or endorsed by Mionix. The vendor's software is not redistributed here.

---

<div align="center">

Built by [Fawaz Alghzawi](https://github.com/fawaz7) · for a ten-year-old mouse that deserved better

</div>
