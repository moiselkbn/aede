// How the pills of the notch come out and go back in, shared by the dictation pill
// (Indicator.tsx) and the proposal pill (LearnProposal.tsx) so that they move the same way.

// How long the opening and the closing last, in seconds.
export const OPEN_SECONDS = 0.45;
export const CLOSE_SECONDS = 0.55;

// How far a pill goes past its size when it comes out, as a share of the way travelled, for a
// curve value `overshoot`: 1.70158 (the usual value of an "ease out back" curve) gives about
// 10 %, 2.5 about 19 %, 3 about 25 %, 3.4 about 30 %. The way is short for the dictation pill
// (40 to 50 pt), so the same share shows less there than on a proposal: it gets a bit more.
export const PROPOSAL_OVERSHOOT = 3;
export const DICTATION_OVERSHOOT = 3.4;

// "Ease out back": from 0 to 1 for t from 0 to 1, going a little over 1 on the way.
export function easeOutBack(t: number, overshoot: number): number {
  const x = t - 1;
  return 1 + (overshoot + 1) * x * x * x + overshoot * x * x;
}

// A CSS `cubic-bezier(x1, y1, x2, y2)` curve: the value for a time `t` from 0 to 1.
function cubicBezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number {
  const cx = 3 * x1;
  const bx = 3 * (x2 - x1) - cx;
  const ax = 1 - cx - bx;
  const cy = 3 * y1;
  const by = 3 * (y2 - y1) - cy;
  const ay = 1 - cy - by;
  const x = (s: number) => ((ax * s + bx) * s + cx) * s;
  const y = (s: number) => ((ay * s + by) * s + cy) * s;
  const slope = (s: number) => (3 * ax * s + 2 * bx) * s + cx;
  return (t) => {
    // The curve is given as x -> y through a parameter s: find the s that has x(s) = t.
    let s = t;
    for (let i = 0; i < 8; i++) {
      const error = x(s) - t;
      if (Math.abs(error) < 1e-7 || Math.abs(slope(s)) < 1e-6) break;
      s -= error / slope(s);
    }
    if (Math.abs(x(s) - t) > 1e-5) {
      let low = 0;
      let high = 1;
      for (let i = 0; i < 40; i++) {
        s = (low + high) / 2;
        if (x(s) < t) low = s;
        else high = s;
      }
    }
    return y(s);
  };
}

// The closing: from 0 to 1, going about 20 % under 0 first (the pill grows a little), then
// slowing down as it reaches the notch. A plain "ease in back" curve did the same growth but
// ran into the notch at full speed, which looked much faster than the opening.
// Used as `open * (1 - easeCloseBack(t))`.
export const easeCloseBack = cubicBezier(0.5, -0.8, 0.2, 1);
