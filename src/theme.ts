// Puts data-theme="dark" on <html> when the window should be dark, and follows what changes it:
// the Mac going light or dark (when the choice is "auto") and the choice made in Settings.
// Arc's tokens read this attribute (src/components/arc/foundation.css).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Appearance, Theme } from "./appearance";

const query = window.matchMedia("(prefers-color-scheme: dark)");
let chosen: Theme = "auto";

function apply() {
  const dark = chosen === "dark" || (chosen === "auto" && query.matches);
  if (dark) document.documentElement.dataset.theme = "dark";
  else delete document.documentElement.dataset.theme;
}

apply();
query.addEventListener("change", apply);

function choose(appearance: Appearance) {
  chosen = appearance.theme;
  apply();
}

// A failure here leaves the window following the Mac, which is the default anyway.
invoke<Appearance>("get_appearance").then(choose).catch(() => {});
listen<Appearance>("appearance-changed", (event) => choose(event.payload));
