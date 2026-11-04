import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { View } from "../LearnProposal";

/** The example of the trial: what Aède "heard", which the field shows at first, and the word it
 *  becomes (learn.rs, `TRIAL_WRONG` and `TRIAL_RIGHT`, are the same two words). */
export const HEARD = "super base";
export const WORD = "Supabase";

/** How long the answer stays in the notch, as in the indicator (`LEARN_DONE_MS`). */
const DONE_MS = 2000;

export type LearnPhase = "idle" | "proposal" | "added" | "nothing" | "failed";

/** The learning trial of the first launch. The field is locked and shows the word as it was
 *  heard. A click on the word corrects it ("Supabase") and marks it as selected, as if the user
 *  had typed the right word and selected it. Then the learning shortcut (Rust only tells the
 *  window, see trial.rs): first press, the notch of the window proposes "super base →
 *  Supabase"; second press, the pair is added to the dictionary (the one real effect), Escape
 *  cancels. Aède reads nothing outside its own window. The answer is always "added", even if an
 *  earlier first launch already put the pair in the dictionary: the trial shows how it works. */
export function useLearnDemo(active: boolean, shortcut: string) {
  const [phase, setPhase] = useState<LearnPhase>("idle");
  // What the notch of the window shows; null when it is back in the notch.
  const [view, setView] = useState<View | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The word of the field was clicked: it is corrected and selected.
  const [fixed, setFixed] = useState(false);
  // Read by the press handler, which must not be registered again at each change.
  const state = useRef({ phase: "idle" as LearnPhase, fixed: false, shortcut, serial: 0 });
  state.current.shortcut = shortcut;

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let timer: number | undefined;
    // The answer is on screen: a press now changes nothing.
    let answering = false;
    const go = (next: LearnPhase) => {
      state.current.phase = next;
      setPhase(next);
    };
    const outcome = (ok: boolean, text: string) => {
      answering = true;
      setView({ kind: "outcome", ok, text });
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        answering = false;
        setView(null);
      }, DONE_MS);
    };
    const press = () => {
      if (answering) return;
      if (state.current.phase === "proposal") {
        invoke("add_trial_word")
          .then(() => {
            go("added");
            outcome(true, "Ajouté au dictionnaire");
          })
          .catch((err) => {
            setError(String(err));
            go("failed");
            outcome(false, "Ajout impossible");
          });
        return;
      }
      if (!state.current.fixed) {
        go("nothing");
        return;
      }
      setError(null);
      go("proposal");
      setView({
        kind: "proposal",
        proposal: {
          pairs: [[HEARD, WORD]],
          shortcut: state.current.shortcut,
          seconds: 0,
          serial: ++state.current.serial,
        },
      });
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code !== "Escape" || state.current.phase !== "proposal") return;
      go("idle");
      setView(null);
    };

    listen<boolean>("learn-test", (event) => {
      if (event.payload) press();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    invoke("set_learn_test", { on: true }).catch(() => {});
    window.addEventListener("keydown", onKeyDown);
    return () => {
      cancelled = true;
      unlisten?.();
      window.clearTimeout(timer);
      window.removeEventListener("keydown", onKeyDown);
      invoke("set_learn_test", { on: false }).catch(() => {});
      state.current.phase = "idle";
      setPhase("idle");
      state.current.fixed = false;
      setFixed(false);
      setView(null);
    };
    // The shortcut is read from `state`.
  }, [active]);

  // The click on the word of the field.
  const select = () => {
    if (!active) return;
    state.current.fixed = true;
    setFixed(true);
  };

  return { phase, view, error, fixed, select };
}

export type LearnDemo = ReturnType<typeof useLearnDemo>;
