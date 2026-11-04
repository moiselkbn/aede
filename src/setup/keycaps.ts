/** The three looks of a key in the Figma: a modifier (name and glyph), a letter (its character,
 *  centered) and the space bar (name and glyph, wider). Any other key (F13, Enter...) is drawn
 *  like a letter, with its name. */
export type KeycapKind = "modifier" | "letter" | "space";

export type Keycap = {
  kind: KeycapKind;
  /** The small word of a modifier or of the space bar. */
  name: string;
  /** What is drawn in the key: a Mac glyph, or the character of a letter. */
  glyph: string;
  /** The `KeyboardEvent.code` values that press this key (left and right for a modifier). */
  codes: string[];
};

const MODIFIERS: Record<string, Keycap> = {
  Ctrl: { kind: "modifier", name: "ctrl", glyph: "⌃", codes: ["ControlLeft", "ControlRight"] },
  Option: { kind: "modifier", name: "option", glyph: "⌥", codes: ["AltLeft", "AltRight"] },
  Maj: { kind: "modifier", name: "maj", glyph: "⇧", codes: ["ShiftLeft", "ShiftRight"] },
  Cmd: { kind: "modifier", name: "cmd", glyph: "⌘", codes: ["MetaLeft", "MetaRight"] },
};

const SPACE: Keycap = { kind: "space", name: "espace", glyph: "␣", codes: ["Space"] };

/** The caps of a shortcut label such as "Ctrl+Option+Espace", in the same order. */
export function keycaps(label: string): Keycap[] {
  return label.split("+").map((key) => {
    if (MODIFIERS[key]) return MODIFIERS[key];
    if (key === "Espace") return SPACE;
    if (/^[A-Za-z]$/.test(key)) {
      return { kind: "letter", name: "", glyph: key.toUpperCase(), codes: [`Key${key.toUpperCase()}`] };
    }
    if (/^[0-9]$/.test(key)) return { kind: "letter", name: "", glyph: key, codes: [`Digit${key}`] };
    return { kind: "letter", name: "", glyph: key.toLowerCase(), codes: [key] };
  });
}

/** The caps for the keys held right now (`KeyboardEvent.code` values): the modifiers first, in
 *  the order of the label of a shortcut, then the other keys in the order they were pressed. */
export function heldKeycaps(codes: Iterable<string>): Keycap[] {
  const modifiers = Object.values(MODIFIERS).filter((cap) => cap.codes.some((code) => hasCode(codes, code)));
  const others: Keycap[] = [];
  for (const code of codes) {
    if (Object.values(MODIFIERS).some((cap) => cap.codes.includes(code))) continue;
    if (code === "Space") others.push(SPACE);
    else if (/^Key[A-Z]$/.test(code)) others.push({ kind: "letter", name: "", glyph: code.slice(3), codes: [code] });
    else if (/^Digit[0-9]$/.test(code)) others.push({ kind: "letter", name: "", glyph: code.slice(5), codes: [code] });
    else others.push({ kind: "letter", name: "", glyph: code.toLowerCase(), codes: [code] });
  }
  return [...modifiers, ...others];
}

function hasCode(codes: Iterable<string>, wanted: string): boolean {
  for (const code of codes) if (code === wanted) return true;
  return false;
}
