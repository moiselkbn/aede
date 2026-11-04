import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HoldToConfirm } from "@/components/arc/hold-to-confirm/hold-to-confirm";
import AppearanceSettings from "./AppearanceSettings";
import KeepRecordings from "./KeepRecordings";
import LaunchAtLogin from "./LaunchAtLogin";
import PermissionRows from "./PermissionRows";
import ResetStats from "./ResetStats";
import type { PermissionsStatus } from "./PermissionDialog";
import SettingsRow, { SettingsSection } from "./SettingsRow";
import ShortcutSetting from "./ShortcutSetting";
import type { ShortcutStatus } from "./ShortcutSetting";

type Props = {
  shortcut: ShortcutStatus | null;
  onShortcutChange: (status: ShortcutStatus) => void;
  learnShortcut: ShortcutStatus | null;
  onLearnShortcutChange: (status: ShortcutStatus) => void;
  permissions: PermissionsStatus | null;
};

/** The settings page (Figma screen 06): open at login, shortcuts, permissions, data. */
function SettingsPage({
  shortcut,
  onShortcutChange,
  learnShortcut,
  onLearnShortcutChange,
  permissions,
}: Props) {
  const [erased, setErased] = useState(false);
  const [eraseError, setEraseError] = useState<string | null>(null);

  /** Called once the button has been held long enough. A failure puts the button back to rest and says why. */
  function eraseAll() {
    setErased(true);
    setEraseError(null);
    invoke("clear_history").catch((err) => {
      setErased(false);
      setEraseError(String(err));
    });
  }

  return (
    <>
      <h1 className="page-title">Réglages</h1>
      <SettingsSection>
        <LaunchAtLogin />
      </SettingsSection>
      <SettingsSection title="Raccourcis">
        <ShortcutSetting
          title="Dicter"
          description="Maintiens pour dicter, relâche pour coller."
          command="set_shortcut"
          status={shortcut}
          onChange={onShortcutChange}
        />
        <ShortcutSetting
          title="Apprendre un mot"
          description="Corrige un mot dans le texte collé, puis appuie."
          command="set_learn_shortcut"
          status={learnShortcut}
          onChange={onLearnShortcutChange}
        />
      </SettingsSection>
      <AppearanceSettings />
      <SettingsSection title="Autorisations">
        <PermissionRows permissions={permissions} />
      </SettingsSection>
      <SettingsSection title="Données">
        <KeepRecordings />
        <ResetStats />
        <SettingsRow
          title="Effacer les données"
          detail="Efface les dictées, les audios, les notes et le dictionnaire."
          error={eraseError}
        >
          <HoldToConfirm
            tone="danger"
            label="Maintenir pour effacer"
            confirmedLabel="Données effacées"
            className="hold-button"
            duration={1500}
            confirmed={erased}
            onConfirm={eraseAll}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}

export default SettingsPage;
