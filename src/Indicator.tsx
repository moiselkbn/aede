// The page of the indicator window, placed over the notch by src-tauri/src/indicator.rs.
// One black pill (Pill.tsx) comes out of the notch and shows what the app has to say, one thing
// at a time: a dictation (wave of the voice while recording, a bump while working, then a status
// line: a green dot and a short text when the text is pasted, a red dot and the kind of failure
// otherwise), or what the learning shortcut shows (LearnProposal.tsx). When the thing changes
// while the pill is out, the pill changes its content and its size in place.
// Without a notch, a smaller pill shows in the menu bar. The wave is drawn here, in a canvas.

import { useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  Layout,
  Spot,
  Status,
  WAVE,
  drawWave,
  layoutFor,
  levelFromVolume,
  ovalHeights,
  statusFor,
} from "./wave";
import DictationContent, { Lit } from "./DictationContent";
import { LearnContent, LearnTrack, Proposal, View } from "./LearnProposal";
import Pill, { Notch } from "./Pill";
import type { Appearance } from "./appearance";
import { DEFAULT_PILL_STYLE, type PillStyle } from "./pill-style";
import { DICTATION_OVERSHOOT, PROPOSAL_OVERSHOOT } from "./motion";
import "./indicator.css";

declare global {
  interface Window {
    // Set by indicator.rs before the page loads.
    __NOTCH__?: Spot;
  }
}

type DictationPhase = {
  phase: "idle" | "recording" | "transcribing" | "rewriting" | "pasting";
  error: string | null;
  // Kind of failure ("too_short", "no_sound"...), set with `error`.
  failure: string | null;
};

// The part of the dictation result the indicator needs.
type DictationResult = {
  // Set when the rewriting was skipped and the raw transcription was pasted.
  fallback_reason: string | null;
};

type Light = "hidden" | Lit;

// What the pill shows: a dictation (its details are the `dictation` state), or what the
// learning shortcut shows.
type PillView = { kind: "dictation" } | View;
const DICTATION: PillView = { kind: "dictation" };

// How long the done and error lights stay on before the pill goes back into the notch.
const DONE_MS = 1500;
const ERROR_MS = 2500;

// A gap between two frames longer than this is a stall, not an animation step.
const STALL_MS = 100;

// How long the wave keeps the levels it shows at its edges.
const HISTORY_MS = 2000;

// The end of an answer of the learning shortcut: a short text, green or red.
type LearnOutcome = { ok: boolean; text: string };

// How long the empty pill stays out when the app starts (App.tsx uses the same time at the end of the first launch).
const ACTIVE_MS = 3000;

// A short text asked by the window (the end of the first launch), shown for `ms` milliseconds.
type Notice = { text: string; ms: number };

// How long the result of the learning shortcut stays: the same as for a dictation. While it
// says that words were added, learn.rs ignores a new press for these words (`ADDED_STAYS`
// there: keep the two equal).
const LEARN_DONE_MS = 2000;

export default function Indicator() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [view, setView] = useState<PillView | null>(null);
  const [pillStyle, setPillStyle] = useState<PillStyle>(DEFAULT_PILL_STYLE);

  // The style the user chose in Settings, and its changes while the app runs.
  useEffect(() => {
    invoke<Appearance>("get_appearance")
      .then((appearance) => setPillStyle(appearance.pill_style))
      .catch(() => {});
    const listening = listen<Appearance>("appearance-changed", (event) =>
      setPillStyle(event.payload.pill_style),
    );
    return () => {
      void listening.then((unlisten) => unlisten());
    };
  }, []);
  const [notch, setNotch] = useState<Notch>({
    width: window.__NOTCH__?.width ?? 0,
    height: window.__NOTCH__?.height ?? 24,
  });
  const [layout, setLayout] = useState<Layout>(() =>
    layoutFor(window.__NOTCH__ ?? { width: 0, height: 24, hasNotch: false }),
  );
  const [dictation, setDictation] = useState<{ light: Lit; status: Status | null }>({
    light: "recording",
    status: null,
  });
  const proposals = useRef(0);

  useEffect(() => {
    let layoutNow = layoutFor(window.__NOTCH__ ?? { width: 0, height: 24, hasNotch: false });
    let light: Light = "hidden";
    let status: Status | null = null;
    let timer: number | undefined;
    // What the pill shows now, kept here too so that the handlers below can look at it.
    let shown: PillView | null = null;
    // Only one thing is shown at a time, and the last one asked takes the pill. The learning
    // shortcut and a dictation are not used together (the fingers are on the dictation keys
    // while recording), so this only matters right after a dictation, while its status line
    // is still there.
    let learnTimer: number | undefined;

    // The wave only runs while there is something to show. Nothing runs while the pill is in
    // the notch.
    let frameId = 0;
    let lastFrame = 0;
    let volume = 0; // last microphone volume sent by Rust
    let level = 0; // smoothed level shown at the centre of the wave
    const history: [number, number][] = []; // [time, level], for the ovals further out
    let scan = 0; // how much of the transcription bump shows

    const levelAt = (time: number) => {
      for (let i = history.length - 1; i >= 0; i--) {
        if (history[i][0] <= time) return history[i][1];
      }
      return 0;
    };

    const frame = (now: number) => {
      const gap = now - lastFrame;
      // A long gap between two frames (the app was busy) is not time the pill should catch up
      // on: it would jump open. Count it as one normal frame, and cap the rest.
      const dt = gap > STALL_MS ? 1 / 60 : Math.min(0.05, Math.max(0, gap / 1000));
      lastFrame = now;

      // The voice fades out after the key is released, while the bump fades in.
      const target = light === "recording" ? levelFromVolume(volume) : 0;
      const delay = (target > level ? WAVE.attackMs : WAVE.releaseMs) / 1000;
      level += (target - level) * (1 - Math.exp(-dt / delay));
      history.push([now, level]);
      while (history[0][0] < now - HISTORY_MS) history.shift();
      scan += ((light === "working" ? 1 : 0) - scan) * (1 - Math.exp(-dt / 0.15));

      const ratio = window.devicePixelRatio || 1;
      const ctx = canvasRef.current?.getContext("2d");
      if (ctx) drawWave(ctx, ratio, layoutNow, ovalHeights(levelAt, now, layoutNow, scan));

      const running = light === "recording" || light === "working" || level > 0.002 || scan > 0.002;
      if (!running) {
        level = scan = 0;
        history.length = 0;
        if (ctx) drawWave(ctx, ratio, layoutNow, []);
        frameId = 0;
        return;
      }
      frameId = requestAnimationFrame(frame);
    };

    const wake = () => {
      if (frameId) return;
      lastFrame = performance.now();
      frameId = requestAnimationFrame(frame);
    };

    // Shows something in the pill, or takes it back into the notch (null). `now`: at once,
    // without waiting for React's turn, so that the pill comes out when the key goes down.
    const show = (next: PillView | null, now = false) => {
      shown = next;
      if (now) flushSync(() => setView(next));
      else setView(next);
    };

    const learnShown = () => shown !== null && shown.kind !== "dictation";

    const setLight = (next: Light) => {
      light = next;
      if (next === "hidden") {
        // The dictation is over: the pill goes back in, unless something else took it.
        if (shown?.kind === "dictation") show(null);
      } else {
        if (next === "recording") volume = 0;
        setDictation({ light: next, status });
        if (shown?.kind !== "dictation") show(DICTATION, true);
      }
      wake();
    };
    // A light that stays on until the next change.
    const keep = (next: Light) => {
      window.clearTimeout(timer);
      setLight(next);
    };
    // A light that goes off by itself.
    const flash = (next: Light, ms: number) => {
      keep(next);
      timer = window.setTimeout(() => setLight("hidden"), ms);
    };

    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    // Listening on this window, not globally: a global listener also gets what Rust sends to the
    // other windows only (the first launch keeps the dictation events away from the notch).
    const self = getCurrentWebviewWindow();
    const listening: Promise<void>[] = [];
    const subscribe = <T,>(name: string, handler: (payload: T) => void) => {
      listening.push(
        self.listen<T>(name, (event) => handler(event.payload)).then((fn) => {
          if (cancelled) fn();
          else unlisteners.push(fn);
        }),
      );
    };

    subscribe<DictationPhase>("dictation-phase", ({ phase, error, failure }) => {
      // A dictation needs the pill: what the learning shortcut shows gives way to it.
      if (phase === "recording") {
        window.clearTimeout(learnTimer);
        keep("recording");
      } else if (phase !== "idle") keep("working");
      else if (error) {
        status = statusFor(failure);
        flash("error", ERROR_MS);
      }
      // Back to idle without an error: the text was pasted. The "done" light is already on,
      // since Rust sends the result just before going back to idle.
      else if (light !== "done") keep("hidden");
    });
    // Sent only when the text was pasted, never after a failure.
    subscribe<DictationResult>("dictation-result", ({ fallback_reason }) => {
      status = statusFor(fallback_reason ? "done_raw" : "done");
      flash("done", DONE_MS);
    });
    // About 30 times per second while recording.
    subscribe<number>("audio-level", (value) => {
      volume = value;
    });
    // Sent when a dictation starts, in case the screens changed since the last one.
    subscribe<Spot>("indicator-spot", (spot) => {
      layoutNow = layoutFor(spot);
      setLayout(layoutNow);
      setNotch({ width: spot.width, height: spot.height });
    });
    // The learning shortcut: a proposal waits for the second press, then it is taken down
    // (cleared) or turns into a result.
    subscribe<Proposal>("learn-proposal", (next) => {
      window.clearTimeout(learnTimer);
      show({ kind: "proposal", proposal: { ...next, serial: ++proposals.current } });
    });
    subscribe<null>("learn-clear", () => {
      window.clearTimeout(learnTimer);
      if (learnShown()) show(null);
    });
    subscribe<LearnOutcome>("learn-result", ({ ok, text }) => {
      window.clearTimeout(learnTimer);
      show({ kind: "outcome", ok, text });
      learnTimer = window.setTimeout(
        () => {
          if (learnShown()) show(null);
        },
        ok ? LEARN_DONE_MS : ERROR_MS,
      );
    });

    // Same look as the end of an answer, but it stays as long as the window asks.
    const say = (text: string, ms: number) => {
      window.clearTimeout(learnTimer);
      show({ kind: "outcome", ok: true, text });
      learnTimer = window.setTimeout(() => {
        if (learnShown()) show(null);
      }, ms);
    };
    subscribe<Notice>("pill-notice", ({ text, ms }) => say(text, ms));

    // Each time the app starts, an empty pill comes out for a few seconds. Not during the first launch: its
    // screens say it themselves when they end (the window asks for it, above).
    void Promise.all(listening)
      .then(() => invoke<boolean>("needs_setup"))
      .then((first) => {
        if (!first && !cancelled) say("", ACTIVE_MS);
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.clearTimeout(learnTimer);
      cancelAnimationFrame(frameId);
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  return (
    <Pill<PillView>
      view={view}
      notch={notch}
      look={pillStyle}
      overshootFor={(shown) =>
        shown.kind === "dictation" ? DICTATION_OVERSHOOT : PROPOSAL_OVERSHOOT
      }
      render={(shown) =>
        shown.kind === "dictation" ? (
          <DictationContent
            layout={layout}
            light={dictation.light}
            status={dictation.status}
            canvasRef={canvasRef}
          />
        ) : (
          <LearnContent view={shown} notch={notch} />
        )
      }
      sizeKey={
        view?.kind === "dictation"
          ? dictation.light === "recording" || dictation.light === "working"
            ? "wave"
            : "status"
          : "other"
      }
      footer={(shown) =>
        shown.kind === "proposal" ? <LearnTrack proposal={shown.proposal} /> : null
      }
    />
  );
}
