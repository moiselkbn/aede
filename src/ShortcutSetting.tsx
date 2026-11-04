import { Button } from "@/components/arc/button/button";
import Keys from "./Keys";
import SettingsRow from "./SettingsRow";
import { useShortcutCapture, type ShortcutCommand, type ShortcutStatus } from "./useShortcutCapture";

export type { ShortcutStatus };

type Props = {
  title: string;
  description: string;
  /** The Rust command that saves the new shortcut. */
  command: ShortcutCommand;
  status: ShortcutStatus | null;
  onChange: (status: ShortcutStatus) => void;
};

/** A global shortcut row: its keys, and a button to press a new combination. */
function ShortcutSetting({ title, description, command, status, onChange }: Props) {
  const { capturing, error, start, cancel } = useShortcutCapture(command, onChange);

  return (
    <SettingsRow
      title={title}
      detail={
        capturing
          ? "Au moins une touche Ctrl, Option, Maj ou Cmd avec une autre touche, ou une touche F13 à F24 seule. Si rien ne se passe, la combinaison est déjà prise par macOS ou par une autre application : choisis-en une autre."
          : description
      }
      error={[error, status?.error].filter(Boolean).join(" ") || null}
    >
      {capturing ? (
        <>
          <span className="settings-row-detail">Appuie sur la combinaison…</span>
          <Button variant="secondary" size="sm" className="action-button" onClick={cancel}>
            Annuler
          </Button>
        </>
      ) : (
        <>
          {status && <Keys label={status.label} />}
          <Button variant="secondary" size="sm" className="action-button" onClick={start}>
            Modifier
          </Button>
        </>
      )}
    </SettingsRow>
  );
}

export default ShortcutSetting;
