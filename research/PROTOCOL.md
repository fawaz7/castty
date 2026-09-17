# Mionix Castor (22d4:1316) — wire protocol

Status: **complete for every setting the vendor software exposes.** Everything below is observed from
live capture of the vendor app (`CASTOR Software.exe` v1.44) running under Wine against real hardware,
then verified by writing the bytes back to the device, unless a row says otherwise. Confidence is
marked per field; the handful of remaining unknowns are called out explicitly rather than glossed.

Every claim here is checkable against a file in [`captures/`](captures/) — those are the actual frames
the vendor application sent. `castty`'s test suite asserts against them, so a passing test is a frame
the hardware has really accepted.

## Transport

- Device `22d4:1316`, two HID interfaces. **Interface 1** is the config channel (`bInterfaceProtocol 0`).
  Resolve it via sysfs (`HID_ID=0003:000022D4:00001316`, `HID_PHYS` ending `input1`) — never a fixed
  `/dev/hidrawN` index.
- Two vendor feature reports (usage page `0xFF01`):
  - `0x60` — 63 data bytes (64 with report ID). Command/response.
  - `0x61` — 1040 data bytes (1041 with ID). Bulk profile/macro blob; this carries every setting.

## Command pattern (confirmed)

The channel is **request/response over the same report**, not a plain register read:

1. `HIDIOCSFEATURE` on report `0x60` — a 64-byte buffer, `[0]=0x60`, `[1]=<command>`, rest zero.
2. `HIDIOCGFEATURE` on report `0x60` — 64 bytes back, `[1]=0x01` (ack/status), payload follows.
   `[0]` reads `0x00` in the captures here, but it is **not** a constant — see "The read path" below,
   where the same byte was observed taking a different value across power states. Do not validate it.

A bare `GET` with no preceding `SET` returns whichever response was latched last, and that survives
re-opening the device; on a freshly powered device it is zeros, which is why probing cold looks dead.
Both ioctls return 64; writes succeed.

## Commands observed

All verified against hardware. Command byte is `[1]`; report ID is `[0]`.

| Report | Cmd | Name | Notes |
|---|---|---|---|
| `0x60` | `0x02` | Identify | Returns firmware version + MCU string |
| `0x60` | `0x03` | Status poll | Vendor app repeats every 2 s; returns zeros while idle |
| `0x60` | `0x04` | **Commit** | Bare frame, payload all zero. Applies pending profile writes |
| `0x60` | `0x07` | **Profile read** | `[5]` = profile index 0-4. Reply is read on report `0x61` |
| `0x61` | `0x08` | Profile write | 1041-byte blob (below), or short terminator form |
| `0x60` | `0x05` | **Surface analyzer** | `[2]`=1 start, `[2]`=2 read result |

### `0x02` identify response

```
offset  10: 89 01 .. .. .. .. 1a 53 54 4d 33 32      "STM32"
```

- `[0x10..0x11]` LE16 = firmware version. Observed `0x0189`, matching USB `bcdDevice`.
- `[0x17..0x1b]` = ASCII `"STM32"`, matching USB `iSerial`.

### `0x61` / `0x08` — profile blob (1041 bytes)

Decoded from a 56-apply capture session, one setting changed per apply.

| Offset | Size | Field | Confidence |
|---|---|---|---|
| `[0]` | 1 | Report ID `0x61` | certain |
| `[1]` | 1 | Command `0x08` | certain |
| `[5]` | 1 | Profile index (0-4) | certain |
| `[6]` | 1 | `0x01` in the short terminator form | certain |
| `[16]` | 1 | Profile index (repeated) | certain |
| `[17..26]` | 10 | Profile name, ASCII, zero-padded | certain |
| `[34]` | 1 | Constant `0x08` | certain |
| `[37]` | 1 | **Polling rate divisor** — see below | high |
| `[38]` | 1 | Constant `0x01`, purpose unknown | — |
| `[39..62]` | 24 | **Six colour records**, see below | certain |
| `[66]` | 1 | Active DPI step. Constant `0x02` in all captures | low |
| `[68..71]` | 4 | **DPI step 1: X then Y, LE16 each** | certain |
| `[77..80]` | 4 | **DPI step 2: X then Y, LE16 each** | certain |
| `[86]` | 1 | **Lift-off distance**, 1-31. Factory default `0x0c` | certain |
| `[88]` | 1 | **Angle snapping**, level `0`-`15`. Default `0` | certain |
| `[98..101]` | 4 | **DPI step 3: X then Y, LE16 each** | certain |
| `[102]` | 1 | DPI step count. Constant `0x03` | high |
| `[104]` | 1 | **Angle tuning**, signed int8 two's complement, -30..+30 | certain |
| `[117..158]` | 42 | **Button table**, 6 x 7 bytes | certain |

#### Colour records

Six records of **`<R> <G> <B> <mode>`** — the colour comes *first*, the mode byte *last*:

```
[39] [43]           two independently settable records  -> the two physical LEDs
[47] [48]           two-byte gap, both zero
[49] [53] [57] [61] four more records -- inert, see below
```

#### LED colour response is not linear

The channels are ordered `<R><G><B>` and written verbatim, but the LEDs do **not** render an sRGB
value faithfully: the blue emitter is far less luminous than red. Full blue `0000ff` reads as a dark
navy, while full red is vivid. A colour like `1d00ff` -- only 11% red -- therefore looks magenta
rather than violet, because the weak blue cannot compete with it.

This is ordinary RGB LED behaviour, not a fault or a sign of ageing, and the vendor software has the
same characteristic; it writes the picked value unchanged too. `castty` does the same deliberately: a
correction curve would mean writing different bytes than the colour chosen, and would make our
results diverge from the vendor's for no gain in honesty.

#### The four extra colour records are inert

`[49]`, `[53]`, `[57]` and `[61]` are real records with the same `<R><G><B><mode>` shape, but
**nothing on this device reads them**.

Tested by setting each to a different colour, and then setting all four to magenta -- a colour no
physical LED was using -- across all five profiles, and exercising everything that might surface
them:

- cycling all three DPI steps with the DPI button: colours unchanged
- switching profiles from the mouse itself, through all five: colours changed to each profile's own
  LED colour, with no magenta at any point

No capture could have settled this: the vendor software never varies them independently. Only
writing distinct values directly could separate them.

They are most likely fields the shared Mionix software stack uses on another model.

**The vendor software does not keep their colour in step with the wheel.** An earlier version of
this document said it did, and that `castty` mirroring the wheel colour into them kept our frames
byte-identical to the vendor's. Both halves are wrong, and the captures in this directory show it:
in `profile1-red.bin` versus `profile1-green.bin` the *only* differing bytes are `[39]`, `[40]`,
`[43]` and `[44]` -- the wheel and logo. The four inert records stayed at the factory `c9ff00`
throughout `profile1-red`, `-green`, `-blue`, `-macros` and `-macro-timing`, while the wheel moved
through four different colours. The fixtures that do show all six agreeing are either factory blobs,
where every record is the same colour anyway, or `castty`'s own writes.

So `castty` mirrors and the vendor does not. The mirroring is harmless -- the records are inert, as
proven above -- but it is `castty`'s behaviour, not a reproduction of the vendor's, and the frames
are not byte-identical. **A new consumer of this protocol should patch `[39]` and `[43]` only and
leave the other four records alone**, which is both closer to the vendor and a smaller change to a
blob read from the device.

The **mode** byte is a separate matter and the advice to write all six stands: every fixture has all
six mode bytes equal, and while no vendor capture varies the mode -- they are all `0x01` -- the
hardware experiments above establish that the firmware reads the mode globally from `[42]`.

**`[39]` is the scroll wheel and `[43]` is the logo.** Established by lighting one at a time: with the
wheel set to red alone, `[39]` held `(255,0,0)` and `[43]` was `(0,0,0)`; with the logo green alone,
the reverse. They are independently settable.

All six **mode** bytes always change together, and this reflects the hardware, not just the vendor
app's habit. Two experiments, covering both nibbles:

- Animation: wheel `0x01` (solid) + logo `0x02` (blinking) -> **both solid**.
- Rainbow: wheel `0x11` (rainbow) + logo `0x01` (solid) -> **both rainbow**.

In each case the whole device followed the **first** record's byte (`[42]`) and the others were
ignored. So the entire mode byte -- animation *and* rainbow -- is a single global setting read from
`[42]`. Per-LED **colour** is independent; nothing else about the lighting is.

Write the same value to all six mode bytes anyway: the vendor app does, and relying on `[42]` alone is
an assumption about undocumented firmware.

**The mode byte is two fields, not an enum.** The low nibble selects the animation and the high
nibble turns on rainbow (colour cycling), and the two combine freely:

| Low nibble | Animation |
|---|---|
| `0x1` | Solid |
| `0x2` | Blinking |
| `0x3` | Pulsating |
| `0x4` | Breathing |

| High nibble | Colour |
|---|---|
| `0x0` | The stored colour |
| `0x1` | Rainbow — cycles the spectrum, stored colour ignored |

So `0x14` is "breathing **and** rainbow" and `0x11` is "solid + rainbow" (what the vendor UI calls
Color Shift). Confirmed on hardware: writing `0x14` produced a breathing rainbow.

This was missed at first because the vendor dropdown presents the eight combinations as a flat list;
the structure only appeared when all four `0x1x` values were tried in sequence.

#### Polling rate — `[37]`

Observed `0x01`, `0x02`, `0x04`, `0x08`, stepped in that order through the GUI's rate list. These are
divisors of 1000 Hz, so almost certainly `1`=1000 Hz, `2`=500 Hz, `4`=250 Hz, `8`=125 Hz.
**Not independently verified** — confirm before relying on it.

#### DPI steps — solved

The Castor stores **exactly three** DPI steps (the GUI slider has three points), which is why the slot
spacing looked irregular: the three slots are `[68]`, `[77]`, `[98]`, with unrelated fields in between.
`[102]` holds the step count and is constantly `0x03`, consistent with three.

Each slot is **X then Y as LE16**. The axes really are independent: a capture with the vendor UI's
link checkbox cleared produced `[68]`=850 (X) and `[70]`=3200 (Y), which also confirms X comes first.

**Nothing on the device records whether the axes are linked.** No byte changed when the checkbox was
toggled, so that is purely vendor-UI state. `castty` infers it from the data (`x == y` for every step)
and writes no flag.

Confirmed values range 400-9150, always multiples of 50.

`[66]` is constantly `0x02` and is the likely "currently active step" index, but it never changed in any
capture, so that is a guess. Switching the active step (the DPI button on the mouse) would confirm it.

#### Button table — `[117..158]`

Six entries of 7 bytes, one per physical button, each laid out as:

```
<type> <param> 00 00 00 00 0f
```

Entry order is fixed and is **not** the bitmask order:

| # | Offset | Button | Factory type/param |
|---|---|---|---|
| 0 | `[117]` | Left | `00` / `01` |
| 1 | `[124]` | Right | `00` / `02` |
| 2 | `[131]` | Wheel click | `00` / `04` |
| 3 | `[138]` | Side, front | `00` / `10` |
| 4 | `[145]` | Side, rear | `00` / `08` |
| 5 | `[152]` | DPI | `09` / `f1` |

Types, from a capture that reassigned five buttons at once:

| Type | Meaning | Param |
|---|---|---|
| `0x00` | Standard mouse button | Button bitmask: `01` left, `02` right, `04` middle, `08` side, `10` side |
| `0x01` | Scroll | Signed direction: `01` up, `ff` (-1) down |
| `0x02` | Single key | **HID usage code** — `0x1a` is `w` |
| `0x03` | **Macro** | See the macro section; the entry carries a pointer and event count |
| `0x08` | **Profile switch** | `f0` up, `f2` down, `f1` roll |
| `0x09` | **DPI switch** | `f0` up, `f2` down, `f1` roll — the same three, captured |
| `0xff` | Disabled | `00` |

Both switches were captured by assigning each direction in turn and take the same three parameters:
`f0` up, `f2` down, `f1` roll.

**The mouse can switch its own profiles.** Profile switch is a button function (`type 0x08`), so no
host software is needed once it is assigned — which is why profiles are worth supporting properly.

An earlier version of this document placed the table at `[118]` with the fields reversed. The
alignment was settled by noticing that remapped bytes always changed in pairs seven apart.

#### Angle tuning — `[104]`

Signed 8-bit two's complement. Captured across the full GUI range, which pins the encoding exactly:

| GUI value | Byte |
|---|---|
| -30 | `0xe2` |
| -15 | `0xf1` |
| 0 | `0x00` |
| +15 | `0x0f` |
| +30 | `0x1e` |

#### Surface analyzer (S.Q.A.T.) — `0x60` / `0x05`

Mionix's **Surface Quality Analyzer Tool**. Per Mionix's own description it measures the *data loss
between successive images taken by the sensor* and reports a rating where higher means less loss;
their marketing rates mousepads on a **1-10** scale and calls 8+ "no loss of tracking".

Two steps on the command report, not part of the profile blob:

1. `[1]=0x05`, `[2]=0x01` — start. Response is a bare ack.
2. `[1]=0x05`, `[2]=0x02` — read result, returned at **`[16]`**.

**The user must move the mouse across the surface between the two calls.** The vendor UI (recovered
from its skin bitmaps) shows "Move the mouse over the surface area... make sure that you cover as much
as possible of the area", with an elapsed-seconds counter and a separate "Show result" button.
Reading immediately after starting measures nothing -- the `0x28` (40) captured early in this project
was taken after ~200 ms with the mouse stationary and should not be treated as a real reading.

**Raw-to-score mapping: `score = round(raw x 0.18)`**, and the vendor UI displays that score times
ten -- which is why a raw of ~40 appears as "70" and looked like a discrepancy between our reading and
theirs. Our raw values match the vendor's exactly; only the presentation differed.

Fitted by running the vendor tool across five surfaces while capturing the raw byte behind each
displayed score:

| Surface | Raw | Vendor score |
|---|---|---|
| A4 paper, mouse not moved | 0 | 0 |
| Clear plastic case | 13 | 20 |
| Wooden desk | 28 | 50 |
| Aluminium laptop lid | 38 | 70 |
| Mousepad | 39 | 70 |

The vendor quantises to whole units, so raw 38, 39 and 40 all show as 70. **The top of the range is
unverified** -- no surface tested scored above 7, so the slope is fitted from the lower two thirds.

A reading of 0 means the sensor gathered nothing, normally because the mouse was not moved during the
measurement window.

The vendor's measurement window is about 10 s (it shows a countdown), with the result read roughly
12-15 s after the start command.

#### Reset to default

**There is no reset command.** The vendor app implements it client-side: it writes a full profile blob
of factory values to every profile and commits, exactly like any other apply. `castty` should do the
same — the captured defaults are in `captures/factory-default-p*.bin`.

Factory defaults: DPI 3000, LED `(201,255,0)` mode `0x01` solid, polling `[37]=1`, angle snapping `0`,
angle tuning `0`.

#### Profile switching works from the mouse alone

Verified end to end: with a button bound to profile switch, pressing it cycles all five profiles with
no software running, each showing its own stored LED colour. The LEDs go **dark for roughly 300 ms**
during a switch, then come up in the new colour -- no flash or blink pattern.

That confirms the whole mechanism together: the commit byte selecting the active profile, five
profiles persisting independently on the device, and the button function doing what we decoded.

#### Profile selection — solved

**Byte `[5]` of the commit frame (`0x60` / `0x04`) is the active profile index.** The commit both
applies pending writes and switches the mouse to that profile; there is no separate select command.

Verified across four sessions: sessions 1-2 always committed `[5]=0` (editing Profile1), session 3
cycles `0`-`4` as profiles were switched, and session 4 sits on `[5]=1` throughout, dropping to `0`
exactly at the switch to Profile1 and returning to `1` on the way back.

Renaming works and round-trips (`[17..26]`, 10 bytes).

Note a vendor-app quirk worth **not** reproducing: switching away and back collapses per-LED colours,
rewriting both LEDs to the same value. The hardware has no such limitation.

#### Effect timing (measured)

Not adjustable, but worth recording -- no vendor documentation states these, and the UI preview is
driven from them. Measured by counting cycles against a stopwatch:

| Effect | Period | How measured |
|---|---|---|
| Blinking | 1.154 s | 26 cycles in 30 s |
| Pulsating | 1.58 s | 19 heartbeats in 30 s (each = two blackouts) |
| Breathing | 6.0 s | 5 cycles in 30 s |
| Rainbow hue loop | ~5 s | one full loop timed |

**Pulsating is a heartbeat, not a sine.** Each cycle is **two** blackouts in quick succession
(together under half a second), then the LEDs are held lit for the remainder. Breathing is the smooth
one. Getting the waveform wrong makes a preview that looks nothing like the hardware even when the
period is exactly right -- this one took three corrections to get from "sine" to "double beat".

#### Effect speed is not adjustable

**Tested and ruled out.** The vendor software exposes no speed control, no byte in four capture
sessions correlated with timing, and direct probing found nothing:

- Mode byte high nibble `0x2`, `0x3`, `0x4` (as `0x24`/`0x34`/`0x44`) all behaved as plain breathing.
  Note `0x34` has bit `0x10` set yet showed no rainbow, so the firmware matches the high nibble
  against an exact value rather than testing a bit; anything but `0` or `1` falls back to `0`.
- `[38]` (always `0x01`), `[47]` and `[48]` (the always-zero gap inside the LED block), and `[86]`
  were each swept across their range while breathing. No change of any kind.

Effect timing is fixed in firmware. Do not re-investigate without new evidence.

#### Writes only take effect on commit

A profile blob write alone changes nothing visible -- the LEDs keep their previous state. Only the
`0x60`/`0x04` commit applies staged data. Verified by writing a full green profile without a commit
(no change) and then committing the identical data (LEDs turned green).

**Consequence: there is no volatile path.** Every visible change costs a commit, and commits persist
to the MCU's flash. Host-driven animation -- repainting colours per frame to synthesise custom
effects -- would mean tens of flash writes per second against an endurance budget on the order of
10,000 cycles, and would destroy the device in about an hour. Do not build it on this path. Custom
lighting is limited to what the firmware implements unless a direct-control command is found.

#### The read path — `0x60` / `0x07`

**Every profile can be read back from flash.** An earlier version of this document stated the
opposite. That was wrong, and the error is worth recording: it reasoned from the vendor
application's behaviour rather than the firmware's. Across 1260 `GET_FEATURE` calls in two capture
sessions the vendor app never reads a profile back — it only ever reads identify (`0x02`) and the
status poll (`0x03`) — and that absence was taken as proof no such command existed. The firmware
implements one anyway. No capture could ever have revealed it; only probing the command space could.

The command follows the same request/response pattern as the rest of the `0x60` channel, but the
reply is read on report `0x61` because a profile does not fit in 64 bytes:

1. `HIDIOCSFEATURE` on report `0x60` — `[0]=0x60`, `[1]=0x07`, `[5]`=profile index 0-4.
2. `HIDIOCGFEATURE` on report `0x61` — 1041 bytes back.

`[5]` is the profile index, the same slot the write frame and the commit frame use. An index written
to `[2]`, `[3]`, `[4]`, `[7]` or `[16]` is ignored.

The response uses the **same field layout as the write blob**, so every offset in the table above
applies unchanged. Only the header differs, and only `[1]` of it is dependable:

- **`[1]` is `0x01`**, the ack the whole `0x60` channel answers with, where a write frame carries the
  `0x08` command. Solid across every read taken.
- **`[0]` carries no reliable value. Do not test it.** The captures in [`captures/`](captures/) all
  show `00`, and an earlier version of this section stated the header was `00 01` on that basis. It
  is not a constant. The same device, read with the same tool, answered `[0]=0x00` in one session and
  `[0]=0x60` in another hours later — stable *within* a session, different *across* power states. The
  five fixtures were all taken in one sitting shortly after a replug, so they record one of the
  values `[0]` can take, not a rule. A consumer that validated `[0]=0x00` refused every read on a
  device that had been up for a while, which is exactly how this was found. libratbag's driver for
  the same hardware ignores `[0]`, and that is the right call.

What *does* identify a good reply is `[1]`, the structural constants (`[34]`=`0x08`,
`[102]`=`0x03`), the DPI slots being in range, and the profile index the device echoes at `[16]`
matching the one that was asked for — the last of these also catches a stale latched response.
Thirty consecutive reads across all five profiles held all four of those without exception.

Verified against hardware:

- All five profiles read back **byte-exactly**. Profiles 0 and 1 matched `castty`'s persisted blobs
  and profiles 2-4 matched the factory-default captures, with zero differing bytes across the whole
  payload `[16..1041)` in all five cases — including the 880-byte macro region.
- The data is in **flash, not RAM**. After unplugging and replugging the mouse, the colours last
  written were still returned. A RAM copy of the last write would not survive losing power.

**Warm-up caveat.** For a short window after enumeration — still true about five seconds in — the
device answers `0x07` with an all-zero buffer instead of refusing. Anything reading a profile must
validate the response before trusting it: check the ack at `[1]` is `0x01`, the constant at `[34]` is
`0x08`, the DPI step count at `[102]` is `0x03`, and the DPI values are in range. A consumer that
patches a field into a zero-filled buffer and writes it back would erase the profile's DPI, buttons
and macros. Retry against a **wall-clock deadline longer than five seconds**, not a small number of
attempts: being asked too early is the ordinary case, and giving up early means falling back to
whatever local state the tool has, permanently.

**Do not validate the DPI values against the 400-9150 figure below.** That is the range *observed in
captures*, not a hardware limit, and a validated range narrower than what some tool can write is a
trap: the profile writes fine, and then every later read of it fails and the tool falls back to its
own stale copy for good. `castty`'s own slider spans 100-10000, so its validator does too. **The
validated range must be at least as wide as anything any tool can write.** Rejecting zero is all the
warm-up buffer needs.

Consequence for `castty`: the application no longer persists its own state as the primary record. It
reads all five profiles on connect and shows those; the state file is kept only as the fallback for
when no mouse is attached, or when a read will not validate. Any new consumer of this protocol
should do the same — read the device, and validate before believing it.

#### Macros

A button assigned a macro uses more of its 7-byte entry:

```
03 00 <ptr lo> <ptr hi> <event count> 00 0f
```

- `type` `0x03` means macro.
- `ptr` is LE16. **The event data lives at blob offset `16 + ptr`** — the pointer is relative to the
  start of the profile payload at `[16]`, not to the blob.
- `event count` counts individual events, and each keypress is **two** events (press and release).

Each event is 7 bytes:

```
01 <hid usage> 00 <0 = press, 1 = release> <delay, LE24 milliseconds>
```

The delay is **milliseconds since the previous event**, little-endian across three bytes. On a press
it is the gap since the last key; on a release it is how long the key was held. Verified against the
vendor editor, which displays exactly these numbers -- a recording of `a`, `b`, `c` with deliberate
pauses stored 2175, 119, 2701, 111, 3927, 93, matching its display of "2175 ms down, 119 ms up" and
so on.

Macros are packed sequentially and each is followed by a **7-byte zero terminator**, so the next
macro's pointer is `ptr + 7 * (count + 1)`. Observed: a 3-key macro at `ptr` 800 (6 events) is
followed by a 6-key macro at `ptr` 849 = 800 + 7 x 7.

The first macro was allocated at `ptr` 800, i.e. blob offset 816, leaving `[816..1040]` — 224 bytes,
or 32 event slots — for macro storage. Everything before that in the tail stayed zero, so 816 appears
to be the base of the macro area.

Worked example, a macro of `a` `b` `c`:

```
01 04 00 00 00 00 00   a press
01 04 00 01 00 00 00   a release
01 05 00 00 00 00 00   b press
01 05 00 01 00 00 00   b release
01 06 00 00 00 00 00   c press
01 06 00 01 00 00 00   c release
00 00 00 00 00 00 00   terminator
```

#### Macro playback modes

The vendor editor offers "record delay" and "record hold", and they are **mutually exclusive** --
they are two uses of the same storage, selected by **byte 1 of the button entry**:

| Byte 1 | Mode | Storage |
|---|---|---|
| `0x00` | Timed playback | Press and release events with real millisecond deltas |
| `0xfe` | Hold | A single press event per key, no release, all delays zero |

A hold macro keeps its keys down for as long as the mouse button is held, so there is nothing to
time and no release to record -- which is why the editor shows 0 ms for one.

Event type `0x01` is "keyboard". The editor does not allow mouse clicks inside a macro, so no other
event type exists to capture.

### Apply sequence (verified by replay)

Per Apply the vendor app sends, for **every** profile 0-4:

1. `0x61` / `0x08`, full 1041-byte blob, `[5]` = profile index
2. `0x61` / `0x08`, short frame with `[5]` = index, `[6]` = `0x01` (per-profile terminator)

then once, globally:

3. `0x60` / `0x04` — commit

**This has been replayed successfully from Linux with no Wine involved**: taking a captured blob,
substituting `[39..41]` and `[43..45]`, and sending blob → terminator → commit changes the physical LED
colour. Writing only the active profile (not all five) is sufficient.

Reference implementation of the above: `tools/castty_probe.py` in this directory. Golden fixtures for the
three captured colours are in `captures/` and should be used as regression inputs for the Rust decoder.

## Reproducing the capture

See [`README.md`](README.md) in this directory for the capture rig. Two non-obvious prerequisites:

1. udev rule tagging `22d4:1316` `uaccess` (`/dev/hidraw*` is `root:root 0600` by default).
2. Wine hides the device otherwise: interface 1's descriptor leads with a **keyboard** collection
   (usage `0001:0006`), and `is_hidraw_enabled()` blanket-rejects mouse/keyboard hidraw devices.
   The `EnableHidraw` multi-string is checked *after* that rejection, so it cannot help. The per-device
   override is checked *before* it and does work:

   ```
   HKLM\System\CurrentControlSet\Services\WineBus\Devices\22d4/1316  →  Hidraw (REG_DWORD) = 1
   ```

   Subkey name format is `%04x/%04x`, lowercase.
