import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** What each shortcut is told apart by: the event Rust sends and the command that turns its test on. */
const SOURCES = {
  dictation: { event: "shortcut-test", command: "set_shortcut_test" },
  learn: { event: "learn-test", command: "set_learn_test" },
} as const;

/** While `active`, the shortcut only tells this window it was pressed (the dictation does not
 *  start, nothing is learned): "recognized" stays once it has been seen.
 *
 *  Two sources say which keys are down. The window sees every key but the one that completes the
 *  shortcut: macOS keeps that one for the shortcut. So `down` holds the keys the window saw, and
 *  `pressed` is true while the whole shortcut is held, until its main key goes up. */
export function useShortcutTest(active: boolean, kind: keyof typeof SOURCES = "dictation") {
  const [pressed, setPressed] = useState(false);
  const [recognized, setRecognized] = useState(false);
  const [down, setDown] = useState<ReadonlySet<string>>(new Set());

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<boolean>(SOURCES[kind].event, (event) => {
      setPressed(event.payload);
      if (event.payload) setRecognized(true);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    invoke(SOURCES[kind].command, { on: true }).catch(() => {});

    const onKeyDown = (event: KeyboardEvent) =>
      setDown((old) => (old.has(event.code) ? old : new Set(old).add(event.code)));
    const onKeyUp = (event: KeyboardEvent) =>
      setDown((old) => {
        const next = new Set(old);
        next.delete(event.code);
        // macOS sends no key up for the keys released while Cmd is held: Cmd going up clears them.
        if (event.code.startsWith("Meta")) next.clear();
        return next.size === old.size ? old : next;
      });
    const clear = () => setDown(new Set());
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", clear);
    return () => {
      cancelled = true;
      unlisten?.();
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", clear);
      setDown(new Set());
      setPressed(false);
      invoke(SOURCES[kind].command, { on: false }).catch(() => {});
    };
  }, [active, kind]);

  const reset = useCallback(() => setRecognized(false), []);

  return { pressed, recognized, down, reset };
}
