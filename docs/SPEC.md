# Pilas — Especificación v1

> Versión en el repo de la spec acordada. Si algo cambia, actualiza este archivo en el mismo commit.

## Resumen

Pilas es una isla minimalista en el notch (o en el borde superior de la pantalla) con una mascota animada
que te avisa cuando Claude Code te necesita y te deja responder sin cambiar de ventana. El nombre juega con
"¡pilas!" (¡atento!) y con la mascota: una pilita cuya carga cuenta el estado.

Principios: nunca bloquear Claude Code · minimalista · local y privado (sin red salvo buscar actualizaciones, sin cuentas, sin telemetría) ·
multiplataforma por diseño, macOS primero · extensible a otros agentes aunque la v1 solo soporte Claude Code.

## Alcance v1

Un agente (Claude Code), una plataforma validada (macOS) y una terminal con salto exacto (Ghostty).

| Funcionalidad | Notas |
| --- | --- |
| Isla en el notch / borde superior | Píldora centrada; se esconde sin sesiones |
| Mascota vectorial con 4 estados | Dormida, cargando, alerta, llena |
| Reacciones: hover y clic | Saludo, aplastarse, mareo con 3 clics |
| Lista de sesiones activas | Proyecto, estado, última acción, tiempo |
| Aprobar permisos | Permitir / Denegar / Siempre |
| Responder AskUserQuestion | Selección única o múltiple, hasta 4 preguntas |
| "Responder en la terminal" | Devuelve el control al prompt nativo |
| Saltar a la terminal | Exacto en Ghostty; resto: traer la app al frente |
| Sonidos 8-bit generados en código | Con opción de silenciar |
| Instalador de hooks con backup y diff | |
| Ajustes mínimos | Pantalla, sonido, idioma, inicio con el sistema |
| Idiomas | Inglés (por defecto) y español |

Fuera de alcance: otros agentes, chat con IA, integraciones, diffs en vivo, uso del plan, app móvil, atajos
globales, vestuario de la mascota, notificaciones del sistema, firma/notarización, builds oficiales de Windows y Linux.

## Stack y arquitectura

Dos binarios Rust: la app Tauri 2 (siempre corriendo) y `pilas-hook` (lo lanza Claude Code en cada evento).
El hook no interpreta nada: reenvía el JSON por el socket y, si el evento es bloqueante, imprime la respuesta
que arma `pilas-core`. La UI es una WebView que recibe el estado por eventos de Tauri y devuelve decisiones por comandos.

```
Claude Code (en Ghostty) ──stdin/stdout──▶ pilas-hook ──socket Unix──▶ App Pilas (Tauri 2)
                                                                        ├─ servidor de socket
                                                                        ├─ pilas-core (sesiones, respuestas)
                                                                        ├─ UI WebView (isla, pilita, sonidos)
                                                                        ├─ pilas-jump ──AppleScript──▶ Ghostty
                                                                        └─ instalador de hooks (settings.json)
```

## Integración con Claude Code

Referencia: https://code.claude.com/docs/en/hooks. Todos los hooks apuntan a `pilas-hook` en forma exec
(`command` + `args`) para que el proceso padre sea Claude Code y no un shell.

| Evento | Bloqueante | Uso |
| --- | --- | --- |
| `SessionStart` | No | Registra la sesión: `session_id`, `cwd`, TTY, proyecto |
| `UserPromptSubmit` | No | Pasa a Trabajando |
| `PreToolUse` | Solo `AskUserQuestion` | Última acción; para AskUserQuestion espera la respuesta |
| `PermissionRequest` | Sí | Tarjeta de permiso |
| `PostToolUse` | No | Vuelve a Trabajando |
| `Notification` (`idle_prompt`) | No | Claude espera un prompt |
| `Stop` | No | Turno terminado; guarda `last_assistant_message` |
| `StopFailure` | No | Error de API |
| `SessionEnd` | No | Elimina la sesión |

**Permisos** (`hookSpecificOutput.decision`): Permitir `{"behavior":"allow"}` · Denegar
`{"behavior":"deny","message":"Denegado desde Pilas"}` · Siempre `{"behavior":"allow","updatedPermissions":[…]}`
reenviando las `permission_suggestions` del payload · Responder en la terminal: salida vacía. El código 2 no se
respeta en `PermissionRequest`.

**AskUserQuestion**: se intercepta en `PreToolUse`. Las respuestas solo entran por `updatedInput`:
`permissionDecision: "allow"` + `updatedInput` = `tool_input` original + `answers` (texto de la pregunta →
etiqueta; varias separadas por coma). "Responder en la terminal" = `permissionDecision: "ask"`. Claude Code también
dispara `PermissionRequest` para AskUserQuestion: se ignora en silencio para no duplicar la tarjeta.

**Fallos**: si la app no está, el hook sale al instante con 0 y sin salida. Hooks bloqueantes con `timeout` de
3600 s; si vence, Claude Code muestra su diálogo nativo. Nunca `onFailure: "block"`.

## Protocolo IPC

NDJSON sobre socket local; una conexión por invocación. macOS: `~/Library/Application Support/Pilas/pilas.sock`
(permisos 0600, carpeta 0700). `PILAS_SOCKET` lo sobrescribe. Tipos en `crates/pilas-core/src/protocol.rs`.

```json
{ "v": 1, "id": "uuid", "agent": "claude-code", "event": "PermissionRequest", "blocking": true,
  "tty": "/dev/ttys004", "pid": 48211, "term_program": "ghostty", "payload": { "...": "JSON original" } }
```
Respuesta (solo bloqueantes): `{ "v": 1, "id": "uuid", "stdout": { … } | null }`. El hook imprime `stdout` tal cual.
El hook nunca reintenta; la app aplica una decisión solo a la conexión con ese `id`; si se cerró, descarta la tarjeta.

## Sesiones y estados

`SessionStart → En espera → (UserPromptSubmit) → Trabajando → (permiso o pregunta) → Pendiente → (respondes) →
Trabajando → (Stop) → En espera`. `StopFailure → Error → (nuevo prompt) → Trabajando`. `SessionEnd` elimina desde
cualquier estado. Al pasar a En espera por `Stop` la pilita se llena ~4 s. Las sesiones viven en memoria; si un
`pid` deja de existir sin `SessionEnd`, se elimina en la revisión siguiente (cada 30 s).

## Interfaz

Isla negra, texto blanco, nunca roba el foco salvo cuando el usuario la abre.

| Tamaño | Cuándo | Contenido |
| --- | --- | --- |
| Oculta | Sin sesiones | Área de hover de 6 px en el borde superior |
| Píldora | Hay sesiones | Pilita a la izquierda, contador de pendientes a la derecha; 32 px de alto |
| Abierta | Clic en la píldora (no en la pilita, salvo en Alerta) o llega un pendiente | Lista de sesiones o tarjeta; 420 × hasta 360 px |

- Pendiente nuevo: la isla se abre sola, suena la alerta, sin tomar el foco. Varios: FIFO con "1 de N".
- Clic fuera o `Esc` contrae a píldora; el pendiente sigue en cola.
- Lista: pendientes arriba, luego por actividad. Fila: punto de estado, proyecto, última acción, tiempo. Clic salta a la terminal.
- Tarjeta de permiso: proyecto + herramienta, detalle en monoespaciada (6 líneas y "ver más"), Permitir / Siempre /
  Denegar, enlace "Responder en la terminal". `Enter` permite, `Esc` cierra sin decidir.
- Tarjeta de pregunta: `header` como etiqueta, pregunta y opciones con descripción; única marca y avanza, múltiple
  con casillas y Enviar; varias preguntas en pasos "2/3" con "Atrás" que conserva lo elegido.
- Pantallas: por defecto la principal. Con notch, la píldora se centra sobre él; sin notch, cuelga del centro del
  borde superior bajo la barra de menú. Ajuste: Principal / La del notch / Una concreta. Se recoloca sola.

## Idiomas

Inglés por defecto y español. Textos en `ui/src/i18n.ts` (diccionario `en`/`es`
y `t()`, sin dependencias). Idioma: el del ajuste; si no hay, el del sistema
(`es*` → español, resto → inglés). `pilas-core` no arma frases: manda la última
acción como datos (`{ kind, arg }`) y la UI la traduce. El mensaje de denegar
va a Claude, no al usuario: siempre en inglés ("Denied from Pilas").

## Mascota: la pilita

Batería vertical vectorial con estilo suave: cuerpo squircle, ojos grandes con brillos, rubor, patitas, borne
metálico y franja de 4 LEDs. El color de la carcasa cuenta el estado; los LEDs, la carga. Implementada como
`<pilas-mascot>` en `ui/src/mascot/pilita.ts` (SVG en Shadow DOM, keyframes CSS, sin dependencias, respeta
`prefers-reduced-motion`, emite `pilita-click`). ~28 px de alto en la píldora, 96 px en la isla abierta.

| Estado | Color | Cara | LEDs | Movimiento |
| --- | --- | --- | --- | --- |
| Dormida | Pizarra | Ojos cerrados, "z" | Apagados | Respira (3 s) |
| Cargando | Azul | Ojos abiertos, boquita "w", rubor | En secuencia | Flota (1,4 s) |
| Alerta | Rojo | Ojos de susto, "O", exclamación | Apagados | Se sacude (0,45 s) |
| Llena | Verde | Ojos ^ ^, sonrisa abierta, destellos | Los 4 | Salta; la app cambia de estado a los 4 s |

Prioridad: Alerta > Llena > Cargando > Dormida. Reacciones: hover → saludo (1,1 s), clic → aplastada (0,9 s),
3 clics en < 1 s → mareo (2 s). Conservan el color del estado; nunca en Alerta.

## Sonidos

Web Audio, ondas cuadrada y triangular, sin archivos (`ui/src/sound/sfx.ts`). Alerta (dos notas ascendentes),
Recordatorio (una vez, a los 60 s), Listo (arpegio), Aprobado (blip), Denegado (nota grave), Toque (boing).
Ajustes: activado + volumen (50 % por defecto). Nunca en bucle.

## Saltar a la terminal

Ghostty ≥ 1.3 expone AppleScript (https://ghostty.org/docs/features/applescript): jerarquía app → ventana →
pestaña → terminal, ids estables, `focus`. Algoritmo: (1) si no es Ghostty, `activate` de su app; (2) id cacheado
→ `focus`; (3) coincidencia única por TTY; (4) coincidencia única por working directory; (5) si nada, solo
`activate`. El cwd que reporta Ghostty es el último `cd` de la shell (OSC 7), así que falla con worktrees.
macOS pedirá permiso de Automatización la primera vez. Código en `crates/pilas-jump` detrás de `TerminalJumper`.

## Instalación, ajustes y onboarding

App de barra de menú sin Dock (`ActivationPolicy::Accessory`). Onboarding: bienvenida → conectar con Claude Code
(muestra el diff de `~/.claude/settings.json`) → aviso de Automatización → reiniciar sesiones abiertas.
El último paso incluye el interruptor "Iniciar con el sistema" (por defecto Sí).
Instalador: copia `pilas-hook` a `~/Library/Application Support/Pilas/bin/`, backup fechado, fusiona sin borrar
hooks ajenos (identifica los suyos por la ruta del binario), no escribe si el JSON es inválido, verifica y
ofrece reparar al iniciar, desinstala solo lo suyo.

| Ajuste | Opciones | Por defecto |
| --- | --- | --- |
| Pantalla | Principal / La del notch / Una concreta | Principal |
| Sonidos | Activado + volumen | Activado, 50 % |
| Abrir isla al llegar un pendiente | Sí / No | Sí |
| Iniciar con el sistema | Sí / No | Sí |
| Buscar actualizaciones | Sí / No | Sí |

Actualizaciones: con el ajuste activado, la app consulta
`https://github.com/HurgotDev/pilas-app/releases/latest/download/latest.json`
al arrancar y cada 24 h (`tauri-plugin-updater`, firmado con clave propia de
Tauri). Si hay versión nueva lo indica en el menú de la pilita y en Ajustes;
solo instala y reinicia cuando el usuario lo pide. Es la única petición de red
de la app y no envía datos del usuario. Al arrancar, si los hooks están
instalados y el `pilas-hook` copiado difiere del que trae la app, se vuelve a
copiar (sin tocar `settings.json`). También se distribuye como cask en el tap
`HurgotDev/homebrew-tap` (`auto_updates true`), que el workflow de release
actualiza en cada versión.

## Seguridad y privacidad

Sin puertos TCP y sin red salvo la búsqueda de actualizaciones (desactivable) · sin telemetría · socket 0600 · decisiones atadas al `id` exacto · nada se aprueba solo ·
"Siempre" solo con reglas sugeridas por Claude Code y mostradas antes · las reglas `deny`/`ask` de Claude Code
siguen mandando · sin privilegios · el instalador siempre muestra diff y hace backup · logs mínimos sin prompts.

## Criterios de aceptación v1 (macOS + Ghostty, pantalla del MacBook y monitor externo)

- Con Pilas cerrada, Claude Code funciona igual y ningún hook añade más de 50 ms perceptibles.
- Una sesión nueva aparece en menos de 1 s; con 3 en paralelo cada una muestra su estado correcto.
- Permitir, Denegar y Siempre funcionan; "Siempre" hace que la siguiente llamada igual no pida permiso.
- AskUserQuestion única, múltiple y de 3 preguntas se responden bien; sin tarjetas duplicadas.
- "Responder en la terminal" muestra el prompt nativo; responder en la terminal cierra la tarjeta sola.
- Dos pendientes simultáneos en FIFO con "1 de 2".
- Clic en una sesión enfoca la pestaña exacta de Ghostty; sin coincidencia exacta, solo trae Ghostty al frente.
- La isla se posiciona bien en notch y monitor externo y se recoloca al cambiar monitores.
- Estados, reacciones y sonidos funcionan; silenciar los apaga.
- El instalador hace backup, muestra diff, conserva hooks ajenos y desinstala solo los suyos.
- La isla nunca roba el foco mientras escribes en otra app.

## Roadmap y riesgos

Después de la v1: más terminales (iTerm2, Terminal.app, VS Code) y notificación de respaldo · Linux y Windows ·
Codex como segundo agente · diff antes de permitir, atajos globales, vestuario.

| Riesgo | Mitigación |
| --- | --- |
| Tauri no logra el comportamiento NSPanel | M1 es el primer hito; respaldo con `objc2` |
| Cambios en el contrato de hooks | El hook reenvía todo; interpretación en `pilas-core` con tests |
| TTY no coincide con Ghostty | Respaldo por cwd y fallo cerrado |

Preguntas abiertas: ¿el diccionario de Ghostty expone el TTY? · ¿qué plugin NSPanel para Tauri 2 está mantenido? ·
¿nombre y mascota bajo MIT o con derechos reservados antes de la primera release pública?
