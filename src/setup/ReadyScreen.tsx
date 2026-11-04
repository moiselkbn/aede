import { Check, LoaderCircle } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import { APP_NAME } from "../appName";
import { formatSize } from "./format";

type TextProps = {
  /** The models are installed and loaded: Aède can dictate. */
  canFinish: boolean;
  onBack: () => void;
  onDone: () => void;
};

/** Screen 10m, the left side: the text, "Retour" and "Ouvrir Aède". */
export function ReadyText({ canFinish, onBack, onDone }: TextProps) {
  return (
    <>
      <div className="setup-titles">
        <h1 className="setup-title">{APP_NAME} est prêt</h1>
        <p className="setup-text">
          Tu peux dicter dans n'importe quelle application : maintiens le raccourci, parle, relâche,
          et le texte propre est collé là où se trouve ton curseur.
        </p>
      </div>
      <div className="setup-buttons">
        <Button variant="ghost" size="md" onClick={onBack}>
          Retour
        </Button>
        <Button variant="primary" size="md" className="setup-primary" disabled={!canFinish} onClick={onDone}>
          Ouvrir {APP_NAME}
        </Button>
      </div>
    </>
  );
}

type CardProps = {
  /** Size of the two models on disk, in bytes (null while it is not known). */
  modelsSize: number | null;
  /** The models are installed and loaded. */
  modelsReady: boolean;
  shortcut: string | null;
  learnShortcut: string | null;
};

/** "Ctrl+Option+Espace" as "Ctrl, Option, Espace". */
const spoken = (label: string | null) => (label ? label.split("+").join(", ") : "pas encore choisi");

/** One line of the recap: a check when it is done, a spinner while it is not. */
function RecapCard({ done, title, text }: { done: boolean; title: string; text: string }) {
  return (
    <article className="setup-recap">
      <span className="setup-recap-icon" data-done={done || undefined}>
        {done ? (
          <Check size={16} strokeWidth={1.75} aria-hidden="true" />
        ) : (
          <LoaderCircle size={16} strokeWidth={1.75} aria-hidden="true" />
        )}
      </span>
      <div className="setup-recap-text">
        <h2 className="setup-recap-title">{title}</h2>
        <p className="setup-card-text">{text}</p>
      </div>
    </article>
  );
}

/** Screen 10m, the right side: what is set up. The permissions are there by then: the first
 *  screen does not let the user leave without them. */
export function ReadyCards({ modelsSize, modelsReady, shortcut, learnShortcut }: CardProps) {
  return (
    <>
      <RecapCard done title="Autorisations accordées" text="Micro et accessibilité." />
      <RecapCard
        done={modelsReady}
        title={modelsReady ? "Modèles installés" : "Modèles en préparation"}
        text={
          modelsReady
            ? `Whisper et Qwen, ${modelsSize ? formatSize(modelsSize) : "4,1 Go"}. Ils tournent sans internet.`
            : `${APP_NAME} s'ouvrira dès qu'ils seront prêts.`
        }
      />
      <RecapCard
        done
        title="Raccourcis"
        text={`Dicter : ${spoken(shortcut)}. Apprendre un mot : ${spoken(learnShortcut)}.`}
      />
    </>
  );
}
