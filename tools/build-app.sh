#!/bin/bash
# Builds Aède.app, signs it with the bundle identifier, then restarts it.
#   npm run app                 debug build (fast to compile, for development)
#   npm run app -- --release    release build (the one to use every day and to measure)
#
# Signing: if the local certificate "Aede Local Signing" exists (tools/create-signing-cert.sh),
# the app is signed with it and macOS keeps the microphone and accessibility permissions from
# one build to the next. Without it the signature is ad hoc and changes at each build, so the
# permissions must be granted again every time.
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

IDENTIFIER="com.lukebanumoise.aede"
CERT_NAME="Aede Local Signing"

PROFILE="debug"
TAURI_ARGS=(--debug)
CARGO_ARGS=()
case "${1:-}" in
  "") ;;
  --release)
    PROFILE="release"
    # Same config plus the release paths of the llama.cpp libraries.
    TAURI_ARGS=(--config "$PWD/src-tauri/tauri.release.conf.json")
    CARGO_ARGS=(--release)
    ;;
  *)
    echo "Usage: npm run app [-- --release]" >&2
    exit 1
    ;;
esac

APP="src-tauri/target/$PROFILE/bundle/macos/Aède.app"

# A fresh worktree must build llama.cpp first, or the bundle step cannot find its libraries.
if [ ! -f "src-tauri/target/$PROFILE/libggml.0.dylib" ]; then
  (cd src-tauri && cargo build ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} -p llama-cpp-sys-2)
fi

npm run tauri build -- "${TAURI_ARGS[@]}" --bundles app

if security find-identity -p codesigning | grep -q "$CERT_NAME"; then
  IDENTITY="$CERT_NAME"
  PERMISSIONS_NOTE="Les permissions micro et accessibilité sont conservées (certificat local)."
else
  IDENTITY="-"
  PERMISSIONS_NOTE="Signature ad hoc : redonne les permissions micro et accessibilité si macOS les demande (voir tools/create-signing-cert.sh pour ne plus avoir à le faire)."
fi
codesign --force --deep --sign "$IDENTITY" --identifier "$IDENTIFIER" "$APP"

# Quit the running copy, or `open` would only bring the old one to the front.
pkill -x aede || true
sleep 1
# `open` does not pass the shell environment on: AEDE_FORCE_SETUP (see docs/premier-lancement.md) is given on purpose.
if [ -n "${AEDE_FORCE_SETUP:-}" ]; then
  open --env AEDE_FORCE_SETUP="$AEDE_FORCE_SETUP" "$APP"
else
  open "$APP"
fi
echo "Aède ($PROFILE) relancé. $PERMISSIONS_NOTE"
