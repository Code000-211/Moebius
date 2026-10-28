#!/usr/bin/env bash
# Packages one built binary for a release.
#
#   dist/package.sh <binary> <target> <version>
#
# <binary> is the path of the built program (`moebius` or `moebius-desktop`), <target> the Rust target triple it was
# built for (Moebius is released for Linux only) and <version> the version without the leading `v`. The archive is written to `dist/out/`
# and its checksum is added to `dist/out/SHA256SUMS`:
#
#   moebius-<version>-<target>.tar.gz           the server: the program and the unit files
#   moebius-desktop-<version>-<target>.tar.gz   the desktop app: the program, the icon and the desktop entry
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
mkdir -p "$out"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage=$work/$name-$version-$target
mkdir -p "$stage"

# Writes the checksum line of an archive to SHA256SUMS, replacing an earlier line for the same archive.
checksum() {
    local file=$1
    local sum
    sum=$(sha256sum "$file" | cut -d' ' -f1)
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
        cp "$dist/moebius.service" "$root/README.md" "$root/LICENSE" "$root/NOTICE" "$stage/"
        archive=$out/$name-$version-$target.tar.gz
        tar -C "$work" -czf "$archive" "$(basename "$stage")"
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
