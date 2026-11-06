#!/usr/bin/env bash
# Packages one built binary for a release.
#
#   dist/package.sh <binary> <target> <version>
#
# <binary> is the path of the built program (`moebius`; on Windows `moebius.exe`), <target> the Rust target triple it
# was built for (Linux or Windows, x86_64) and <version> the version without the leading `v`. The archive is written to `dist/out/` and its checksum is added to
# `dist/out/SHA256SUMS`:
#
#   moebius-<version>-<target>.tar.gz           Linux, the server: the program and the unit files
#   moebius-<target>                            Linux, the program alone, which `moebius connect` installs over SSH
#   moebius-windows-x86_64.zip                  Windows: moebius.exe, LICENSE and NOTICE
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
        # Named without the version.
        stage=$work/moebius-windows-x86_64
        mkdir -p "$stage"
        cp "$binary" "$stage/moebius.exe"
        cp "$root/LICENSE" "$root/NOTICE" "$stage/"
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
    *)
        echo "package.sh: do not know how to package $name" >&2
        exit 2
        ;;
esac

checksum "$archive"
echo "$archive"
