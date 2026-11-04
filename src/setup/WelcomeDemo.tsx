import { useEffect, useRef, useState } from "react";
import { Check } from "lucide-react";
import type { Cell } from "./diffWords";
import { FIELD_PADDING } from "./DemoScreen";
import NotchWave from "./NotchWave";
import RewriteText from "./RewriteText";
import { movePill, resetPill, type PillParts } from "./welcomeNotch";

/** What Whisper heard, and the text that is pasted: the example of the welcome screen. */
const RAW = "euh, regarde le, le bug sur le, euh, dashboard";
const RESULT = "Regarde le bug sur le dashboard.";

const LABELS = ["Hésitations retirées", "Demande à l'impératif", "Termes anglais gardés"] as const;

/** Durations in ms, the ones of the animation validated by the author (Figma screen 10). The sweep, the light and the
 *  word by word pass are the ones of the dictation trial (rewriteAnimation.ts). */
const T = { cursorOnly: 300, speaking: 1400, whisper: 1200, hold: 1600, fade: 250, closed: 900 };

type Phase = "cursor" | "speaking" | "whisper" | "rewrite" | "done" | "fading";

/** A made-up voice: bursts like syllables, so the wave looks like someone is talking. 0 to 1. */
function voice(time: number): number {
  const syllable = Math.max(0, Math.sin(time * 0.019 + Math.sin(time * 0.003) * 2));
  const envelope = 0.5 + 0.5 * Math.sin(time * 0.0045);
  return 0.15 + 0.85 * syllable * (0.5 + 0.5 * envelope);
}

/** The card of the welcome screen (Figma 10), in a loop: the user speaks (the wave), lets go (the
 *  wave scans), the text heard appears, then it is rewritten and the labels light up, then the
 *  notch closes and opens on an empty field again. */
function WelcomeDemo() {
  const [phase, setPhase] = useState<Phase>("cursor");
  const [lit, setLit] = useState<boolean[]>([false, false, false]);
  const [height, setHeight] = useState<number | null>(null);
  const volume = useRef(0);
  const talking = useRef(false);
  const rewriteDone = useRef<(() => void) | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const left = useRef<HTMLDivElement>(null);
  const right = useRef<HTMLDivElement>(null);
  const wave = useRef<HTMLDivElement>(null);

  // The microphone of the card: a made-up voice, held at zero unless someone "talks".
  useEffect(() => {
    let gain = 0;
    let frame = 0;
    const tick = (now: number) => {
      gain += ((talking.current ? 1 : 0) - gain) * 0.12;
      // The level of the wave (0 to 1) back to a volume, the way wave.ts reads it: -55 dB is silence.
      volume.current = 10 ** ((voice(now) * gain * 43 - 55) / 20);
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      // No movement: the end of the story, once.
      setPhase("done");
      setLit([true, true, true]);
      setHeight(46);
      return;
    }
    let alive = true;
    const timers = new Set<number>();
    const wait = (ms: number) =>
      new Promise<void>((resolve) => {
        const id = window.setTimeout(() => {
          timers.delete(id);
          resolve();
        }, ms);
        timers.add(id);
      });
    const parts = (): PillParts | null =>
      body.current && left.current && right.current && wave.current
        ? { body: body.current, left: left.current, right: right.current, wave: wave.current }
        : null;

    async function loop() {
      while (alive) {
        setPhase("cursor");
        setLit([false, false, false]);
        setHeight(null);
        await wait(T.cursorOnly);
        if (!alive) return;

        // The key is held: only the wave reacts, nothing is written yet (Whisper starts at the release).
        setPhase("speaking");
        talking.current = true;
        await wait(T.speaking);
        if (!alive) return;

        // Released: Whisper works on the whole recording, the wave scans.
        talking.current = false;
        setPhase("whisper");
        await wait(T.whisper);
        if (!alive) return;

        // The text heard, swept, lit, then rewritten (the same animation as the dictation trial).
        const finished = new Promise<void>((resolve) => (rewriteDone.current = resolve));
        setPhase("rewrite");
        await finished;
        if (!alive) return;
        setPhase("done");
        await wait(T.hold);
        if (!alive) return;

        // The notch goes back, the text fades while it is closed, then it opens on a new field.
        const pill = parts();
        if (pill) await movePill(pill, true);
        if (!alive) return;
        setPhase("fading");
        await wait(T.fade);
        if (!alive) return;
        setHeight(null);
        await wait(T.closed - T.fade);
        if (!alive) return;
        setLit([false, false, false]);
        setPhase("cursor");
        if (pill) await movePill(pill, false);
        if (pill) resetPill(pill);
      }
    }
    void loop();
    return () => {
      alive = false;
      timers.forEach((id) => window.clearTimeout(id));
      const pill = parts();
      if (pill) resetPill(pill);
    };
  }, []);

  // The label of what just happened to the text lights up (once per pass).
  const light = (cell: Cell) => {
    const index =
      cell.kind === "removed" ? 0 : cell.kind === "kept" && cell.raw === "regarde" ? 1 : cell.kind === "kept" && cell.raw === "bug" ? 2 : -1;
    if (index >= 0) setLit((old) => (old[index] ? old : old.map((value, i) => value || i === index)));
  };

  const scanning = phase === "whisper" || phase === "rewrite";
  return (
    <div className="setup-mic">
      <div className="setup-welcome-notch" aria-hidden="true">
        <div className="setup-fakenotch-still" />
        <div className="setup-welcome-body" ref={body} />
        <div className="setup-welcome-flare" data-side="left" ref={left} />
        <div className="setup-welcome-flare" data-side="right" ref={right} />
        <div className="setup-welcome-wave" ref={wave}>
          <NotchWave volume={volume} scanning={scanning} bare />
        </div>
      </div>
      <div
        className="setup-welcome-field"
        role="img"
        aria-label="Exemple de texte dicté puis réécrit"
        data-fading={phase === "fading" || undefined}
        style={height === null ? undefined : { height }}
      >
        {phase === "cursor" && <span className="setup-welcome-caret" />}
        {phase === "rewrite" && (
          <RewriteText
            raw={RAW}
            result={RESULT}
            onHeight={(text) => setHeight(text + FIELD_PADDING)}
            onDone={() => rewriteDone.current?.()}
            onStep={light}
          />
        )}
        {(phase === "done" || phase === "fading") && (
          // The words are the ones of the rewriting (inline blocks with a margin, not spaces): the text
          // does not move when the animation hands over to it.
          <p className="setup-welcome-text">
            {RESULT.split(" ").map((word, index) => (
              <span className="rw-w" key={index}>
                {word}
              </span>
            ))}
            <span className="setup-welcome-caret" data-inline />
          </p>
        )}
      </div>
      <ul className="setup-welcome-chips">
        {LABELS.map((label, index) => (
          <li className="setup-welcome-chip" data-lit={lit[index] || undefined} key={label}>
            <Check size={12} strokeWidth={2} aria-hidden="true" />
            {label}
          </li>
        ))}
      </ul>
    </div>
  );
}

export default WelcomeDemo;
