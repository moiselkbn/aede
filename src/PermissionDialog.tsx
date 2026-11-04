import { invoke } from "@tauri-apps/api/core";
import { TriangleAlert } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import Dialog from "./Dialog";
import { APP_NAME } from "./appName";

export type PermissionsStatus = { accessibility: boolean; microphone: boolean };
export type PermissionKind = "microphone" | "accessibility";

type Props = {
  kind: PermissionKind;
  /** The user closes the dialog without allowing: an alert stays in the top bar. */
  onClose: () => void;
  onError: (message: string) => void;
};

/** A dialog asking for a macOS permission. It goes away by itself once the permission is given. */
function PermissionDialog({ kind, onClose, onError }: Props) {
  const fail = (err: unknown) => onError(String(err));
  const microphone = kind === "microphone";

  return (
    <Dialog
      title={microphone ? "Micro non autorisé" : "Accessibilité non autorisée"}
      description={
        microphone
          ? "Pour enregistrer ta voix pendant que tu maintiens le raccourci."
          : `Pour coller le texte là où se trouve ton curseur : ${APP_NAME} simule la touche Cmd+V dans l'application active.`
      }
      icon={<TriangleAlert className="dialog-icon" size={20} strokeWidth={1.75} aria-hidden="true" />}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Plus tard
          </Button>
          {microphone ? (
            <>
              <Button
                variant="secondary"
                size="sm"
                className="action-button"
                onClick={() => invoke("open_microphone_settings").catch(fail)}
              >
                Ouvrir les réglages
              </Button>
              <Button
                size="sm"
                className="accent-button"
                onClick={() => invoke("request_microphone").catch(fail)}
              >
                Autoriser
              </Button>
            </>
          ) : (
            <Button
              size="sm"
              className="accent-button"
              onClick={() => invoke("request_accessibility").catch(fail)}
            >
              Ouvrir les réglages
            </Button>
          )}
        </>
      }
    >
      <p>
        {microphone
          ? `macOS ne pose la question qu'une fois. Si tu as déjà refusé, active ${APP_NAME} dans Réglages Système, Confidentialité et sécurité, Micro.`
          : `Dans la liste, active ${APP_NAME}. Cette fenêtre se ferme toute seule.`}
      </p>
    </Dialog>
  );
}

export default PermissionDialog;
