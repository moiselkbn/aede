// What the user chose for the look of Aède, as sent by src-tauri/src/appearance.rs.
export type Theme = "auto" | "light" | "dark";
export type PillStyle = "black" | "fade";

export type Appearance = {
  theme: Theme;
  pill_style: PillStyle;
};
