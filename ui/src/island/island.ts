// Isla mínima funcional (M2/M3): píldora, lista de sesiones y tarjeta de permiso.
// La tarjeta de AskUserQuestion completa es parte de M4 (docs/PLAN.md).
import type { PilitaMascot, PilitaState } from "../mascot/pilita";
import { play } from "../sound/sfx";
import * as api from "./api";

type Size = "hidden" | "pill" | "open";

const OPEN_MAX_H = 360; // tope de la tarjeta abierta (styles.css); igual que en Rust
const HIDDEN_H = 6; // alturas de la ventana por modo (styles.css)
const PILL_H = 32;

const STATE_LABEL: Record<api.SessionState, string> = {
  en_espera: "En espera",
  trabajando: "Trabajando",
  pendiente: "Pendiente",
  error: "Error",
};

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

  root.innerHTML = `
    <div class="pill">
      <pilas-mascot state="dormida"></pilas-mascot>
      <span class="badge" hidden></span>
    </div>
    <div class="panel" hidden></div>`;
  const mascot = root.querySelector("pilas-mascot") as PilitaMascot;
  const badge = root.querySelector(".badge") as HTMLElement;
  const panel = root.querySelector(".panel") as HTMLElement;

  const size = (): Size => (open ? "open" : snap.sessions.length === 0 && !peek ? "hidden" : "pill");

  // La ventana toma el tamaño de la isla. Al crecer se avisa antes de pintar; al encoger,
  // tras la transición CSS (~250 ms) para no recortar la isla.
  function syncWindow(): void {
    clearTimeout(shrinkTimer);
    const mode = size();
    const h = mode === "open" ? Math.min(root.scrollHeight, OPEN_MAX_H) : mode === "pill" ? PILL_H : HIDDEN_H;
    if (mode === sent.mode && h === sent.h) return;
    const send = () => {
      sent = { mode, h };
      void api.setIslandMode(mode, mode === "open" ? h : undefined);
    };
    const grows = h > sent.h || (mode === "open" && sent.mode !== "open");
    if (grows) send();
    else shrinkTimer = window.setTimeout(send, 260);
  }

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
    return `<div class="card-head">${esc(p.project)} · ${esc(p.tool_name ?? "")}${counter}</div>
      <div class="card-body">${esc(detail)}</div>
      <div class="buttons">
        <button class="b-allow" data-choice="allow">Permitir</button>
        <button class="b-always" data-choice="always">Siempre</button>
        <button class="b-deny" data-choice="deny">Denegar</button>
      </div>
      <button class="link" data-choice="terminal">Responder en la terminal</button>`;
  }

  function render(): void {
    root.dataset.size = size();
    mascot.state = mascotState();
    badge.hidden = snap.pending.length === 0;
    badge.textContent = String(snap.pending.length);
    panel.hidden = !open;
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
    }
    syncWindow();
  }

  panel.addEventListener("click", async (e) => {
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-choice],[data-session],[data-q-terminal]");
    if (!el) return;
    if (el.dataset.choice && snap.pending[0]) {
      const choice = el.dataset.choice as api.PermissionChoice;
      await api.resolvePermission(snap.pending[0].request_id, choice);
      play(choice === "deny" ? "denegado" : "aprobado");
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
