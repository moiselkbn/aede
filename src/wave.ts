// The wave of the dictation pill: a row of ovals of the same width and their own height, joined
// into one smooth shape and drawn in a small canvas (DictationContent.tsx). The pill itself and
// the status line are not drawn here: they are HTML (Pill.tsx, DictationContent.tsx).
// Sizes are in points (1 CSS pixel in the indicator window).
// The settings come from the mockup tuned by the author (see docs/encoche.md).

// Size of the notch, or a notch of width 0 in the menu bar on a screen without one.
export type Spot = { width: number; height: number; hasNotch: boolean };

// Sizes of the dictation content of the pill and of its wave for one screen.
export type Layout = {
  // The content of the pill while it shows a dictation (the notch part included): the width,
  // the height with the wave, and the height with the status line, which has no room to leave
  // under the wave and is centred on its own middle line.
  openWidth: number;
  openHeight: number;
  statusHeight: number;
  statusCenter: number;
  // Wave: total width, middle line, height of an oval at silence and at most.
  waveWidth: number;
  waveCenter: number;
  minHeight: number;
  maxHeight: number;
};

// How the wave moves.
export const WAVE = {
  ovals: 17,
  // An oval at the edge shows the voice this many ms after the centre.
  travelMs: 680,
  // Ovals at the edge are this much smaller (0.9 = 90 % smaller).
  falloff: 0.9,
  // How fast the wave follows a louder voice, and falls back after it.
  attackMs: 35,
  releaseMs: 380,
};

// While transcribing, a bump crosses the wave from left to right, again and again.
const SCAN = {
  periodMs: 1200,
  // Width of the bump and its height, as a share of the wave.
  width: 0.32,
  height: 0.38,
  // Like `WAVE.falloff`, for the bump.
  falloff: 0.9,
};

// Short texts of the status line. The error codes are the failure kinds sent by
// src-tauri/src/dictation.rs.
const TEXTS: Record<string, string> = {
  done: "Reformulation validée",
  done_raw: "Texte brut collé",
  too_short: "Trop court",
  too_quiet: "Son trop faible",
  no_sound: "Micro muet",
  nothing_heard: "Réessaie",
  no_microphone: "Pas de micro",
  microphone_failed: "Micro indisponible",
  model_loading: "Modèle en chargement",
  model_missing: "Modèle manquant",
  transcription_failed: "Transcription ratée",
  no_accessibility: "Collage refusé",
  paste_failed: "Collage impossible",
  cancelled: "Annulé",
};

export type Status = { kind: "done" | "error"; text: string };

// Status line for a code: "done", "done_raw" or a failure kind. Unknown codes say "Erreur".
export function statusFor(code: string | null): Status {
  const kind = code === "done" || code === "done_raw" ? "done" : "error";
  return { kind, text: (code && TEXTS[code]) || "Erreur" };
}

// The wave is 10 % smaller than in the Figma mockup (204 pt wide, 28.5 pt high at most).
const WAVE_SCALE = 0.9;
// Room between the notch and the top of the tallest wave, and between the bottom of the tallest
// wave and the bottom of the pill (16 pt, chosen by the author; 8 pt was tried). It only
// belongs to the wave: the pill is lower when it shows the status line.
const WAVE_GAP_TOP = 6;
const WAVE_PADDING_BOTTOM = 16;

export function layoutFor(spot: Spot): Layout {
  if (spot.hasNotch) {
    const maxHeight = 28.5 * WAVE_SCALE;
    // The pill is as high as the notch, the room above the wave, the wave and the room below it.
    const openHeight = Math.ceil(spot.height + WAVE_GAP_TOP + maxHeight + WAVE_PADDING_BOTTOM);
    return {
      openWidth: 234,
      openHeight,
      // As high as the status line of the learning pill: the notch and 40 pt below it.
      statusHeight: spot.height + 40,
      statusCenter: spot.height + 20,
      waveWidth: Math.round(204 * WAVE_SCALE),
      // The middle of the tallest wave, on a half point so that the canvas stays sharp.
      waveCenter: Math.round((spot.height + WAVE_GAP_TOP + maxHeight / 2) * 2) / 2,
      minHeight: 1.1,
      maxHeight,
    };
  }
  // No notch: a smaller pill inside the menu bar, the wave in its middle. Wide enough for
  // the longest status text, narrow enough for the window (160 pt, flares included). The menu
  // bar has a fixed height: no room to add below the wave.
  const bar = spot.height;
  return {
    openWidth: 140,
    openHeight: bar,
    statusHeight: bar,
    statusCenter: bar / 2,
    waveWidth: Math.round(112 * WAVE_SCALE),
    waveCenter: bar / 2,
    minHeight: 1,
    maxHeight: Math.max(8, bar - 8) * WAVE_SCALE,
  };
}

// Height of the canvas the wave is drawn in: the tallest oval, with a little room.
export function waveCanvasHeight(layout: Layout): number {
  return Math.ceil(layout.maxHeight) + 2;
}

// Microphone volume (RMS, 0 to 1) to a wave level (0 to 1): -55 dB is silence, -12 dB loud.
export function levelFromVolume(volume: number): number {
  const db = 20 * Math.log10(volume + 1e-9);
  return Math.min(1, Math.max(0, (db + 55) / 43));
}

// Level of the transcription bump at `x` (0 = left end of the wave, 1 = right end). The bump
// starts and ends outside the wave, so it comes in and goes out smoothly.
function scanLevel(x: number, now: number): number {
  const w = SCAN.width;
  const position = -2 * w + (1 + 4 * w) * ((now % SCAN.periodMs) / SCAN.periodMs);
  return SCAN.height * Math.exp(-Math.pow((x - position) / w, 2));
}

// Height of each oval. The centre shows the voice now, an oval at the edge shows it
// `travelMs` later, so each syllable spreads from the centre to the edges.
// `scan` (0 to 1) brings in the transcription bump.
export function ovalHeights(
  levelAt: (time: number) => number,
  now: number,
  layout: Layout,
  scan: number,
): number[] {
  const middle = (WAVE.ovals - 1) / 2;
  const heights: number[] = [];
  for (let i = 0; i < WAVE.ovals; i++) {
    const distance = Math.abs(i - middle) / middle;
    const voice = levelAt(now - distance * WAVE.travelMs) * (1 - WAVE.falloff * distance);
    const bump =
      scan > 0
        ? scan * scanLevel(i / (WAVE.ovals - 1), now) * (1 - SCAN.falloff * distance)
        : 0;
    // A small shimmer so that a steady voice still looks alive.
    const shimmer = 1 + 0.1 * Math.sin(now * 0.007 + i * 1.7);
    const level = Math.min(1, Math.max(voice, bump) * shimmer);
    heights.push(layout.minHeight + (layout.maxHeight - layout.minHeight) * level);
  }
  return heights;
}

// Draws the wave in the middle of its canvas (`waveCanvasHeight` high, `layout.waveWidth` wide).
export function drawWave(
  ctx: CanvasRenderingContext2D,
  pixelRatio: number,
  layout: Layout,
  heights: number[],
) {
  const width = ctx.canvas.width / pixelRatio;
  const height = ctx.canvas.height / pixelRatio;
  ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  ctx.clearRect(0, 0, width, height);
  if (!heights.length) return;

  // The wave: its top edge from left to right, then its bottom edge back, as one shape.
  const left = (width - layout.waveWidth) / 2;
  const step = layout.waveWidth / heights.length;
  const y = height / 2;
  const xs: number[] = [];
  const radii: number[] = [];
  for (let x = left; x <= left + layout.waveWidth + 0.01; x += 0.5) {
    xs.push(x);
    radii.push(halfHeight(x - left, heights, step));
  }
  ctx.beginPath();
  xs.forEach((x, i) => (i ? ctx.lineTo(x, y - radii[i]) : ctx.moveTo(x, y - radii[i])));
  for (let i = xs.length - 1; i >= 0; i--) ctx.lineTo(xs[i], y + radii[i]);
  ctx.closePath();
  ctx.fillStyle = "#fff";
  ctx.fill();
}

// Half the height of the wave at `x` (from its left end): a smooth curve through the middle
// of each oval, with rounded ends.
function halfHeight(x: number, heights: number[], step: number): number {
  const n = heights.length;
  const u = (x - step / 2) / step; // position in ovals, 0 = middle of the first one
  if (u < 0) return (heights[0] / 2) * Math.sqrt(Math.max(0, 1 - 4 * u * u));
  if (u > n - 1) {
    const v = u - (n - 1);
    return (heights[n - 1] / 2) * Math.sqrt(Math.max(0, 1 - 4 * v * v));
  }
  const i = Math.floor(u);
  const h = (k: number) => heights[Math.min(n - 1, Math.max(0, k))];
  return Math.max(0, catmullRom(h(i - 1), h(i), h(i + 1), h(i + 2), u - i) / 2);
}

// Smooth curve through b (t = 0) and c (t = 1), bent by the points around them.
function catmullRom(a: number, b: number, c: number, d: number, t: number): number {
  return (
    0.5 *
    (2 * b +
      (-a + c) * t +
      (2 * a - 5 * b + 4 * c - d) * t * t +
      (-a + 3 * b - 3 * c + d) * t * t * t)
  );
}
