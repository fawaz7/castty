# Captures

The actual bytes. Every protocol claim in [`../PROTOCOL.md`](../PROTOCOL.md) is checkable against a
file here, and `castty`'s test suite asserts against them directly — so a passing test is a frame the
hardware has really accepted, not one synthesised to match an expectation.

All of it was recorded from `CASTOR Software.exe` v1.44 running under Wine against a real Mionix
Castor (`22d4:1316`), using [`../tools/hidsnoop.c`](../tools/hidsnoop.c).

## Profile blobs (`*.bin`)

Single 1041-byte `0x61`/`0x08` frames, exactly as the vendor software sent them. Byte `[0]` is the
report ID.

| File | What it isolates |
|---|---|
| `factory-default-p0.bin` … `p4.bin` | The five profiles written by the vendor app's "reset to default". Embedded in `castty` as its starting state. |
| `profile1-red.bin`, `profile1-green.bin`, `profile1-blue.bin` | One profile, LED colour the only difference. The core regression fixture: recolour red → must equal green, byte for byte. |
| `profile1-split-leds.bin` | Wheel and logo lit differently. Established which colour record drives which physical LED. |
| `profile1-macros.bin` | A profile carrying recorded macros — located the macro area and its event encoding. |
| `profile1-macro-timing.bin` | Macros with real millisecond deltas, distinguishing timed playback from hold mode. |
| `profile2-buttons.bin` | Non-default button assignments across all six buttons. |
| `profile2-renamed.bin` | A renamed profile — located the 10-byte ASCII name field. |
| `read-0x07-profile0.bin` … `profile4.bin` | The five profiles **read back off the device** with `0x60`/`0x07`, not sent by the vendor app. Evidence for the read path: profiles 0 and 1 are byte-identical to what `castty` had written, profiles 2-4 to the factory defaults. Note the `00 01` response header where a write frame carries `61 08`. |

## Session logs (`sessions/*.log.gz`)

Raw capture logs, one per session, gzipped (7.2 MB → ~140 KB). These are the unedited record: every
feature report the vendor app issued, in order, with timestamps.

```sh
python3 ../tools/decode_capture.py sessions/session-01.log.gz --profile 0
```

`decode_capture.py` reads gzipped or plain logs. It groups the traffic into Apply bursts — each ends
with a `0x60`/`0x04` commit — and diffs consecutive profile blobs, so a single setting changed in the
GUI shows up as the exact set of bytes it moved.

`session-01` is the big one: 56 applies, one setting each, and most of the field map fell out of it.

### Log format

```
[8144.831332] SET_FEATURE fd=71 len=64 ret=-1
  0000  60 02 00 00 00 00 00 00 00 00 00 00 00 00 00 00  |`...............|
  0010  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  |................|
```

Monotonic timestamp, direction, file descriptor, length, ioctl return, then a canonical hexdump.
`[open]` lines map file descriptors to hidraw nodes, which is how you tell which interface a given
`fd` belongs to. The decoder slices columns 8–55 out of the hexdump lines, so the spacing matters if
you write your own producer.
