// Isla mínima funcional (M2/M3): píldora, lista de sesiones y tarjeta de permiso.
// La tarjeta de AskUserQuestion completa es parte de M4 (docs/PLAN.md).
import type { PilitaMascot, PilitaState } from "../mascot/pilita";
import { play } from "../sound/sfx";
import * as api from "./api";

type Size = "hidden" | "pill" | "open";

export function mountIsland(root: HTMLElement): void {
  let snap: api.Snapshot = { sessions: [], mascot: "dormida", pending: [] };
  let open = false;
  let fullUntil = 0; // la pilita se muestra "llena" ~4 s tras terminar una sesión

  root.innerHTML = `
    <div class="pill">
      <pilas-mascot state="dormida"></pilas-mascot>
      <span class="badge" hidden></span>
    </div>
    <div class="panel" hidden></div>`;
  const mascot = root.querySelector("pilas-mascot") as PilitaMascot;
  const badge = root.querySelector(".badge") as HTMLElement;
  const panel = root.querySelector(".panel") as HTMLElement;

  const size = (): Size => (snap.sessions.length === 0 && !open ? "hidden" : open ? "open" : "pill");

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
    if (!open) return;

    const first = snap.pending[0];
    panel.innerHTML = first
      ? renderCard(first, 0)
      : snap.sessions
          .map((s) => `<div class="row" data-session="${s.session_id}">
              <span class="dot" data-s="${s.state}"></span>
              <span class="name">${esc(s.project)}</span>
              <span class="action">${esc(s.last_action ?? "")}</span></div>`)
          .join("");
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

  api.onSnapshot((s) => { snap = s; if (s.pending.length === 0 && s.sessions.length === 0) open = false; render(); });
  api.onPending(() => { open = true; play("alerta"); render(); });
  api.onFinished(() => { fullUntil = Date.now() + 4000; play("listo"); render(); setTimeout(render, 4100); });
  api.getSnapshot().then((s) => { snap = s; render(); });
  render();
}
