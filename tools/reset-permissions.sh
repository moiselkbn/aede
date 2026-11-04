#!/bin/bash
# Takes the microphone and accessibility permissions away from Aède, as on a first launch.
# Run it with Aède closed: `bash tools/reset-permissions.sh`, then start the packaged app
# (`npm run app`) with AEDE_FORCE_SETUP=1 if the models are already installed.
#
# An app cannot revoke its own permissions: macOS only lets `tccutil` do it from outside.
# It works on the packaged app (identifier com.lukebanumoise.aede). With `npm run tauri dev`,
# macOS attaches the permissions to the terminal, so this script has no effect there.
set -euo pipefail

BUNDLE_ID="com.lukebanumoise.aede"

pkill -x aede 2>/dev/null || true
/usr/bin/tccutil reset Microphone "$BUNDLE_ID"
/usr/bin/tccutil reset Accessibility "$BUNDLE_ID"
echo "Permissions removed. Start Aède again."
