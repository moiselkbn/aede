import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/arc/button/button";
import SettingsRow from "./SettingsRow";
import { APP_NAME } from "./appName";

/** As sent by src-tauri/src/login_item.rs. */
type LoginItemStatus = "enabled" | "disabled" | "requires_approval";

/** The "open at login" row: a macOS switch, kept in sync with System Settings. */
function LaunchAtLogin() {
  const [status, setStatus] = useState<LoginItemStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const refresh = () => invoke<LoginItemStatus>("get_launch_at_login").then(setStatus);
    refresh();
    // The user can change it in System Settings: read it again when the window comes back.
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);

  // Waiting for approval, or turned off in System Settings: either way Aède will not open.
  const on = status === "enabled";
  const needsApproval = status === "requires_approval";

  function toggle() {
    setError(null);
    // macOS takes about a second to answer: move the switch now, then show what it says.
    setStatus(on ? "disabled" : "enabled");
    invoke<LoginItemStatus>("set_launch_at_login", { enabled: !on })
      .then(setStatus)
      .catch((err) => {
        setError(String(err));
        invoke<LoginItemStatus>("get_launch_at_login").then(setStatus);
      });
  }

  return (
    <SettingsRow
      title="Ouvrir au démarrage du Mac"
      detail={
        needsApproval
          ? `Désactivée dans Réglages Système. Pour l'allumer, autorise ${APP_NAME} dans Général, Ouverture.`
          : "Sans l'app ouverte, le raccourci ne fonctionne pas."
      }
      error={error}
    >
      {needsApproval && (
        <Button
          variant="secondary"
          size="sm"
          className="action-button"
          onClick={() => invoke("open_login_items_settings")}
        >
          Ouvrir les réglages
        </Button>
      )}
      <button
        type="button"
        className="switch"
        role="switch"
        aria-checked={on}
        aria-label="Ouvrir au démarrage du Mac"
        disabled={status === null}
        onClick={toggle}
      />
    </SettingsRow>
  );
}

export default LaunchAtLogin;
