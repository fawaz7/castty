#!/usr/bin/env bash
# Launch the Mionix vendor app under Wine with HID capture attached.
#
# This is the KNOWN-GOOD configuration. It was arrived at the hard way; the
# comments below record what does NOT work so it is not retried:
#
#   * gamescope upscaling (any backend) -- the window scales but clicks do not
#     register, and the app hangs. Not worth further effort.
#   * Patching the exe's `dpiAware` manifest flag to make Wine scale it -- the
#     app is a 2015 skinned MFC build with fixed-pixel layout. It renders bigger
#     but becomes unstable and hangs within minutes.
#   * `wineserver -k` -- kills the calling shell too. Kill the processes directly
#     from an isolated process group (setsid) instead.
#
# The app renders small on a HiDPI display and there is no fix for that which
# keeps it stable. Small but working beats large but hung.
#
# Usage:
#   APP_DIR=~/CASTOR\ Software\ V1.44 research/tools/capture-vendor-app.sh
#
# The vendor software is not redistributed here; see ../README.md for where to
# get it. Install ../../packaging/60-mionix-castor.rules first, or /dev/hidraw*
# stays root-only and the app sees no device.
set -u

here=$(cd "$(dirname "$0")" && pwd)

APP_DIR="${APP_DIR:?set APP_DIR to the directory containing 'CASTOR Software.exe'}"
WORK="${WORK:-$(mktemp -d)}"
SHIM="${SHIM:-$WORK/hidsnoop.so}"
LOG="${LOG:-$WORK/capture.log}"

# Build the ioctl shim unless one was supplied. It has no dependencies beyond
# libdl, so this is cheaper than keeping a binary around.
if [ ! -f "$SHIM" ]; then
    cc -shared -fPIC -O2 -o "$SHIM" "$here/hidsnoop.c" -ldl || exit 1
fi

export WINEPREFIX="${WINEPREFIX:-$WORK/wp}"
export DISPLAY="${DISPLAY:-:0}"
export WINEDEBUG=-all

# Wine hides this device unless told otherwise: interface 1's descriptor leads
# with a keyboard collection, and is_hidraw_enabled() rejects mouse/keyboard
# hidraw devices *before* consulting the documented EnableHidraw list.
wine reg add 'HKLM\System\CurrentControlSet\Services\WineBus\Devices\22d4/1316' \
     /v Hidraw /t REG_DWORD /d 1 /f >/dev/null 2>&1

: > "$LOG"
cd "$APP_DIR" || exit 1
LD_PRELOAD="$SHIM" HIDSNOOP_LOG="$LOG" \
  setsid nohup wine "CASTOR Software.exe" >"$WORK/run.log" 2>&1 </dev/null &

echo "capture log: $LOG"
echo "change ONE setting per Apply in the GUI; decode with research/tools/decode_capture.py"
