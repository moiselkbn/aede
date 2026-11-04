import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ShortcutStatus = { label: string; error: string | null };

/** Keys that only modify another one: we wait for the main key. */
const MODIFIER_CODES = [
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "CapsLock",
  "Fn",
];

export type ShortcutCommand = "set_shortcut" | "set_learn_shortcut";

/** How long the first launch asks to hold a combination before it is kept. */
export const SHORTCUT_HOLD_MS = 1500;

/** A held combination has 2 or 3 keys (modifiers included). */
export const SHORTCUT_MIN_KEYS = 2;
export const SHORTCUT_MAX_KEYS = 3;

/** Once the hold is complete, the screen stays this long so the bar can reach 100% (its spring
 *  needs about 0.4 s) before the combination is kept and the screen changes. */
const SETTLE_MS = 700;

type Options = {
  /** The combination is kept once it was held this long (0: as soon as its main key is pressed).
   *  Releasing a key before the end starts over. With a hold, the combination also needs a
   *  modifier (Ctrl, Option, Maj or Cmd) and 2 or 3 keys in all. */
  holdMs?: number;
};

/** Waits for the user to press a new combination and saves it with the Rust `command`.
 *  Shared by the settings page and the first launch. `progress` (0 to 1) is how far the hold is. */
export function useShortcutCapture(
  command: ShortcutCommand,
  onChange: (status: ShortcutStatus) => void,
  { holdMs = 0 }: Options = {},
) {
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  // The latest `onChange`, read when the combination is kept: a new function at each render (the
  // progress re-renders the screen at each frame) must not restart the wait, which would also
  // release and take the shortcuts again each time.
  const changed = useRef(onChange);
  changed.current = onChange;

  useEffect(() => {
    if (!capturing) return;
    // The shortcuts are released while waiting: a registered combination would be taken by macOS
    // and never reach the window, so the user would get no answer at all.
    invoke("pause_shortcuts").catch((err) => {
      setError(String(err));
      setCapturing(false);
    });
    const cancel = () => setCapturing(false);

    // The keys held that are not modifiers, and the hold in progress. A shortcut has only one
    // such key: with two (Option + Space + X), there is nothing to keep, the user must let one go.
    const others = new Set<string>();
    let hold: { parts: string; frame: number } | null = null;
    // The hold is complete: the keys no longer matter, the combination is about to be kept.
    let kept: number | null = null;
    const stopHold = () => {
      if (hold) cancelAnimationFrame(hold.frame);
      hold = null;
      setProgress(0);
    };
    const save = (parts: string) => {
      setCapturing(false);
      // Registered again first: the change below replaces the shortcut that is registered.
      invoke("resume_shortcuts")
        .then(() => invoke<ShortcutStatus>(command, { text: parts }))
        .then((status) => changed.current(status))
        .catch((err) => setError(String(err)));
    };
    // The combination held after this event, or null if there is none to count.
    const combination = (event: KeyboardEvent) => {
      if (others.size !== 1) return null;
      const [main] = others;
      const anyModifier = event.ctrlKey || event.altKey || event.shiftKey || event.metaKey;
      if (holdMs > 0) {
        const keys = [event.ctrlKey, event.altKey, event.shiftKey, event.metaKey].filter(Boolean).length + 1;
        if (!anyModifier || keys < SHORTCUT_MIN_KEYS || keys > SHORTCUT_MAX_KEYS) return null;
      }
      // Written the way the Rust side reads it, e.g. "Control+Alt+Space".
      const parts: string[] = [];
      if (event.ctrlKey) parts.push("Control");
      if (event.altKey) parts.push("Alt");
      if (event.shiftKey) parts.push("Shift");
      if (event.metaKey) parts.push("Super");
      parts.push(main);
      return parts.join("+");
    };
    const update = (event: KeyboardEvent) => {
      if (kept !== null) return;
      const parts = combination(event);
      if (!parts) return stopHold();
      if (hold?.parts === parts) return;
      stopHold();
      if (holdMs <= 0) return save(parts);
      const start = performance.now();
      const tick = (now: number) => {
        if (!hold) return;
        const done = Math.min(1, (now - start) / holdMs);
        setProgress(done);
        if (done >= 1) {
          hold = null;
          kept = window.setTimeout(() => save(parts), SETTLE_MS);
        } else {
          hold.frame = requestAnimationFrame(tick);
        }
      };
      hold = { parts, frame: 0 };
      hold.frame = requestAnimationFrame(tick);
    };

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      if (kept !== null) return;
      const anyModifier = event.ctrlKey || event.altKey || event.shiftKey || event.metaKey;
      if (event.code === "Escape" && !anyModifier) {
        cancel();
        return;
      }
      if (!MODIFIER_CODES.includes(event.code)) others.add(event.code);
      update(event);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      // macOS sends no key up for the keys released while Cmd is held: Cmd going up ends them all.
      if (event.code.startsWith("Meta")) others.clear();
      else others.delete(event.code);
      update(event);
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    // Leaving the window must not leave the shortcuts released.
    window.addEventListener("blur", cancel);
    return () => {
      stopHold();
      if (kept !== null) window.clearTimeout(kept);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", cancel);
      invoke("resume_shortcuts").catch((err) => setError(String(err)));
    };
  }, [capturing, command, holdMs]);

  return {
    capturing,
    error,
    progress,
    start: () => {
      setError(null);
      setCapturing(true);
    },
    cancel: () => setCapturing(false),
  };
}
