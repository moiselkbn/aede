#!/bin/bash
# Puts the two shortcuts of Aède back to their defaults, as on a first launch (Option+Space to
# dictate, Option+Shift+Space to learn a word). Run it with Aède closed: `bash tools/reset-shortcuts.sh`.
#
# The shortcuts chosen in earlier tests stay in settings.json, and the first launch screens show
# them: a real first launch has no such file. Only these two settings are removed (the microphone
# is kept), after a copy to settings.json.bak.
set -euo pipefail

FILE="$HOME/Library/Application Support/com.lukebanumoise.aede/settings.json"

pkill -x aede 2>/dev/null || true
if [ ! -f "$FILE" ]; then
  echo "No settings file: the defaults already apply."
  exit 0
fi
cp "$FILE" "$FILE.bak"
/usr/bin/python3 - "$FILE" <<'PY'
import json, sys
path = sys.argv[1]
with open(path) as f:
    settings = json.load(f)
for key in ("shortcut", "learn_shortcut"):
    settings.pop(key, None)
with open(path, "w") as f:
    json.dump(settings, f, indent=2)
    f.write("\n")
PY
echo "Shortcuts back to their defaults (copy in settings.json.bak). Start Aède again."
