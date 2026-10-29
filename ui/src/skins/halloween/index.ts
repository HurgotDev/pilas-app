// Skin Halloween: sombrero de bruja, LEDs calabaza, murciélago, telaraña y fantasmas (CSS en halloween.css).
import type { SkinDef } from "../types.ts";

export const halloween: SkinDef = {
  id: "halloween",
  label: "set.skin.halloween",
  season: { from: [10, 15], to: [11, 2] },
  mascot: {
    // Sombrero de bruja sobre el borne; todo dentro del viewBox (y >= 0) para no recortarse en la píldora.
    accessory: `
      <ellipse cx="70" cy="30" rx="36" ry="8" fill="#2A1A4A"/>
      <path d="M48 29 L58 12 Q64 3 80 1 Q72 7 76 13 L92 29 Z" fill="#3B2566"/>
      <path d="M53.7 25 L56.6 20 L83 20 L88 25 Z" fill="#FF8A1F"/>`,
    leds: ["#FF8A1F"],
  },
  ambience: `
    <div class="bat-track"><svg class="bat" viewBox="1 2 40 18">
      <path class="wing wing-l" d="M16 12 Q9 3 2 8 Q5 11 5 16 Q9 13 11 18 Q14 17 16 19 Z"/>
      <path class="wing wing-r" d="M26 12 Q33 3 40 8 Q37 11 37 16 Q33 13 31 18 Q28 17 26 19 Z"/>
      <ellipse cx="21" cy="15" rx="5" ry="7"/>
      <path d="M17.5 9 L18 3 L20.5 8 M24.5 9 L24 3 L21.5 8"/>
      <circle cx="19" cy="13" r="1" fill="#FF8A1F"/><circle cx="23" cy="13" r="1" fill="#FF8A1F"/>
    </svg></div>
    <svg class="web" viewBox="0 0 40 40" fill="none" stroke="#fff" stroke-width=".6">
      <path d="M0 40 L6 0 M0 40 L22 4 M0 40 L36 18 M0 40 L40 34"/>
      <path d="M1.8 28 Q3.6 30.3 6.6 29.2 Q7.4 32.6 10.8 33.4 Q9.7 36.4 12 38.2 M3.6 16 Q7.1 20.6 13.2 18.4 Q14.8 25.2 21.6 26.8 Q19.4 32.9 24 36.4 M5.4 4 Q10.7 10.9 19.8 7.6 Q22.2 17.8 32.4 20.2 Q29.1 29.3 36 34.6"/>
    </svg>
    <svg class="ghost" viewBox="0 0 12 14"><path d="M1 13 V6 Q1 1 6 1 Q11 1 11 6 V13 L9.3 11.6 L7.6 13 L6 11.6 L4.4 13 L2.7 11.6 Z" fill="#fff"/><circle cx="4.4" cy="6" r="1" fill="#000"/><circle cx="7.6" cy="6" r="1" fill="#000"/></svg>
    <svg class="ghost g2" viewBox="0 0 12 14"><path d="M1 13 V6 Q1 1 6 1 Q11 1 11 6 V13 L9.3 11.6 L7.6 13 L6 11.6 L4.4 13 L2.7 11.6 Z" fill="#fff"/><circle cx="4.4" cy="6" r="1" fill="#000"/><circle cx="7.6" cy="6" r="1" fill="#000"/></svg>`,
  // Solo redefine estos cuatro; el resto suena como los clásicos.
  sounds: {
    // tritono descendente y vuelta: inquietante pero urgente (0.28 s)
    alerta: [{ f: 988, t: 0, d: 0.09 }, { f: 698, t: 0.1, d: 0.09 }, { f: 988, t: 0.2, d: 0.08 }],
    // arpegio menor ascendente, cadencia "bruja" (0.37 s)
    listo:  [{ f: 523, t: 0, d: 0.09 }, { f: 622, t: 0.09, d: 0.09 }, { f: 784, t: 0.18, d: 0.09 }, { f: 1047, t: 0.27, d: 0.1 }],
    // "buuu" fantasmal: deslizamiento descendente (0.17 s)
    toque:  [{ f: 400, t: 0, d: 0.17, type: "triangle", to: 150 }],
    // murciélagos: chillidos agudos alternando subida/bajada, en descenso (0.73 s)
    mareo:  [{ f: 2800, t: 0, d: 0.08, type: "triangle", to: 3400 }, { f: 2400, t: 0.09, d: 0.08, type: "triangle", to: 1900 }, { f: 2600, t: 0.18, d: 0.08, type: "triangle", to: 3100 }, { f: 2200, t: 0.27, d: 0.08, type: "triangle", to: 1700 }, { f: 2400, t: 0.36, d: 0.08, type: "triangle", to: 2800 }, { f: 2000, t: 0.45, d: 0.08, type: "triangle", to: 1600 }, { f: 2200, t: 0.54, d: 0.08, type: "triangle", to: 2500 }, { f: 1800, t: 0.63, d: 0.1, type: "triangle", to: 1400 }],
  },
};
