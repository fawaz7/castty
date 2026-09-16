#!/usr/bin/env sh
# Regenerate every raster icon from the one source SVG, so the sizes can never
# drift apart. The SVG under packaging/icons/hicolor/scalable/apps/ is the only
# hand-edited artwork; everything else here is generated.
#
# Needs rsvg-convert (librsvg), which ImageMagick uses as its SVG delegate.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
app=io.github.fawaz7.castty
src="$root/packaging/icons/hicolor/scalable/apps/$app.svg"
[ -f "$src" ] || { echo "missing source: $src" >&2; exit 1; }

for size in 16 24 32 48 64 128 256 512; do
    dir="$root/packaging/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    magick -background none "$src" -resize "${size}x${size}" "$dir/$app.png"
    printf '  %sx%s\n' "$size" "$size"
done
echo "regenerated from $(basename "$src")"
