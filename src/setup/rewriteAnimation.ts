/** The text of the dictation trial, as animated in the window (Figma screen 10j).
 *  Once Whisper is done, its text appears in grey italics
 *  under a sweep; a soft light passes over it while the rewriting goes on; when the final text
 *  is known, the words are kept (and turn upright), removed or added one by one, the caret
 *  following. Only transforms and opacities move, played by the browser (Web Animations). */
import { alignWords, type Cell, keptShare, splitWords } from "./diffWords";

/** Timings in ms, the minimums of the prototype: each step lasts at least this long, even when
 *  the real work is faster. The word by word pass has no minimum: it goes at a fixed pace, so a
 *  short sentence is not slower per word than a long one. */
const T = {
  sweep: 900,
  pauseAfterSweep: 150,
  sheen: 1500,
  sheenPause: 300,
  // The pace of the prototype (6 s for its 21 words). A very long text speeds up so that the
  // pass stays under `maxRewrite`, but never below `minStep` per word.
  wordStep: 285,
  maxRewrite: 12000,
  minStep: 60,
  fadeGone: 180,
  flip: 300,
  endHold: 800,
};

/** Below this share of kept words the text was rebuilt: no word by word animation. */
const MIN_KEPT = 0.35;

const SHEEN_WIDTH = 140;
const SWEEP_EDGE = 48;
const EASE = "cubic-bezier(.2,.8,.2,1)";

export type RewriteHooks = {
  /** The height the text needs, to let the field follow it. */
  onHeight: (px: number) => void;
  /** The animation is over (or was skipped): the real text of the field can be shown. */
  onDone: () => void;
  /** A word is about to be kept, removed or added (the welcome screen lights its labels). */
  onStep?: (cell: Cell) => void;
  reduced: boolean;
};

export type RewriteHandle = {
  setResult: (text: string) => void;
  cancel: () => void;
};

function make(tag: string, className: string): HTMLElement {
  const element = document.createElement(tag);
  element.className = className;
  return element;
}

export function startRewrite(box: HTMLElement, raw: string, hooks: RewriteHooks): RewriteHandle {
  let alive = true;
  const timers = new Set<number>();
  const wait = (ms: number) =>
    new Promise<void>((resolve) => {
      const id = window.setTimeout(() => {
        timers.delete(id);
        resolve();
      }, ms);
      timers.add(id);
    });
  let result: string | null = null;
  let resolveResult: () => void = () => {};
  // Only the reduced motion path waits on it: the others look at `result` between two passes.
  const resultReady = new Promise<void>((resolve) => (resolveResult = resolve));

  box.replaceChildren();
  const inner = make("div", "rw-inner");
  const textEl = make("div", "rw-text");
  const caret = make("div", "rw-caret");
  caret.style.visibility = "hidden";
  inner.append(textEl, caret);
  box.append(inner);

  const rawWords = splitWords(raw);
  const words = rawWords.map((word) => {
    const element = make("span", "rw-w raw");
    element.textContent = word;
    return element;
  });
  textEl.append(...words);

  const lineHeight = parseFloat(getComputedStyle(textEl).lineHeight) || 22;
  const fit = (px = textEl.getBoundingClientRect().height) => hooks.onHeight(Math.ceil(px));
  const rel = (element: HTMLElement) => {
    const a = element.getBoundingClientRect();
    const b = inner.getBoundingClientRect();
    return { x: a.left - b.left, y: a.top - b.top, r: a.right - b.left };
  };
  const setCaret = (x: number, y: number, ms: number) => {
    caret.style.transition = ms ? `transform ${ms}ms ${EASE}` : "none";
    caret.style.transform = `translate(${x}px, ${y}px)`;
  };

  /** Runs `change`, then slides every word still shown from where it was to where it is. */
  const flip = (els: HTMLElement[], change: () => void) => {
    const shown = () => els.filter((element) => element.style.display !== "none");
    const before = new Map(shown().map((element) => [element, rel(element)]));
    change();
    fit();
    const after = new Map(shown().map((element) => [element, rel(element)]));
    after.forEach((now, element) => {
      const was = before.get(element);
      if (was && (was.x !== now.x || was.y !== now.y)) {
        element.animate(
          [{ transform: `translate(${was.x - now.x}px, ${was.y - now.y}px)` }, { transform: "none" }],
          { duration: T.flip, easing: EASE },
        );
      }
    });
    return after;
  };

  async function run() {
    // The lines of the text and where each one ends.
    fit();
    const rows: { y: number; r: number }[] = [];
    words.forEach((word) => {
      const spot = rel(word);
      const row = rows.find((candidate) => Math.abs(candidate.y - spot.y) < 2);
      if (row) row.r = Math.max(row.r, spot.r);
      else rows.push({ y: spot.y, r: spot.r });
    });
    rows.sort((a, b) => a.y - b.y);
    const total = rows.reduce((sum, row) => sum + row.r, 0) || 1;

    if (hooks.reduced || rows.length === 0) {
      await resultReady;
      if (alive) hooks.onDone();
      return;
    }

    // The sweep: the heard text is uncovered from left to right, line after line, and the field
    // grows when the sweep reaches a new line.
    const width = inner.getBoundingClientRect().width;
    let delay = 0;
    const sweeps = rows.map((row, index) => {
      const sweep = make("div", "rw-sweep");
      sweep.style.top = `${row.y}px`;
      sweep.style.height = `${lineHeight}px`;
      sweep.style.width = `${width + 60}px`;
      sweep.style.transform = `translateX(-${SWEEP_EDGE}px)`;
      inner.append(sweep);
      const duration = (T.sweep * row.r) / total;
      sweep.animate(
        [{ transform: `translateX(-${SWEEP_EDGE}px)` }, { transform: `translateX(${row.r}px)` }],
        { duration, delay, easing: "linear", fill: "forwards" },
      );
      if (index > 0) window.setTimeout(() => alive && fit(row.y + lineHeight), delay);
      delay += duration;
      return sweep;
    });
    fit(rows[0].y + lineHeight);
    await wait(T.sweep + 50);
    if (!alive) return;
    sweeps.forEach((sweep) => sweep.remove());
    await wait(T.pauseAfterSweep);
    if (!alive) return;

    // The rewriting goes on: a soft light passes over the text, pass after pass, until the final
    // text is known. A pass is never cut short, and there is always at least one.
    do {
      const speed = (total + SHEEN_WIDTH) / T.sheen;
      let start = 0;
      const sheens = rows.map((row) => {
        const sheen = make("div", "rw-sheen");
        sheen.style.top = `${row.y}px`;
        sheen.style.height = `${lineHeight}px`;
        sheen.style.opacity = "0";
        inner.append(sheen);
        sheen.animate(
          [
            { transform: `translateX(-${SHEEN_WIDTH}px)`, opacity: 1 },
            { transform: `translateX(${row.r}px)`, opacity: 1 },
          ],
          { duration: (row.r + SHEEN_WIDTH) / speed, delay: start, easing: "linear", fill: "both" },
        );
        start += row.r / speed;
        return sheen;
      });
      await wait(T.sheen);
      sheens.forEach((sheen) => sheen.remove());
      if (!alive) return;
      await wait(T.sheenPause);
    } while (alive && result === null);
    if (!alive) return;

    // The final text is known: compare it with what was heard.
    const final = splitWords(result ?? "");
    const cells = alignWords(rawWords, final);
    if (!cells || /\n/.test(result ?? "") || keptShare(cells, rawWords.length, final.length) < MIN_KEPT) {
      hooks.onDone();
      return;
    }
    const els: HTMLElement[] = [];
    let next = 0;
    for (const cell of cells) {
      if (cell.kind === "added") {
        const added = make("span", "rw-w");
        added.style.display = "none";
        els.push(added);
      } else {
        els.push(words[next++]);
      }
    }
    textEl.replaceChildren(...els);

    caret.style.visibility = "";
    setCaret(0, 3, 0);
    const step = Math.max(T.minStep, Math.min(T.wordStep, T.maxRewrite / cells.length));
    for (let i = 0; i < cells.length; i++) {
      const cell = cells[i];
      const element = els[i];
      hooks.onStep?.(cell);
      if (cell.kind === "removed") {
        element.classList.add("gone");
        await wait(T.fadeGone);
        if (!alive) return;
        flip(els, () => (element.style.display = "none"));
        await wait(Math.max(0, step - T.fadeGone));
      } else {
        const after = flip(els, () => {
          element.style.display = "";
          element.textContent = cell.final;
          element.className = "rw-w kept";
        });
        if (cell.kind === "added") element.animate([{ opacity: 0 }, { opacity: 1 }], { duration: T.fadeGone });
        const spot = after.get(element);
        if (spot) setCaret(spot.r + 1, spot.y + 3, Math.min(200, step * 1.6));
        await wait(step);
      }
      if (!alive) return;
    }
    const last = [...els].reverse().find((element) => element.style.display !== "none");
    if (last) {
      const spot = rel(last);
      setCaret(spot.r + 1, spot.y + 3, 200);
    }
    await wait(T.endHold);
    if (alive) hooks.onDone();
  }
  void run();

  return {
    setResult(text) {
      result = text;
      resolveResult();
    },
    cancel() {
      alive = false;
      timers.forEach((id) => window.clearTimeout(id));
      timers.clear();
      box.getAnimations({ subtree: true }).forEach((animation) => animation.cancel());
      box.replaceChildren();
    },
  };
}
