#!/usr/bin/env bash
# Packages one built binary for a release.
#
#   dist/package.sh <binary> <target> <version>
#
# <binary> is the path of the built program (`moebius` or `moebius-desktop`; on Windows `moebius.exe`, with
# `moebius-desktop.exe` beside it), <target> the Rust target triple it was built for (Linux or Windows, x86_64) and
# <version> the version without the leading `v`. The archive is written to `dist/out/` and its checksum is added to
# `dist/out/SHA256SUMS`:
#
#   moebius-<version>-<target>.tar.gz           Linux, the server: the program and the unit files
#   moebius-<target>                            Linux, the server's program alone, which the desktop app installs over SSH
#   moebius-desktop-<version>-<target>.tar.gz   Linux, the desktop app: the program, the icon and the desktop entry
#   moebius-windows-x86_64.zip                  Windows: moebius.exe, moebius-desktop.exe, LICENSE and NOTICE
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
case $target in
    x86_64-*-windows-*) name=windows ;;
esac
case $name in
    windows)
        # Both programs in one archive, named without the version.
        stage=$work/moebius-windows-x86_64
        mkdir -p "$stage"
        cp "$binary" "$stage/moebius.exe"
        cp "$(dirname "$binary")/moebius-desktop.exe" "$root/LICENSE" "$root/NOTICE" "$stage/"
        archive=$out/moebius-windows-x86_64.zip
        rm -f "$archive"
        if command -v zip >/dev/null 2>&1; then
            (cd "$work" && zip -qr "$archive" "$(basename "$stage")")
        else
            # The Windows runners have no zip, but they have PowerShell.
            powershell.exe -NoProfile -Command \
                "Compress-Archive -Path '$(cygpath -w "$stage")' -DestinationPath '$(cygpath -w "$archive")' -Force"
        fi
        ;;
    moebius)
        cp "$binary" "$stage/moebius"
        chmod 755 "$stage/moebius"
        cp "$dist/moebius.service" "$root/README.md" "$root/LICENSE" "$root/NOTICE" "$stage/"
        archive=$out/$name-$version-$target.tar.gz
        tar -C "$work" -czf "$archive" "$(basename "$stage")"
        install -m 755 "$binary" "$out/moebius-$target"
        checksum "$out/moebius-$target"
        ;;
    moebius-desktop)
        cp "$binary" "$stage/moebius-desktop"
        chmod 755 "$stage/moebius-desktop"
        cp "$icon" "$stage/moebius-desktop.png"
        cp "$dist/moebius-desktop.desktop" "$root/LICENSE" "$root/NOTICE" "$stage/"
        archive=$out/$name-$version-$target.tar.gz
        tar -C "$work" -czf "$archive" "$(basename "$stage")"
        ;;
    *)
        echo "package.sh: do not know how to package $name" >&2
        exit 2
        ;;
esac

checksum "$archive"
echo "$archive"
