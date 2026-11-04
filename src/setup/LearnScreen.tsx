import { Button } from "@/components/arc/button/button";
import { APP_NAME } from "../appName";
import Keys from "../Keys";
import { LearnContent, type View } from "../LearnProposal";
import Pill, { type Notch } from "../Pill";
import { PROPOSAL_OVERSHOOT } from "../motion";
import { HEARD, type LearnDemo, type LearnPhase, WORD } from "./useLearnDemo";

type Tone = "neutral" | "success" | "danger" | "warning";

/** The status line under the field. The wording is ours, not the Figma's (to confirm). */
function statusOf(demo: LearnDemo): { label: string; tone: Tone } {
  const labels: Record<LearnPhase, { label: string; tone: Tone }> = {
    // Before the click, the field is what Aède understood; after it, the word was corrected.
    idle: demo.fixed
      ? { label: "En attente de ton raccourci", tone: "neutral" }
      : { label: `Aède a compris « ${HEARD} »`, tone: "neutral" },
    proposal: { label: "Confirmer pour l'ajouter", tone: "neutral" },
    added: { label: "Mot ajouté au dictionnaire", tone: "success" },
    nothing: { label: `Corrige d'abord le mot : clique sur « ${HEARD} ».`, tone: "warning" },
    failed: { label: demo.error ?? "L'ajout a échoué.", tone: "danger" },
  };
  return labels[demo.phase];
}

/** The notch of the Mac the pill comes out of: the size of the real one (185 x 32). */
const NOTCH: Notch = { width: 185, height: 32 };

/** The notch of the Figma (a still black shape) and the real pill of the app over it, which
 *  comes out of it and goes back in. It is only a picture. */
function FakeNotch({ view }: { view: View | null }) {
  return (
    <div className="setup-fakenotch" aria-hidden="true">
      <div className="setup-fakenotch-still" />
      <Pill<View>
        view={view}
        notch={NOTCH}
        look="black"
        overshootFor={() => PROPOSAL_OVERSHOOT}
        render={(shown) => <LearnContent view={shown} notch={NOTCH} />}
      />
    </div>
  );
}

type CardsProps = {
  shortcutLabel: string | null;
  demo: LearnDemo;
};

/** Screen 10l, the right side: the two steps, then the notch and the locked field. */
export function LearnCards({ shortcutLabel, demo }: CardsProps) {
  const status = statusOf(demo);
  return (
    <section className="setup-demo">
      <ol className="setup-demo-steps">
        <li>
          <span className="setup-demo-icon">1</span>
          <span className="setup-demo-line">Corrige le mot mal compris : clique dessus</span>
        </li>
        <li>
          <span className="setup-demo-icon">2</span>
          <span className="setup-demo-line">
            Appuie sur {shortcutLabel && <Keys label={shortcutLabel} />}
          </span>
        </li>
        <li>
          <span className="setup-demo-icon">3</span>
          <span className="setup-demo-line">
            Appuie une seconde fois sur {shortcutLabel && <Keys label={shortcutLabel} />} pour confirmer
          </span>
        </li>
      </ol>
      <div className="setup-mic">
        <FakeNotch view={demo.view} />
        <div className="setup-demo-field">
          {/* Locked: the text cannot change. A click on the word corrects it and marks it as selected. */}
          <div className="setup-demo-text" role="textbox" aria-readonly="true" aria-label="Champ d'essai">
            Le login ne marche pas avec{" "}
            <button type="button" className="setup-demo-word" data-marked={demo.fixed || undefined} onClick={demo.select}>
              {demo.fixed ? WORD : HEARD}
            </button>
            .
          </div>
        </div>
        <span className="setup-state" data-tone={status.tone} role="status">
          <span className="setup-state-dot" />
          {status.label}
        </span>
      </div>
    </section>
  );
}

type TextProps = {
  onBack: () => void;
  onContinue: () => void;
};

/** The left side: the text, "Retour" and "Continuer". */
export function LearnText({ onBack, onContinue }: TextProps) {
  return (
    <>
      <div className="setup-titles">
        <h1 className="setup-title">Apprends-lui un mot</h1>
        <p className="setup-text">
          {`Quand ${APP_NAME} entend mal un mot, corrige-le dans ton texte, sélectionne le mot corrigé, puis appuie sur le raccourci : ${APP_NAME} le retiendra. Ici, il a compris « ${HEARD} » au lieu de « ${WORD} ».`}
        </p>
      </div>
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
