import { MutableRefObject, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { levelFromVolume } from "../wave";

/** A wave level (0 to 1) from which the microphone is hearing something: about -44 dB, a little
 *  above a quiet room and well under a voice. */
const DETECTED_LEVEL = 0.25;

/** How often the list of microphones is read again, to see one plugged in or out. */
const REFRESH_MS = 2000;

export type Microphone = { id: string; name: string };
type Microphones = { devices: Microphone[]; selected: string | null };

export type MicTest = {
  phase: "starting" | "listening" | "failed";
  error: string | null;
  /** The microphones macOS knows about, and the id of the one in use. */
  devices: Microphone[];
  selected: string | null;
  /** The user picks another microphone: it is kept for the dictations too, and the test restarts on it. */
  choose: (id: string) => void;
  /** True once a sound has been heard during this test. */
  detected: boolean;
  /** The last volume sent by Rust (RMS, 0 to 1), read by the wave at each frame. */
  volume: MutableRefObject<number>;
};

/** The microphone test (mic_test.rs): the chosen microphone is open while `active` is true, and
 *  Rust sends its volume. Nothing is recorded. */
export function useMicTest(active: boolean): MicTest {
  const volume = useRef(0);
  // Starts and stops go one after the other: the new test waits for the stop of the old one.
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const [phase, setPhase] = useState<MicTest["phase"]>("starting");
  const [error, setError] = useState<string | null>(null);
  const [devices, setDevices] = useState<Microphone[]>([]);
  // undefined until Rust has answered: the test starts on the right microphone, not twice.
  const [selected, setSelected] = useState<string | null | undefined>(undefined);
  const [detected, setDetected] = useState(false);

  // The list, now and while the screen is open. A microphone that goes away makes Rust answer
  // with another one in use: `selected` changes and the test restarts on it.
  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    const refresh = () =>
      invoke<Microphones>("list_microphones").then((answer) => {
        if (cancelled) return;
        setDevices((old) => (JSON.stringify(old) === JSON.stringify(answer.devices) ? old : answer.devices));
        setSelected(answer.selected);
      });
    refresh();
    const timer = setInterval(refresh, REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [active]);

  useEffect(() => {
    if (!active || selected === undefined) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    volume.current = 0;
    setPhase("starting");
    setError(null);
    setDetected(false);

    listen<number>("mic-test-level", (event) => {
      volume.current = event.payload;
      if (levelFromVolume(event.payload) >= DETECTED_LEVEL) setDetected(true);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    const started = queue.current.then(() => invoke("start_mic_test")).then(
      () => {
        if (!cancelled) setPhase("listening");
      },
      (err) => {
        if (cancelled) return;
        setPhase("failed");
        setError(String(err));
      },
    );

    return () => {
      cancelled = true;
      unlisten?.();
      // Stopped once the start is over, so that a screen left in a hurry does not leave the
      // microphone open, and before the next start, which would take it for a test still running.
      queue.current = started.then(() => invoke("stop_mic_test").catch(() => {}));
    };
  }, [active, selected]);

  const choose = useCallback((id: string) => {
    invoke("set_microphone", { id }).then(
      () => setSelected(id),
      (err) => setError(String(err)),
    );
  }, []);

  return { phase, error, devices, selected: selected ?? null, choose, detected, volume };
}
