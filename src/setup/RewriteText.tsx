import { useEffect, useRef } from "react";
import type { Cell } from "./diffWords";
import { startRewrite, type RewriteHandle } from "./rewriteAnimation";

type Props = {
  /** What Whisper heard. */
  raw: string;
  /** What landed in the field, once known. */
  result: string | null;
  onHeight: (px: number) => void;
  onDone: () => void;
  onStep?: (cell: Cell) => void;
};

/** The text of the dictation trial, drawn over the field while the rewriting is played
 *  (rewriteAnimation.ts). The real text is in the field under it, hidden until the end. */
function RewriteText({ raw, result, onHeight, onDone, onStep }: Props) {
  const boxRef = useRef<HTMLDivElement>(null);
  const handle = useRef<RewriteHandle | null>(null);
  // The latest callbacks, so that a new one does not restart the animation.
  const hooks = useRef({ onHeight, onDone, onStep });
  hooks.current = { onHeight, onDone, onStep };

  useEffect(() => {
    if (!boxRef.current) return;
    handle.current = startRewrite(boxRef.current, raw, {
      onHeight: (px) => hooks.current.onHeight(px),
      onDone: () => hooks.current.onDone(),
      onStep: (cell) => hooks.current.onStep?.(cell),
      reduced: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    });
    return () => handle.current?.cancel();
  }, [raw]);

  useEffect(() => {
    if (result !== null) handle.current?.setResult(result);
  }, [result]);

  return <div ref={boxRef} className="rw" aria-hidden="true" />;
}

export default RewriteText;
