// Skin Navidad: gorro de Papá Noel, LEDs rojo/verde, nieve y muñeco troll (CSS en christmas.css).
import type { SkinDef } from "../types.ts";
import { nextAction, type Action } from "./snowman.ts";

// Copos: left, --s (tamaño), duración y retraso; el retraso negativo ya los deja cayendo al cargar.
const FLAKES: [number, number, number, number][] = [
  [4, 3, 8, -1], [11, 4, 11, -4], [18, 5, 9, -7], [25, 3, 13, -10], [32, 4, 10, -2], [39, 5, 12, -5], [46, 3, 8, -8],
  [53, 4, 14, -11], [60, 5, 9, -3], [67, 3, 11, -6], [74, 4, 10, -9], [81, 5, 13, -12], [88, 3, 8, -4.5], [95, 4, 12, -7.5],
];

// Muñeco de nieve: un único setTimeout encadenado; la pose la pinta el CSS según data-act y data-dir.
let timer = 0, x = Math.random(), prev: Action | undefined;
function step(track: HTMLElement): void {
  const { action, to, ms } = nextAction(Math.random, x, prev);
  if (to !== x) track.dataset.dir = to < x ? "l" : "r";
  track.dataset.act = action;
  // Solo walk/run cambian left, así que la transición solo se nota al moverse.
  track.style.transitionDuration = `${ms}ms`;
  // Fracción del ancho libre (sin la pose), así la pose nunca se sale por ningún borde.
  track.style.left = `calc(${to} * (100% - var(--sm)))`;
  x = to; prev = action;
  timer = window.setTimeout(() => step(track), ms);
}

export const christmas: SkinDef = {
  id: "christmas",
  label: "set.skin.christmas",
  season: { from: [12, 1], to: [12, 26] },
  mascot: {
    // Gorro de Papá Noel sobre el borne; cabe en la misma caja que el sombrero de bruja (y 1..38).
    accessory: `
      <path d="M50 28 Q52 6 74 4 Q92 3 100 18 L95 21 Q88 12 82 13 Q88 20 90 28 Z" fill="#D7263D"/>
      <path d="M74 4 Q92 3 100 18 L97 19.5 Q90 9 76 8 Z" fill="#A81B30"/>
      <rect x="46" y="24" width="48" height="12" rx="6" fill="#F4F4F8"/>
      <circle cx="99" cy="19" r="5.5" fill="#F4F4F8"/>`,
    // Luces alternas: LEDs 0 y 2 rojos, 1 y 3 verdes.
    leds: ["#FF4D5E", "#5CFF8A"],
  },
  ambience: `
    ${FLAKES.map(([left, s, dur, delay]) => `<i class="flake" style="left:${left}%;--s:${s}px;animation-duration:${dur}s;animation-delay:${delay}s"></i>`).join("")}
    <div class="snow-track" data-act="idle"><svg class="snowman" viewBox="0 0 12 12">
      <g class="sm-body">
        <path class="leg" d="M5 9.8V12"/><path class="leg leg-b" d="M7 9.8V12"/>
        <path class="arm" d="M3.6 6.6L1.4 5.6"/><path class="arm arm-r" d="M8.4 6.6L10.6 5.6"/>
        <g class="sm-front">
          <circle cx="6" cy="7.4" r="2.6" fill="#fff"/>
          <circle cx="6" cy="2.6" r="2" fill="#fff"/>
          <circle cx="5.3" cy="2.3" r=".4" fill="#000"/><circle cx="6.7" cy="2.3" r=".4" fill="#000"/>
          <path d="M6.6 2.8L9 3.2L6.6 3.6Z" fill="#FF8A1F"/>
          <path d="M3.4 4.7H8.6V5.7H3.4Z" fill="#D7263D"/>
          <path d="M7.6 5.7H8.6L8.9 7.4H8.2Z" fill="#D7263D"/>
        </g>
        <g class="sm-back">
          <circle cx="6" cy="7.4" r="2.6" fill="#fff"/>
          <circle cx="6" cy="2.6" r="2" fill="#fff"/>
          <path d="M3.4 4.7H8.6V5.7H3.4Z" fill="#D7263D"/>
          <circle cx="5.1" cy="9.2" r="1.1" fill="#fff" stroke="#C9DAEA" stroke-width=".3"/>
          <circle cx="6.9" cy="9.2" r="1.1" fill="#fff" stroke="#C9DAEA" stroke-width=".3"/>
        </g>
      </g>
      <path class="dizzy" d="M-5 4.9L-4.5 6L-3.4 6.5L-4.5 7L-5 8.1L-5.5 7L-6.6 6.5L-5.5 6Z" fill="#FFD23F"/>
      <circle class="ball" cx="10.6" cy="5.6" r=".9" fill="#fff"/>
    </svg></div>`,
  // Cascabeles en los mismos cuatro; el resto suena como los clásicos.
  sounds: {
    // "jin-gle-bells": tres Mi agudos seguidos (0.27 s)
    alerta: [{ f: 1319, t: 0, d: 0.07 }, { f: 1319, t: 0.09, d: 0.07 }, { f: 1319, t: 0.18, d: 0.09 }],
    // remate del estribillo Mi-Sol-Do-Re-Mi (0.40 s)
    listo:  [{ f: 1319, t: 0, d: 0.07 }, { f: 1568, t: 0.08, d: 0.07 }, { f: 1047, t: 0.16, d: 0.07 }, { f: 1175, t: 0.24, d: 0.07 }, { f: 1319, t: 0.32, d: 0.08 }],
    // cascabel: dos golpes agudos (0.16 s)
    toque:  [{ f: 1568, t: 0, d: 0.06, type: "triangle" }, { f: 2093, t: 0.07, d: 0.09, type: "triangle" }],
    // trineo: cascabeles agudos que alternan y bajan (0.73 s)
    mareo:  [{ f: 2637, t: 0, d: 0.08, type: "triangle" }, { f: 2093, t: 0.09, d: 0.08, type: "triangle" }, { f: 2349, t: 0.18, d: 0.08, type: "triangle" }, { f: 1976, t: 0.27, d: 0.08, type: "triangle" }, { f: 2093, t: 0.36, d: 0.08, type: "triangle" }, { f: 1760, t: 0.45, d: 0.08, type: "triangle" }, { f: 1976, t: 0.54, d: 0.08, type: "triangle" }, { f: 1568, t: 0.63, d: 0.1, type: "triangle" }],
  },
  behavior: {
    start(layer) {
      const track = layer.querySelector<HTMLElement>(".snow-track");
      window.clearTimeout(timer);
      if (track) step(track);
    },
    stop() {
      window.clearTimeout(timer);
    },
  },
};
