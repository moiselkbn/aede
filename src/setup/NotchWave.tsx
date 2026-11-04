import { MutableRefObject, useEffect, useRef } from "react";
import {
  Layout,
  WAVE,
  drawWave,
  levelFromVolume,
  ovalHeights,
  waveCanvasHeight,
} from "../wave";

/** The notch of the Figma (234 x 59) and its wave: the same wave as the pill of a dictation
 *  (wave.ts), sized for the card of the microphone test. */
const LAYOUT: Layout = {
  openWidth: 234,
  openHeight: 59,
  statusHeight: 59,
  statusCenter: 32,
  waveWidth: 184,
  waveCenter: 32,
  minHeight: 1.1,
  maxHeight: 25.65,
};

/** How long the wave remembers its levels, for the ovals at its edges. */
const HISTORY_MS = 2000;

/** A gap between two frames longer than this is a stall, not an animation step. */
const STALL_MS = 100;

type Props = {
  /** The last volume of the microphone (RMS, 0 to 1). */
  volume: MutableRefObject<number>;
  /** The text is being worked on: the bump of the transcription crosses the wave, as in the
   *  pill of a real dictation. */
  scanning?: boolean;
  /** Only the wave, without the black notch around it: the welcome screen draws its own, which
   *  closes and opens. */
  bare?: boolean;
};

function NotchWave({ volume, scanning = false, bare = false }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Read by the animation frames, so that a change does not restart them.
  const scanningRef = useRef(scanning);
  scanningRef.current = scanning;

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(LAYOUT.waveWidth * ratio);
    canvas.height = Math.round(waveCanvasHeight(LAYOUT) * ratio);

    // The same smoothing as the indicator: the centre follows the voice, the ovals further out
    // show it a little later.
    let level = 0;
    let scan = 0;
    let last = performance.now();
    let frameId = 0;
    const history: [number, number][] = [];
    const levelAt = (time: number) => {
      for (let i = history.length - 1; i >= 0; i--) {
        if (history[i][0] <= time) return history[i][1];
      }
      return 0;
    };
    const frame = (now: number) => {
      const gap = now - last;
      const dt = gap > STALL_MS ? 1 / 60 : Math.min(0.05, Math.max(0, gap / 1000));
      last = now;
      const target = levelFromVolume(volume.current);
      const delay = (target > level ? WAVE.attackMs : WAVE.releaseMs) / 1000;
      level += (target - level) * (1 - Math.exp(-dt / delay));
      // The bump comes in and goes out in 0.15 s, like in the indicator.
      scan += ((scanningRef.current ? 1 : 0) - scan) * (1 - Math.exp(-dt / 0.15));
      history.push([now, level]);
      while (history[0][0] < now - HISTORY_MS) history.shift();
      drawWave(ctx, ratio, LAYOUT, ovalHeights(levelAt, now, LAYOUT, scan));
      frameId = requestAnimationFrame(frame);
    };
    frameId = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(frameId);
  }, [volume]);

  return (
    <div className={bare ? "notch-wave" : "notch-preview"} aria-hidden="true">
      <canvas
        ref={canvasRef}
        style={{ top: LAYOUT.waveCenter, width: LAYOUT.waveWidth, height: waveCanvasHeight(LAYOUT) }}
      />
    </div>
  );
}

export default NotchWave;
