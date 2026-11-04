import { Info } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import { APP_NAME } from "../appName";
import type { ModelStatus } from "../Setup";
import { formatSize } from "./format";
import { OpenAILogo, QwenLogo } from "./logos";
import type { ModelInfo, Progress } from "./useModels";

type Tone = "neutral" | "success" | "warning" | "danger";

/** The role, the name and the description of each model (Figma "Carte de modèle"). Whisper is the
 *  file starting with "ggml", the other one is the rewriting model. */
function describe(model: ModelInfo) {
  return model.file_name.startsWith("ggml")
    ? {
        role: "Il écoute",
        name: "Whisper large-v3-turbo",
        text: "Il transcrit ta voix en français. Les termes anglais (deploy, dashboard,...) restent bien orthographiés.",
        icon: <OpenAILogo />,
      }
    : {
        role: "Il réécrit",
        name: "Qwen3 4B Instruct",
        text: "Il nettoie l'oral (hésitations, répétitions) et met ta demande à l'impératif, sans changer le sens.",
        icon: <QwenLogo />,
      };
}

type CardsProps = {
  models: ModelInfo[] | null;
  progress: Record<string, Progress>;
  downloading: boolean;
  /** The connection is lost and the download is trying again by itself. */
  waiting: boolean;
  downloadError: string | null;
  /** Whether each model is loaded in memory, not only on disk. */
  transcription: ModelStatus;
  rewriting: ModelStatus;
};

function stateOf(model: ModelInfo, p: Progress | undefined, props: CardsProps): { label: string; tone: Tone } {
  if (model.present) {
    const loaded = model.file_name.startsWith("ggml") ? props.transcription : props.rewriting;
    if (loaded.state === "ready") return { label: "Installé", tone: "success" };
    if (loaded.state === "failed") return { label: "Chargement impossible", tone: "danger" };
    return { label: "Chargement", tone: "neutral" };
  }
  if (props.waiting || (props.downloadError && !props.downloading)) {
    return p ? { label: "Interrompu", tone: "warning" } : { label: "En attente", tone: "neutral" };
  }
  if (p?.step === "check") return { label: "Vérification", tone: "neutral" };
  if (p) return { label: "Téléchargement", tone: "neutral" };
  return { label: "En attente", tone: "neutral" };
}

/** Screens 10d and 10e, the right side: one card per model with its state. */
export function ModelsCards(props: CardsProps) {
  return (
    <>
      {props.models?.map((model) => {
        const info = describe(model);
        const state = stateOf(model, props.progress[model.file_name], props);
        return (
          <article className="setup-card" key={model.file_name}>
            <header className="setup-card-head">
              <span className="setup-card-icon">{info.icon}</span>
              <div className="setup-card-titles">
                <span className="setup-card-role">{info.role}</span>
                <h2 className="setup-card-title">{info.name}</h2>
              </div>
            </header>
            <p className="setup-card-text">{info.text}</p>
            <footer className="setup-card-foot">
              <span className="setup-card-size">{formatSize(model.size)}</span>
              <span className="setup-state" data-tone={state.tone}>
                <span className="setup-state-dot" />
                {state.label}
              </span>
            </footer>
          </article>
        );
      })}
    </>
  );
}

type TextProps = {
  modelsError: string | null;
  transcription: ModelStatus;
  rewriting: ModelStatus;
  /** A model that is not on disk yet cannot be loaded: its "introuvable" error is expected, not shown. */
  modelsReady: boolean;
  onBack: () => void;
  onContinue: () => void;
};

/** The left side: the text, "Retour" and "Continuer". */
export function ModelsText({
  modelsError,
  transcription,
  rewriting,
  modelsReady,
  onBack,
  onContinue,
}: TextProps) {
  return (
    <>
      <div className="setup-titles">
        <h1 className="setup-title">Deux modèles à installer</h1>
        <p className="setup-text">
          {APP_NAME} ne fait rien en ligne : ces deux modèles s'installent sur ton Mac, puis tout
          tourne sans internet.
        </p>
      </div>
      <p className="setup-note">
        <Info size={16} strokeWidth={1.75} aria-hidden="true" />
        <span>
          Tu peux fermer la fenêtre, le téléchargement continue. Quitter {APP_NAME} l'interrompt : il
          reprendra là où il s'était arrêté.
        </span>
      </p>
      {modelsError && <p className="setup-error">{modelsError}</p>}
      {modelsReady && transcription.error && <p className="setup-error">{transcription.error}</p>}
      {modelsReady && rewriting.error && <p className="setup-error">{rewriting.error}</p>}
      <div className="setup-buttons">
        <Button variant="ghost" size="md" onClick={onBack}>
          Retour
        </Button>
        <Button variant="primary" size="md" className="setup-primary" onClick={onContinue}>
          Continuer
        </Button>
      </div>
    </>
  );
}
