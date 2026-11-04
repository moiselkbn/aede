import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Info, Keyboard, Mic } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import { APP_NAME } from "../appName";
import type { PermissionsStatus } from "../PermissionDialog";

type MicrophoneAuthorization = "not_asked" | "refused" | "granted";

type CardProps = {
  icon: ReactNode;
  title: string;
  /** "Autorisé" for the microphone, "Autorisée" for accessibility. */
  grantedLabel: string;
  deniedLabel: string;
  text: string;
  granted: boolean;
  actionLabel: string;
  onAction: () => Promise<unknown>;
};

/** One permission: why it is needed, its state, and the button that asks for it. */
function PermissionCard({ icon, title, grantedLabel, deniedLabel, text, granted, actionLabel, onAction }: CardProps) {
  const [error, setError] = useState<string | null>(null);

  function act() {
    setError(null);
    onAction().catch((err) => setError(String(err)));
  }

  return (
    <article className="setup-card">
      <header className="setup-card-head">
        <span className="setup-card-icon">{icon}</span>
        <h2 className="setup-card-title">{title}</h2>
        <span className="setup-state" data-tone={granted ? "success" : "danger"}>
          <span className="setup-state-dot" />
          {granted ? grantedLabel : deniedLabel}
        </span>
      </header>
      <p className="setup-card-text">{text}</p>
      {error && <p className="setup-error">{error}</p>}
      {!granted && (
        <div className="setup-card-actions">
          <Button variant="secondary" size="sm" className="setup-action" onClick={act}>
            {actionLabel}
          </Button>
        </div>
      )}
    </article>
  );
}

/** Screens 10b and 10c, the two macOS permissions. The left side: the text and "Continuer". */
export function PermissionsText({
  permissions,
  onContinue,
}: {
  permissions: PermissionsStatus;
  onContinue: () => void;
}) {
  const allGranted = permissions.microphone && permissions.accessibility;
  return (
    <>
      <div className="setup-titles">
        <h1 className="setup-title">Deux autorisations pour dicter</h1>
        <p className="setup-text">
          macOS te demande ton accord une seule fois pour chacune. {APP_NAME} ne surveille jamais en
          continu ce que tu écris dans les autres applications.
        </p>
      </div>
      {!allGranted && (
        <p className="setup-note">
          <Info size={16} strokeWidth={1.75} aria-hidden="true" />
          <span>
            Si tu as déjà refusé, active {APP_NAME} dans Réglages Système, Confidentialité et
            sécurité.
          </span>
        </p>
      )}
      <div className="setup-buttons">
        <Button variant="primary" size="md" className="setup-primary" disabled={!allGranted} onClick={onContinue}>
          Continuer
        </Button>
      </div>
    </>
  );
}

/** The right side: one card per permission. */
export function PermissionsCards({
  permissions,
  onMicrophoneRefused,
}: {
  permissions: PermissionsStatus;
  /** The user refused in the macOS dialog: it will not ask again, only System Settings can help. */
  onMicrophoneRefused: () => void;
}) {
  // macOS shows its dialog only once. Not asked yet: the dialog, then we wait for the answer.
  // Refused before: no dialog will ever come, so the button opens System Settings directly.
  async function askMicrophone() {
    const state = () => invoke<MicrophoneAuthorization>("microphone_authorization");
    const first = await state();
    if (first === "granted") return;
    if (first === "refused") {
      await invoke("open_microphone_settings");
      return;
    }
    await invoke("request_microphone");
    for (let wait = 0; wait < 120; wait++) {
      await new Promise((resolve) => setTimeout(resolve, 500));
      const now = await state();
      if (now === "granted") return;
      if (now === "refused") {
        onMicrophoneRefused();
        return;
      }
    }
  }

  return (
    <>
      <PermissionCard
        icon={<Mic size={18} strokeWidth={1.75} aria-hidden="true" />}
        title="Micro"
        grantedLabel="Autorisé"
        deniedLabel="Non autorisé"
        text="Pour enregistrer ta voix pendant que tu maintiens le raccourci. L'audio reste sur ce Mac."
        granted={permissions.microphone}
        actionLabel="Autoriser"
        onAction={askMicrophone}
      />
      <PermissionCard
        icon={<Keyboard size={18} strokeWidth={1.75} aria-hidden="true" />}
        title="Accessibilité"
        grantedLabel="Autorisée"
        deniedLabel="Non autorisée"
        text={`Pour coller la transcription là où se trouve ton curseur : ${APP_NAME} simule la touche Cmd+V dans l'application active.`}
        granted={permissions.accessibility}
        actionLabel="Ouvrir les réglages"
        onAction={() => invoke("request_accessibility")}
      />
    </>
  );
}
