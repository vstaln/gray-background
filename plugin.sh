#!/bin/sh
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
sys="$(uname -s)"
case "$sys" in
MINGW* | MSYS* | CYGWIN*) rel="x86_64-pc-windows-msvc/background.exe" ;;
Darwin*)
    case "$(uname -m)" in
    arm64) rel="aarch64-apple-darwin/background" ;;
    *) rel="x86_64-apple-darwin/background" ;;
    esac ;;
*)
    case "$(uname -m)" in
    aarch64 | arm64) rel="aarch64-unknown-linux-musl/background" ;;
    *) rel="x86_64-unknown-linux-musl/background" ;;
    esac ;;
esac
if [ -x "$here/$rel" ]; then exec "$here/$rel" "$@"; fi
for cand in "$here/target/release/background.exe" "$here/target/release/background" "$here/target/debug/background.exe" "$here/target/debug/background"; do
    if [ -x "$cand" ]; then exec "$cand" "$@"; fi
done
echo "background binary not found in $here" >&2
exit 1
