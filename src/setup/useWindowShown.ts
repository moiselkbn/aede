import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** Whether the window can be seen. The close button only hides it (tray.rs), so the screens inside
 *  stay mounted: this is what tells them to let go of the microphone and the shortcuts. */
export function useWindowShown(): boolean {
  const [shown, setShown] = useState(true);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<boolean>("main-window-visible", (event) => setShown(event.payload)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  return shown;
}
