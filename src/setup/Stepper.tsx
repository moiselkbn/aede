import { Check, TriangleAlert } from "lucide-react";

const STEPS = ["Autorisations", "Modèles", "Configuration", "Prêt"] as const;

/** Parts of the bar after each of the first three steps (Figma "Segments"): permissions (microphone,
 *  accessibility), models, then microphone, shortcut 1, demo 1, shortcut 2, demo 2. */
export const BAR_PARTS = [2, 1, 5] as const;

type Props = {
  /** Index of the step on screen (0 to 3). */
  current: number;
  /** Finished parts of each of the three bars. A full bar means its left step is done. */
  filled: readonly [number, number, number];
  /** The current step failed (the download stopped): its marker and label turn red with an alert. */
  failed?: boolean;
};

/** The progress of the first launch (Figma "Stepper"): four numbered markers, a bar cut in parts
 *  after the first three, one part per sub-step. A finished step shows a check. */
function Stepper({ current, filled, failed = false }: Props) {
  return (
    <ol className="stepper" aria-label="Étapes du premier lancement">
      {STEPS.map((label, index) => {
        const hasBar = index < BAR_PARTS.length;
        const done = hasBar && index !== current && filled[index] >= BAR_PARTS[index];
        const state = index === current ? (failed ? "failed" : "current") : done ? "done" : "upcoming";
        return (
          <li
            key={label}
            className="stepper-item"
            data-state={state}
            data-last={hasBar ? undefined : ""}
            aria-current={index === current ? "step" : undefined}
          >
            <div className="stepper-marker">
              <span className="stepper-halo">
                <span className="stepper-dot">
                  {state === "done" ? (
                    <Check size={14} strokeWidth={2.5} aria-hidden="true" />
                  ) : state === "failed" ? (
                    <TriangleAlert size={14} strokeWidth={1.75} aria-hidden="true" />
                  ) : (
                    index + 1
                  )}
                </span>
              </span>
              {hasBar && (
                <span className="stepper-bar" aria-hidden="true">
                  {Array.from({ length: BAR_PARTS[index] }, (_, part) => (
                    <span key={part} className="stepper-part" data-filled={part < filled[index] ? "" : undefined} />
                  ))}
                </span>
              )}
            </div>
            {/* 36 px wide like the marker: the label is centered under it and overflows on both sides. */}
            <span className="stepper-label">
              <span>{label}</span>
            </span>
          </li>
        );
      })}
    </ol>
  );
}

export default Stepper;
