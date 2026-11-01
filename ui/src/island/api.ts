// Types mirror `Snapshot` in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SkinChoice } from "../skins/index.ts";

export type SessionState = "idle" | "working" | "pending" | "error";
export type MascotState = "sleeping" | "working" | "alert";
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
/** `answers = null` → answer in the terminal. */
export const answerQuestion = (requestId: string, answers: Record<string, string> | null) =>
  invoke<void>("answer_question", { requestId, answers });
export const jumpToSession = (sessionId: string) => invoke<void>("jump_to_session", { sessionId });
export const setIslandMode = (mode: "hidden" | "pill" | "open", openHeight?: number) =>
  invoke<void>("set_island_mode", { mode, openHeight: openHeight ?? null });

export const onSnapshot = (cb: (s: Snapshot) => void) => listen<Snapshot>("pilas://snapshot", (e) => cb(e.payload));
export const onPending = (cb: (requestId: string) => void) => listen<string>("pilas://pending", (e) => cb(e.payload));
export const onHover = (cb: (inside: boolean) => void) => listen<boolean>("pilas://hover", (e) => cb(e.payload));
export const onFinished = (cb: (sessionId: string) => void) => listen<string>("pilas://finished", (e) => cb(e.payload));

export type ScreenChoice = "main" | "notch" | { named: string };

export interface Settings {
  screen: ScreenChoice;
  sound_enabled: boolean;
  volume: number; // 0..1
  open_on_pending: boolean;
  launch_at_login: boolean;
  lang: "en" | "es" | null; // null = system language
  onboarded: boolean;
  check_updates: boolean;
  /** `auto` follows the date (see `ui/src/skins/index.ts`). */
  skin: SkinChoice;
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
/** Rejects with an error code (e.g. `settings_not_json`), shown through `errorText`. */
export const previewHooks = (action: HookAction) => invoke<DiffLine[]>("preview_hooks", { action });
/** Backup path, or `null` if there was nothing to back up. */
export const applyHooks = (action: HookAction) => invoke<string | null>("apply_hooks", { action });
export const onSettings = (cb: (s: Settings) => void) => listen<Settings>("pilas://settings", (e) => cb(e.payload));

// src-tauri/src/updates.rs. `null` = up to date.
export interface UpdateInfo {
  version: string;
  notes: string | null;
}
export const getUpdateStatus = () => invoke<UpdateInfo | null>("update_status");
/** Rejects with an error code (e.g. `update_check_failed`), shown through `errorText`. */
export const checkUpdatesNow = () => invoke<UpdateInfo | null>("check_updates_now");
/** Downloads, installs and restarts Pilas. Only on an explicit button press. */
export const installUpdate = () => invoke<void>("install_update");
export const onUpdate = (cb: (u: UpdateInfo | null) => void) =>
  listen<UpdateInfo | null>("pilas://update", (e) => cb(e.payload));
