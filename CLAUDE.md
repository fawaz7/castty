# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

Goal: `castty`, a Linux desktop application (Rust, iced) that configures the Mionix Castor mouse (LED colour,
DPI, polling rate, button mapping). No Linux tool for this device exists — not in OpenRGB, not elsewhere —
so the protocol is being derived from scratch.

**The protocol is cracked and verified against hardware.** LED colour can be set from Linux with no Wine
involved. See `PROTOCOL.md` for the wire format — read it before touching anything device-related.

What exists:
- `PROTOCOL.md` — the wire protocol. Authoritative; keep it updated as fields are decoded.
- `captures/*.bin` — three golden 1041-byte profile blobs (red/green/blue) captured from the vendor app.
  Use these as regression fixtures for the decoder; they are the only record of known-good frames.
- `tools/castty_probe.py` — research probe. Replays a fixture with substituted RGB. `python3
  tools/castty_probe.py RR GG BB`. Proves the pipeline; not the real implementation.
- Vendor binaries (see "Reverse engineering the vendor software").

**No Rust crate yet.** The build commands below are the intended layout; verify against `Cargo.toml` once
it exists and rewrite that section when it does.

Still to decode before the GUI can be feature-complete: LED effect modes (only solid `0x01` seen), DPI
step encoding, polling rate, lift-off distance, and button/macro mapping. Each needs another capture round
with the vendor app — the rig is already set up and reproducible.

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

Two vendor artifacts are kept in the repo root:

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

Prepared under the session scratchpad (`hidsnoop.c`, `hidsnoop.so`, `60-mionix-castor.rules`, prefix `wp/`):

1. **udev rule** `60-mionix-castor.rules` grants `uaccess` on `22d4:1316`. Required for both the capture and
   the finished tool — `/dev/hidraw*` is `root:root 0600` by default.
2. **Wine prefix** must whitelist the device, or winebus hides it (non-gamepad HID is excluded by default):
   `HKLM\System\CurrentControlSet\Services\winebus\Parameters` → `EnableHidraw` = `22d4/1316`
   (format is `vid/pid`; the Proton equivalent is `PROTON_ENABLE_HIDRAW=0x22d4/0x1316`).
3. **`hidsnoop.so`** is an `LD_PRELOAD` shim wrapping `ioctl()`, hex-dumping every `HIDIOCSFEATURE` (before the
   call) and `HIDIOCGFEATURE` (after), to `$HIDSNOOP_LOG`. Preferred over usbmon: it needs no root and logs the
   exact buffers the app passes, already framed per report.

```sh
export WINEPREFIX=<scratch>/wp
LD_PRELOAD=<scratch>/hidsnoop.so HIDSNOOP_LOG=<scratch>/cap.log \
  wine "CASTOR Software V1.44/CASTOR Software.exe"
```

Method: change **one** setting at a time in the GUI, then diff consecutive `SET_FEATURE` dumps. That isolates
which byte carries which field almost immediately.

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
`led <RRGGBB>`, `mode <solid|strobe>`, `dpi <1|2|3> <value>`, `surface`, `reset`. The CLI stays useful
for scripting and for working on the hardware layer without a display.

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

`tests/profile.rs` runs entirely against the captured blobs in `captures/`. The important one is
`recolouring_reproduces_the_captured_transition`: it takes the frame the vendor app sent for red,
recolours it, and requires the result to equal the frame it sent for green. A passing encode is
therefore a frame the device has actually accepted. Prefer adding tests in that style -- assertions
against real captured frames -- over synthesising expected bytes by hand.

### Ruled out — do not re-investigate

`PROTOCOL.md` records these as tested negatives, with the evidence. They cost real time to establish:

- **Effect speed is not adjustable.** Fixed in firmware.
- **Per-LED effects are impossible.** Colour is per-LED; the mode byte is global, enforced by hardware.
- **Host-driven custom effects are unsafe.** Nothing is visible until the commit, and commits write
  flash. Animating from the host would destroy the device.

### Hardware notes

- The device has **no read path** (see PROTOCOL.md). Current settings cannot be queried, so state is
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
- `packaging/io.github.fawaz7.castty.desktop` and `packaging/icons/hicolor/` — desktop integration.
  The scalable SVG is the **only** hand-edited artwork; every PNG is generated from it by
  `tools/generate-icons.sh`, and the 128px one is also embedded in the binary as the window icon.
  Edit the SVG and rerun the script; never touch a PNG directly. The application id
  (`iced_ui::APP_ID`), the desktop entry's basename, its `Icon` key and its `StartupWMClass` must all
  stay equal or the shell shows a placeholder icon for the running window; a test in
  `src/iced_ui/mod.rs` checks this.
- `captures/*.bin` — device frames used as test fixtures. Every protocol claim should be checkable
  against one of these.
- `captures/*.log.gz` — raw capture sessions, gzipped (7.2 MB to 140 KB). `tools/decode_capture.py`
  reads either form.
- The vendor binaries are **not** in git and must not be committed — see `.gitignore`.

## Vendor-app capture rig

Only needed to decode *new* fields. `tools/capture-vendor-app.sh` has the known-good invocation and
records the approaches that do not work; `tools/decode_capture.py` turns a capture into a field map.
The vendor app is unstable under Wine and hangs periodically -- the log is flushed per report, so
captures survive a hang.
