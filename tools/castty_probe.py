#!/usr/bin/env python3
"""Mionix Castor (22d4:1316) LED probe.

Replays a captured vendor profile blob with the RGB bytes substituted, to verify the
protocol against real hardware. Usage:  python3 tools/castty_probe.py RR GG BB
This is a research tool -- the real implementation belongs in src/hardware/.
"""
import fcntl, os, glob, sys, time

VID, PID = "22D4", "1316"

def ioc(d, t, nr, size): return (d << 30) | (size << 16) | (ord(t) << 8) | nr
def HIDIOCSFEATURE(n): return ioc(3, 'H', 0x06, n)
def HIDIOCGFEATURE(n): return ioc(3, 'H', 0x07, n)

def find_config_node():
    """Interface 1 (vendor collection). Never trust a fixed hidrawN index."""
    for p in glob.glob("/sys/class/hidraw/hidraw*"):
        try:
            uevent = open(os.path.join(p, "device/uevent")).read()
        except OSError:
            continue
        if f"HID_ID=0003:0000{VID}:0000{PID}" in uevent and "input1" in uevent:
            return "/dev/" + os.path.basename(p)
    return None

def set_feature(fd, payload):
    buf = bytearray(payload)
    fcntl.ioctl(fd, HIDIOCSFEATURE(len(buf)), buf, True)

def get_feature(fd, rid, size):
    buf = bytearray(size); buf[0] = rid
    fcntl.ioctl(fd, HIDIOCGFEATURE(size), buf, True)
    return bytes(buf)

def identify(fd):
    set_feature(fd, bytes([0x60, 0x02] + [0] * 62))
    time.sleep(0.05)
    r = get_feature(fd, 0x60, 64)
    ver = r[0x10] | (r[0x11] << 8)
    mcu = r[0x17:0x1c].decode('ascii', 'replace')
    return ver, mcu, r

LED1, LED2 = 39, 43          # <mode><R><G><B> records live at 38 and 42
CMD_COMMIT = 0x04

def main():
    # usage: castty_probe.py RR GG BB [MODE]
    rgb = tuple(int(sys.argv[i], 16) for i in (1, 2, 3)) if len(sys.argv) > 3 else (0xFF, 0x00, 0xFF)
    mode = int(sys.argv[4], 16) if len(sys.argv) > 4 else None
    node = find_config_node()
    if not node:
        sys.exit("Castor config interface not found")
    print(f"device: {node}")

    here = os.path.dirname(os.path.abspath(__file__))
    fixture = os.path.join(here, "..", "captures", "profile1-blue.bin")
    blob = list(open(fixture, "rb").read())
    assert blob[0] == 0x61 and blob[1] == 0x08 and len(blob) == 1041, "bad fixture"

    fd = os.open(node, os.O_RDWR)
    try:
        ver, mcu, _ = identify(fd)
        print(f"firmware: 0x{ver:04x}  mcu: {mcu}")

        print(f"before: LED1={blob[39]:02x}{blob[40]:02x}{blob[41]:02x} mode=0x{blob[38]:02x}  "
              f"LED2={blob[43]:02x}{blob[44]:02x}{blob[45]:02x} mode=0x{blob[42]:02x}")
        for base in (LED1, LED2):
            blob[base], blob[base + 1], blob[base + 2] = rgb
            if mode is not None:
                blob[base - 1] = mode
        desc = f"{rgb[0]:02x}{rgb[1]:02x}{rgb[2]:02x}"
        if mode is not None:
            desc += f" mode=0x{mode:02x}"
        print(f"writing LED1=LED2={desc}")

        set_feature(fd, bytes(blob))                       # profile blob
        time.sleep(0.05)
        term = [0] * 1041; term[0] = 0x61; term[1] = 0x08; term[5] = 0; term[6] = 0x01
        set_feature(fd, bytes(term))                       # per-profile terminator
        time.sleep(0.05)
        set_feature(fd, bytes([0x60, CMD_COMMIT] + [0] * 62))   # global commit
        print("commit sent (0x60 cmd 0x04)")
    finally:
        os.close(fd)

if __name__ == "__main__":
    main()
