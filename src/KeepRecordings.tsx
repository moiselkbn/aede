import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import SettingsRow from "./SettingsRow";

/** The "keep the audio" row: off by default, the test week turns it on. */
function KeepRecordings() {
  const [keep, setKeep] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<boolean>("get_keep_recordings")
      .then(setKeep)
      .catch((err) => setError(String(err)));
  }, []);

  function toggle() {
    if (keep === null) return;
    const next = !keep;
    setError(null);
    // The switch moves at once; a failure puts it back and says why.
    setKeep(next);
    invoke("set_keep_recordings", { keep: next }).catch((err) => {
      setKeep(!next);
      setError(String(err));
    });
  }

  return (
    <SettingsRow
      title="Garder l'audio des dictées"
      detail="Garde l'enregistrement de chaque dictée sur ce Mac, pour le réécouter et signaler un problème. Désactivé, rien n'est gardé."
      error={error}
    >
      <button
        type="button"
        className="switch"
        role="switch"
        aria-checked={keep === true}
        aria-label="Garder l'audio des dictées"
        disabled={keep === null}
        onClick={toggle}
      />
    </SettingsRow>
  );
}

export default KeepRecordings;
