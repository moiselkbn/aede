import { MutableRefObject, RefObject, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type DemoPhase =
  | "idle"
  | "recording"
  | "transcribing"
  | "rewriting"
  | "pasting"
  /** The text has landed: the window plays the rewriting on it, word by word. */
  | "revealing"
  | "done"
  /** The dictation worked, but the text did not land in the field. */
  | "missed"
  | "failed";

type PhaseChange = {
  phase: "idle" | "recording" | "transcribing" | "rewriting" | "pasting";
  error: string | null;
  failure: string | null;
};
type Result = { text: string };

/** The step "Transcription en cours" shows at least this long (the prototype's Whisper step),
 *  even when Whisper is faster: what comes next waits for it. */
const TRANSCRIBING_MIN_MS = 1200;

/** Time the paste needs to reach the field once Rust says it is done. */
const PASTE_SETTLES_MS = 250;

export type DictationDemo = {
  phase: DemoPhase;
  /** What Whisper heard, known from the start of the rewriting. */
  raw: string | null;
  /** What landed in the field, known once the dictation is over. */
  result: string | null;
  /** The window is done playing the rewriting. */
  finish: () => void;
  error: string | null;
  /** The last volume of the microphone (RMS, 0 to 1), read by the wave at each frame. */
  volume: MutableRefObject<number>;
};

/** The dictation trial: while `active`, a real dictation leaves no trace (trial.rs) and its
 *  steps are followed here. The text is pasted in `field`, the same way as in any application. */
export function useDictationDemo(
  active: boolean,
  field: RefObject<HTMLTextAreaElement | null>,
): DictationDemo {
  const volume = useRef(0);
  const [phase, setPhase] = useState<DemoPhase>("idle");
  const [raw, setRaw] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const subscribe = <T,>(name: string, handler: (payload: T) => void) =>
      listen<T>(name, (event) => handler(event.payload)).then((fn) => {
        if (cancelled) fn();
        else unlisteners.push(fn);
      });
    // What the field held before the dictation: the text is there if it changed.
    let before = "";
    let settle: ReturnType<typeof setTimeout> | undefined;
    // What happens after the transcription waits here until its minimum time is over.
    let gate = 0;
    let queue: (() => void)[] = [];
    let gateTimer: ReturnType<typeof setTimeout> | undefined;
    const later = (action: () => void) => {
      if (Date.now() >= gate) return action();
      queue.push(action);
      gateTimer ??= setTimeout(() => {
        gateTimer = undefined;
        const waiting = queue;
        queue = [];
        waiting.forEach((run) => run());
      }, gate - Date.now());
    };
    const dropQueue = () => {
      window.clearTimeout(gateTimer);
      gateTimer = undefined;
      queue = [];
      gate = 0;
    };

    subscribe<PhaseChange>("dictation-phase", ({ phase: next, error: message, failure }) => {
      // The idle that ends a dictation comes right after its result: it must not cancel it.
      if (next !== "idle") window.clearTimeout(settle);
      if (next === "recording") {
        dropQueue();
        // A new dictation starts from an empty field.
        if (field.current) field.current.value = "";
        before = "";
        volume.current = 0;
        setRaw(null);
        setResult(null);
        setError(null);
        setPhase("recording");
      } else if (next === "transcribing") {
        gate = Date.now() + TRANSCRIBING_MIN_MS;
        setPhase(next);
      } else if (next !== "idle") {
        later(() => setPhase(next));
      } else if (failure === "cancelled") {
        dropQueue();
        setPhase("idle");
      } else if (message) {
        dropQueue();
        setError(message);
        setPhase("failed");
      }
      // Idle without an error: the result event comes first and has the last word.
    });
    subscribe<string>("dictation-raw", (text) => later(() => setRaw(text)));
    subscribe<Result>("dictation-result", () => {
      settle = setTimeout(
        () =>
          later(() => {
            const text = field.current?.value ?? "";
            const landed = text !== before;
            if (landed) setResult(text);
            setPhase(landed ? "revealing" : "missed");
          }),
        PASTE_SETTLES_MS,
      );
    });
    subscribe<number>("audio-level", (value) => {
      volume.current = value;
    });
    invoke("set_dictation_trial", { on: true }).catch(() => {});

    return () => {
      cancelled = true;
      window.clearTimeout(settle);
      dropQueue();
      unlisteners.forEach((fn) => fn());
      invoke("set_dictation_trial", { on: false }).catch(() => {});
    };
  }, [active, field]);

  const finish = useCallback(() => setPhase((now) => (now === "revealing" ? "done" : now)), []);

  return { phase, raw, result, finish, error, volume };
}
