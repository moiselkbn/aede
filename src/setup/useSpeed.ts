import { useEffect, useRef, useState } from "react";

const WINDOW_MS = 5000;
const TICK_MS = 1000;

/** Download speed in bytes per second, averaged over the last 5 seconds from the bytes received so far.
 *  Recomputed every second, so that it falls to zero when nothing arrives any more.
 *  Null until a second has passed, and when nothing is downloading. */
export function useSpeed(bytes: number, active: boolean) {
  const latest = useRef(bytes);
  latest.current = bytes;
  const [speed, setSpeed] = useState<number | null>(null);

  useEffect(() => {
    if (!active) {
      setSpeed(null);
      return;
    }
    const samples = [{ at: performance.now(), bytes: latest.current }];
    const timer = setInterval(() => {
      const now = performance.now();
      samples.push({ at: now, bytes: latest.current });
      while (samples.length > 1 && now - samples[0].at > WINDOW_MS) samples.shift();
      const seconds = (now - samples[0].at) / 1000;
      setSpeed(Math.max(0, latest.current - samples[0].bytes) / seconds);
    }, TICK_MS);
    return () => clearInterval(timer);
  }, [active]);

  return speed;
}
