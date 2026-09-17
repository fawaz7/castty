# Reverse engineering the Mionix Castor

This directory is the part of the project that isn't the app. It is the protocol, the evidence behind
it, and the rig used to get both — published so that anyone can use it, whether or not they ever run
`castty`.

**Take it.** Port the protocol to OpenRGB, libratbag, Piper, a kernel driver, a shell script, your own
GUI. That is what it is here for. It's GPL-3.0 like the rest of the repo; a credit is appreciated but
the licence is what governs.

```
research/
  PROTOCOL.md          the wire format — start here
  captures/            every frame the vendor software sent, as bytes
    *.bin              1041-byte profile blobs used as test fixtures
    sessions/*.log.gz  raw capture logs, gzipped
  tools/
    hidsnoop.c         LD_PRELOAD shim that logs hidraw feature reports
    capture-vendor-app.sh  runs the vendor app under Wine with the shim attached
    decode_capture.py  turns a capture log into a byte-level field map
    castty_probe.py    minimal standalone LED writer — the protocol in 88 lines
```

## The short version of what was found

The Castor speaks a plain HID feature-report protocol on **interface 1** (`22d4:1316`, usage page
`0xFF01`). Two reports carry everything:

| Report | Size | Role |
|---|---|---|
| `0x60` | 64 B | Commands: identify, status, commit, profile read, surface analyzer |
| `0x61` | 1041 B | The whole profile — name, LEDs, DPI, polling, buttons, macros |

Configuring the mouse is: write the 1041-byte blob, write a short per-profile terminator, then send
one `0x60`/`0x04` commit. [`PROTOCOL.md`](PROTOCOL.md) has every offset.

Reading works too: `0x60`/`0x07` with the profile index in `[5]` returns that profile on report
`0x61`, byte-for-byte as stored in flash. The vendor software never issues it, which is why this
document once claimed there was no read path at all — an absence in the captures was mistaken for an
absence in the firmware. Probing the command space directly found it. Validate what comes back: for
a few seconds after the mouse is plugged in, the command answers with zeros rather than refusing.

## How it was done

No Windows VM, no decompiler, no logic analyser. The method was:

1. **Find the channel.** `lsusb -v` and the HID report descriptors showed two interfaces. Interface 0
   is the ordinary boot-protocol mouse; interface 1 carries a vendor-defined collection with exactly
   two feature reports. That narrowed the search to two report IDs before a single byte was captured.

2. **Run the vendor software on Linux.** The Windows app (`CASTOR Software.exe` v1.44, a 32-bit MFC
   binary) runs under Wine, and Wine turns its `HidD_SetFeature`/`HidD_GetFeature` calls into
   `HIDIOCSFEATURE`/`HIDIOCGFEATURE` ioctls on `/dev/hidraw*`. So the real, working implementation
   could be made to drive the real hardware, on this machine, with its traffic in reach.

3. **Snoop the ioctls.** [`tools/hidsnoop.c`](tools/hidsnoop.c) is an `LD_PRELOAD` shim wrapping
   `ioctl()`. It hexdumps every feature report to a log. Preferred over usbmon: no root needed, and
   it logs the exact buffers the application passed, already framed one report per entry.

4. **Change one thing at a time.** In the vendor GUI: change a single setting, press Apply, repeat.
   Each Apply writes all five profiles and commits, so consecutive blobs for the same profile differ
   in exactly the bytes that setting owns. [`tools/decode_capture.py`](tools/decode_capture.py) does
   that diff automatically and prints a field map. Most fields fell out in one session of 56 applies.

5. **Verify by writing.** A field is not decoded until the bytes have been sent back to the device
   from Linux and the mouse has done the right thing. Several plausible-looking fields did not survive
   this — see the "ruled out" section below.

The whole loop is reproducible; step 2 onwards is one script.

## Reproducing a capture

You need the vendor software. It is **not** in this repository — redistributing Mionix's installer is
not ours to do. V1.44 is the last original-Castor release and is the one that matches PID `1316`; it
unzips to a portable `CASTOR Software.exe` with no installer. Archives of it are still findable.

```sh
# 1. Device access — /dev/hidraw* is root-only by default.
sudo install -m644 ../packaging/60-mionix-castor.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=hidraw

# 2. Capture. Builds the shim, configures the Wine prefix, launches the app.
APP_DIR="$HOME/CASTOR Software V1.44" WORK=/tmp/castor-capture \
  ./tools/capture-vendor-app.sh

# 3. In the vendor GUI: change ONE setting, press Apply. Repeat.

# 4. Decode.
python3 tools/decode_capture.py /tmp/castor-capture/capture.log --profile 0
```

Two prerequisites are non-obvious and cost real time to find:

- **Wine hides this device by default.** Interface 1's descriptor leads with a keyboard collection,
  and Wine's `is_hidraw_enabled()` blanket-rejects mouse and keyboard hidraw devices *before* it
  consults the documented `EnableHidraw` list — so that setting cannot help. The per-device override
  is checked first and does work:
  `HKLM\System\CurrentControlSet\Services\WineBus\Devices\22d4/1316` → `Hidraw` (REG_DWORD) = 1.
  The capture script sets this for you.
- **The vendor app is unstable under Wine** and hangs every so often. The shim flushes per report, so
  a capture survives the hang; just restart it. Do not try to make the window bigger — see the
  comments in the capture script for the approaches that make it worse.

## Using the protocol without this app

[`tools/castty_probe.py`](tools/castty_probe.py) is the smallest useful demonstration: it finds the
device, replays a captured blob with the RGB bytes substituted, and commits. Roughly 88 lines, no
dependencies.

```sh
python3 tools/castty_probe.py ff 66 00
```

If you are porting this, the pattern worth copying is **replay, don't synthesise**: start from a
captured blob and patch the fields you know. The blob has an 880-byte macro region and several
constants whose purpose is still unclear, and writes go to STM32 flash. Preserving the bytes you don't
understand is free; guessing at them is not.

## What the captures are

| File | What it is |
|---|---|
| `factory-default-p0..p4.bin` | The five profiles the vendor app writes for "reset to default". Embedded in `castty` as its starting state. |
| `profile1-red/green/blue.bin` | The same profile with only the LED colour changed. The regression fixture that proves an encoder is right. |
| `profile1-split-leds.bin` | Wheel and logo lit differently — established which record is which LED. |
| `profile1-macros.bin` | A profile carrying recorded macros. |
| `profile1-macro-timing.bin` | Macros with real millisecond deltas, vs. hold mode. |
| `profile2-buttons.bin` | Non-default button assignments. |
| `profile2-renamed.bin` | A renamed profile — located the name field. |
| `sessions/session-*.log.gz` | Raw capture logs, one file per session. 7.2 MB → 140 KB gzipped. `decode_capture.py` reads either form. |

Every protocol claim should be checkable against one of these. The app's test suite
(`cargo test --test profile`) asserts against them directly — the key test takes the frame the vendor
app sent for red, recolours it, and requires the result to be byte-identical to the frame it sent for
green. A passing encode is therefore a frame the device has actually accepted, not one synthesised to
match an expectation.

## Ruled out — please don't spend time re-deriving these

Recorded with evidence in [`PROTOCOL.md`](PROTOCOL.md):

- **Effect speed is not adjustable.** Fixed in firmware. No byte controls it.
- **Per-LED effects are impossible.** Colour is per-LED, but the mode byte is global and the hardware
  enforces it — verified by writing different modes to the two records.
- **Four of the six colour records are inert.** Real records with the right shape, kept in step by the
  vendor software, but nothing on this device reads them. Probably shared with another Mionix model.
- **Host-driven custom effects are unsafe.** Nothing is visible until the commit, and every commit
  writes flash. Animating from the host would wear the device out.

## A warning about writes

Reads are the safe half. Writes go to STM32 flash on a device that has been out of production for
years, and a malformed one can brick it. Know the firmware-revert procedure before your first write.
The device also has a hardware LED self-test if you need to check it is alive: hold left button +
right button + wheel click while plugging it in.

Nothing in this repository has ever damaged the hardware it was developed against, but that is not a
guarantee, and the licence disclaims warranty for a reason.
