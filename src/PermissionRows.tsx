import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/arc/button/button";
import type { PermissionsStatus } from "./PermissionDialog";
import SettingsRow from "./SettingsRow";

type RowProps = {
  title: string;
  /** Undefined until the first answer of the permission check. */
  granted: boolean | undefined;
  /** The two words agree with the permission: "Autorisé" for the microphone, "Autorisée" for accessibility. */
  grantedLabel: string;
  deniedLabel: string;
  /** Opens the place where the user can grant it. */
  open: () => Promise<unknown>;
};

/** One permission: its state with a colored dot, and a way to fix it when it is missing. */
function PermissionRow({ title, granted, grantedLabel, deniedLabel, open }: RowProps) {
  const [error, setError] = useState<string | null>(null);

  function fix() {
    setError(null);
    open().catch((err) => setError(String(err)));
  }

  return (
    <SettingsRow title={title} error={error}>
      {granted === false && (
        <Button variant="secondary" size="sm" className="action-button" onClick={fix}>
          Ouvrir les réglages
        </Button>
      )}
      {granted !== undefined && (
        <span className="status" data-tone={granted ? "success" : "danger"}>
          <span className="status-dot" />
          {granted ? grantedLabel : deniedLabel}
        </span>
      )}
    </SettingsRow>
  );
}

/** The microphone and accessibility rows of the settings page. */
function PermissionRows({ permissions }: { permissions: PermissionsStatus | null }) {
  return (
    <>
      <PermissionRow
        title="Micro"
        granted={permissions?.microphone}
        grantedLabel="Autorisé"
        deniedLabel="Non autorisé"
        open={() => invoke("open_microphone_settings")}
      />
      <PermissionRow
        title="Accessibilité"
        granted={permissions?.accessibility}
        grantedLabel="Autorisée"
        deniedLabel="Non autorisée"
        open={() => invoke("request_accessibility")}
      />
    </>
  );
}

export default PermissionRows;
