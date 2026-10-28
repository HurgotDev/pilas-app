// Textos de la interfaz: inglés por defecto y español. Sin dependencias.
export type Lang = "en" | "es";

const en = {
  "state.en_espera": "Idle",
  "state.trabajando": "Working",
  "state.pendiente": "Pending",
  "state.error": "Error",
  "dest.localSettings": "project (local)",
  "dest.projectSettings": "project",
  "dest.userSettings": "user",
  "dest.session": "this session",
  "sugg.directory": "Directory: {path}",
  "sugg.mode": "Mode: {mode}",
  "ago.now": "now",
  "ago.min": "{n} min ago",
  "ago.h": "{n} h ago",
  "card.question": "{project} has a question",
  "card.rules": "Claude Code will save these rules",
  "card.counter": " · {i} of {n}",
  "btn.allow": "Allow",
  "btn.always": "Always",
  "btn.deny": "Deny",
  "btn.save_allow": "Save and allow",
  "btn.back": "Back",
  "btn.prev": "‹ Back",
  "btn.next": "Next",
  "btn.send": "Send",
  "btn.terminal": "Answer in terminal",
  "btn.more": "show more",
  "btn.less": "show less",
  "act.bash": "Running {arg}",
  "act.edit": "Editing {arg}",
  "act.write": "Writing {arg}",
  "act.read": "Reading {arg}",
  "act.search_project": "Searching the project",
  "act.search_web": "Searching the web",
  "act.question": "Has a question",
  "act.other": "Using {arg}",
  "arg.command": "a command",
};

const es: Record<keyof typeof en, string> = {
  "state.en_espera": "En espera",
  "state.trabajando": "Trabajando",
  "state.pendiente": "Pendiente",
  "state.error": "Error",
  "dest.localSettings": "proyecto (local)",
  "dest.projectSettings": "proyecto",
  "dest.userSettings": "usuario",
  "dest.session": "esta sesión",
  "sugg.directory": "Directorio: {path}",
  "sugg.mode": "Modo: {mode}",
  "ago.now": "ahora",
  "ago.min": "hace {n} min",
  "ago.h": "hace {n} h",
  "card.question": "{project} tiene una pregunta",
  "card.rules": "Claude Code guardará estas reglas",
  "card.counter": " · {i} de {n}",
  "btn.allow": "Permitir",
  "btn.always": "Siempre",
  "btn.deny": "Denegar",
  "btn.save_allow": "Guardar y permitir",
  "btn.back": "Volver",
  "btn.prev": "‹ Atrás",
  "btn.next": "Siguiente",
  "btn.send": "Enviar",
  "btn.terminal": "Responder en la terminal",
  "btn.more": "ver más",
  "btn.less": "ver menos",
  "act.bash": "Ejecutando {arg}",
  "act.edit": "Editando {arg}",
  "act.write": "Escribiendo {arg}",
  "act.read": "Leyendo {arg}",
  "act.search_project": "Buscando en el proyecto",
  "act.search_web": "Buscando en la web",
  "act.question": "Tiene una pregunta",
  "act.other": "Usando {arg}",
  "arg.command": "un comando",
};

export type Key = keyof typeof en;

const dict = { en, es };

/** Idioma del sistema: `es*` → español, el resto → inglés. */
export const lang: Lang = navigator.language.startsWith("es") ? "es" : "en";

/** Traduce `key` y reemplaza `{name}` por `vars.name`. */
export function t(key: Key, vars: Record<string, string | number> = {}): string {
  return dict[lang][key].replace(/\{(\w+)\}/g, (m, k: string) => String(vars[k] ?? m));
}
