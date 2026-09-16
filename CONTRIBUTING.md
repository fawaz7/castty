# Contributing

Thanks for looking. This is an independent, unpaid project built around one mouse that its author
happens to own, so help is genuinely welcome.

## The honest constraint

**I only have a Mionix Castor.** I can't test what I can't hold. That shapes what's most useful:

### Other Mionix mice

The single highest-value contribution. If you own a Naos, an Avior, a Zibal, or a **Castor PRO**
(`22d4:1320`/`1321` — a different device, not supported here), the protocol is very likely a close
relative. [`research/README.md`](research/README.md) documents the capture rig end to end for exactly
this purpose: run the vendor software under Wine, snoop the ioctls, diff one setting at a time.

A new device needs, roughly:

1. Captures, decoded into a byte map (`research/tools/decode_capture.py` does most of this).
2. Fixtures committed to `research/captures/` and a section in `research/PROTOCOL.md`.
3. A device entry in `src/hardware/` and a udev rule line in `packaging/`.

Open an issue before you start and I'll help where I can, even blind.

### Everything else

- **Packaging** — AUR, Flatpak, Nix, `.deb`. None exist yet.
- **Testing** on other distributions, compositors and GPU drivers.
- **The remaining unknowns** in `research/PROTOCOL.md` — a few constants have no known purpose.
- **Porting the protocol elsewhere.** If you'd rather add Castor support to OpenRGB, libratbag or a
  kernel driver than contribute here, please do. That's a better outcome than everyone deriving it
  separately, and it's why `research/` is published separately from the app.

## Working on the code

```sh
cargo test                       # full suite; no hardware or display needed
cargo clippy --all-targets       # must stay clean
cargo test --lib layout_tests    # renders every page into target/ui-snapshots/
cargo run                        # launch the GUI
```

Everything runs without a mouse plugged in. The tests assert against real captured device frames in
`research/captures/`, so a passing test means a frame the hardware has actually accepted.

### Conventions worth knowing before you start

- **Device protocol stays inside `src/hardware/`.** The UI never constructs a report buffer.
- **All device I/O happens off the UI thread**, in `src/iced_ui/worker.rs`. Feature reports take tens
  of milliseconds and the mouse can be unplugged mid-call, so every hardware call returns a `Result`
  the UI renders as a disconnected state — never an `unwrap`.
- **One page per file** in `src/iced_ui/pages/`; `lighting.rs` is the template. Pages don't reach into
  one another.
- **No literal sizes or colours in page code.** Colours come from `theme.rs`, spacing and type scale
  from `widgets.rs`.
- **`Profile` patches only known fields on encode**, so unknown regions — including the unmapped
  880-byte macro area — survive a read/modify/write. Preserve that property when adding fields.
- **Layout gotcha:** never put `Length::Fill` on the vertical axis inside a vertical `scrollable`. It
  resolves against infinity and collapses to zero.
- **Look at the snapshot PNGs after any layout change.** A green suite proves the logic, not the
  picture.

`docs/UI-DESIGN-BRIEF.md` records the design rules and the reasoning behind them.

## Touching the hardware

Writes go to STM32 flash on a device that has been out of production for years. A malformed write can
brick it.

- Reads are the safe half. Start there.
- **Replay, don't synthesise**: patch a captured blob rather than building one from zero.
- Know Mionix's firmware-revert procedure before your first write.
- The hardware LED self-test is hold left + right + wheel while plugging in.

## Licence

By contributing you agree your work is licensed under GPL-3.0, like the rest of the project. See
[`NOTICE.md`](NOTICE.md) for the one carve-out (the Mionix artwork) and a request about credit.
