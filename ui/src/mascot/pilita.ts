/**
 * <pilas-mascot> — la pilita de Pilas.
 *
 * Uso:
 *   <pilas-mascot state="cargando" style="height: 28px"></pilas-mascot>
 *   el.state = "alerta";            // dormida | cargando | alerta | llena
 *   el.skin = "classic";            // cualquier id del registro de skins (ui/src/skins)
 *   el.setAttribute("cache", "fria"); // caliente | enfriandose | fria; solo se ve en "dormida"
 *   el.react("saludo");             // saludo | aplastada | mareo
 *   el.addEventListener("pilita-click", (e) => abrirTarjeta(e.detail.reaction));
 *       // detail.reaction: "mareo" | "aplastada" | null (null = ya estaba mareada)
 *
 * Interacción incluida: hover → saludo, clic → aplastada, 3 clics en < 1 s → mareo.
 * En "alerta" no hay reacciones: el clic solo emite "pilita-click".
 * Se dibuja en SVG dentro de Shadow DOM, sin dependencias. Respeta prefers-reduced-motion.
 */

import { SKINS, type Skin } from "../skins/index.ts";

export type PilitaState = "dormida" | "cargando" | "alerta" | "llena";
export type PilitaReaction = "saludo" | "aplastada" | "mareo";
export type PilitaCache = "caliente" | "enfriandose" | "fria";

type Eyes = "abiertos" | "cerrados" | "felices" | "susto" | "apretados" | "enojados" | "espiral";
type Mouth = "gato" | "neutra" | "o" | "abierta" | "ondas" | "puchero";
type Leds = "0" | "2" | "4" | "ciclo";

interface Pose {
  eyes: Eyes;
  mouth: Mouth;
  leds: Leds;
  blush: boolean;
  extras: Array<"zzz" | "exclamacion" | "destellos" | "brazo">;
}

const STATE_POSE: Record<PilitaState, Pose> = {
  dormida: { eyes: "cerrados", mouth: "neutra", leds: "0", blush: false, extras: ["zzz"] },
  cargando: { eyes: "abiertos", mouth: "gato", leds: "ciclo", blush: true, extras: [] },
  alerta: { eyes: "susto", mouth: "o", leds: "0", blush: false, extras: ["exclamacion"] },
  llena: { eyes: "felices", mouth: "abierta", leds: "4", blush: true, extras: ["destellos"] },
};

const REACTION_MS: Record<PilitaReaction, number> = { saludo: 1100, aplastada: 900, mareo: 2000 };
const STATES: PilitaState[] = ["dormida", "cargando", "alerta", "llena"];
const CACHES: PilitaCache[] = ["caliente", "enfriandose", "fria"];

const INK = "#1B1B2E";

const TEMPLATE = /* html */ `
<style>
  :host { display: inline-block; aspect-ratio: 144 / 156; height: 96px; line-height: 0; cursor: pointer;
          -webkit-tap-highlight-color: transparent; }
  svg { width: 100%; height: 100%; overflow: visible; }

  /* Paleta por estado: cuerpo, sombra, oscuro, patas */
  svg { --led:#FFE066; --body:#4F7DF3; --shade:#3B63D6; --dark:#2A47A8; --foot:#22398A; }
  svg[data-state="dormida"] { --body:#5B6584; --shade:#4A5370; --dark:#3A4159; --foot:#2E3448; }
  svg[data-state="alerta"]  { --body:#EF4B5F; --shade:#D23349; --dark:#9E1F33; --foot:#8A1A2C; }
  svg[data-state="llena"]   { --body:#34C46A; --shade:#24A554; --dark:#1C7A43; --foot:#17643A; }
  .c-body  { fill: var(--body);  transition: fill .4s ease; }
  .c-shade { fill: var(--shade); transition: fill .4s ease; }
  .c-dark  { fill: var(--dark);  transition: fill .4s ease; }
  .c-foot  { fill: var(--foot);  transition: fill .4s ease; }
  .s-body  { stroke: var(--body); transition: stroke .4s ease; }

  /* Variantes: solo se ve la que pide la pose */
  .v { display: none; }
  svg[data-eyes="abiertos"]  .e-abiertos,
  svg[data-eyes="cerrados"]  .e-cerrados,
  svg[data-eyes="felices"]   .e-felices,
  svg[data-eyes="susto"]     .e-susto,
  svg[data-eyes="apretados"] .e-apretados,
  svg[data-eyes="enojados"]  .e-enojados,
  svg[data-eyes="espiral"]   .e-espiral,
  svg[data-mouth="gato"]     .m-gato,
  svg[data-mouth="neutra"]   .m-neutra,
  svg[data-mouth="o"]        .m-o,
  svg[data-mouth="abierta"]  .m-abierta,
  svg[data-mouth="ondas"]    .m-ondas,
  svg[data-mouth="puchero"]  .m-puchero,
  svg[data-blush="true"]     .blush,
  svg[data-zzz]              .x-zzz,
  svg[data-exclamacion]      .x-exclamacion,
  svg[data-destellos]        .x-destellos,
  svg[data-brazo]            .x-brazo,
  svg[data-cache="caliente"] .k-vapor,
  svg[data-cache="enfriandose"] .k-copo,
  svg[data-cache="fria"]     .k-copo { display: inline; }

  /* LEDs */
  .lit { fill: var(--led); opacity: 0; transition: opacity .3s ease; }
  svg[data-leds="2"] .lit-0, svg[data-leds="2"] .lit-1, svg[data-leds="4"] .lit { opacity: 1; }
  svg[data-leds="ciclo"] .lit-0 { animation: led0 2.4s infinite; }
  svg[data-leds="ciclo"] .lit-1 { animation: led1 2.4s infinite; }
  svg[data-leds="ciclo"] .lit-2 { animation: led2 2.4s infinite; }
  svg[data-leds="ciclo"] .lit-3 { animation: led3 2.4s infinite; }
  @keyframes led0 { 0%{opacity:0} 8%,88%{opacity:1} 100%{opacity:0} }
  @keyframes led1 { 0%,20%{opacity:0} 28%,88%{opacity:1} 100%{opacity:0} }
  @keyframes led2 { 0%,40%{opacity:0} 48%,88%{opacity:1} 100%{opacity:0} }
  @keyframes led3 { 0%,60%{opacity:0} 68%,88%{opacity:1} 100%{opacity:0} }

  /* Movimiento del cuerpo (el "rig") por estado */
  .rig { transform-box: view-box; transform-origin: 70px 150px; }
  svg[data-state="dormida"]  .rig { animation: breathe 3s ease-in-out infinite; }
  svg[data-state="cargando"] .rig { animation: bob 1.4s ease-in-out infinite; }
  svg[data-state="alerta"]   .rig { animation: shake .45s ease-in-out infinite; }
  svg[data-state="llena"]    .rig { animation: hop 1.8s ease-in-out infinite; }
  @keyframes breathe { 50% { transform: scale(1.03, .97); } }
  @keyframes bob     { 50% { transform: translateY(-4px); } }
  @keyframes shake   { 0%,100%{transform:translate(0,0)} 25%{transform:translate(-2px,-4px)} 75%{transform:translate(2px,-1px)} }
  @keyframes hop     { 0%,55%,100%{transform:translateY(0)} 20%{transform:translateY(-10px)}
                       35%{transform:translateY(0)} 45%{transform:translateY(-4px)} }

  /* Reacciones: sobrescriben el movimiento del estado */
  svg[data-react="saludo"]    .rig { animation: peek 1.1s cubic-bezier(.3,1.5,.5,1) 1; }
  svg[data-react="aplastada"] .rig { animation: squish .9s cubic-bezier(.3,1.6,.5,1) 1; }
  svg[data-react="mareo"]     .rig { animation: sway .5s ease-in-out infinite; transform-origin: 70px 140px; }
  @keyframes peek   { 0%{transform:translateY(28px)} 35%{transform:translateY(-6px)} 55%,100%{transform:translateY(0)} }
  @keyframes squish { 0%{transform:scale(1,1)} 15%{transform:scale(1.22,.72)} 40%{transform:scale(.92,1.1)}
                      60%{transform:scale(1.04,.96)} 100%{transform:scale(1,1)} }
  @keyframes sway   { 0%,100%{transform:rotate(-6deg)} 50%{transform:rotate(6deg)} }

  .x-brazo { transform-box: view-box; transform-origin: 114px 86px; animation: wave .36s ease-in-out 3 alternate; }
  @keyframes wave { from { transform: rotate(-18deg); } to { transform: rotate(22deg); } }

  .e-espiral path { transform-box: fill-box; transform-origin: center; animation: spin .7s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .eye-blink { transform-box: fill-box; transform-origin: center; transition: transform .07s ease-in; }
  svg.blink .eye-blink { transform: scaleY(.1); }

  .z { opacity: 0; animation: zfloat 3s ease-out infinite; }
  .z2 { animation-delay: 1s; } .z3 { animation-delay: 2s; }
  @keyframes zfloat { 0%{opacity:0; transform:translate(0,0) scale(.6)} 20%{opacity:1}
                      100%{opacity:0; transform:translate(10px,-22px) scale(1)} }
  .z { transform-box: fill-box; transform-origin: center; }

  .tw { transform-box: fill-box; transform-origin: center; animation: twinkle 1.6s ease-in-out infinite; }
  .tw2 { animation-delay: .5s; } .tw3 { animation-delay: 1s; }
  @keyframes twinkle { 0%,100%{transform:scale(.4); opacity:.3} 50%{transform:scale(1); opacity:1} }

  .x-exclamacion { animation: exbob .45s ease-in-out infinite; }

  /* Caché (solo en dormida): vapor que sube, tiritona en ráfagas y copo; sin tocar los colores del cuerpo */
  .k-vapor path { opacity: .45; transform-box: view-box; animation: steam 2.4s ease-out infinite; }
  .k-vapor path + path { animation-delay: 1.2s; }
  @keyframes steam { 0%{opacity:0; transform:translateY(4px)} 30%{opacity:.6} 100%{opacity:0; transform:translateY(-6px)} }
  svg[data-cache="enfriandose"] .shiver { animation: shiver 2s linear infinite; }
  @keyframes shiver { 0%,80%,94%,100%{transform:translateX(0)} 82%,86%,90%{transform:translateX(-2px)}
                      84%,88%,92%{transform:translateX(2px)} }
  @keyframes exbob { 50% { transform: translateY(-3px); } }

  @media (prefers-reduced-motion: reduce) {
    *, *::before, *::after { animation: none !important; transition: none !important; }
    .z { opacity: 1; }
    svg[data-leds="ciclo"] .lit-0, svg[data-leds="ciclo"] .lit-1 { opacity: 1; }
  }
</style>

<svg viewBox="0 0 144 156" role="img" aria-label="Pilita">
  <g class="rig"><g class="shiver">
    <ellipse class="c-foot" cx="52" cy="144" rx="11" ry="7"/>
    <ellipse class="c-foot" cx="88" cy="144" rx="11" ry="7"/>

    <g class="v x-brazo">
      <path class="s-body" d="M114 88 Q126 82 128 68" fill="none" stroke-width="9" stroke-linecap="round"/>
      <circle class="c-body" cx="128" cy="64" r="6.5"/>
    </g>

    <rect x="55" y="18" width="30" height="18" rx="7" fill="#9EA3B8"/>
    <rect x="59" y="21" width="9" height="4" rx="2" fill="#E4E6EF"/>

    <rect class="c-body" x="22" y="30" width="96" height="112" rx="38"/>
    <path class="c-shade" d="M118 80 L118 104 Q118 142 80 142 L60 142 Q90 136 102 110 Q112 88 118 80 Z"/>
    <ellipse cx="46" cy="56" rx="11" ry="17" fill="#FFFFFF" opacity=".22"/>
    <circle cx="42" cy="46" r="4" fill="#FFFFFF" opacity=".75"/>

    <!-- ojos -->
    <g class="v e-abiertos">
      <g class="eye-blink">
        <ellipse cx="52" cy="72" rx="9" ry="12" fill="${INK}"/>
        <circle cx="49" cy="67" r="3.5" fill="#FFF"/><circle cx="55" cy="77" r="1.5" fill="#FFF"/>
      </g>
      <g class="eye-blink">
        <ellipse cx="88" cy="72" rx="9" ry="12" fill="${INK}"/>
        <circle cx="85" cy="67" r="3.5" fill="#FFF"/><circle cx="91" cy="77" r="1.5" fill="#FFF"/>
      </g>
    </g>
    <g class="v e-cerrados" fill="none" stroke="${INK}" stroke-width="3.5" stroke-linecap="round">
      <path d="M44 74 Q52 81 60 74"/><path d="M80 74 Q88 81 96 74"/>
    </g>
    <g class="v e-felices" fill="none" stroke="${INK}" stroke-width="3.5" stroke-linecap="round">
      <path d="M43 75 Q52 61 61 75"/><path d="M79 75 Q88 61 97 75"/>
    </g>
    <g class="v e-susto">
      <ellipse cx="52" cy="70" rx="12" ry="14" fill="#FFF"/><ellipse cx="88" cy="70" rx="12" ry="14" fill="#FFF"/>
      <circle cx="52" cy="72" r="5.5" fill="${INK}"/><circle cx="88" cy="72" r="5.5" fill="${INK}"/>
      <circle cx="50" cy="70" r="1.8" fill="#FFF"/><circle cx="86" cy="70" r="1.8" fill="#FFF"/>
    </g>
    <g class="v e-apretados" fill="none" stroke="${INK}" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M45 64 L58 72 L45 80"/><path d="M95 64 L82 72 L95 80"/>
    </g>
    <g class="v e-enojados">
      <ellipse cx="52" cy="76" rx="9" ry="7" fill="${INK}"/><ellipse cx="88" cy="76" rx="9" ry="7" fill="${INK}"/>
      <circle cx="49" cy="74" r="2" fill="#FFF"/><circle cx="85" cy="74" r="2" fill="#FFF"/>
      <g fill="none" stroke="${INK}" stroke-width="3.5" stroke-linecap="round">
        <path d="M41 60 L60 67"/><path d="M99 60 L80 67"/>
      </g>
    </g>
    <g class="v e-espiral" fill="none" stroke="${INK}" stroke-width="2.6" stroke-linecap="round">
      <path d="M52 72 a2 2 0 1 1 4 0 a4 4 0 1 1 -8 0 a6 6 0 1 1 12 0 a8 8 0 1 1 -16 0"/>
      <path d="M88 72 a2 2 0 1 1 4 0 a4 4 0 1 1 -8 0 a6 6 0 1 1 12 0 a8 8 0 1 1 -16 0"/>
    </g>

    <g class="v blush">
      <ellipse cx="36" cy="90" rx="7" ry="4" fill="#FF8FAB" opacity=".78"/>
      <ellipse cx="104" cy="90" rx="7" ry="4" fill="#FF8FAB" opacity=".78"/>
    </g>

    <!-- bocas -->
    <g fill="none" stroke="${INK}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round">
      <path class="v m-gato" d="M64 88 Q67 93 70 89 Q73 93 76 88"/>
      <path class="v m-neutra" d="M66 91 L74 91"/>
      <path class="v m-ondas" d="M58 91 l4 -3 l4 3 l4 -3 l4 3 l4 -3 l4 3"/>
      <path class="v m-puchero" d="M63 93 Q70 85 77 93"/>
    </g>
    <g class="v m-o">
      <ellipse cx="70" cy="96" rx="6" ry="7" fill="${INK}"/><ellipse cx="70" cy="99" rx="3.5" ry="2.5" fill="#FF6B81"/>
    </g>
    <g class="v m-abierta">
      <path d="M60 84 Q70 102 80 84 Z" fill="${INK}" stroke="${INK}" stroke-width="2" stroke-linejoin="round"/>
      <ellipse cx="70" cy="91" rx="4.5" ry="2.8" fill="#FF6B81"/>
    </g>

    <!-- franja de LEDs -->
    <rect class="c-dark" x="36" y="106" width="68" height="19" rx="9.5" opacity=".55"/>
    <g>
      <rect class="c-foot" x="43" y="112" width="12" height="7" rx="3.5"/>
      <rect class="c-foot" x="57" y="112" width="12" height="7" rx="3.5"/>
      <rect class="c-foot" x="71" y="112" width="12" height="7" rx="3.5"/>
      <rect class="c-foot" x="85" y="112" width="12" height="7" rx="3.5"/>
      <rect class="lit lit-0" x="43" y="112" width="12" height="7" rx="3.5"/>
      <rect class="lit lit-1" x="57" y="112" width="12" height="7" rx="3.5"/>
      <rect class="lit lit-2" x="71" y="112" width="12" height="7" rx="3.5"/>
      <rect class="lit lit-3" x="85" y="112" width="12" height="7" rx="3.5"/>
    </g>

    <!-- ranura del accesorio de la skin sobre el borne: dentro del rig, así se mueve con ella -->
    <g class="accessory"></g>

    <!-- caché, a la izquierda del borne: libre de los sombreros de las skins (x >= 34) y de la "z" (x >= 114) -->
    <g class="v k-vapor" fill="none" stroke="#E4E6EF" stroke-width="3" stroke-linecap="round">
      <path d="M22 28 q-5 -6 0 -12 q5 -6 0 -12"/>
      <path d="M32 26 q-4 -5 0 -10 q4 -5 0 -10"/>
    </g>
    <g class="v k-copo" stroke="#8FD3FF" stroke-width="3" stroke-linecap="round">
      <path d="M24 8 v20 M15.3 13 l17.4 10 M15.3 23 l17.4 -10"/>
      <path d="M21 9.5 l3 3 l3 -3 M21 26.5 l3 -3 l3 3" fill="none" stroke-width="2.2"/>
    </g>
  </g></g>

  <!-- extras fuera del cuerpo -->
  <g class="v x-zzz" fill="none" stroke="#C7D0FF" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
    <path class="z" d="M114 22 h7 l-7 7 h7"/>
    <path class="z z2" d="M114 22 h7 l-7 7 h7"/>
    <path class="z z3" d="M114 22 h7 l-7 7 h7"/>
  </g>
  <g class="v x-exclamacion">
    <rect x="128" y="2" width="8" height="20" rx="4" fill="#FFD84A"/>
    <circle cx="132" cy="30" r="4.5" fill="#FFD84A"/>
  </g>
  <g class="v x-destellos">
    <path class="tw" d="M10 32 Q10 40 18 40 Q10 40 10 48 Q10 40 2 40 Q10 40 10 32 Z" fill="#FFD84A"/>
    <path class="tw tw2" d="M132 12 Q132 18 138 18 Q132 18 132 24 Q132 18 126 18 Q132 18 132 12 Z" fill="#FFFFFF"/>
    <path class="tw tw3" d="M134 106 Q134 111 139 111 Q134 111 134 116 Q134 111 129 111 Q134 111 134 106 Z" fill="#FFD84A"/>
  </g>
</svg>
`;

export class PilitaMascot extends HTMLElement {
  static observedAttributes = ["state", "skin", "cache"];

  private svg!: SVGSVGElement;
  private accessory!: SVGGElement;
  private lit!: SVGElement[];
  private dressed: Skin | null = null; // skin cuyo vestuario está pintado
  private reaction: PilitaReaction | null = null;
  private reactionTimers: number[] = [];
  private blinkTimer = 0;
  private clicks: number[] = [];

  constructor() {
    super();
    const root = this.attachShadow({ mode: "open" });
    root.innerHTML = TEMPLATE;
    this.svg = root.querySelector("svg")!;
    this.accessory = root.querySelector(".accessory")!;
    this.lit = [...root.querySelectorAll<SVGElement>(".lit")];
  }

  get state(): PilitaState {
    const s = this.getAttribute("state") as PilitaState;
    return STATES.includes(s) ? s : "dormida";
  }

  set state(value: PilitaState) {
    this.setAttribute("state", value);
  }

  get skin(): Skin {
    const s = this.getAttribute("skin") as Skin;
    return Object.keys(SKINS).includes(s) ? s : "classic";
  }

  set skin(value: Skin) {
    this.setAttribute("skin", value);
  }

  connectedCallback(): void {
    this.addEventListener("pointerenter", this.onEnter);
    this.addEventListener("click", this.onClick);
    this.render();
    this.scheduleBlink();
  }

  disconnectedCallback(): void {
    this.removeEventListener("pointerenter", this.onEnter);
    this.removeEventListener("click", this.onClick);
    window.clearTimeout(this.blinkTimer);
    this.clearReaction();
  }

  attributeChangedCallback(): void {
    if (this.state === "alerta") this.clearReaction();   // la alerta siempre gana
    this.render();
  }

  /** Reproduce una reacción una vez. Ignorada en "alerta". */
  react(name: PilitaReaction): void {
    if (this.state === "alerta") return;
    this.clearReaction();
    this.reaction = name;
    this.render();
    if (name === "aplastada") {   // primero aprieta los ojos, luego pone puchero
      this.reactionTimers.push(window.setTimeout(() => this.render("enojo"), 420));
    }
    this.reactionTimers.push(window.setTimeout(() => {
      this.reaction = null;
      this.render();
    }, REACTION_MS[name]));
  }

  private clearReaction(): void {
    this.reactionTimers.forEach((t) => window.clearTimeout(t));
    this.reactionTimers = [];
    this.reaction = null;
  }

  private pose(phase?: "enojo"): Pose {
    const base = STATE_POSE[this.state];
    switch (this.reaction) {
      case "saludo":
        return { ...base, eyes: "felices", mouth: "abierta", blush: true, extras: ["brazo"] };
      case "aplastada":
        return phase === "enojo"
          ? { ...base, eyes: "enojados", mouth: "puchero", blush: false, extras: [] }
          : { ...base, eyes: "apretados", mouth: "ondas", blush: false, extras: [] };
      case "mareo":
        return { ...base, eyes: "espiral", mouth: "ondas", blush: false, extras: [] };
      default:
        return base;
    }
  }

  private render(phase?: "enojo"): void {
    const p = this.pose(phase);
    const d = this.svg.dataset;
    d.state = this.state;
    d.eyes = p.eyes;
    d.mouth = p.mouth;
    d.leds = p.leds;
    d.blush = String(p.blush);
    for (const x of ["zzz", "exclamacion", "destellos", "brazo"] as const) {
      if (p.extras.includes(x)) d[x] = ""; else delete d[x];
    }
    if (this.reaction) d.react = this.reaction; else delete d.react;
    // En Cargando la sesión trabaja; Alerta y Llena mandan: la caché solo se ve dormida.
    const cache = this.getAttribute("cache") as PilitaCache;
    if (this.state === "dormida" && CACHES.includes(cache)) d.cache = cache; else delete d.cache;
    this.svg.setAttribute("aria-label", `Pilita: ${this.state}`);
    this.dress();
  }

  // Vestuario de la skin: solo el accesorio en su ranura y el tinte de los LEDs encendidos; la identidad no cambia.
  private dress(): void {
    if (this.dressed === this.skin) return;
    this.dressed = this.skin;
    const mascot = SKINS[this.skin].mascot;
    this.accessory.innerHTML = mascot?.accessory ?? "";
    const leds = mascot?.leds ?? [];
    // Sin colores queda el --led amarillo por defecto; con menos colores que LEDs se reparten en ciclo.
    this.lit.forEach((el, i) => {
      if (leds.length) el.style.setProperty("--led", leds[i % leds.length]);
      else el.style.removeProperty("--led");
    });
  }

  private onEnter = (): void => {
    if (!this.reaction) this.react("saludo");
  };

  private onClick = (): void => {
    const now = performance.now();
    // Durante el mareo los clics no cuentan para uno nuevo.
    if (this.reaction !== "mareo") this.clicks = [...this.clicks.filter((t) => now - t < 1000), now];
    let reaction: PilitaReaction | null = null;
    if (this.clicks.length >= 3) {
      this.clicks = [];
      this.react("mareo");
      reaction = "mareo";
    } else if (this.reaction !== "mareo") {
      this.react("aplastada");
      reaction = "aplastada";
    }
    this.dispatchEvent(new CustomEvent("pilita-click", { bubbles: true, composed: true, detail: { reaction } }));
  };

  private scheduleBlink = (): void => {
    this.blinkTimer = window.setTimeout(() => {
      if (this.svg.dataset.eyes === "abiertos") {
        this.svg.classList.add("blink");
        window.setTimeout(() => this.svg.classList.remove("blink"), 140);
      }
      this.scheduleBlink();
    }, 3000 + Math.random() * 3000);
  };
}

if (!customElements.get("pilas-mascot")) {
  customElements.define("pilas-mascot", PilitaMascot);
}

declare global {
  interface HTMLElementTagNameMap {
    "pilas-mascot": PilitaMascot;
  }
}
