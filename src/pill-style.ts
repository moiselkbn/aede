// The look of the pill, the same for everything it shows (dictation, proposal, result).
// The user picks it in Settings (Apparence); this is the one used until they do:
//   "fade":  black at the top, almost see-through at the bottom, with a soft light on the edges
//            (the design of the proposal pill, variant C in the Figma file);
//   "black": plain black, no light on the edges (the first design of the dictation pill).
// With "fade" the wave and the status text sit on a part of the pill that lets the screen show
// through: on a light wallpaper they are less readable than on "black".
export type { PillStyle } from "./appearance";
export const DEFAULT_PILL_STYLE = "fade" as const;
