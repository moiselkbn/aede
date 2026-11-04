import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/arc/button/button";
import Dialog from "./Dialog";
import SettingsRow from "./SettingsRow";

/** The row that puts the statistics back to zero, with the confirmation it asks for first. */
function ResetStats() {
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function close() {
    setConfirming(false);
    setError(null);
  }

  function reset() {
    invoke("reset_stats")
      .then(close)
      .catch((err) => setError(String(err)));
  }

  return (
    <>
      <SettingsRow
        title="Réinitialiser les statistiques"
        detail="Remet à zéro les compteurs de la page Statistiques. L'historique n'est pas touché."
      >
        <Button variant="secondary" size="sm" className="action-button" onClick={() => setConfirming(true)}>
          Réinitialiser
        </Button>
      </SettingsRow>
      {confirming && (
        <Dialog
          title="Réinitialiser les statistiques ?"
          description="Tous les compteurs repartent de zéro. L'historique des dictées n'est pas touché. Impossible de revenir en arrière."
          onClose={close}
          actions={
            <>
              <Button variant="secondary" size="sm" className="action-button" onClick={close}>
                Annuler
              </Button>
              <Button variant="danger" size="sm" onClick={reset}>
                Réinitialiser
              </Button>
            </>
          }
        >
          {error && <p className="dialog-error">{error}</p>}
        </Dialog>
      )}
    </>
  );
}

export default ResetStats;
