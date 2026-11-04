import type { ReactNode } from "react";
import Stepper from "./Stepper";
import ToastStack, { type ToastData } from "./Toast";

type Props = {
  /** The step on screen and the finished parts of the bars. Left out on the welcome screen, which
   *  has no stepper. */
  current?: number;
  filled?: readonly [number, number, number];
  failed?: boolean;
  /** The title, the text and the buttons. */
  left: ReactNode;
  /** The cards of the screen, in the right panel. */
  right?: ReactNode;
  /** Where the cards sit in the panel: at its top by default, in its middle ("center"), or at the
   *  fixed height of the keys of the shortcut screens ("shortcut"). */
  placement?: "center" | "shortcut";
  /** The download notice only peeks out at the bottom of the window (the screens whose cards need the room). */
  sinkToast?: boolean;
  toasts: ToastData[];
};

/** The window of the first launch (Figma, 880 x 680): the stepper on top, the text on the left,
 *  a panel on the right with the cards and, at its bottom, the notices. */
function SetupFrame({ current, filled, failed, left, right, placement, sinkToast, toasts }: Props) {
  const stepper = current !== undefined && filled !== undefined;
  return (
    <main className="setup" data-bare={stepper ? undefined : ""}>
      {/* The title bar is hidden (overlay): this strip is what the window is dragged by. */}
      <div className="setup-drag" data-tauri-drag-region />
      {stepper && <Stepper current={current} filled={filled} failed={failed} />}
      <div className="setup-body">
        <section className="setup-left">{left}</section>
        <div className="setup-side">
          <aside className="setup-panel">
            <div className="setup-cards" data-placement={placement}>{right}</div>
          </aside>
          {/* Next to the panel, not in it: a notice that sinks is cut by the window, not by the panel. */}
          <ToastStack toasts={toasts} sunk={sinkToast} />
        </div>
      </div>
    </main>
  );
}

export default SetupFrame;
