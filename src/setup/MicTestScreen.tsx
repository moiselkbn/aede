import { Mic } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import { Select } from "@/components/arc/select/select";
import { APP_NAME } from "../appName";
import NotchWave from "./NotchWave";
import type { MicTest } from "./useMicTest";

type Tone = "neutral" | "success" | "danger";

function statusOf(test: MicTest): { label: string; tone: Tone } {
  if (test.phase === "failed") return { label: "Micro indisponible", tone: "danger" };
  return test.detected
    ? { label: "Son détecté", tone: "success" }
    : { label: "En attente de ta voix", tone: "neutral" };
}

/** Screens 10f and 10g, the right side: the notch with the live wave, the choice of microphone,
 *  and whether a sound was heard. */
export function MicTestCards({ test }: { test: MicTest }) {
  const status = statusOf(test);
  return (
    <section className="setup-mic">
      <NotchWave volume={test.volume} />
      <div className="setup-mic-field">
        {/* The Arc Select has no icon slot: the icon sits over the left of its trigger. */}
        <Mic className="setup-mic-icon" size={16} strokeWidth={1.75} aria-hidden="true" />
        <Select
          label="Micro"
          value={test.selected ?? ""}
          onValueChange={test.choose}
          disabled={test.devices.length === 0}
          placeholder="Aucun micro trouvé"
          className="setup-mic-trigger"
          options={test.devices.map((device) => ({ value: device.id, label: device.name }))}
        />
      </div>
      <span className="setup-state" data-tone={status.tone} role="status">
        <span className="setup-state-dot" />
        {status.label}
      </span>
    </section>
  );
}

type TextProps = {
  test: MicTest;
  /** What the models are doing: the test goes on while they download. */
  models: { label: string; tone: Tone };
  onBack: () => void;
  onContinue: () => void;
};

/** The left side: the text, the state of the models, "Retour" and "Continuer". */
export function MicTestText({ test, models, onBack, onContinue }: TextProps) {
  return (
    <>
      <div className="setup-titles">
        <h1 className="setup-title">Teste ton micro</h1>
        <p className="setup-text">
          Dis une phrase à voix haute. La vague bouge quand {APP_NAME} t'entend. Rien n'est
          enregistré.
        </p>
      </div>
      <span className="setup-state" data-tone={models.tone}>
        <span className="setup-state-dot" />
        {models.label}
      </span>
      {test.error && <p className="setup-error">{test.error}</p>}
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
