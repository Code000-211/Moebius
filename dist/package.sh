#!/usr/bin/env bash
# Packages one built binary for a release.
#
#   dist/package.sh <binary> <target> <version>
#
# <binary> is the path of the built program (`moebius` or `moebius-desktop`, with `.exe` on Windows), <target> the Rust
# target triple it was built for and <version> the version without the leading `v`. The archive is written to `dist/out/`
# and its checksum is added to `dist/out/SHA256SUMS`:
#
#   moebius-<version>-<target>.tar.gz           the server: the program and the unit files
#   moebius-desktop-<version>-<target>.tar.gz   the desktop app on Linux: the program, the icon and the desktop entry
#   moebius-desktop-<version>-<target>.tar.gz   the desktop app on macOS: Moebius.app
#   moebius-desktop-<version>-<target>.zip      the desktop app on Windows: moebius-desktop.exe
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "usage: dist/package.sh <binary> <target> <version>" >&2
    exit 2
fi
binary=$1
target=$2
version=${3#v}

dist=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(dirname "$dist")
icon=$root/crates/moebius-desktop/assets/icon.png
out=$dist/out
name=$(basename "$binary")
name=${name%.exe}
mkdir -p "$out"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage=$work/$name-$version-$target
mkdir -p "$stage"

# Writes the checksum line of an archive to SHA256SUMS, replacing an earlier line for the same archive.
checksum() {
    local file=$1
    local sum
    if command -v sha256sum >/dev/null 2>&1; then
        sum=$(sha256sum "$file" | cut -d' ' -f1)
    else
        sum=$(shasum -a 256 "$file" | cut -d' ' -f1)
    fi
    touch "$out/SHA256SUMS"
    grep -v "  $(basename "$file")\$" "$out/SHA256SUMS" >"$work/sums" || true
    echo "$sum  $(basename "$file")" >>"$work/sums"
    sort -k2 "$work/sums" >"$out/SHA256SUMS"
}

archive=
case $name in
    moebius)
        cp "$binary" "$stage/moebius"
        chmod 755 "$stage/moebius"
        cp "$dist/moebius.service" "$dist/dev.moebius.server.plist" "$root/README.md" "$stage/"
        archive=$out/$name-$version-$target.tar.gz
        tar -C "$work" -czf "$archive" "$(basename "$stage")"
        ;;
    moebius-desktop)
        case $target in
            *-apple-darwin)
                app=$stage/Moebius.app
                mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
                cp "$binary" "$app/Contents/MacOS/moebius-desktop"
                chmod 755 "$app/Contents/MacOS/moebius-desktop"
                sed "s/@VERSION@/$version/g" "$dist/Info.plist" >"$app/Contents/Info.plist"
                if command -v sips >/dev/null 2>&1 && command -v iconutil >/dev/null 2>&1; then
                    iconset=$work/Moebius.iconset
                    mkdir -p "$iconset"
                    for size in 16 32 128 256; do
                        sips -z "$size" "$size" "$icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
                        sips -z "$((size * 2))" "$((size * 2))" "$icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
                    done
                    iconutil -c icns "$iconset" -o "$app/Contents/Resources/Moebius.icns"
                else
                    echo "package.sh: sips and iconutil not found; Moebius.app gets no icon" >&2
                fi
                archive=$out/$name-$version-$target.tar.gz
                tar -C "$stage" -czf "$archive" Moebius.app
                ;;
            *-windows-*)
                cp "$binary" "$stage/moebius-desktop.exe"
                archive=$out/$name-$version-$target.zip
                if command -v zip >/dev/null 2>&1; then
                    (cd "$work" && zip -qr "$archive" "$(basename "$stage")")
                else
                    # The Windows runners have no zip, but they have PowerShell.
                    powershell.exe -NoProfile -Command \
                        "Compress-Archive -Path '$(cygpath -w "$stage")' -DestinationPath '$(cygpath -w "$archive")' -Force"
                fi
                ;;
            *)
                cp "$binary" "$stage/moebius-desktop"
                chmod 755 "$stage/moebius-desktop"
                cp "$icon" "$stage/moebius-desktop.png"
                cp "$dist/moebius-desktop.desktop" "$stage/"
                archive=$out/$name-$version-$target.tar.gz
                tar -C "$work" -czf "$archive" "$(basename "$stage")"
                ;;
        esac
        ;;
    *)
        echo "package.sh: do not know how to package $name" >&2
        exit 2
        ;;
esac

checksum "$archive"
echo "$archive"
