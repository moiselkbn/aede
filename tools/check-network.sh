#!/bin/bash
# Checks that the running Aède opens no network connection. Run it, then use the app as usual
# (a few dictations) until it ends:
#   bash tools/check-network.sh [seconds]      (60 seconds by default)
#
# It lists the network sockets (TCP and UDP, listening or connected) of the aede process and of
# its children (the curl of the model download would be one) about five times a second. Each
# new socket found is printed with the time, and the script ends with code 1.
#
# Limits: a connection shorter than a fifth of a second can slip between two samples, and the
# web views of macOS run in separate system processes that this script does not follow (the
# pages of Aède are local, see docs/verification-reseau.md).
set -uo pipefail

DURATION="${1:-60}"
HISTORY="$HOME/Library/Application Support/com.lukebanumoise.aede/history.jsonl"

PID="$(pgrep -x aede | head -1)"
if [ -z "$PID" ]; then
  echo "Aède ne tourne pas : lance-le d'abord, puis relance ce script." >&2
  exit 2
fi

count_dictations() {
  if [ -f "$HISTORY" ]; then wc -l < "$HISTORY" | tr -d ' '; else echo 0; fi
}

dictations_before="$(count_dictations)"
echo "Surveillance du processus aede (pid $PID) pendant $DURATION s. Fais quelques dictées maintenant."

samples=0
found=0
last=""
end=$((SECONDS + DURATION))
while [ "$SECONDS" -lt "$end" ]; do
  # The process and its children, as a comma list for lsof.
  pids="$PID"
  for child in $(pgrep -P "$PID"); do pids="$pids,$child"; done
  # -i: network sockets only; -a: and only those of the listed processes.
  sockets="$(lsof -nP -i -a -p "$pids" 2>/dev/null | tail -n +2)"
  samples=$((samples + 1))
  if [ -n "$sockets" ]; then
    found=$((found + 1))
    # The same sockets are not printed again at every sample.
    if [ "$sockets" != "$last" ]; then
      echo "[$(date +%T)] connexion trouvée :"
      echo "$sockets"
      last="$sockets"
    fi
  fi
  sleep 0.1
done

dictations=$(($(count_dictations) - dictations_before))
echo "---"
echo "$samples échantillons, $found avec une connexion réseau, $dictations dictée(s) enregistrée(s) dans l'historique pendant la mesure."
if [ "$found" -gt 0 ]; then
  echo "ÉCHEC : Aède a ouvert au moins une connexion réseau."
  exit 1
fi
if [ "$dictations" -lt 1 ]; then
  echo "Aucune connexion, mais aucune dictée n'a eu lieu pendant la mesure : elle ne prouve rien. Recommence en dictant."
  exit 3
fi
echo "OK : aucune connexion réseau pendant $dictations dictée(s)."
