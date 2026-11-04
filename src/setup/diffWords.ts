/** Compares what Whisper heard with the final text, word by word, for the animation of the
 *  dictation trial (rewriteAnimation.ts). Case, accents and punctuation do not count: "j'ai"
 *  and "J'ai", "matin," and "matin." are the same word, which only changes its look. */
export type Cell =
  | { kind: "kept"; raw: string; final: string }
  | { kind: "removed"; raw: string }
  | { kind: "added"; final: string };

export function splitWords(text: string): string[] {
  return text.split(/\s+/).filter(Boolean);
}

// A word reduced to its letters and digits, without accents or case.
const key = (word: string) => word.toLowerCase().normalize("NFD").replace(/[^\p{L}\p{N}]+/gu, "");

/** The two texts as one list of cells, in reading order: words kept (maybe rewritten), words
 *  only in the raw text (removed) and words only in the final text (added). Null if the texts
 *  are too long to compare cheaply. */
export function alignWords(raw: string[], final: string[]): Cell[] | null {
  const n = raw.length;
  const m = final.length;
  if (n * m > 250_000) return null;
  const a = raw.map(key);
  const b = final.map(key);

  // Length of the longest common run of words from (i, j) to the end.
  const width = m + 1;
  const table = new Uint16Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i * width + j] =
        a[i] !== "" && a[i] === b[j]
          ? table[(i + 1) * width + j + 1] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }

  const cells: Cell[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] !== "" && a[i] === b[j]) {
      cells.push({ kind: "kept", raw: raw[i], final: final[j] });
      i++;
      j++;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      cells.push({ kind: "removed", raw: raw[i++] });
    } else {
      cells.push({ kind: "added", final: final[j++] });
    }
  }
  while (i < n) cells.push({ kind: "removed", raw: raw[i++] });
  while (j < m) cells.push({ kind: "added", final: final[j++] });
  return cells;
}

/** Share of the words that stay (0 to 1), the larger of the two texts as the base: a low value
 *  means the text was rebuilt, and the word by word animation would only be noise. */
export function keptShare(cells: Cell[], rawCount: number, finalCount: number): number {
  const kept = cells.filter((cell) => cell.kind === "kept").length;
  return kept / Math.max(1, rawCount, finalCount);
}
