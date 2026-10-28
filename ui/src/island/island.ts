// Isla mínima funcional (M2/M3): píldora, lista de sesiones y tarjeta de permiso.
// La tarjeta de AskUserQuestion completa es parte de M4 (docs/PLAN.md).
import type { PilitaMascot, PilitaState } from "../mascot/pilita";
import { play } from "../sound/sfx";
import * as api from "./api";

type Size = "hidden" | "pill" | "open";

const OPEN_MAX_H = 360; // tope de la tarjeta abierta (styles.css); igual que en Rust
const HIDDEN_H = 6; // alturas de la ventana por modo (styles.css)
const PILL_H = 32;
const OPEN_W = 420; // ancho de la ventana abierta; igual que en Rust

const STATE_LABEL: Record<api.SessionState, string> = {
  en_espera: "En espera",
  trabajando: "Trabajando",
  pendiente: "Pendiente",
  error: "Error",
};

const DEST: Record<string, string> = {
  localSettings: "proyecto (local)",
  projectSettings: "proyecto",
  userSettings: "usuario",
  session: "esta sesión",
};

// Una línea por regla o directorio que Claude Code guardaría con "Siempre".
function describeSuggestions(list: unknown[]): string[] {
  const lines: string[] = [];
  for (const raw of list) {
    const s = raw as { type?: string; rules?: { toolName: string; ruleContent?: string }[]; directories?: string[]; mode?: string; destination?: string };
    const dest = DEST[s.destination ?? ""] ?? s.destination ?? "";
    if (s.type === "addRules") {
      for (const r of s.rules ?? []) lines.push(`${r.ruleContent ? `${r.toolName}(${r.ruleContent})` : r.toolName} · ${dest}`);
    } else if (s.type === "addDirectories") {
      for (const d of s.directories ?? []) lines.push(`Directorio: ${d} · ${dest}`);
    } else if (s.type === "setMode") {
      lines.push(`Modo: ${s.mode} · ${dest}`);
    } else {
      lines.push(`${JSON.stringify(raw)} · ${dest}`);
    }
  }
  return lines;
}

// "ahora", "hace N min" o "hace N h" desde la última actividad de la sesión.
function ago(ms: number): string {
  const min = Math.floor((Date.now() - ms) / 60_000);
  if (min < 1) return "ahora";
  if (min < 60) return `hace ${min} min`;
  return `hace ${Math.floor(min / 60)} h`;
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

  function esc(s: string): string {
    return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
  }

  function renderCard(p: api.PendingView, index: number): string {
    const detail = p.tool_input
      ? String((p.tool_input as Record<string, unknown>).command ?? (p.tool_input as Record<string, unknown>).file_path ?? JSON.stringify(p.tool_input, null, 2))
      : "";
    const counter = snap.pending.length > 1 ? ` · ${index + 1} de ${snap.pending.length}` : "";
    if (p.kind === "question") {
      return `<div class="card-head">${esc(p.project)} tiene una pregunta${counter}</div>
        <button class="link" data-q-terminal="${p.request_id}">Responder en la terminal</button>`;
    }
    if (confirmAlways === p.request_id) {
      const lines = describeSuggestions(p.permission_suggestions ?? []);
      return `<div class="card-head">Claude Code guardará estas reglas${counter}</div>
        <div class="rules">${lines.map((l) => `<div>${esc(l)}</div>`).join("")}</div>
        <div class="buttons">
          <button class="b-allow" data-confirm>Guardar y permitir</button>
          <button class="b-always" data-back>Volver</button>
        </div>`;
    }
    const canAlways = (p.permission_suggestions ?? []).length > 0; // sin reglas sugeridas, Pilas no inventa "Siempre"
    return `<div class="card-head">${esc(p.project)} · ${esc(p.tool_name ?? "")}${counter}</div>
      <div class="card-body${expanded === p.request_id ? " expanded" : ""}">${esc(detail)}</div>
      <button class="link" data-expand hidden>ver más</button>
      <div class="buttons">
        <button class="b-allow" data-choice="allow">Permitir</button>
        ${canAlways ? `<button class="b-always" data-choice="always">Siempre</button>` : ""}
        <button class="b-deny" data-choice="deny">Denegar</button>
      </div>
      <button class="link" data-choice="terminal">Responder en la terminal</button>`;
  }

  // Tras pintar, ofrece "ver más" solo si el detalle pasa de 6 líneas, o "ver menos" si ya está expandido.
  function fitDetail(): void {
    const body = panel.querySelector<HTMLElement>(".card-body");
    const more = panel.querySelector<HTMLElement>("[data-expand]");
    if (!body || !more) return;
    const isExp = body.classList.contains("expanded");
    more.hidden = !isExp && body.scrollHeight <= body.clientHeight;
    more.textContent = isExp ? "ver menos" : "ver más";
  }

  // Única vía para decidir, compartida por clic y Enter: va por request_id exacto.
  async function decide(p: api.PendingView, choice: api.PermissionChoice): Promise<void> {
    confirmAlways = null;
    await api.resolvePermission(p.request_id, choice);
    play(choice === "deny" ? "denegado" : "aprobado");
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
                <span class="dot" data-s="${s.state}" title="${STATE_LABEL[s.state]}"></span>
                <span class="name">${esc(s.project)}</span>
                <span class="action">${esc(s.last_action ?? "")}</span>
                <span class="ago">${ago(s.updated_ms)}</span></div>`)
            .join("");
      fitDetail();
    }
    syncWindow();
  }

  panel.addEventListener("click", async (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>(
      "[data-choice],[data-confirm],[data-back],[data-expand],[data-session],[data-q-terminal]",
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
    } else if (el.dataset.qTerminal) {
      await api.answerQuestion(el.dataset.qTerminal, null);
    } else if (el.dataset.session) {
      await api.jumpToSession(el.dataset.session);
    }
  });

  mascot.addEventListener("pilita-click", () => {
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
    if (inside && size() === "hidden") { peek = true; render(); }
    else if (!inside && peek) { peek = false; render(); }
  });

  api.onSnapshot((s) => { snap = s; if (s.pending.length === 0 && s.sessions.length === 0) open = false; render(); });
  api.onPending(() => { open = true; play("alerta"); render(); });
  api.onFinished(() => { fullUntil = Date.now() + 4000; play("listo"); render(); setTimeout(render, 4100); });
  api.getSnapshot().then((s) => { snap = s; render(); });
  // Refresca los "hace N min" mientras la lista está abierta.
  setInterval(() => { if (open) render(); }, 30_000);
  render();
}
