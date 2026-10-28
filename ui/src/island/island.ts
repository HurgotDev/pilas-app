// Isla mínima funcional (M2/M3): píldora, lista de sesiones y tarjeta de permiso.
import type { PilitaMascot, PilitaState } from "../mascot/pilita";
import { configure, play } from "../sound/sfx";
import * as api from "./api";
import { setLang, t, type Key } from "../i18n";

type Size = "hidden" | "pill" | "open";

const OPEN_MAX_H = 360; // tope de la tarjeta abierta (styles.css); igual que en Rust
const HIDDEN_H = 6; // alturas de la ventana por modo (styles.css)
const PILL_H = 32;
const OPEN_W = 420; // ancho de la ventana abierta; igual que en Rust

const STATE_LABEL: Record<api.SessionState, Key> = {
  en_espera: "state.en_espera",
  trabajando: "state.trabajando",
  pendiente: "state.pendiente",
  error: "state.error",
};

const DEST: Record<string, Key> = {
  localSettings: "dest.localSettings",
  projectSettings: "dest.projectSettings",
  userSettings: "dest.userSettings",
  session: "dest.session",
};

// Frase de last_action según su tipo; el backend solo manda datos.
const ACTION: Record<api.ActionKind, Key> = {
  bash: "act.bash",
  edit: "act.edit",
  write: "act.write",
  read: "act.read",
  search_project: "act.search_project",
  search_web: "act.search_web",
  question: "act.question",
  other: "act.other",
};

interface Question {
  question: string;
  header?: string;
  multiSelect?: boolean;
  options: { label: string; description?: string }[];
}

// Preguntas de AskUserQuestion; vacío si el payload no trae ninguna.
function questionsOf(p: api.PendingView): Question[] {
  const list = p.tool_input?.questions;
  return Array.isArray(list) ? (list as Question[]) : [];
}

// Una línea por regla o directorio que Claude Code guardaría con "Siempre".
function describeSuggestions(list: unknown[]): string[] {
  const lines: string[] = [];
  for (const raw of list) {
    const s = raw as { type?: string; rules?: { toolName: string; ruleContent?: string }[]; directories?: string[]; mode?: string; destination?: string };
    const key = DEST[s.destination ?? ""];
    const dest = key ? t(key) : s.destination ?? "";
    if (s.type === "addRules") {
      for (const r of s.rules ?? []) lines.push(`${r.ruleContent ? `${r.toolName}(${r.ruleContent})` : r.toolName} · ${dest}`);
    } else if (s.type === "addDirectories") {
      for (const d of s.directories ?? []) lines.push(`${t("sugg.directory", { path: d })} · ${dest}`);
    } else if (s.type === "setMode") {
      lines.push(`${t("sugg.mode", { mode: s.mode ?? "" })} · ${dest}`);
    } else {
      lines.push(`${JSON.stringify(raw)} · ${dest}`);
    }
  }
  return lines;
}

// "ahora", "hace N min" o "hace N h" desde la última actividad (textos en i18n.ts).
function ago(ms: number): string {
  const min = Math.floor((Date.now() - ms) / 60_000);
  if (min < 1) return t("ago.now");
  if (min < 60) return t("ago.min", { n: min });
  return t("ago.h", { n: Math.floor(min / 60) });
}

// Texto de last_action; sin argumento, un comando se muestra como "un comando" (i18n).
function actionText(a: api.LastAction): string {
  const arg = a.arg ?? (a.kind === "bash" ? t("arg.command") : "");
  return t(ACTION[a.kind], { arg });
}

export function mountIsland(root: HTMLElement): void {
  let snap: api.Snapshot = { sessions: [], mascot: "dormida", pending: [] };
  let open = false;
  let peek = false; // con la isla oculta, el cursor encima muestra la píldora
  let fullUntil = 0; // la pilita se muestra "llena" ~4 s tras terminar una sesión
  let sent = { mode: "", h: 0 }; // último tamaño de ventana enviado al backend
  let shrinkTimer = 0;
  let seq = 0; // invalida aplicaciones de tamaño pendientes
  let pillW = window.innerWidth; // ancho de la píldora = ancho de la ventana fuera del modo abierto
  // Estado de la tarjeta por request_id: deja de coincidir al llegar otro pendiente, así que no hace falta resetearlo.
  let confirmAlways: string | null = null; // paso de confirmación de "Siempre"
  let expanded: string | null = null; // detalle expandido ("ver más")
  // Progreso de AskUserQuestion: paso actual y elecciones por pregunta; se reinicia si cambia la tarjeta.
  let qCard: { id: string; step: number; picks: string[][] } | null = null;
  let openOnPending = true; // ajuste "Abrir la isla al llegar una solicitud"

  root.innerHTML = `
    <div class="pill">
      <pilas-mascot state="dormida"></pilas-mascot>
      <span class="badge" hidden></span>
    </div>
    <div class="panel"></div>`;
  const mascot = root.querySelector("pilas-mascot") as PilitaMascot;
  const badge = root.querySelector(".badge") as HTMLElement;
  const panel = root.querySelector(".panel") as HTMLElement;

  const size = (): Size => (open ? "open" : snap.sessions.length === 0 && !peek ? "hidden" : "pill");

  // Tamaño de la isla en CSS (ancho y alto explícitos para que transicionen).
  function applySize(mode: Size, h: number): void {
    root.dataset.size = mode;
    root.style.setProperty("--w", `${mode === "open" ? OPEN_W : pillW}px`);
    root.style.setProperty("--h", `${h}px`);
  }

  // La ventana toma el tamaño de la isla. Al crecer, primero la ventana y en el siguiente frame el CSS
  // (la isla anima dentro de la ventana ya grande); al encoger, el CSS ya y la ventana tras la transición (~300 ms).
  function syncWindow(): void {
    clearTimeout(shrinkTimer);
    const mode = size();
    // Se mide el contenido, no la isla (su alto ahora es fijo).
    const content = (root.querySelector(".pill") as HTMLElement).offsetHeight + panel.offsetHeight;
    const h = mode === "open" ? Math.min(content, OPEN_MAX_H) : mode === "pill" ? PILL_H : HIDDEN_H;
    const mine = ++seq;
    if (mode === sent.mode && h === sent.h) return applySize(mode, h);
    const send = () => {
      sent = { mode, h };
      return api.setIslandMode(mode, mode === "open" ? h : undefined);
    };
    const grows = h > sent.h || (mode === "open" && sent.mode !== "open");
    if (grows) {
      void send().finally(() => requestAnimationFrame(() => requestAnimationFrame(() => mine === seq && applySize(mode, h))));
    } else {
      applySize(mode, h);
      shrinkTimer = window.setTimeout(() => void send(), 320);
    }
  }
  // Fuera del modo abierto la ventana mide exactamente lo que la píldora.
  window.addEventListener("resize", () => {
    if (sent.mode === "open") return;
    pillW = window.innerWidth;
    root.style.setProperty("--w", `${pillW}px`);
  });

  function mascotState(): PilitaState {
    if (snap.mascot === "alerta") return "alerta";
    if (Date.now() < fullUntil) return "llena";
    return snap.mascot;
  }

  // " · 1 de N" (o "1 of N") cuando hay más de una solicitud pendiente.
  function counterOf(index: number): string {
    return snap.pending.length > 1 ? t("card.counter", { i: index + 1, n: snap.pending.length }) : "";
  }

  function esc(s: string): string {
    return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
  }

  function qCardOf(p: api.PendingView): { step: number; picks: string[][] } {
    if (qCard?.id !== p.request_id) qCard = { id: p.request_id, step: 0, picks: questionsOf(p).map(() => []) };
    return qCard;
  }

  function renderQuestion(p: api.PendingView, index: number): string {
    const counter = counterOf(index);
    const qs = questionsOf(p);
    const terminal = `<button class="link" data-q-terminal="${p.request_id}">${t("btn.terminal")}</button>`;
    if (qs.length === 0) return `<div class="card-head">${esc(t("card.question", { project: p.project }))}${counter}</div>${terminal}`;
    const card = qCardOf(p);
    const q = qs[card.step];
    const picks = card.picks[card.step];
    const last = card.step === qs.length - 1;
    const header = q.header ? ` · ${esc(q.header)}` : "";
    const steps = qs.length > 1 ? ` · ${card.step + 1}/${qs.length}` : "";
    const options = q.options
      .map((o, j) => {
        const on = picks.includes(o.label);
        const role = q.multiSelect ? `role="checkbox" aria-checked="${on}"` : `aria-pressed="${on}"`;
        const text = `<span class="q-label">${esc(o.label)}</span>${o.description ? `<span class="q-desc">${esc(o.description)}</span>` : ""}`;
        return `<button class="q-opt${q.multiSelect ? " q-multi" : ""}${on ? " on" : ""}" ${role} data-q-opt="${j}">${
          q.multiSelect ? `<span class="q-box"></span><span class="q-col">${text}</span>` : text
        }</button>`;
      })
      .join("");
    const answered = card.picks.every((l) => l.length > 0);
    const actions = [
      card.step > 0 ? `<button class="q-back" data-q-back>${t("btn.prev")}</button>` : "",
      q.multiSelect && !last ? `<button class="b-allow" data-q-next ${picks.length ? "" : "disabled"}>${t("btn.next")}</button>` : "",
      last ? `<button class="b-allow" data-q-send ${answered ? "" : "disabled"}>${t("btn.send")}</button>` : "",
    ].join("");
    return `<div class="card-head">${esc(p.project)}${header}${steps}${counter}</div>
      <div class="q-text">${esc(q.question)}</div>
      <div class="q-options">${options}</div>
      ${actions ? `<div class="buttons">${actions}</div>` : ""}
      ${terminal}`;
  }

  function renderCard(p: api.PendingView, index: number): string {
    const detail = p.tool_input
      ? String((p.tool_input as Record<string, unknown>).command ?? (p.tool_input as Record<string, unknown>).file_path ?? JSON.stringify(p.tool_input, null, 2))
      : "";
    const counter = counterOf(index);
    if (p.kind === "question") return renderQuestion(p, index);
    if (confirmAlways === p.request_id) {
      const lines = describeSuggestions(p.permission_suggestions ?? []);
      return `<div class="card-head">${t("card.rules")}${counter}</div>
        <div class="rules">${lines.map((l) => `<div>${esc(l)}</div>`).join("")}</div>
        <div class="buttons">
          <button class="b-allow" data-confirm>${t("btn.save_allow")}</button>
          <button class="b-always" data-back>${t("btn.back")}</button>
        </div>`;
    }
    const canAlways = (p.permission_suggestions ?? []).length > 0; // sin reglas sugeridas, Pilas no inventa "Siempre"
    return `<div class="card-head">${esc(p.project)} · ${esc(p.tool_name ?? "")}${counter}</div>
      <div class="card-body${expanded === p.request_id ? " expanded" : ""}">${esc(detail)}</div>
      <button class="link" data-expand hidden>${t("btn.more")}</button>
      <div class="buttons">
        <button class="b-allow" data-choice="allow">${t("btn.allow")}</button>
        ${canAlways ? `<button class="b-always" data-choice="always">${t("btn.always")}</button>` : ""}
        <button class="b-deny" data-choice="deny">${t("btn.deny")}</button>
      </div>
      <button class="link" data-choice="terminal">${t("btn.terminal")}</button>`;
  }

  // Tras pintar, ofrece "ver más" solo si el detalle pasa de 6 líneas, o "ver menos" si ya está expandido.
  function fitDetail(): void {
    const body = panel.querySelector<HTMLElement>(".card-body");
    const more = panel.querySelector<HTMLElement>("[data-expand]");
    if (!body || !more) return;
    const isExp = body.classList.contains("expanded");
    more.hidden = !isExp && body.scrollHeight <= body.clientHeight;
    more.textContent = t(isExp ? "btn.less" : "btn.more");
  }

  // Única vía para decidir, compartida por clic y Enter: va por request_id exacto.
  async function decide(p: api.PendingView, choice: api.PermissionChoice): Promise<void> {
    confirmAlways = null;
    await api.resolvePermission(p.request_id, choice);
    play(choice === "deny" ? "denegado" : "aprobado");
  }

  // Marca una opción: única avanza sola (salvo en la última); múltiple alterna la casilla.
  function pickQuestion(p: api.PendingView, j: number): void {
    const card = qCardOf(p);
    const qs = questionsOf(p);
    const label = qs[card.step].options[j].label;
    if (qs[card.step].multiSelect) {
      const picks = card.picks[card.step];
      if (picks.includes(label)) card.picks[card.step] = picks.filter((l) => l !== label);
      else picks.push(label);
    } else {
      card.picks[card.step] = [label];
      if (card.step < qs.length - 1) card.step++;
    }
  }

  // Respuestas en orden de opciones, con la pregunta como clave, según el contrato de Claude Code.
  async function sendQuestion(p: api.PendingView): Promise<void> {
    const card = qCardOf(p);
    const qs = questionsOf(p);
    if (!card.picks.every((l) => l.length > 0)) return;
    const answers: Record<string, string> = {};
    qs.forEach((q, i) => {
      answers[q.question] = q.options.map((o) => o.label).filter((l) => card.picks[i].includes(l)).join(", ");
    });
    await api.answerQuestion(p.request_id, answers);
    play("aprobado");
  }

  function render(): void {
    mascot.state = mascotState();
    badge.hidden = snap.pending.length === 0;
    badge.textContent = String(snap.pending.length);
    if (open) {
      const first = snap.pending[0];
      panel.innerHTML = first
        ? renderCard(first, 0)
        : snap.sessions
            .map((s) => `<div class="row" data-session="${s.session_id}">
                <span class="dot" data-s="${s.state}" title="${t(STATE_LABEL[s.state])}"></span>
                <span class="name">${esc(s.project)}</span>
                <span class="action">${esc(s.last_action ? actionText(s.last_action) : "")}</span>
                <span class="ago">${ago(s.updated_ms)}</span></div>`)
            .join("");
      fitDetail();
    }
    syncWindow();
  }

  panel.addEventListener("click", async (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>(
      "[data-choice],[data-confirm],[data-back],[data-expand],[data-session],[data-q-terminal],[data-q-opt],[data-q-next],[data-q-back],[data-q-send]",
    );
    const first = snap.pending[0];
    if (!el) return;
    if (el.hasAttribute("data-back")) {
      confirmAlways = null;
      render();
    } else if (el.hasAttribute("data-expand") && first) {
      expanded = expanded === first.request_id ? null : first.request_id;
      render();
    } else if (el.dataset.choice === "always" && first) {
      confirmAlways = first.request_id; // "Siempre" no decide: primero muestra las reglas
      render();
    } else if (el.dataset.choice && first) {
      await decide(first, el.dataset.choice as api.PermissionChoice);
    } else if (el.hasAttribute("data-confirm") && first) {
      await decide(first, "always");
    } else if (el.hasAttribute("data-q-opt") && first) {
      pickQuestion(first, Number(el.dataset.qOpt));
      render();
    } else if (el.hasAttribute("data-q-next") && first) {
      qCardOf(first).step++;
      render();
    } else if (el.hasAttribute("data-q-back") && first) {
      qCardOf(first).step--;
      render();
    } else if (el.hasAttribute("data-q-send") && first) {
      await sendQuestion(first);
    } else if (el.dataset.qTerminal) {
      await api.answerQuestion(el.dataset.qTerminal, null);
    } else if (el.dataset.session) {
      await api.jumpToSession(el.dataset.session);
    }
  });

  // Abrir/cerrar va en la píldora y no en la pilita: chocaría con los 3 clics del mareo.
  root.querySelector(".pill")!.addEventListener("click", (e) => {
    if ((e.target as Element).closest("pilas-mascot")) return;
    open = !open;
    render();
  });
  mascot.addEventListener("pilita-click", () => {
    if (mascot.state !== "alerta") { play("toque"); return; } // en alerta no hay reacciones ni sonido de toque
    open = !open;
    render();
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { open = false; render(); }
    // Enter decide sobre la tarjeta de permiso; con un botón enfocado, su activación nativa ya actúa.
    const first = snap.pending[0];
    if (e.key !== "Enter" || !open || !first || first.kind !== "permission" || e.repeat) return;
    if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.target instanceof HTMLButtonElement) return;
    e.preventDefault();
    void decide(first, confirmAlways === first.request_id ? "always" : "allow");
  });
  // Clic fuera de la isla (la ventana pierde el foco) → contraer a píldora.
  window.addEventListener("blur", () => {
    if (open) { open = false; render(); }
  });
  // Con la isla oculta, el cursor sobre su franja la muestra un momento (aviso nativo: la app no está activa).
  api.onHover((inside) => {
    if (inside) mascot.react("saludo");
    if (inside && size() === "hidden") { peek = true; render(); }
    else if (!inside && peek) { peek = false; render(); }
  });

  api.onSnapshot((s) => { snap = s; if (s.pending.length === 0 && s.sessions.length === 0) open = false; render(); });
  // Recordatorio una sola vez por request_id, a los 60 s, solo si sigue pendiente.
  const reminded = new Set<string>();
  api.onPending((requestId) => {
    if (openOnPending) open = true;
    play("alerta"); render();
    if (reminded.has(requestId)) return;
    reminded.add(requestId);
    setTimeout(() => {
      if (snap.pending.some((p) => p.request_id === requestId)) play("recordatorio");
    }, 60_000);
  });
  api.onFinished(() => { fullUntil = Date.now() + 4000; play("listo"); render(); setTimeout(render, 4100); });
  // Ajustes: idioma, sonido y apertura al llegar un pendiente; se aplican al arrancar y en cada cambio.
  function applySettings(s: api.Settings): void {
    setLang(s.lang);
    configure({ enabled: s.sound_enabled, volume: s.volume });
    openOnPending = s.open_on_pending;
    render();
  }
  api.getSettings().then(applySettings);
  api.onSettings(applySettings);
  api.getSnapshot().then((s) => { snap = s; render(); });
  // Refresca los "hace N min" mientras la lista está abierta.
  setInterval(() => { if (open) render(); }, 30_000);
  render();
}
