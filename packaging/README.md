# Packaging

What a castty installation consists of, and how each package is built.

## The four files that matter

| File | Installed to | Why |
|---|---|---|
| `castty` | `/usr/bin/` | The binary. GUI with no arguments, CLI with them. |
| `60-mionix-castor.rules` | `/usr/lib/udev/rules.d/` | **Required.** `/dev/hidraw*` is `root:root 0600`, so without this castty finds no device. Tags `22d4:1316` with `uaccess`, granting the user logged in at the seat — no group to join, nothing else on the system touched. |
| `io.github.fawaz7.castty.desktop` | `/usr/share/applications/` | Application menu entry. |
| `icons/hicolor/**` | `/usr/share/icons/hicolor/` | Menu and window icons, 16px to 512px plus the SVG. |

A packaged rule belongs in `/usr/lib/udev/rules.d/`; `/etc/udev/rules.d/` is
reserved for the administrator's own overrides. `install.sh` writes to `/etc`
because it is acting *as* the administrator, not as a package.

The application id, the desktop entry's basename, its `Icon` key and its
`StartupWMClass` must all stay equal to `io.github.fawaz7.castty` or the shell
shows a placeholder icon for the running window. A test in `src/iced_ui/mod.rs`
enforces this — if you rename anything, that test tells you what else to rename.

## Runtime dependencies

`ldd` on the binary shows only libc, libm and libgcc. That is misleading: winit
and wgpu `dlopen` everything else, so nothing else appears in the ELF headers
while the app still cannot open a window without it.

The real set, established by tracing an actual run (`LD_DEBUG=libs`):

- **Required** — `libxkbcommon`, `libxkbcommon-x11`, `wayland-client`,
  `wayland-egl`, `libX11`, `libX11-xcb`, `libxcb`, `libXcursor`, `libXi`,
  `libXrandr`
- **Rendering** — `libvulkan` preferred, with a GL fallback. Neither is hard:
  wgpu picks what it finds, and falls back to software rendering if it finds
  nothing.

Any desktop system already has all of this. It is listed explicitly so a
minimal container or a headless-turned-desktop machine fails at install time
with a clear message rather than at launch with a blank window.

## Arch

[`arch/PKGBUILD`](arch/PKGBUILD) builds from the GitHub release tarball.

```sh
cd packaging/arch
makepkg -si
```

For the AUR, that PKGBUILD is the package source as-is; update `pkgver` and
regenerate `.SRCINFO` (`makepkg --printsrcinfo > .SRCINFO`) per release. Set
real `sha256sums` rather than `SKIP` when publishing — `makepkg -g` prints them
once the tag exists.

A `-git` variant, if you want one, is the same file with `pkgver()` deriving the
version from `git describe` and `source=("git+$url.git")`.

## Debian and Ubuntu

Driven by [`cargo-deb`](https://github.com/kornelski/cargo-deb) from the
`[package.metadata.deb]` block in `Cargo.toml`, so there is no separate
`debian/` directory to keep in step with it.

```sh
cargo install cargo-deb
cargo deb                 # writes target/debian/castty_<version>_amd64.deb
```

[`debian/postinst`](debian/postinst) reloads and triggers udev so the rule
applies immediately, rather than on the next replug or reboot.
[`debian/postrm`](debian/postrm) reloads after removal. Neither touches
`~/.config/castty/` — those settings are the user's, and a purge that silently
discarded the only record of a mouse's configuration would be unkind, given the
device cannot be read back.

Build on the **oldest** glibc you intend to support: a binary built against
glibc N runs on N and newer only. The release workflow uses `ubuntu-22.04`
(glibc 2.35), which covers Ubuntu 22.04+ and Debian 12+.

## Everything else

[`../install.sh`](../install.sh) installs from source on any distribution, or
from an already-built binary with `--no-build`. `--prefix ~/.local` gives a
single-user install that needs no root for anything except the udev rule, which
has to go in `/etc` because the kernel reads nowhere else.

## Releases

Tagging triggers [`.github/workflows/release.yml`](../.github/workflows/release.yml),
which tests, builds, and attaches a `.deb` and a portable tarball to a **draft**
GitHub release:

```sh
git tag -a v1.0.0 -m "castty 1.0.0"
git push origin v1.0.0
```

It stops at a draft on purpose — review the artifacts, then publish by hand.

Bump `version` in `Cargo.toml` before tagging. The About page and `castty
version` both read it from there, so that one edit is the whole version bump.

## Not yet packaged

Flatpak and Nix. Both are wanted; neither exists. The Flatpak would need
`--device=all` or a udev portal to reach the mouse, which is the part worth
thinking about before starting.
