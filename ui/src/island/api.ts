// Puente con el backend de Tauri. Los tipos reflejan `Snapshot` en src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type SessionState = "en_espera" | "trabajando" | "pendiente" | "error";
export type MascotState = "dormida" | "cargando" | "alerta";
export type PermissionChoice = "allow" | "deny" | "always" | "terminal";
export type ActionKind = "bash" | "edit" | "write" | "read" | "search_project" | "search_web" | "question" | "other";

export interface LastAction {
  kind: ActionKind;
  arg: string | null;
}

export interface Session {
  session_id: string;
  project: string;
  cwd: string;
  state: SessionState;
  last_action: LastAction | null;
  pending: number;
  updated_ms: number;
}

export interface PendingView {
  request_id: string;
  session_id: string;
  project: string;
  kind: "permission" | "question";
  tool_name: string | null;
  tool_input: Record<string, unknown> | null;
  permission_suggestions: unknown[] | null;
}

export interface Snapshot {
  sessions: Session[];
  mascot: MascotState;
  pending: PendingView[];
}

export const getSnapshot = () => invoke<Snapshot>("get_snapshot");
export const resolvePermission = (requestId: string, choice: PermissionChoice) =>
  invoke<void>("resolve_permission", { requestId, choice });
/** `answers = null` → responder en la terminal. */
export const answerQuestion = (requestId: string, answers: Record<string, string> | null) =>
  invoke<void>("answer_question", { requestId, answers });
export const jumpToSession = (sessionId: string) => invoke<void>("jump_to_session", { sessionId });
/** Modo visible de la isla; el backend dimensiona la ventana del panel. */
export const setIslandMode = (mode: "hidden" | "pill" | "open", openHeight?: number) =>
  invoke<void>("set_island_mode", { mode, openHeight: openHeight ?? null });

export const onSnapshot = (cb: (s: Snapshot) => void) => listen<Snapshot>("pilas://snapshot", (e) => cb(e.payload));
export const onPending = (cb: (requestId: string) => void) => listen<string>("pilas://pending", (e) => cb(e.payload));
export const onHover = (cb: (inside: boolean) => void) => listen<boolean>("pilas://hover", (e) => cb(e.payload));
export const onFinished = (cb: (sessionId: string) => void) => listen<string>("pilas://finished", (e) => cb(e.payload));

// Ajustes y instalador: tipos reflejan el contrato del backend (docs/SPEC.md → Instalación, ajustes y onboarding).
export type ScreenChoice = "main" | "notch" | { named: string };

export interface Settings {
  screen: ScreenChoice;
  sound_enabled: boolean;
  volume: number; // 0..1
  open_on_pending: boolean;
  launch_at_login: boolean;
  lang: "en" | "es" | null; // null = idioma del sistema
  onboarded: boolean;
  check_updates: boolean;
  /** Disfraz de temporada; `auto` sigue la fecha (ver `ui/src/mascot/skins.ts`). */
  skin: "auto" | "classic" | "halloween";
}

export type HookStatus = "ok" | "missing" | "needs_repair" | "invalid_json";
export interface HooksInfo {
  status: HookStatus;
  settings_path: string;
}
export type HookAction = "install" | "uninstall";
export interface DiffLine {
  op: " " | "+" | "-";
  text: string;
}

export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) => invoke<void>("set_settings", { settings });
export const listScreens = () => invoke<string[]>("list_screens");
export const hooksStatus = () => invoke<HooksInfo>("hooks_status");
/** Rechaza con un texto de error (p. ej. JSON inválido). */
export const previewHooks = (action: HookAction) => invoke<DiffLine[]>("preview_hooks", { action });
/** Devuelve la ruta del backup, o `null` si no había nada que respaldar. */
export const applyHooks = (action: HookAction) => invoke<string | null>("apply_hooks", { action });
export const onSettings = (cb: (s: Settings) => void) => listen<Settings>("pilas://settings", (e) => cb(e.payload));

// Actualizaciones (src-tauri/src/updates.rs). `null` = al día.
export interface UpdateInfo {
  version: string;
  notes: string | null;
}
export const getUpdateStatus = () => invoke<UpdateInfo | null>("update_status");
/** Rechaza con un texto de error (p. ej. sin red). */
export const checkUpdatesNow = () => invoke<UpdateInfo | null>("check_updates_now");
/** Descarga, instala y reinicia Pilas. Solo al pulsar el botón. */
export const installUpdate = () => invoke<void>("install_update");
export const onUpdate = (cb: (u: UpdateInfo | null) => void) =>
  listen<UpdateInfo | null>("pilas://update", (e) => cb(e.payload));
