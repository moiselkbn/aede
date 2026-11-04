import { RefObject, useEffect, useState } from "react";
import { Check, Keyboard, Mic } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import Keys from "../Keys";
import NotchWave from "./NotchWave";
import RewriteText from "./RewriteText";
import WelcomeDemo from "./WelcomeDemo";
import type { DemoPhase, DictationDemo } from "./useDictationDemo";

type Tone = "neutral" | "success" | "danger" | "warning";

/** The status line under the field. The wording of the steps is ours, not the Figma's. */
function statusOf(demo: DictationDemo): { label: string; tone: Tone } {
  const labels: Record<DemoPhase, { label: string; tone: Tone }> = {
    idle: { label: "En attente de ta dictée", tone: "neutral" },
    recording: { label: "Dictée en cours", tone: "neutral" },
    transcribing: { label: "Transcription en cours", tone: "neutral" },
    rewriting: { label: "Réécriture en cours", tone: "neutral" },
    // The text is pasted in a fraction of a second, then shown being rewritten: one line for both.
    pasting: { label: "Réécriture en cours", tone: "neutral" },
    revealing: { label: "Réécriture en cours", tone: "neutral" },
    done: { label: "Texte collé", tone: "success" },
    missed: {
      label: "Le texte n'est pas arrivé ici. Clique dans le champ, puis recommence.",
      tone: "warning",
    },
    failed: { label: demo.error ?? "La dictée a échoué.", tone: "danger" },
  };
  return labels[demo.phase];
}

/** Top and bottom padding of the field (setup.css: space-3 twice). */
export const FIELD_PADDING = 24;

/** The height a field needs for its own text, padding included. */
function heightOfText(field: HTMLTextAreaElement): number {
  const { height, transition } = field.style;
  // Squeezed for a moment, without animation, so that its scroll height is the one of its text.
  field.style.transition = "none";
  field.style.height = "0px";
  const needed = field.scrollHeight;
  field.style.height = height;
  field.getBoundingClientRect();
  field.style.transition = transition;
  return needed;
}

type CardsProps = {
  shortcutLabel: string | null;
  /** The models are not ready yet: the dictation cannot work, an animation takes the place of the field. */
  locked: boolean;
  demo: DictationDemo;
  field: RefObject<HTMLTextAreaElement | null>;
};

/** Screens 10j, 10j bis and 10j ter, the right side: the three steps, then the notch with the
 *  wave and the field where the text is pasted. */
export function DemoCards({ shortcutLabel, locked, demo, field }: CardsProps) {
  const status = statusOf(demo);
  const covered =
    demo.raw !== null &&
    (demo.phase === "rewriting" || demo.phase === "pasting" || demo.phase === "revealing");
  // The pill of a real dictation shows the bump from the release of the key to the end.
  const scanning = demo.phase === "transcribing" || covered;
  const busy = demo.phase !== "idle" && demo.phase !== "done" && demo.phase !== "missed" && demo.phase !== "failed";
  // The field is one line high (the Figma) and follows its text: the one drawn over it while it
  // is rewritten, then its own. Null is the height of one line, for a new try.
  const [height, setHeight] = useState<number | null>(null);
  const fitToText = () => {
    if (field.current) setHeight(heightOfText(field.current));
  };
  useEffect(() => {
    if (demo.phase === "recording") setHeight(null);
    else if (!busy) fitToText();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [demo.phase]);
  return (
    <section className="setup-demo">
      <ol className="setup-demo-steps">
        <li>
          <span className="setup-demo-icon">
            <Keyboard size={16} strokeWidth={1.75} aria-hidden="true" />
          </span>
          Maintiens
          {shortcutLabel && <Keys label={shortcutLabel} />}
        </li>
        <li>
          <span className="setup-demo-icon">
            <Mic size={16} strokeWidth={1.75} aria-hidden="true" />
          </span>
          {locked ? "Parle en français" : "Dicte la phrase d'exemple"}
        </li>
        <li>
          <span className="setup-demo-icon">
            <Check size={16} strokeWidth={1.75} aria-hidden="true" />
          </span>
          {locked ? "Relâche, le texte est collé" : "Relâche, le texte est nettoyé et collé"}
        </li>
      </ol>
      {locked ? (
        // No trial without the models: the animation of the welcome screen shows what it will do.
        <WelcomeDemo />
      ) : (
        <div className="setup-mic">
          <NotchWave volume={demo.volume} scanning={scanning} />
          <div className="setup-demo-field">
            <textarea
              ref={field}
              rows={1}
              autoFocus
              aria-label="Champ d'essai"
              placeholder="Clique ici, puis maintiens le raccourci."
              data-busy={busy || undefined}
              style={height === null ? undefined : { height }}
              onInput={fitToText}
            />
            {/* What Whisper heard, then its rewriting: the real text is under it until the end. */}
            {covered && demo.raw && (
              <RewriteText
                raw={demo.raw}
                result={demo.result}
                onHeight={(text) => setHeight(text + FIELD_PADDING)}
                onDone={demo.finish}
              />
            )}
          </div>
          <span className="setup-state" data-tone={status.tone} role="status">
            <span className="setup-state-dot" />
            {status.label}
          </span>
        </div>
      )}
    </section>
  );
}

type TextProps = {
  locked: boolean;
  onBack: () => void;
  onContinue: () => void;
};

/** The left side: the text, the sentence to say, "Retour" and "Continuer" (or "Passer pour
 *  l'instant" while the models are not ready). */
export function DemoText({ locked, onBack, onContinue }: TextProps) {
  return (
    <>
      <div className="setup-titles">
        <h1 className="setup-title">Essaie la dictée</h1>
        <p className="setup-text">
          {locked
            ? "Les modèles se téléchargent encore : voici ce que fera la dictée. Tu pourras l'essayer dès qu'ils seront prêts."
            : "Clique dans le champ, maintiens le raccourci, dis une phrase et relâche. Le texte apparaît là où se trouve ton curseur."}
        </p>
      </div>
      {!locked && (
        <div className="setup-example">
          <span className="setup-example-label">Essaie, par exemple</span>
          <p>
            « Alors euh, j'ai eu Marc au téléphone ce matin, et du coup euh, son feedback sur le
            meeting est plutôt positif. »
          </p>
        </div>
      )}
      <div className="setup-buttons">
        <Button variant="ghost" size="md" onClick={onBack}>
          Retour
        </Button>
        <Button
          variant={locked ? "secondary" : "primary"}
          size="md"
          className={locked ? undefined : "setup-primary"}
          onClick={onContinue}
        >
          {locked ? "Passer pour l'instant" : "Continuer"}
        </Button>
      </div>
    </>
  );
}
