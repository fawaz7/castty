#!/usr/bin/env sh
# Capture the running GUI with its real renderer (wgpu, system fonts), for
# checking what the headless tiny-skia snapshots in target/ui-snapshots/
# cannot: font metrics, GPU compositing, the connected-device status line.
#
# GNOME on Wayland refuses org.gnome.Shell.Screenshot to anything but its own
# tools, and there is no grim/xdotool on this machine, so the trick is to run
# the app on XWayland instead (unset WAYLAND_DISPLAY and winit picks X11) and
# grab its window by id with ImageMagick. `import -window root` sees nothing
# on XWayland, but a specific window id works.
#
# Usage: tools/screenshot-gui.sh [out.png]   (defaults to target/gui.png)
set -eu
out=${1:-target/gui.png}
bin=target/debug/castty
[ -x "$bin" ] || cargo build
env -u WAYLAND_DISPLAY DISPLAY="${DISPLAY:-:0}" "$bin" >/dev/null 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT
sleep 4
wid=
for w in $(xprop -root _NET_CLIENT_LIST | grep -o '0x[0-9a-f]*'); do
    case "$(xprop -id "$w" WM_NAME 2>/dev/null)" in
        *Castty*) wid=$w; break ;;
    esac
done
[ -n "$wid" ] || { echo "no Castty window found on $DISPLAY" >&2; exit 1; }
import -window "$wid" "$out"
echo "$out"
