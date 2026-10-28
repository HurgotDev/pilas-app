// Ventana de Ajustes y onboarding (docs/SPEC.md → Instalación, ajustes y onboarding).
import "./mascot/pilita";
import * as api from "./island/api";
import { configure, play } from "./sound/sfx";
import { lang, setLang, t, type Key, type Lang } from "./i18n";

const STEPS = 4;
const HOOK_LABEL: Record<api.HookStatus, Key> = {
  ok: "hooks.ok",
  missing: "hooks.missing",
  needs_repair: "hooks.needs_repair",
  invalid_json: "hooks.invalid_json",
};
type BoolKey = "sound_enabled" | "open_on_pending" | "launch_at_login";

let settings: api.Settings;
let hooks: api.HooksInfo = { status: "missing", settings_path: "" };
let screens: string[] = [];
let step = 0; // paso actual del onboarding
let diff: { action: api.HookAction; lines: api.DiffLine[] } | null = null; // se muestra antes de escribir
let backup: string | null = null; // copia de seguridad del último cambio
let error = ""; // último error del instalador o de guardar
let busy = false; // hay una llamada al instalador en curso

const root = document.getElementById("app")!;

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

// Valor del <select> de pantalla: "main", "notch" o "named:<nombre>".
const screenValue = (s: api.ScreenChoice) => (typeof s === "string" ? s : `named:${s.named}`);
const screenFrom = (v: string): api.ScreenChoice =>
  v === "main" || v === "notch" ? v : { named: v.slice("named:".length) };

// Diff en bloque monoespaciado; las tiras largas sin cambios se pliegan en una línea.
function diffHtml(lines: api.DiffLine[]): string {
  const out: string[] = [];
  let run: api.DiffLine[] = [];
  const flush = () => {
    if (run.length > 4) out.push(`<div class="ctx">${t("hooks.unchanged", { n: run.length })}</div>`);
    else for (const l of run) out.push(line(l));
    run = [];
  };
  for (const l of lines) {
    if (l.op === " ") run.push(l);
    else { flush(); out.push(line(l)); }
  }
  flush();
  return out.join("");
}

function line(l: api.DiffLine): string {
  const cls = l.op === "+" ? "add" : l.op === "-" ? "del" : "ctx";
  return `<div class="${cls}">${esc(l.op)} ${esc(l.text)}</div>`;
}

function errorLine(): string {
  return error ? `<p class="error" role="alert">${esc(error)}</p>` : "";
}

function backupLine(): string {
  return backup ? `<p class="muted">${esc(t("hooks.backup", { path: backup }))}</p>` : "";
}

// Guarda al instante y aplica el sonido en esta ventana. Un fallo se muestra en la vista.
function save(): void {
  configure({ enabled: settings.sound_enabled, volume: settings.volume });
  api.setSettings(settings).catch((e: unknown) => {
    error = String(e);
    render();
  });
}

// Pide el diff de install/uninstall; nada se escribe hasta confirmar.
async function showDiff(action: api.HookAction): Promise<void> {
  busy = true;
  error = "";
  backup = null;
  render();
  try {
    diff = { action, lines: await api.previewHooks(action) };
  } catch (e) {
    diff = null;
    error = String(e);
  }
  busy = false;
  render();
}

async function apply(): Promise<void> {
  if (!diff) return;
  const { action } = diff;
  busy = true;
  error = "";
  render();
  try {
    backup = await api.applyHooks(action);
    diff = null;
    hooks = await api.hooksStatus();
  } catch (e) {
    error = String(e);
  }
  busy = false;
  render();
}

function go(n: number): void {
  step = n;
  error = "";
  if (n === 1 && hooks.status !== "ok" && diff?.action !== "install") void showDiff("install");
  else render();
}

function connectBody(): string {
  const head = `<h1>${t("onb.connect.title")}</h1><p>${t("onb.connect.body")}</p>
    <p class="path"><code>${esc(hooks.settings_path)}</code></p>`;
  if (hooks.status === "ok") return `${head}<p class="status s-ok">${t("hooks.ok")}</p>${backupLine()}${errorLine()}`;
  const preview = diff
    ? `<div class="diff">${diffHtml(diff.lines)}</div>`
    : busy ? `<p class="muted">${t("set.loading")}</p>` : "";
  return `${head}${preview}${errorLine()}
    <div class="btns"><button class="primary" data-apply ${diff && !busy ? "" : "disabled"}>${t("hooks.connect")}</button></div>`;
}

const STEP_BODY: (() => string)[] = [
  () => `<pilas-mascot class="hero" state="cargando"></pilas-mascot>
    <h1>${t("onb.welcome.title")}</h1><p>${t("onb.welcome.body")}</p>`,
  connectBody,
  () => `<h1>${t("onb.automation.title")}</h1><p>${t("onb.automation.body")}</p>`,
  () => `<h1>${t("onb.restart.title")}</h1><p>${t("onb.restart.body")}</p>`,
];

function onboardingView(): string {
  const connected = hooks.status === "ok";
  const dots = Array.from({ length: STEPS }, (_, i) => `<span class="dot${i === step ? " on" : ""}"></span>`).join("");
  const blocked = step === 1 && !connected; // Next espera a conectar; Omitir siempre deja pasar
  const skip = blocked ? `<button class="link" data-next>${t("onb.skip")}</button>` : "";
  const next =
    step === STEPS - 1
      ? `<button class="primary" data-done>${t("onb.done")}</button>`
      : `<button class="primary" data-next ${blocked ? "disabled" : ""}>${t("btn.next")}</button>`;
  return `<main class="page">
    <div class="steps" role="img" aria-label="${t("onb.step", { i: step + 1, n: STEPS })}">${dots}</div>
    <section class="step">${STEP_BODY[step]()}</section>
    <footer class="nav">
      ${step > 0 ? `<button data-back>${t("btn.back")}</button>` : "<span></span>"}
      <div class="btns">${skip}${next}</div>
    </footer>
  </main>`;
}

function settingsView(): string {
  const st = hooks.status;
  const names =
    typeof settings.screen === "object" && !screens.includes(settings.screen.named)
      ? [settings.screen.named, ...screens]
      : screens;
  const current = screenValue(settings.screen);
  const screenOpts: [string, string][] = [
    ["main", t("set.screen.main")],
    ["notch", t("set.screen.notch")],
    ...names.map((n): [string, string] => [`named:${n}`, n]),
  ];
  const langOpts: [string, string][] = [
    ["system", t("set.lang.system")],
    ["en", t("set.lang.en")],
    ["es", t("set.lang.es")],
  ];
  const langCurrent = settings.lang ?? "system";
  const check = (key: BoolKey, label: Key) =>
    `<label class="row"><span>${t(label)}</span><input type="checkbox" data-bool="${key}" ${settings[key] ? "checked" : ""}></label>`;
  const vol = Math.round(settings.volume * 100);
  const preview = diff
    ? `<div class="diff">${diffHtml(diff.lines)}</div>
       <div class="btns"><button class="primary" data-apply>${t("hooks.apply")}</button><button data-cancel>${t("btn.cancel")}</button></div>`
    : "";
  return `<main class="page">
    <h2>${t("set.hooks")}</h2>
    <section class="group">
      <div class="row">
        <span class="status s-${st}">${t(HOOK_LABEL[st])}</span>
        <span class="btns">
          <button data-show="install" ${busy ? "disabled" : ""}>${t(st === "missing" || st === "invalid_json" ? "hooks.connect" : "hooks.repair")}</button>
          ${st === "missing" ? "" : `<button data-show="uninstall" ${busy ? "disabled" : ""}>${t("hooks.disconnect")}</button>`}
        </span>
      </div>
      <div class="row block">
        <code class="path">${esc(hooks.settings_path)}</code>
        ${backupLine()}
        ${preview}
        ${errorLine()}
      </div>
    </section>
    <section class="group">
      <label class="row"><span>${t("set.screen")}</span>
        <select data-screen>${screenOpts.map(([v, l]) => `<option value="${esc(v)}" ${v === current ? "selected" : ""}>${esc(l)}</option>`).join("")}</select>
      </label>
      ${check("sound_enabled", "set.sounds")}
      <label class="row"><span>${t("set.volume")}</span>
        <span class="vol"><input type="range" min="0" max="100" step="1" value="${vol}" data-volume ${settings.sound_enabled ? "" : "disabled"}><output>${vol}</output></span>
      </label>
      ${check("open_on_pending", "set.open_on_pending")}
      ${check("launch_at_login", "set.launch_at_login")}
      <label class="row"><span>${t("set.language")}</span>
        <select data-lang>${langOpts.map(([v, l]) => `<option value="${v}" ${v === langCurrent ? "selected" : ""}>${esc(l)}</option>`).join("")}</select>
      </label>
    </section>
  </main>`;
}

function render(): void {
  document.title = t("set.title");
  root.innerHTML = settings.onboarded ? settingsView() : onboardingView();
}

root.addEventListener("click", (e) => {
  const el = (e.target as HTMLElement).closest<HTMLElement>("[data-show],[data-apply],[data-cancel],[data-next],[data-back],[data-done]");
  if (!el) return;
  if (el.dataset.show) void showDiff(el.dataset.show as api.HookAction);
  else if (el.hasAttribute("data-apply")) void apply();
  else if (el.hasAttribute("data-cancel")) { diff = null; render(); }
  else if (el.hasAttribute("data-next")) go(step + 1);
  else if (el.hasAttribute("data-back")) go(step - 1);
  else if (el.hasAttribute("data-done")) { settings.onboarded = true; save(); render(); }
});

// "change" llega al soltar el control de volumen y al cambiar teclas de flecha; no se re-pinta para no perder el foco.
root.addEventListener("change", (e) => {
  const el = e.target as HTMLElement;
  if (el instanceof HTMLInputElement && el.dataset.bool) {
    settings[el.dataset.bool as BoolKey] = el.checked;
    save();
  } else if (el instanceof HTMLInputElement && el.hasAttribute("data-volume")) {
    settings.volume = Number(el.value) / 100;
    save();
    play("toque");
  } else if (el instanceof HTMLSelectElement && el.hasAttribute("data-screen")) {
    settings.screen = screenFrom(el.value);
    save();
  } else if (el instanceof HTMLSelectElement && el.hasAttribute("data-lang")) {
    settings.lang = el.value === "system" ? null : (el.value as Lang);
    setLang(settings.lang);
    save();
    render();
  }
});

root.addEventListener("input", (e) => {
  const el = e.target as HTMLInputElement;
  if (el.hasAttribute("data-volume")) el.nextElementSibling!.textContent = el.value;
});

async function init(): Promise<void> {
  [settings, hooks, screens] = await Promise.all([
    api.getSettings(),
    api.hooksStatus(),
    api.listScreens().catch(() => [] as string[]),
  ]);
  setLang(settings.lang);
  configure({ enabled: settings.sound_enabled, volume: settings.volume });
  // Re-pinta solo si cambian el idioma o el paso de onboarding; el resto ya está en pantalla.
  api.onSettings((s) => {
    const prev = settings;
    const before = lang;
    settings = s;
    setLang(s.lang);
    if (lang !== before || s.onboarded !== prev.onboarded) render();
  });
  render();
}

init().catch((e: unknown) => {
  root.textContent = String(e);
});
