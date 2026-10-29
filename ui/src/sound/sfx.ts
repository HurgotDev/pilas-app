// Sonidos 8-bit sintetizados con Web Audio: sin archivos (docs/SPEC.md → Sonidos).
export type Sound = "alerta" | "recordatorio" | "listo" | "aprobado" | "denegado" | "toque" | "mareo";

interface Note { f: number; t: number; d: number; type?: OscillatorType; to?: number }

const SOUNDS: Record<Sound, Note[]> = {
  alerta:       [{ f: 880, t: 0, d: 0.11 }, { f: 1320, t: 0.12, d: 0.13 }],
  recordatorio: [{ f: 660, t: 0, d: 0.09, type: "triangle" }, { f: 990, t: 0.1, d: 0.1, type: "triangle" }],
  listo:        [{ f: 784, t: 0, d: 0.1 }, { f: 988, t: 0.1, d: 0.1 }, { f: 1319, t: 0.2, d: 0.16 }],
  aprobado:     [{ f: 1568, t: 0, d: 0.07 }],
  denegado:     [{ f: 330, t: 0, d: 0.12, to: 196 }],
  toque:        [{ f: 520, t: 0, d: 0.15, type: "triangle", to: 260 }],
  mareo:        [{ f: 1320, t: 0, d: 0.08, type: "triangle" }, { f: 990, t: 0.09, d: 0.08, type: "triangle" }, { f: 1245, t: 0.18, d: 0.08, type: "triangle" }, { f: 932, t: 0.27, d: 0.08, type: "triangle" }, { f: 1175, t: 0.36, d: 0.08, type: "triangle" }, { f: 880, t: 0.45, d: 0.08, type: "triangle" }, { f: 1109, t: 0.54, d: 0.08, type: "triangle" }, { f: 831, t: 0.63, d: 0.1, type: "triangle" }],
};

let ctx: AudioContext | null = null;
let volume = 0.5;
let enabled = true;

export function configure(opts: { enabled?: boolean; volume?: number }): void {
  if (opts.enabled !== undefined) enabled = opts.enabled;
  if (opts.volume !== undefined) volume = Math.min(1, Math.max(0, opts.volume));
}

export function play(name: Sound): void {
  if (!enabled || volume === 0) return;
  ctx ??= new AudioContext();
  const now = ctx.currentTime;
  for (const n of SOUNDS[name]) {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = n.type ?? "square";
    osc.frequency.setValueAtTime(n.f, now + n.t);
    if (n.to) osc.frequency.exponentialRampToValueAtTime(n.to, now + n.t + n.d);
    const peak = 0.12 * volume; // la onda cuadrada es fuerte: volumen base bajo
    gain.gain.setValueAtTime(0, now + n.t);
    gain.gain.linearRampToValueAtTime(peak, now + n.t + 0.005);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + n.t + n.d);
    osc.connect(gain).connect(ctx.destination);
    osc.start(now + n.t);
    osc.stop(now + n.t + n.d + 0.02);
  }
}
