#!/bin/bash
# Builds the downloadable Aède_VERSION_aarch64.dmg (release build, ad hoc signature, no Apple
# account), then checks it.
#   bash tools/build-dmg.sh
#
# What it does on top of a plain `tauri build`:
#   - hides the builder's home folder: compilers write source paths (cargo registry, project
#     folder) into the binaries, which would publish the name of the Mac account;
#   - rebuilds whisper.cpp and llama.cpp (cargo keeps them cached when only these flags change)
#     and copies the fresh llama.cpp libraries to target/release, where the bundle step reads them;
#   - mounts the .dmg and refuses it if a home path is still inside, or if the signature is bad.
# The first run after a normal `npm run app -- --release` rebuilds everything, which takes a few
# minutes. The next normal release build rebuilds everything again.
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

export RUSTFLAGS="--remap-path-prefix=$HOME=/build"
export CFLAGS="-ffile-prefix-map=$HOME=/build"
export CXXFLAGS="-ffile-prefix-map=$HOME=/build"

TARGET="src-tauri/target/release"

(cd src-tauri && cargo clean --release -p whisper-rs-sys -p llama-cpp-sys-2)
(cd src-tauri && cargo build --release -p llama-cpp-sys-2)

# The newest build output of llama-cpp-sys-2 holds the libraries just compiled.
LIBS="$(ls -td "$TARGET"/build/llama-cpp-sys-2-*/out/lib | head -n 1)"
rm -f "$TARGET"/libggml*.dylib "$TARGET"/libllama*.dylib
cp -a "$LIBS"/libggml*.dylib "$LIBS"/libllama*.dylib "$TARGET"/

npm run tauri build -- --config "$PWD/src-tauri/tauri.release.conf.json" --bundles dmg

DMG="$(ls -t "$TARGET"/bundle/dmg/*.dmg | head -n 1)"
MOUNT="$(mktemp -d /tmp/aede-dmg.XXXXXX)"
hdiutil attach -nobrowse -readonly -mountpoint "$MOUNT" "$DMG" > /dev/null
trap 'hdiutil detach "$MOUNT" > /dev/null 2>&1 || true' EXIT
APP="$MOUNT/Aède.app"

codesign --verify --deep --strict "$APP"
for file in "$APP/Contents/MacOS/aede" "$APP"/Contents/Frameworks/*.dylib; do
  if strings -a "$file" | grep -q "$HOME"; then
    echo "REFUSED: $file contains the path of the home folder." >&2
    exit 1
  fi
done
for resource in LICENSE THIRD-PARTY-NOTICES.md; do
  [ -f "$APP/Contents/Resources/$resource" ] || { echo "REFUSED: $resource is not in the app." >&2; exit 1; }
done

echo "OK: $DMG"
shasum -a 256 "$DMG"
