import Alert from "./Alert";
import type { AlertItem } from "./Alert";
import Keys from "./Keys";
import type { ShortcutStatus } from "./ShortcutSetting";

export type Tone = "success" | "danger" | "neutral";
export type StatusLine = { label: string; tone: Tone };
export type TopBarProps = {
  status: StatusLine;
  alerts: AlertItem[];
  shortcut: ShortcutStatus | null;
  onShortcutChange: (status: ShortcutStatus) => void;
  learnShortcut: ShortcutStatus | null;
};

/** Always visible: dictation state, the two shortcuts, and what needs the user's attention. */
function TopBar({ status, alerts, shortcut, learnShortcut }: TopBarProps) {
  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="status-row" data-tauri-drag-region>
        <span className="status" data-tone={status.tone}>
          <span className="status-dot" />
          {status.label}
        </span>
        <div className="shortcuts">
          <span className="shortcut">
            <span className="shortcut-name">Dicter</span>
            {shortcut && <Keys label={shortcut.label} />}
          </span>
          <span className="shortcut">
            <span className="shortcut-name">Apprendre un mot</span>
            {learnShortcut && <Keys label={learnShortcut.label} />}
          </span>
        </div>
      </div>
      {alerts.map(({ key, ...alert }) => (
        <Alert key={key} {...alert} />
      ))}
    </header>
  );
}

export default TopBar;
