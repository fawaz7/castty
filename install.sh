#!/usr/bin/env sh
#
# castty installer.
#
#   ./install.sh                   build and install system-wide (/usr/local)
#   ./install.sh --prefix ~/.local install for this user only, no root
#   ./install.sh --uninstall       remove everything it installed
#   ./install.sh --help            all options
#
# Distribution packages are better if one exists for you -- see packaging/.
# This script is the fallback that works everywhere.
set -eu

APP_ID=io.github.fawaz7.castty
SIZES="16 24 32 48 64 128 256 512"
RULE=60-mionix-castor.rules
# The kernel reads rules from here and nowhere else, so this is fixed even for a
# --prefix install. `want_udev` is what --skip-udev turns off; the path itself
# stays set, because the "you still need to do this" message has to name it.
UDEV_DIR=/etc/udev/rules.d
want_udev=yes

prefix=/usr/local
action=install
build=yes
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

# --- output -----------------------------------------------------------------

if [ -t 1 ] && [ -z "${NO_COLOUR:-}" ]; then
    b=$(printf '\033[1m'); dim=$(printf '\033[2m')
    red=$(printf '\033[31m'); grn=$(printf '\033[32m'); ylw=$(printf '\033[33m')
    r=$(printf '\033[0m')
else
    b=''; dim=''; red=''; grn=''; ylw=''; r=''
fi

say()  { printf '%s\n' "$*"; }
step() { printf '%s==>%s %s%s%s\n' "$grn" "$r" "$b" "$*" "$r"; }
warn() { printf '%s==>%s %s\n' "$ylw" "$r" "$*" >&2; }
die()  { printf '%serror:%s %s\n' "$red" "$r" "$*" >&2; exit 1; }

usage() {
    cat <<EOF
${b}castty installer${r}

USAGE:
    ./install.sh [options]

OPTIONS:
    --prefix DIR     install under DIR (default: /usr/local)
                     Use --prefix ~/.local for a single-user install with no root.
    --uninstall      remove an installation made by this script
    --no-build       install an already-built target/release/castty
    --skip-udev      do not touch ${UDEV_DIR} (you will need root later,
                     or castty cannot reach the mouse)
    --help           this message

WHAT IT INSTALLS:
    PREFIX/bin/castty
    PREFIX/share/applications/${APP_ID}.desktop
    PREFIX/share/icons/hicolor/<size>/apps/${APP_ID}.png  (and the SVG)
    ${UDEV_DIR}/${RULE}          always here; the kernel reads no other path

The udev rule is what makes the mouse reachable without root, so it needs root
to install even when everything else is going into your home directory.
EOF
}

while [ $# -gt 0 ]; do
    case $1 in
        --prefix) [ $# -ge 2 ] || die "--prefix needs a directory"; prefix=$2; shift 2 ;;
        --prefix=*) prefix=${1#--prefix=}; shift ;;
        --uninstall) action=uninstall; shift ;;
        --no-build) build=no; shift ;;
        --skip-udev) want_udev=no; shift ;;
        --help|-h) usage; exit 0 ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac
done

# Expand a leading ~ that the shell left alone (it does not expand inside
# --prefix=~/.local), and make the path absolute so the summary is honest.
case $prefix in "~/"*) prefix="$HOME/${prefix#\~/}" ;; "~") prefix="$HOME" ;; esac
case $prefix in /*) ;; *) prefix="$PWD/$prefix" ;; esac

# --- privilege --------------------------------------------------------------

# Only shell out to sudo for the parts that actually need it: a --prefix inside
# $HOME needs none, and asking for a password you do not need is a bad habit to
# teach.
SUDO=''
need_root_for() {
    dir=$1
    [ -n "$dir" ] || return 1
    while [ ! -d "$dir" ] && [ "$dir" != / ]; do dir=$(dirname "$dir"); done
    [ ! -w "$dir" ]
}
pick_sudo() {
    [ "$(id -u)" = 0 ] && return 0
    if command -v sudo >/dev/null 2>&1; then SUDO=sudo
    elif command -v doas >/dev/null 2>&1; then SUDO=doas
    else die "need root for $1, and neither sudo nor doas is installed"
    fi
}

# --- uninstall --------------------------------------------------------------

if [ "$action" = uninstall ]; then
    step "Removing castty from $prefix"
    need_root_for "$prefix" && pick_sudo "$prefix"
    $SUDO rm -f "$prefix/bin/castty"
    $SUDO rm -f "$prefix/share/applications/$APP_ID.desktop"
    for s in $SIZES; do
        $SUDO rm -f "$prefix/share/icons/hicolor/${s}x${s}/apps/$APP_ID.png"
    done
    $SUDO rm -f "$prefix/share/icons/hicolor/scalable/apps/$APP_ID.svg"

    if [ "$want_udev" = yes ] && [ -f "$UDEV_DIR/$RULE" ]; then
        need_root_for "$UDEV_DIR" && pick_sudo "$UDEV_DIR"
        $SUDO rm -f "$UDEV_DIR/$RULE"
        $SUDO udevadm control --reload 2>/dev/null || true
    fi

    command -v update-desktop-database >/dev/null 2>&1 &&
        $SUDO update-desktop-database "$prefix/share/applications" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 &&
        $SUDO gtk-update-icon-cache -f "$prefix/share/icons/hicolor" 2>/dev/null || true

    say ""
    # Under sudo, HOME is root's -- naming /root/.config/castty sends the user
    # to a directory that is not theirs and probably does not exist.
    user_home=$HOME
    [ -n "${SUDO_USER:-}" ] && user_home=$(eval echo "~$SUDO_USER")
    say "Removed. Your settings in ${dim}${XDG_CONFIG_HOME:-$user_home/.config}/castty${r} were left alone;"
    say "delete that directory too if you want no trace."
    exit 0
fi

# --- build ------------------------------------------------------------------

# Two layouts put the binary in two places. A source checkout builds to
# target/release/; the release tarball ships it at the top level beside this
# script, with no source to build from. Look for both, newest-wins is not the
# question -- a tarball simply has no target/ at all.
bin="$here/target/release/castty"
[ -x "$bin" ] || [ ! -x "$here/castty" ] || bin="$here/castty"

if [ "$build" = yes ]; then
    if ! command -v cargo >/dev/null 2>&1; then
        say ""
        die "cargo is not installed. Rust 1.88+ is needed to build castty.

  Debian/Ubuntu   sudo apt install cargo        (or rustup, for a current Rust)
  Arch            sudo pacman -S rust
  Fedora          sudo dnf install cargo
  anywhere        https://rustup.rs

Already have a built binary? Re-run with --no-build."
    fi
    step "Building castty (this takes a few minutes the first time)"
    ( cd "$here" && cargo build --release ) || die "build failed"
fi

if [ ! -x "$bin" ]; then
    if [ -f "$here/Cargo.toml" ]; then
        die "no binary at $bin -- drop --no-build to build it, or run cargo build --release first."
    else
        die "no castty binary found next to this script.

This looks like the release tarball rather than a source checkout, so there is
nothing here to build. Unpack the tarball again and run install.sh from inside
it, or clone the repository and run ./install.sh without --no-build."
    fi
fi

# --- install ----------------------------------------------------------------

step "Installing to $prefix"
need_root_for "$prefix" && pick_sudo "$prefix"

$SUDO install -Dm755 "$bin" "$prefix/bin/castty"
$SUDO install -Dm644 "$here/packaging/$APP_ID.desktop" \
    "$prefix/share/applications/$APP_ID.desktop"
for s in $SIZES; do
    $SUDO install -Dm644 "$here/packaging/icons/hicolor/${s}x${s}/apps/$APP_ID.png" \
        "$prefix/share/icons/hicolor/${s}x${s}/apps/$APP_ID.png"
done
$SUDO install -Dm644 "$here/packaging/icons/hicolor/scalable/apps/$APP_ID.svg" \
    "$prefix/share/icons/hicolor/scalable/apps/$APP_ID.svg"

# --- device access ----------------------------------------------------------

udev_installed=no
if [ "$want_udev" = yes ]; then
    step "Granting access to the mouse"
    need_root_for "$UDEV_DIR" && pick_sudo "$UDEV_DIR"
    $SUDO install -Dm644 "$here/packaging/$RULE" "$UDEV_DIR/$RULE"
    # Reload so it applies without a reboot; trigger so an already-connected
    # mouse picks it up rather than waiting to be replugged.
    $SUDO udevadm control --reload 2>/dev/null || true
    $SUDO udevadm trigger --subsystem-match=hidraw 2>/dev/null || true
    $SUDO udevadm trigger --subsystem-match=usb --attr-match=idVendor=22d4 2>/dev/null || true
    udev_installed=yes
fi

# --- caches -----------------------------------------------------------------

command -v update-desktop-database >/dev/null 2>&1 &&
    $SUDO update-desktop-database "$prefix/share/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 &&
    $SUDO gtk-update-icon-cache -f "$prefix/share/icons/hicolor" 2>/dev/null || true

# --- report -----------------------------------------------------------------

say ""
step "Installed"
say "  castty        $prefix/bin/castty"
say "  desktop entry $prefix/share/applications/$APP_ID.desktop"
if [ "$udev_installed" = yes ]; then
    say "  udev rule     $UDEV_DIR/$RULE"
else
    warn "udev rule NOT installed -- castty cannot reach the mouse until it is:"
    warn "  sudo install -Dm644 packaging/$RULE $UDEV_DIR/$RULE && sudo udevadm control --reload"
fi

# A prefix outside the default PATH is the single most common way a successful
# install still looks broken, so check rather than assume.
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) say ""
       warn "$prefix/bin is not in your PATH. Add this to your shell profile:"
       warn "  export PATH=\"$prefix/bin:\$PATH\"" ;;
esac

say ""
say "Check it: ${b}castty info${r}"
say "Launch it: ${b}castty${r}, or find it in your application menu."
if [ "$udev_installed" = yes ]; then
    say ""
    say "${dim}If castty reports no device, replug the mouse once -- the new rule${r}"
    say "${dim}applies on connect.${r}"
fi
