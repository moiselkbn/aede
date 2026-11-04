#!/bin/bash
# Makes the models folder of Aède point to a small disk (300 MB), so that the download of the models
# runs out of space: the screen must then say "Plus assez de place sur le disque" with "Reprendre".
#
#   bash tools/test-disk-full.sh start   (Aède closed), then start Aède and click "Commencer"
#   bash tools/test-disk-full.sh stop    (Aède closed): puts everything back as it was
#
# Your real models folder is only renamed to "models.real" while the test runs, never deleted.
# DATA_DIR can point to another data folder (used to try this script out).
set -euo pipefail

DATA_DIR="${DATA_DIR:-$HOME/Library/Application Support/com.lukebanumoise.aede}"
MODELS="$DATA_DIR/models"
SAVED="$DATA_DIR/models.real"
IMAGE="${TMPDIR:-/tmp}/aede-full-disk.dmg"
VOLUME="aede-full"

case "${1:-}" in
  start)
    pkill -x aede 2>/dev/null || true
    if [ -e "$SAVED" ]; then
      echo "$SAVED already exists: run 'stop' first." >&2
      exit 1
    fi
    if [ -L "$MODELS" ]; then
      echo "$MODELS is already a link: run 'stop' first." >&2
      exit 1
    fi
    mkdir -p "$DATA_DIR"
    [ -d "$MODELS" ] && mv "$MODELS" "$SAVED"
    /usr/bin/hdiutil create -size 300m -fs APFS -volname "$VOLUME" -ov "$IMAGE" >/dev/null 2>&1
    /usr/bin/hdiutil attach "$IMAGE" -nobrowse >/dev/null 2>&1
    ln -s "/Volumes/$VOLUME" "$MODELS"
    echo "Done: models -> /Volumes/$VOLUME (300 MB). Start Aède and click Commencer."
    ;;
  stop)
    pkill -x aede 2>/dev/null || true
    [ -L "$MODELS" ] && rm "$MODELS"
    /usr/bin/hdiutil detach "/Volumes/$VOLUME" >/dev/null 2>&1 || true
    rm -f "$IMAGE"
    [ -d "$SAVED" ] && mv "$SAVED" "$MODELS"
    echo "Back to normal."
    ;;
  *)
    echo "Usage: bash tools/test-disk-full.sh start|stop" >&2
    exit 1
    ;;
esac
