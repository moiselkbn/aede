import { Check } from "lucide-react";
import { Button } from "@/components/arc/button/button";
import { APP_NAME } from "../appName";
import Logo from "../Logo";
import { formatSize } from "./format";

type TextProps = {
  /** Size of the two models, in bytes (null while it is not known). */
  modelsSize: number | null;
  onStart: () => void;
};

/** Screen 10, the left side: the wordmark, what Aède does, the three promises, and "Commencer".
 *  "Commencer" is the user's go-ahead for the download of the models. */
export function WelcomeText({ modelsSize, onStart }: TextProps) {
  const promises = [
    "Tout reste sur ce Mac : ni ta voix ni ton texte ne sont envoyés sur internet.",
    "Aucun compte, aucune télémétrie.",
    `Un seul téléchargement, une seule fois : c'est le seul moment où ${APP_NAME} se connecte à internet.`,
  ];
  return (
    <>
      <div className="setup-welcome-logo">
        <Logo height={41} />
      </div>
      <div className="setup-welcome-copy">
        <div className="setup-titles">
          <h1 className="setup-title">Parle, {APP_NAME} écrit pour toi</h1>
          <p className="setup-text">
            Maintiens un raccourci, parle en français, relâche : un texte propre est collé là où se
            trouve ton curseur.
          </p>
        </div>
        <ul className="setup-promises">
          {promises.map((promise) => (
            <li key={promise}>
              <span className="setup-promise-icon">
                <Check size={16} strokeWidth={1.75} aria-hidden="true" />
              </span>
              {promise}
            </li>
          ))}
        </ul>
      </div>
      <div className="setup-welcome-actions">
        <p className="setup-muted setup-welcome-note">
          Le téléchargement des modèles ({modelsSize ? formatSize(modelsSize) : "4,1 Go"}) démarre au clic.
        </p>
        <div className="setup-buttons">
          <Button variant="primary" size="md" className="setup-primary" onClick={onStart}>
            Commencer
          </Button>
        </div>
      </div>
    </>
  );
}
