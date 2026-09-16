# castty

A Linux configuration tool for the **Mionix Castor** gaming mouse.

Mionix never shipped Linux software, and the device is out of production — so there was no way to
change its lighting, DPI or button mapping on Linux. `castty` fills that gap. The protocol was
reverse engineered from scratch by capturing the Windows vendor application under Wine and verifying
every field against real hardware.

![status](https://img.shields.io/badge/status-usable-green) ![license](https://img.shields.io/badge/license-GPL--3.0-blue)

## What works

| Feature | Status |
|---|---|
| LED colour, per LED (logo and scroll wheel independently) | ✅ |
| Effects — solid, blinking, pulsating, breathing | ✅ |
| Rainbow, combinable with any effect | ✅ |
| DPI — three steps, independent X/Y | ✅ |
| Polling rate — 125 / 250 / 500 / 1000 Hz | ✅ |
| Angle snapping, angle tuning, lift-off distance | ✅ |
| Button assignment, including profile switching | ✅ |
| Five profiles with editable names | ✅ |
| Surface analyzer (Mionix S.Q.A.T.) | ✅ |
| Macros — record, edit, timing and hold mode | ✅ |
| Single-key button assignment | ✅ |

## Requirements

- Linux with `hidraw`
- Rust 1.88+
- A Wayland or X11 session. The window is drawn with [iced](https://iced.rs); it uses the GPU
  through Vulkan or OpenGL when a driver is present and falls back to software rendering when not.
  Nothing from the desktop theme is used, so it looks the same on every distribution.

## Install

**1. Device access.** `/dev/hidraw*` is root-only by default:

```sh
sudo install -m644 packaging/60-mionix-castor.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=hidraw
```

This tags the mouse `uaccess`, granting access to the logged-in user. No group membership needed,
and nothing else on the system is affected.

**2. Build and run.**

```sh
cargo build --release
./target/release/castty
```

**3. Desktop entry.** Optional, and only needed to launch castty from your application menu
rather than a terminal:

```sh
sudo install -Dm755 target/release/castty /usr/local/bin/castty
sudo install -Dm644 packaging/io.github.fawaz7.castty.desktop \
    /usr/share/applications/io.github.fawaz7.castty.desktop
sudo cp -r packaging/icons/hicolor /usr/share/icons/
sudo update-desktop-database /usr/share/applications
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
```

The last two commands refresh the desktop and icon caches; both are safe to skip if the
commands are missing on your system.

## Command line

The GUI launches with no arguments. There is also a CLI, useful for scripting:

```sh
castty info                      # device identity and current stored profile
castty led ff6600                # set both LEDs
castty mode breathing rainbow    # effect, optionally with rainbow
castty dpi 1 1600                # set a DPI step
castty surface                   # run the surface analyzer
castty reset                     # restore factory defaults
castty -p 2 led 00ff00           # act on profile 2
```

## Two things worth knowing

**LED colours are approximate.** The mouse's blue channel is much dimmer than its red, so a colour
with even a little red in it can look markedly warmer on the LED than in the picker. Full blue reads
as a dark navy. This is how the hardware behaves — the Windows software is no different — and the
value you pick is written unchanged rather than "corrected" behind your back.

## A caveat worth knowing

**The mouse cannot be read back.** No command returns its stored settings — the vendor software
has the same limitation and keeps its own state. `castty` therefore shows what *it* last wrote,
persisted under `~/.config/castty/`. If you configure the mouse from another machine or from the
Windows software, this app will not know, and will show stale values until you next apply.

## Protocol

[`PROTOCOL.md`](PROTOCOL.md) documents the wire protocol in full: report layout, every decoded
field, and the things that were tested and ruled out. `captures/` holds the raw capture logs and the
device frames used as test fixtures, so every claim in that document can be checked against the
bytes it came from.

Contributions for the undecoded parts — macros in particular — are welcome. The tooling used to
derive all of this is in `tools/`.

## Licence

GPL-3.0.

Not affiliated with or endorsed by Mionix. The vendor software is not redistributed here.
