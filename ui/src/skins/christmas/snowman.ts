// Elección pura de la siguiente acción del muñeco de nieve: sin DOM ni Tauri, para probarla con node:test.
export type Action = "walk" | "run" | "idle" | "wave" | "throw" | "moon" | "slip";

// Peso de cada acción en el sorteo (suman 1).
const WEIGHTS: [Action, number][] = [["walk", 0.2], ["run", 0.15], ["idle", 0.15], ["wave", 0.15], ["throw", 0.15], ["moon", 0.1], ["slip", 0.1]];
// Duración [mín, máx] en ms de las acciones quietas; throw y slip son fijas porque duran lo que su animación CSS.
const STILL: Record<string, [number, number]> = { idle: [800, 3000], wave: [1200, 2000], throw: [1200, 1200], moon: [1500, 2500], slip: [2000, 2000] };
// ms por ancho completo de la isla.
const PACE = { walk: 8000, run: 5000 };
const MIN = 0.05, MAX = 0.95, GAP = 0.15;

/** `x` y `to` son fracciones 0..1 del ancho de la isla. `prev` evita repetir una acción quieta (no reiniciaría su animación). */
export function nextAction(rand: () => number, x: number, prev?: Action): { action: Action; to: number; ms: number } {
  x = Math.min(MAX, Math.max(MIN, x));
  let r = rand(), action: Action = "walk";
  for (const [a, w] of WEIGHTS) { if (r < w) { action = a; break; } r -= w; }
  if (action === prev && action in STILL) action = "walk";
  if (action === "walk" || action === "run") {
    // Destino al azar en [MIN, x-GAP] ∪ [x+GAP, MAX], uniforme sobre ambos tramos.
    const left = Math.max(0, x - GAP - MIN), right = Math.max(0, MAX - x - GAP);
    const d = rand() * (left + right);
    const to = d < left ? MIN + d : x + GAP + (d - left);
    return { action, to, ms: Math.round(Math.abs(to - x) * PACE[action]) };
  }
  const [lo, hi] = STILL[action];
  return { action, to: x, ms: Math.round(lo + rand() * (hi - lo)) };
}
