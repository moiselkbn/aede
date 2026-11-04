import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Monitor, Moon, Sun } from "lucide-react";
import SegmentedControl from "@/components/arc/segmented-control/segmented-control";
import SettingsRow, { SettingsSection } from "./SettingsRow";
import type { Appearance, PillStyle, Theme } from "./appearance";
import { DEFAULT_PILL_STYLE } from "./pill-style";

// Icons only: the name is kept for screen readers, in a text that is not drawn.
const hidden = { position: "absolute", width: 1, height: 1, overflow: "hidden", clip: "rect(0 0 0 0)" } as const;
const themeOption = (value: Theme, name: string, Icon: typeof Sun) => ({
  value,
  label: "",
  accessory: (
    <>
      <Icon size={16} aria-hidden="true" />
      <span style={hidden}>{name}</span>
    </>
  ),
});

const THEMES = [
  themeOption("auto", "Automatique", Monitor),
  themeOption("light", "Clair", Sun),
  themeOption("dark", "Sombre", Moon),
];

const PILL_STYLES: { value: PillStyle; label: string }[] = [
  { value: "fade", label: "Dégradé" },
  { value: "black", label: "Noir plein" },
];

/** The "Apparence" block of the settings: the theme of the window and the style of the notch pill. */
function AppearanceSettings() {
  const [appearance, setAppearance] = useState<Appearance>({
    theme: "auto",
    pill_style: DEFAULT_PILL_STYLE,
  });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<Appearance>("get_appearance").then(setAppearance).catch((err) => setError(String(err)));
    const listening = listen<Appearance>("appearance-changed", (event) => setAppearance(event.payload));
    return () => {
      void listening.then((unlisten) => unlisten());
    };
  }, []);

  /** Shows the choice at once, then asks Rust to keep it; a failure puts the old choice back. */
  function choose(change: Partial<Appearance>, command: string, args: Record<string, string>) {
    const before = appearance;
    setError(null);
    setAppearance({ ...appearance, ...change });
    invoke(command, args).catch((err) => {
      setAppearance(before);
      setError(String(err));
    });
  }

  return (
    <SettingsSection title="Apparence">
      <SettingsRow
        title="Thème"
        detail="Automatique suit le mode clair ou sombre du Mac."
        error={error}
      >
        <SegmentedControl
          label="Thème"
          className="icon-segments"
          options={THEMES}
          value={appearance.theme}
          onValueChange={(value) => choose({ theme: value as Theme }, "set_theme", { theme: value })}
        />
      </SettingsRow>
      <SettingsRow
        title="Pastille de l'encoche"
        detail="Le dégradé laisse voir l'écran en bas. Le noir plein se lit mieux sur un fond clair."
      >
        <SegmentedControl
          label="Pastille de l'encoche"
          options={PILL_STYLES}
          value={appearance.pill_style}
          onValueChange={(value) =>
            choose({ pill_style: value as PillStyle }, "set_pill_style", { style: value })
          }
        />
      </SettingsRow>
    </SettingsSection>
  );
}

export default AppearanceSettings;
