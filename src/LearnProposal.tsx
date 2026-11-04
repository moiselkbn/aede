// What the pill shows for the learning shortcut (the pill itself is Pill.tsx): a proposal, with
// one line per correction ("misheard → wanted") and a frame that says how to confirm or cancel,
// then the end of an answer (a dot and a short text). A bar at the bottom empties while the
// proposal waits. It is only a picture: the window lets the clicks through, and the answer is
// the second press of the shortcut (see learn.rs).

import type { Notch } from "./Pill";
import "./learn-proposal.css";

export type Proposal = {
  pairs: [string, string][];
  // The shortcut to press again, e.g. "Option+Maj+Espace".
  shortcut: string;
  seconds: number;
  // Set by the page, to restart the bar for each new proposal.
  serial?: number;
};

// What the learning shortcut shows: a proposal waiting for the second press, or the end of an
// answer.
export type View =
  { kind: "proposal"; proposal: Proposal } | { kind: "outcome"; ok: boolean; text: string };

const SYMBOLS: Record<string, string> = {
  Ctrl: "⌃",
  Option: "⌥",
  Maj: "⇧",
  Shift: "⇧",
  Cmd: "⌘",
  Command: "⌘",
};

// "Ctrl+Option+A" becomes "⌃⌥A", as on a Mac keyboard.
export function keysOf(shortcut: string): string {
  return shortcut
    .split("+")
    .map((key) => SYMBOLS[key] ?? key)
    .join("");
}

export function LearnContent({ view, notch }: { view: View; notch: Notch }) {
  const proposal = view.kind === "proposal" ? view.proposal : null;
  return (
    <div
      className={proposal ? "learn-content" : "learn-content learn-content-outcome"}
      style={{ paddingTop: notch.height }}
    >
      {proposal ? (
        <div className="learn-fade" key={proposal.serial}>
          <div className="learn-pairs">
            {proposal.pairs.map(([wrong, right]) => (
              <div className="learn-pair" key={`${wrong}→${right}`}>
                <span className="learn-wrong">{wrong}</span>
                <span className="learn-wrong">→</span>
                <span className="learn-right">{right}</span>
              </div>
            ))}
          </div>
          <div className="learn-keys">
            <span className="learn-key">{keysOf(proposal.shortcut)}</span>
            <span className="learn-label">Valider</span>
            <span className="learn-separator" />
            <span className="learn-key">Échap</span>
            <span className="learn-label">Annuler</span>
          </div>
        </div>
      ) : (
        <div className="learn-fade learn-outcome" key={view.kind === "outcome" ? view.text : ""}>
          {/* No text: an empty pill (the one of the start of the app). */}
          {view.kind === "outcome" && view.text && (
            <>
              <span className={view.ok ? "pill-dot pill-ok" : "pill-dot pill-fail"} />
              <span>{view.text}</span>
            </>
          )}
        </div>
      )}
    </div>
  );
}

// The time left for a proposal: a bar on the bottom edge of the pill, from full to empty.
export function LearnTrack({ proposal }: { proposal: Proposal }) {
  return (
    <div className="learn-track">
      <div
        className="learn-bar"
        key={proposal.serial}
        style={{ animationDuration: `${proposal.seconds}s` }}
      />
    </div>
  );
}
