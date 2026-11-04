import { APP_NAME } from "../appName";
import { Button } from "@/components/arc/button/button";
import { Progress } from "@/components/arc/progress/progress";
import { SHORTCUT_HOLD_MS, SHORTCUT_MAX_KEYS } from "../useShortcutCapture";
import { heldKeycaps, keycaps } from "./keycaps";

// The hold in seconds, written the French way (1,5).
const seconds = String(SHORTCUT_HOLD_MS / 1000).replace(".", ",");

type CardsProps = {
  label: string | null;
  /** The keys the window saw going down (`KeyboardEvent.code`). */
  down: ReadonlySet<string>;
  /** The whole shortcut is held down right now: its last key goes down with it. */
  pressed: boolean;
  /** It was held down at least once on this screen. */
  recognized: boolean;
  /** The window waits for a new combination. */
  capturing: boolean;
  /** How far the hold of a new combination is, from 0 to 1. */
  holdProgress: number;
  error: string | null;
  onModify: () => void;
  onCancel: () => void;
};

/** Screens 10h and 10i, the right side: the keys of the shortcut, what was heard, and the
 *  button to choose another combination. While a new combination is awaited (10h bis), an empty
 *  slot takes the place of the keys. */
export function ShortcutCards({
  label,
  down,
  pressed,
  recognized,
  capturing,
  holdProgress,
  error,
  onModify,
  onCancel,
}: CardsProps) {
  const caps = label ? keycaps(label) : [];
  if (capturing) {
    // The keys held while a combination is awaited show in the slot, as they go down.
    const held = heldKeycaps(down);
    return (
      <section className="setup-shortcut">
        <div className="setup-keys">
          {held.length === 0 && <div className="setup-keys-slot" />}
          {held.map((cap, index) => (
            <kbd className="setup-keycap" data-kind={cap.kind} data-down key={index}>
              {cap.name && <span className="setup-keycap-name">{cap.name}</span>}
              <span className="setup-keycap-glyph">{cap.glyph}</span>
            </kbd>
          ))}
        </div>
        <div className="setup-hold">
          <Progress
            value={holdProgress * 100}
            label={
              holdProgress >= 1
                ? "Combinaison gardée"
                : holdProgress > 0
                  ? "Continue de maintenir"
                  : `Maintiens la combinaison ${seconds} secondes`
            }
          />
        </div>
        <span className="setup-state" data-tone="neutral" role="status">
          <span className="setup-state-dot" />
          En attente de ta combinaison
        </span>
        <p className="setup-hint" data-muted>
          {held.filter((cap) => cap.kind !== "modifier").length > 1
            ? "Une seule touche autre que Ctrl, Option, Maj et Cmd : relâche-en une."
            : held.length > SHORTCUT_MAX_KEYS
              ? "Trois touches au maximum : relâche-en une."
              : "Deux ou trois touches, dont au moins une touche Ctrl, Option, Maj ou Cmd."}
        </p>
        {error && <p className="setup-error setup-shortcut-error">{error}</p>}
        <Button variant="secondary" size="sm" onClick={onCancel}>
          Annuler
        </Button>
      </section>
    );
  }
  return (
    <section className="setup-shortcut">
      <div className="setup-keys">
        {caps.map((cap, index) => {
          // Each key goes down and up on its own. The last one is the one macOS keeps for the shortcut.
          const isDown = cap.codes.some((code) => down.has(code)) || (pressed && index === caps.length - 1);
          return (
            <kbd className="setup-keycap" data-kind={cap.kind} data-down={isDown || undefined} key={index}>
              {cap.name && <span className="setup-keycap-name">{cap.name}</span>}
              <span className="setup-keycap-glyph">{cap.glyph}</span>
            </kbd>
          );
        })}
      </div>
      {recognized ? (
        <span className="setup-state" data-tone="success" role="status">
          <span className="setup-state-dot" />
          Raccourci reconnu
        </span>
      ) : (
        <p className="setup-hint">Appuie sur les touches pour les essayer</p>
      )}
      {error && <p className="setup-error setup-shortcut-error">{error}</p>}
      <Button variant="secondary" size="sm" onClick={onModify}>
        Modifier le raccourci
      </Button>
    </section>
  );
}

/** The two shortcuts of the first launch share the screen: only the words change. */
export type ShortcutKind = "dictation" | "learn";

const TEXTS = {
  dictation: {
    title: "Ton raccourci pour dicter",
    choose: "Choisis ton raccourci pour dicter",
    text: "Maintiens-le pour parler, relâche-le pour coller le texte. Tu pourras le changer plus tard dans Réglages.",
  },
  learn: {
    title: "Ton raccourci d'apprentissage",
    choose: "Choisis ton raccourci d'apprentissage",
    text: `Quand ${APP_NAME} entend mal un mot, corrige-le, sélectionne-le et appuie sur ce raccourci. ${APP_NAME} le retient pour la suite.`,
  },
} as const;

type TextProps = {
  kind?: ShortcutKind;
  /** A new combination is awaited (10h bis): "Retour" gives up the change, "Continuer" waits. */
  capturing: boolean;
  /** "Continuer" waits until the shortcut was seen working. */
  canContinue: boolean;
  onBack: () => void;
  onContinue: () => void;
};

/** The left side: the text, "Retour" and "Continuer". */
export function ShortcutText({ kind = "dictation", capturing, canContinue, onBack, onContinue }: TextProps) {
  const texts = TEXTS[kind];
  return (
    <>
      <div className="setup-titles">
        <h1 className="setup-title">
          {capturing ? texts.choose : texts.title}
        </h1>
        <p className="setup-text">
          {capturing
            ? `Maintiens la combinaison que tu veux utiliser (deux ou trois touches) pendant ${seconds} secondes. Si tu la relâches avant, elle n'est pas gardée. Tu peux annuler à tout moment.`
            : texts.text}
        </p>
      </div>
      <div className="setup-buttons">
        <Button variant="ghost" size="md" onClick={onBack}>
          Retour
        </Button>
        <Button variant="primary" size="md" className="setup-primary" disabled={capturing || !canContinue} onClick={onContinue}>
          Continuer
        </Button>
      </div>
    </>
  );
}
