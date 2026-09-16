# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

`castty` is a Linux desktop application (Rust, iced) that configures the Mionix Castor mouse. **Version
1.0.0, shipped and public** at `github.com/fawaz7/castty`. No other Linux tool supports this device —
not OpenRGB, not libratbag — so the protocol was derived from scratch.

**The protocol is cracked, verified against hardware, and complete for every setting the vendor
software exposes**: per-LED colour, effects, rainbow, DPI, polling rate, angle snapping/tuning,
lift-off, button mapping, macros, profiles, surface analyzer. Read `research/PROTOCOL.md` before
touching anything device-related.

The repository is split deliberately:

- **`research/`** — the reverse engineering, published so it is usable without the app. Contains
  `PROTOCOL.md` (authoritative; keep it updated as fields are decoded), `captures/` (the actual
  frames, plus gzipped session logs), and `tools/` (the capture rig: `hidsnoop.c`,
  `capture-vendor-app.sh`, `decode_capture.py`, `castty_probe.py`). Each directory has its own README.
- **`src/`** — the application. `hardware/` owns the protocol, `iced_ui/` the interface.
- **`tools/`** — development tooling only (`generate-icons.sh`, `screenshot-gui.sh`).
- Vendor binaries are gitignored and must stay that way (see "Reverse engineering the vendor software").

Remaining unknowns are a handful of constants with no observed effect, listed with their confidence
levels in `research/PROTOCOL.md`. Nothing user-facing is missing.

## Verified hardware facts

Established by inspecting the connected device on this machine — do not re-derive these, but re-verify if
behavior contradicts them.

- **USB ID `22d4:1316`** — "Laview Technology Mionix Castor". `iSerial` reports `STM32`, so the MCU is an
  STM32F1xx (consistent with other Mionix devices, e.g. NAOS 8200 = `22d4:1301`).
- **Two HID interfaces**, both `wMaxPacketSize` 64, `bInterval` 1:
  - **Interface 0** → boot-protocol mouse (`bInterfaceProtocol 2`). Normal input. Not the config channel.
  - **Interface 1** → `bInterfaceProtocol 0`. **This is the configuration channel.** Talk to this one.
- Interface 1's report descriptor contains a keyboard collection (report ID `0x02`, for macro/remap keystroke
  playback) **and a vendor-defined collection, usage page `0xFF01`, usage `0x02`**, holding exactly two
  feature reports:

  | Report ID | Payload | Descriptor shape | Working hypothesis |
  |---|---|---|---|
  | `0x60` | 63 bytes (64 with ID) | `Report Size 8`, `Report Count 0x3F`, `Feature` | Command / single-setting channel |
  | `0x61` | 1040 bytes (1041 with ID) | `Report Size 0x80` (16 B), `Report Count 0x41` (65) | Full profile / macro blob |

  Reports are declared `Feature`, so drive them with `HIDIOCGFEATURE`/`HIDIOCSFEATURE` (hidapi
  `get_feature_report`/`send_feature_report`) — **not** interrupt writes. The first byte of the buffer is the
  report ID.

- On this machine interface 1 currently enumerates as `/dev/hidraw5` (interface 0 is `/dev/hidraw4`), but
  **hidraw numbering is not stable across reboots or replugs**. Always resolve the node by walking
  `/sys/class/hidraw/*/device/uevent` for `HID_ID=0003:000022D4:00001316` and selecting the one whose
  `HID_PHYS` ends in `input1`, or match on the vendor usage page via hidapi. Never hardcode a hidraw index.
- `/dev/hidraw*` is `root:root 0600` here with no matching udev rule installed. Shipping a udev rule
  (`SUBSYSTEM=="hidraw", ATTRS{idVendor}=="22d4", ATTRS{idProduct}=="1316", TAG+="uaccess"`) is a
  prerequisite for running unprivileged — prefer `uaccess` over a group + `0660`.

## Reverse engineering the vendor software

Only needed to decode *new* fields; everything the vendor software exposes is already mapped. The
full method is written up for outside readers in `research/README.md` — that is the canonical
description now, and this section is the short internal version.

Two vendor artifacts are kept in the repo root, **gitignored and never to be committed**:

- **`CASTOR+Software+V1.44.zip` — the correct, primary target.** Unzips to a *portable* `CASTOR Software.exe`
  (2 MB, no installer). This is the last original-Castor release and matches the `1316` device.
- `CastorPROSoftwareSetupV1.00.exe` — for the later **Castor PRO** (different PID, `22d4:1320`/`1321`).
  Secondary reference only; its protocol may differ. It is an InstallShield launcher whose `[0]` blob is an
  `ISSetupStream` containing `Castor PRO Software.msi` (7z cannot recurse into it; needs wine or an
  ISSetupStream unpacker).

`CASTOR Software.exe` is a **native 32-bit MFC app** (not .NET, so no clean decompile). What matters:

- It imports **`HidD_GetFeature` / `HidD_SetFeature`** plus `HidD_GetAttributes`, `HidP_GetCaps`, and the
  `SetupDi*` enumeration family. That is a direct match for the `0x60`/`0x61` feature reports above — the
  vendor tool drives exactly the channel we identified, via the standard Win32 HID path.
- RTTI leaks the dialog classes, which maps the feature set: `CColorSettingsDlg`, `CMacroSettingsDlg`,
  `CDhsMacroCtrl`, `CMacro`, `CMacroEvent`, `CColorButton`.

### Capture rig (no Windows VM needed)

Wine translates `HidD_SetFeature` into hidraw `HIDIOCSFEATURE` ioctls on the Linux side, so the vendor app can
be run **natively against the real mouse on this machine** and its traffic captured. This is the fastest route
to a byte map — far faster than reading decompiled MFC.

The rig is committed under `research/tools/` and `capture-vendor-app.sh` does all of it — build the
shim, set the registry key, launch the app:

```sh
APP_DIR="$HOME/CASTOR Software V1.44" WORK=/tmp/castor-capture \
  research/tools/capture-vendor-app.sh
python3 research/tools/decode_capture.py /tmp/castor-capture/capture.log --profile 0
```

Three prerequisites, each of which cost real time to find:

1. **udev rule** `packaging/60-mionix-castor.rules` grants `uaccess` on `22d4:1316`. Required for both the
   capture and the finished tool — `/dev/hidraw*` is `root:root 0600` by default.
2. **Wine hides this device**, and the documented knob does *not* help: interface 1's descriptor leads with a
   keyboard collection, and `is_hidraw_enabled()` blanket-rejects mouse/keyboard hidraw devices **before** it
   consults the `EnableHidraw` multi-string. The per-device override is checked first and does work:
   `HKLM\System\CurrentControlSet\Services\WineBus\Devices\22d4/1316` → `Hidraw` (REG_DWORD) = 1.
   Subkey name format is `%04x/%04x`, lowercase. The capture script sets this.
3. **`hidsnoop.c`** is an `LD_PRELOAD` shim wrapping `ioctl()`, hex-dumping every `HIDIOCSFEATURE` (snapshotted
   before the call) and `HIDIOCGFEATURE` (after), to `$HIDSNOOP_LOG`. Preferred over usbmon: no root needed, and
   it logs the exact buffers the app passes, already framed per report. `decode_capture.py` parses columns 8–55
   of its hexdump lines, so that formatting is load-bearing.

Method: change **one** setting at a time in the GUI, press Apply, then diff consecutive `SET_FEATURE` dumps.
`decode_capture.py` does the diffing. That isolates which byte carries which field almost immediately.

### Safety

Reads are the safe half — dumping `0x60`/`0x61` is non-destructive and yields the current settings blob to
diff against. Be deliberate about writes: the profile lives in STM32 flash, and a malformed write can brick a
device that is out of production. Know Mionix's firmware-revert procedure before the first write. The device
also has a hardware LED self-test: hold LMB + RMB + wheel while plugging in.

## Intended architecture

- Root crate `castty`, Rust 2021, GPL-3.0.
- **iced 0.14** (features `advanced`, `canvas`, `image`, `tokio`). The GTK4/libadwaita front end it
  replaced is gone; `docs/UI-DESIGN-BRIEF.md` records the design and the layout rules that were
  learned the hard way.
- `serde`/`serde_json` for config.
- **One page per file in `src/iced_ui/pages/`; one hardware surface per file in `src/hardware/`.** Keep
  the device protocol entirely inside `src/hardware/` — the UI layer should never construct a raw report
  buffer.
- Colours come from `src/iced_ui/theme.rs` (the app's own palettes, never the desktop's); spacing tokens
  and the type scale from `src/iced_ui/widgets.rs`. No literal sizes or colours in page code.
- Release profile: `lto = "thin"`, `codegen-units = 1`, stripped.
- **Keep `panic = "unwind"`.** This is intentional, not an oversight: hardware reads run on background
  threads, and a panic there must kill only that thread, not take down the window.

Two structural consequences of the hardware that should shape the design from the start: all device I/O must
be off the UI thread (feature reports on a 1 ms-interval device will stall the UI otherwise), and the
device can be unplugged at any moment, so every hardware call returns a `Result` the UI has to render as a
disconnected state rather than unwrap.

## Commands

```sh
cargo test                     # all tests -- no hardware or display needed
cargo test <name>              # single test by substring
cargo test -- --nocapture      # show stdout
cargo clippy --all-targets     # lint (currently clean)
cargo run                      # launch the GUI
cargo test --lib layout_tests  # headless layout checks; writes PNGs of every page to target/ui-snapshots/
tools/screenshot-gui.sh        # real-renderer screenshot of the running GUI (via XWayland + ImageMagick)
cargo run -- info              # identify the connected device (CLI mode)
cargo build --release
```

`src/main.rs` launches the GUI when given no arguments, and acts as a CLI otherwise: `info`,
`led <RRGGBB>`, `mode <solid|blinking|pulsating|breathing> [rainbow]`, `dpi <1|2|3> <value>`,
`surface`, `reset`, plus `help` and `version`. `-p <1-5>` selects a profile. `help`, `version` and an
unrecognised command are answered **before** `Device::open()`, so they work with no mouse attached —
keep it that way. The CLI stays useful for scripting and for working on the hardware layer without a
display.

### UI layer

- `src/iced_ui/worker.rs` owns **all** device I/O on a named background thread. The UI thread never
  blocks on hardware: feature reports take tens of milliseconds and the mouse can disappear mid-call.
  Jobs go in over an `mpsc` channel; results come back over an `async_channel` exposed as an iced
  `Subscription`. On any device error the worker drops its handle so the next job reconnects —
  unplug/replug recovers on its own.
- Settings are applied on an explicit **Apply** press, never live as a colour is dragged. Writes go to
  the mouse's flash, so a write per drag event would be wear for nothing. Apply is enabled when any
  profile is dirty or the selected profile differs from the one last committed to the device.
- One page per file in `src/iced_ui/pages/`. `lighting.rs` is the template: a `State` with
  `from_profile(&Profile)` / `apply_to(&mut Profile)` / `update(Message)`, and a free `view(&State,
  &Palette)`. Pages do not reach into one another; `mod.rs` owns the profiles and routes messages.
- **Never put `Length::Fill` on the vertical axis inside a vertical `scrollable`**; it resolves against
  infinity and collapses to zero. Only page content is wrapped in the scrollable; the hero and chrome
  sit outside it.
- Within one render layer iced draws paths, then images, then text, whatever order they were issued.
  Anything that must sit on top of the mouse artwork goes in a second canvas inside a clipping
  container (`preview.rs::Callouts`).
- `view`/`update`/`subscription`/`theme` are free `fn` items, not closures; `iced::Pixels` converts from
  `f32`, not integers.

### Testing without hardware

`src/iced_ui/layout_tests.rs` builds every page's real `view()` in `iced_test`'s headless simulator
(tiny-skia, pinned in `.cargo/config.toml`), asserts layout invariants (content has real size, the hero
measures what its size says, the tab bar is a fixed strip, Lighting fits the default window) and writes a
PNG of every page and several states to `target/ui-snapshots/`. **Look at those PNGs after any layout
change**; a green suite proves the logic, not the picture.

`tests/profile.rs` runs entirely against the captured blobs in `research/captures/`. The important one is
`recolouring_reproduces_the_captured_transition`: it takes the frame the vendor app sent for red,
recolours it, and requires the result to equal the frame it sent for green. A passing encode is
therefore a frame the device has actually accepted. Prefer adding tests in that style -- assertions
against real captured frames -- over synthesising expected bytes by hand.

### Ruled out — do not re-investigate

`research/PROTOCOL.md` records these as tested negatives, with the evidence. They cost real time to establish:

- **Effect speed is not adjustable.** Fixed in firmware.
- **Per-LED effects are impossible.** Colour is per-LED; the mode byte is global, enforced by hardware.
- **Host-driven custom effects are unsafe.** Nothing is visible until the commit, and commits write
  flash. Animating from the host would destroy the device.

### Hardware notes

- The device has **no read path** (see research/PROTOCOL.md). Current settings cannot be queried, so state is
  persisted to `$XDG_CONFIG_HOME/castty/profile0.bin` and seeded from the factory-default blob, which
  is embedded in the binary via `include_bytes!`.
- `Profile` keeps the bytes it decoded from and patches only known fields on encode, so unknown
  regions -- including the unmapped 880-byte macro area -- survive a read/modify/write. Preserve this
  property when adding fields.
- Writes go to flash. Replaying a captured frame with a few bytes changed is the safe pattern; do not
  synthesise blobs from zero.

## Repository layout

- `packaging/60-mionix-castor.rules` — the udev rule users must install; `/dev/hidraw*` is root-only
  by default. Keep it in step with any VID/PID change.
- `install.sh` — the universal installer (build, install, udev rule, caches; `--prefix`,
  `--uninstall`, `--no-build`). `packaging/arch/PKGBUILD` and the `[package.metadata.deb]` block in
  `Cargo.toml` are the Arch and Debian packages; `packaging/README.md` documents all three plus the
  release flow. A packaged udev rule goes to `/usr/lib/udev/rules.d/`, `install.sh`'s to
  `/etc/udev/rules.d/` — the latter is the administrator's directory and the script is acting as the
  administrator. Keep the four installed artefacts (binary, rule, desktop entry, icons) in step
  across all three.
- **Runtime dependencies are invisible to `ldd`.** Only libc, libm and libgcc are linked; winit and
  wgpu dlopen libxkbcommon, wayland, X11/xcb, Vulkan and GL. Establish them by tracing a real run
  (`LD_DEBUG=libs`), never from the ELF headers, or a package will install and then fail to open a
  window.
- `packaging/io.github.fawaz7.castty.desktop` and `packaging/icons/hicolor/` — desktop integration.
  The scalable SVG is the **only** hand-edited artwork; every PNG is generated from it by
  `tools/generate-icons.sh`, and the 128px one is also embedded in the binary as the window icon.
  Edit the SVG and rerun the script; never touch a PNG directly. The application id
  (`iced_ui::APP_ID`), the desktop entry's basename, its `Icon` key and its `StartupWMClass` must all
  stay equal or the shell shows a placeholder icon for the running window; a test in
  `src/iced_ui/mod.rs` checks this.
- `research/captures/*.bin` — device frames used as test fixtures. Every protocol claim should be checkable
  against one of these.
- `research/captures/sessions/*.log.gz` — raw capture sessions, gzipped (7.2 MB to 140 KB).
  `research/tools/decode_capture.py`
  reads either form.
- `docs/screenshots/*.png` — README screenshots, regenerated from `target/ui-snapshots/` after a
  layout change (`cargo test --lib layout_tests`, then resize to 1200px wide). They are the headless
  renderer's real output, not staged captures; keep it that way.
- `LICENSE` (GPL-3.0 verbatim), `NOTICE.md` (the Mionix artwork carve-out and the credit request) and
  `CONTRIBUTING.md`. The artwork in `resources/` is **not** GPL — it is Mionix's, included so the app
  can show the device. Do not relicense it or imply otherwise.
- The vendor binaries are **not** in git and must not be committed — see `.gitignore`.

## Vendor-app capture rig

See "Reverse engineering the vendor software" above, and `research/README.md` for the full write-up.
`research/tools/capture-vendor-app.sh` has the known-good invocation and records the approaches that do
not work. The vendor app is unstable under Wine and hangs periodically -- the log is flushed per
report, so captures survive a hang.
