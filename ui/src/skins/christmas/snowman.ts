// Pure choice of the snowman's next action: no DOM or Tauri, so node:test can run it.
export type Action = "walk" | "run" | "idle" | "wave" | "throw" | "moon" | "slip";

// Draw weight per action (they sum to 1).
const WEIGHTS: [Action, number][] = [["walk", 0.2], ["run", 0.15], ["idle", 0.15], ["wave", 0.15], ["throw", 0.15], ["moon", 0.1], ["slip", 0.1]];
// [min, max] ms of still actions; throw and slip are fixed because they last as long as their CSS animation.
const STILL: Record<string, [number, number]> = { idle: [800, 3000], wave: [1200, 2000], throw: [1200, 1200], moon: [1500, 2500], slip: [2000, 2000] };
// ms per full island width.
const PACE = { walk: 8000, run: 5000 };
const MIN = 0.05, MAX = 0.95, GAP = 0.15;

/** `x` and `to` are 0..1 fractions of the island width. `prev` avoids repeating a still action (its animation would not restart). */
export function nextAction(rand: () => number, x: number, prev?: Action): { action: Action; to: number; ms: number } {
  x = Math.min(MAX, Math.max(MIN, x));
  let r = rand(), action: Action = "walk";
  for (const [a, w] of WEIGHTS) { if (r < w) { action = a; break; } r -= w; }
  if (action === prev && action in STILL) action = "walk";
  if (action === "walk" || action === "run") {
    // Random target in [MIN, x-GAP] ∪ [x+GAP, MAX], uniform over both ranges.
    const left = Math.max(0, x - GAP - MIN), right = Math.max(0, MAX - x - GAP);
    const d = rand() * (left + right);
    const to = d < left ? MIN + d : x + GAP + (d - left);
    return { action, to, ms: Math.round(Math.abs(to - x) * PACE[action]) };
  }
  const [lo, hi] = STILL[action];
  return { action, to: x, ms: Math.round(lo + rand() * (hi - lo)) };
}
