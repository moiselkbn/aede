// What the pill shows for a dictation (the pill itself is Pill.tsx): the wave of the voice
// while recording, a bump that crosses it while transcribing, then a status line (a dot and a
// short text) when the text is pasted or when it failed. The wave is drawn in a small canvas by
// the loop of Indicator.tsx; the status line is HTML. The two cross-fade.

import { MutableRefObject, useLayoutEffect, useRef } from "react";
import { Layout, Status, waveCanvasHeight } from "./wave";
import "./dictation.css";

// The lights a dictation goes through while the pill is out.
export type Lit = "recording" | "working" | "done" | "error";

export default function DictationContent({
  layout,
  light,
  status,
  canvasRef,
}: {
  layout: Layout;
  light: Lit;
  status: Status | null;
  // Where the page keeps the canvas it draws the wave in.
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
}) {
  const waveRef = useRef<HTMLCanvasElement>(null);
  const height = waveCanvasHeight(layout);

  // The canvas has the screen's resolution. Giving it a size clears it: the loop draws it again.
  useLayoutEffect(() => {
    const canvas = waveRef.current;
    if (!canvas) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(layout.waveWidth * ratio);
    canvas.height = Math.round(height * ratio);
    canvasRef.current = canvas;
    return () => {
      canvasRef.current = null;
    };
  }, [layout.waveWidth, height]);

  const showWave = light === "recording" || light === "working";
  // The room left under the wave only belongs to the wave: the status line has its own height.
  return (
    <div
      className="dictation"
      style={{ width: layout.openWidth, height: showWave ? layout.openHeight : layout.statusHeight }}
    >
      <canvas
        className="dictation-wave"
        ref={waveRef}
        style={{
          top: layout.waveCenter,
          width: layout.waveWidth,
          height,
          opacity: showWave ? 1 : 0,
        }}
      />
      {status && (
        <div className="dictation-status" style={{ top: layout.statusCenter, opacity: showWave ? 0 : 1 }}>
          <span className={status.kind === "done" ? "pill-dot pill-ok" : "pill-dot pill-fail"} />
          <span>{status.text}</span>
        </div>
      )}
    </div>
  );
}
