#!/usr/bin/env python3
"""Decode a hidsnoop capture of the Mionix vendor app into a field map.

Groups the capture into Apply bursts (each ends with a 0x60/0x04 commit) and
diffs consecutive profile blobs, so each GUI change shows up as the exact set of
bytes it moved.

    python3 research/tools/decode_capture.py capture.log [--profile N]
"""
import re, sys, collections

CMD_NAMES = {0x02: "identify", 0x03: "status-poll", 0x04: "COMMIT", 0x08: "profile-write"}
KNOWN = {
    5: "profile index", 16: "profile index (dup)", 34: "const 0x08",
    38: "LED1 mode", 39: "LED1 R", 40: "LED1 G", 41: "LED1 B",
    42: "LED2 mode", 43: "LED2 R", 44: "LED2 G", 45: "LED2 B",
}
for i in range(17, 25):
    KNOWN[i] = f"name[{i-17}]"


def parse(path):
    # Capture logs are stored gzipped in the repo; accept either form.
    if path.endswith(".gz"):
        import gzip
        text = gzip.open(path, "rt", errors="replace").read()
    else:
        text = open(path, errors="replace").read()
    lines = text.split("\n")
    ops, i = [], 0
    while i < len(lines):
        m = re.match(r'\[(\d+\.\d+)\] (SET_FEATURE|GET_FEATURE) fd=(\d+) len=(\d+)', lines[i])
        if not m:
            i += 1
            continue
        ts, kind, ln = float(m.group(1)), m.group(2), int(m.group(4))
        data, j = [], i + 1
        while j < len(lines) and re.match(r'^  [0-9a-f]{4}  ', lines[j]):
            data += [int(x, 16) for x in lines[j][8:8 + 47].split()]
            j += 1
        if ln > 0:
            ops.append({"ts": ts, "kind": kind, "len": ln, "data": data})
        i = j
    return ops


def main():
    path = sys.argv[1]
    want = None
    if "--profile" in sys.argv:
        want = int(sys.argv[sys.argv.index("--profile") + 1])
    ops = parse(path)
    sets = [o for o in ops if o["kind"] == "SET_FEATURE"]
    print(f"{len(ops)} feature ops ({len(sets)} writes)\n")

    counts = collections.Counter((o['data'][0], o['data'][1]) for o in sets)
    print("command histogram:")
    for (rid, cmd), n in sorted(counts.items()):
        print(f"  report 0x{rid:02x} cmd 0x{cmd:02x}  x{n:<4} {CMD_NAMES.get(cmd,'?')}")

    # full profile blobs, grouped by profile index
    # real profile writes carry an ASCII name; the short per-profile terminator
    # frames are the same length but blank, so exclude them or every diff is noise
    blobs = [o for o in sets
             if o["len"] > 1000 and o["data"][1] == 0x08 and o["data"][17] != 0]
    by_prof = collections.defaultdict(list)
    for b in blobs:
        by_prof[b["data"][5]].append(b)

    print(f"\nprofile blobs captured: " +
          ", ".join(f"P{k}:{len(v)}" for k, v in sorted(by_prof.items())))

    for prof in sorted(by_prof):
        if want is not None and prof != want:
            continue
        seq = by_prof[prof]
        changed_any = False
        print(f"\n===== profile {prof}: {len(seq)} writes, diffing consecutively =====")
        for a, b in zip(seq, seq[1:]):
            d1, d2 = a["data"], b["data"]
            diffs = [(i, d1[i], d2[i]) for i in range(min(len(d1), len(d2))) if d1[i] != d2[i]]
            if not diffs:
                continue
            changed_any = True
            print(f"\n  t={b['ts']:.2f}  ({len(diffs)} bytes changed)")
            for i, x, y in diffs:
                label = KNOWN.get(i, "")
                print(f"    [{i:4d}] 0x{i:03x}  {x:02x} -> {y:02x}"
                      f"   ({x:5d} -> {y:5d}){'  ' + label if label else ''}")
            # flag plausible LE16 fields among the changed offsets
            offs = {i for i, _, _ in diffs}
            for i, _, _ in diffs:
                if i + 1 in offs:
                    old = d1[i] | (d1[i + 1] << 8)
                    new = d2[i] | (d2[i + 1] << 8)
                    if new and (new % 50 == 0 or old % 50 == 0):
                        print(f"      -> LE16 at [{i}]: {old} -> {new}")
        if not changed_any:
            print("  (no changes between writes)")


if __name__ == "__main__":
    main()
