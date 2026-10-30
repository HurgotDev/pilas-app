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
| Skins de temporada | Clásica, Halloween y Navidad; automática por fecha |
| Temperatura de caché | Caliente, enfriándose, fría; aviso con sonido |

Fuera de alcance: otros agentes, chat con IA, integraciones, diffs en vivo, uso del plan, app móvil, atajos
globales, skins hechas por el usuario, notificaciones del sistema, firma/notarización, builds oficiales de Windows y Linux.

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
| `PreCompact` | No | Detecta la compactación por inactividad |

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
| Abierta | Clic en la píldora o la pilita, o llega un pendiente | Lista de sesiones o tarjeta; 420 × hasta 360 px |

- Clic en la píldora contraída (también sobre la pilita, en cualquier estado)
  la abre; sobre la pilita, además, hace su reacción y su sonido de toque
  (salvo en Alerta). Con la isla abierta, el clic en la pilita no la cierra:
  solo reacciona (aplastada, mareo con 3 clics). Clic en la píldora fuera de
  la pilita alterna abrir/cerrar, como hasta ahora.
- Pendiente nuevo: la isla se abre sola, suena la alerta, sin tomar el foco. Varios: FIFO con "1 de N".
- Si la pestaña de Ghostty de esa sesión ya tiene el foco (Ghostty al frente
  y su terminal enfocada con el mismo TTY; en sesiones en segundo plano, con
  el título `<glifo> <nombre>` igual al nombre que da `claude agents --json`
  para su `pid`), Pilas devuelve el control a la terminal al instante, como
  "Responder en la terminal": sin tarjeta, sin abrir la isla y sin sonido.
  Si luego cambias de pestaña no se avisa. Otras terminales o cualquier fallo
  al comprobarlo: se avisa como siempre.
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
Recordatorio (una vez, a los 60 s), Listo (arpegio), Aprobado (blip), Denegado (nota grave), Toque (boing),
Mareo (pajaritos: 8 notas triangulares que alternan y bajan, ~0,7 s; sustituye
al toque en el 3.er clic; los clics durante el mareo no inician otro).
Ajustes: activado + volumen (50 % por defecto). Nunca en bucle.

## Skins

Disfraces de temporada para la pilita. Solo cambian lo visual y el sonido:
estados, prioridad, reacciones, tiempos y lógica siguen idénticos. Todo vive
en código (SVG, CSS y notas de Web Audio): sin archivos externos, sin red y
sin dependencias nuevas. `classic` (la de siempre), `halloween` y
`christmas`.

**Regla de oro: el color del cuerpo sigue contando el estado.** Una skin
añade accesorios y retoca detalles, pero nunca cambia `--body`, `--shade`,
`--dark` ni `--foot`, ni oculta la cara, la exclamación o los LEDs.

### Activación

Ajuste "Costume" / "Disfraz" con cuatro opciones:

- Automático (por defecto): `halloween` del 15 de octubre al 2 de noviembre,
  ambos incluidos, y `christmas` del 1 al 26 de diciembre, ambos incluidos,
  con la fecha local; `classic` el resto del año.
- Clásico: siempre `classic`.
- Halloween: siempre `halloween`.
- Navidad ("Christmas"): siempre `christmas`.

En `settings.json` es `skin: "auto" | "classic" | "halloween" | "christmas"`. Un valor
desconocido (por ejemplo, de una versión más nueva) se lee como `"auto"` sin
perder el resto de los ajustes. La skin efectiva la resuelve una función pura
`resolveSkin(choice, date)` en `ui/src/skins/index.ts`. Se recalcula al
arrancar, al cambiar el ajuste y cada hora, así que el 3 de noviembre la
pilita vuelve sola a la clásica sin reiniciar la app.

### Halloween

- **Pilita**: `<pilas-mascot skin="halloween">` (sin atributo o `classic` =
  la de siempre). Sombrero de bruja sobre el borne, dentro del `rig`, así que
  respira, flota, se aplasta y se marea con ella. Los LEDs encendidos pasan a
  naranja calabaza. El disfraz completo se ve dentro de la píldora de 28 px
  sin recortarse; se aplica a todas las pilitas (píldora, isla abierta y
  onboarding).
- **Punto de estado** (filas de la lista de sesiones): silueta de calabaza
  pintada con el mismo color de estado de hoy; misma alineación y área de
  clic.
- **Contador de pendientes** (badge de la píldora): calabaza naranja con el
  número encima. Con 0 pendientes no se ve, en cualquier skin.
- **Ambiente de la isla** (píldora y abierta), siempre detrás del contenido,
  sin recibir clics y sin tapar texto ni botones:
  - Un murciélago cruza la isla de lado a lado en bucle, revoloteando.
    Sustituye al murciélago que iba junto a la pilita.
  - Resplandor naranja tenue en el borde inferior.
  - Telaraña fina en una esquina inferior de la isla abierta.
  - Uno o dos fantasmitas semitransparentes suben y se desvanecen de vez en
    cuando en la isla abierta.
  - Acentos naranja: separadores, enlaces y anillo de foco.
  - No cambian: los colores de estado ni Permitir/Denegar.
  - Con `prefers-reduced-motion` no hay murciélago volando ni fantasmas.
  - Solo CSS (y SVG en línea): sin temporizadores de JS ni imágenes externas.
- **Sonidos**: variantes "spooky" de Alerta, Listo, Toque y Mareo, con la
  misma forma de onda 8-bit, una duración parecida (±20 %) y el mismo
  volumen. Mareo cambia los pajaritos por murciélagos: chillidos agudos y
  cortos que alternan y bajan, ~0,7 s. El resto (Recordatorio, Aprobado,
  Denegado) usa los clásicos. Siguen respetando "activado" y el volumen, y
  nunca suenan en bucle.

### Navidad

- **Pilita**: `<pilas-mascot skin="christmas">`. Gorro de Papá Noel sobre el
  borne, dentro del `rig` (respira, flota, se aplasta y se marea con ella).
  Los LEDs encendidos pasan a luces navideñas alternas rojo/verde. Se ve
  entero en la píldora de 28 px sin recortarse.
- **Punto de estado**: silueta de bola de Navidad pintada con el color de
  estado de hoy; misma alineación y área de clic.
- **Contador de pendientes**: bola roja con el número encima. Con 0 no se ve.
- **Ambiente de la isla**, detrás del contenido y sin clics, sin tapar texto
  ni botones:
  - Copos de nieve que caen despacio, de distintos tamaños, repartidos por
    toda la isla.
  - Un muñeco de nieve troll vive en el borde inferior de la isla y encadena
    acciones al azar: caminar o correr hasta un punto al azar, pararse,
    saludar, tirar una bola de nieve (vuela en arco y se deshace, sin tocar
    a la pilita ni provocar reacciones), y darse la vuelta para enseñar el
    trasero meneándolo, o resbalarse y caer de espaldas para luego
    levantarse. Mide unos 18 px de alto, para que se vean los detalles.
    Ni el orden, ni la duración, ni el destino se
    repiten de forma fija.
  - Resplandor rojo en el borde inferior.
  - Acentos rojo/verde: separadores, enlaces y anillo de foco.
  - No cambian: los colores de estado ni Permitir/Denegar.
  - Con `prefers-reduced-motion` no cae nieve ni aparece el muñeco.
  - Nieve, resplandor y poses del muñeco en CSS y SVG en línea. Excepción a
    "sin temporizadores de JS": el muñeco usa un solo `setTimeout`
    encadenado que elige la siguiente acción. Solo corre con la skin
    `christmas` activa y sin reduced motion, y se detiene al cambiar de
    skin. La elección vive en una función pura
    (`ui/src/skins/christmas/snowman.ts`, `nextAction(rand, x)`) con tests en
    `snowman.test.ts`.
- **Sonidos**: variantes de cascabeles de Alerta, Listo, Toque y Mareo, misma
  forma de onda 8-bit, duración parecida (±20 %) y mismo volumen. El resto
  usa los clásicos. Nunca en bucle.

### Implementación

Registro de skins autocontenidas (Registry + Strategy). Cada skin es un
objeto `SkinDef` con la misma forma; los consumidores piden la activa al
registro y la usan sin nombrarla. Ningún consumidor tiene un `if` ni un
selector por nombre de skin.

```ts
// ui/src/skins/types.ts
interface SkinDef {
  id: Skin;
  label: Key;                                   // clave de i18n
  season?: { from: [number, number]; to: [number, number] }; // [mes, día]
  mascot?: { accessory: string; leds?: string[] };
  ambience?: string;                            // marcado de la capa de la isla
  sounds?: Partial<Record<Sound, Note[]>>;      // los que faltan: clásicos
  behavior?: { start(layer: HTMLElement): void; stop(): void };
}
```

- Solo `id` y `label` son obligatorios. `classic` es la skin vacía: todo lo
  demás cae a los valores por defecto (sin accesorio, LEDs amarillos, sin
  ambiente, sonidos clásicos, nunca elegida por Automático).
- **Identidad vs. vestuario.** La identidad de la pilita (cuerpo, cara,
  colores de estado, poses, gestos, reacciones y tiempos) vive solo en
  `pilita.ts` y ninguna skin la recibe. Una skin no aporta CSS a la pilita:
  `accessory` es un SVG que la pilita coloca en su ranura sobre el borne,
  dentro del `rig`, y `leds` tiñe los LEDs encendidos (se reparten en ciclo
  si hay menos colores que LEDs).
- `behavior` solo para lo que necesita JS (el muñeco de nieve). Recibe solo
  la capa de ambiente de su skin, nunca la pilita. La isla llama `stop()` en
  la anterior y `start()` en la nueva; con reduced motion no llama `start()`.

Archivos:

- `ui/src/skins/types.ts`: `SkinDef`.
- `ui/src/skins/index.ts`: registro `SKINS` (`satisfies Record<Skin,
  SkinDef>`), tipos `Skin`/`SkinChoice` y `resolveSkin(choice, date)`, que
  devuelve la skin cuya `season` contiene la fecha (admite rangos que cruzan
  el año, p. ej. 24 dic → 6 ene) o `classic`. Importable desde Node: sin DOM
  al importar ni imports de CSS.
- `ui/src/skins/<id>/`: `index.ts` con su `SkinDef` y, si lo necesita,
  `<id>.css` (punto de estado, contador, ambiente y acentos bajo
  `:root[data-skin="<id>"]`). Los CSS se cargan todos con
  `import.meta.glob` desde la entrada de la isla.
- `ui/src/skins/christmas/snowman.ts`: `nextAction(rand, x)` pura, con tests.
- `ui/src/mascot/pilita.ts`: atributo observado `skin`; pinta el accesorio y
  los LEDs de la skin activa del registro.
- `ui/src/island/island.ts`: monta el `ambience` de cada skin en su capa,
  fija `data-skin` en la raíz y gestiona `behavior`.
- `ui/src/sound/sfx.ts`: `SKINS[skin].sounds?.[name] ?? CLASSIC[name]`.
- `ui/src/settings.ts` y `/playground.html`: opciones generadas desde el
  registro ("Automático" fijo en Ajustes). Textos en `ui/src/i18n.ts`.
- `src-tauri/src/settings.rs`: el enum `Skin` se queda (valida lo leído de
  `settings.json`; desconocido → `auto`). `ui/src/skins/ids.json` es la lista
  compartida: Rust la lee en un test y TS comprueba que coincide con las
  claves de `SKINS`.

Añadir una skin = su carpeta, una línea en `SKINS`, su id en `ids.json`, la
variante en Rust y sus claves en `i18n.ts`.

### Pruebas

- Rust (`cargo test -p pilas`): `skin` falta →
  `auto`; valor desconocido → `auto` sin perder `volume` ni `lang`; ida y
  vuelta de `classic`/`halloween`/`christmas`.
- TS (`npm run test:ui`, nuevo script =
  `node --test` con los tests de `ui/src/skins/`; Node ≥ 22.18 quita los tipos
  solo, sin dependencias nuevas): `resolveSkin` en 14 oct → `classic`,
  15 oct y 2 nov → `halloween`, 3 nov → `classic`; 30 nov → `classic`,
  1 dic y 26 dic → `christmas`, 27 dic → `classic`; las opciones fijas
  ignoran la fecha; un rango que cruza el año (24 dic → 6 ene) contiene el
  31 dic y el 1 ene.
- TS, registro: las claves de `SKINS` coinciden con `ids.json` y cada `id`
  con su clave; una skin con solo `id` y `label` no rompe a ningún
  consumidor puro.
- TS, límites (cadenas, para cada skin): `accessory` no contiene `<style`,
  `class="v`, `--body`, `--shade`, `--dark`, `--foot`, `rig` ni
  `data-state`; ningún CSS de skin selecciona `pilas-mascot` ni `.lit`, y
  todos sus selectores van bajo `:root[data-skin="<id>"]`.
- A mano en el playground y en la app: los 4 estados y las 3 reacciones con
  cada skin, en la píldora y en la isla abierta, con y sin reduced-motion.

### Límites

- Siempre: color de estado intacto; textos vía `t()`; `npm run typecheck`,
  `cargo clippy --workspace -- -D warnings` y los tests en verde.
- Preguntar antes: otra skin, cambiar la ventana de fechas, más adornos en
  la isla de los descritos arriba, añadir una dependencia o un runner de
  tests.
- Nunca: descargar skins o assets, cambiar el comportamiento por la skin,
  sonidos en bucle, cambiar los colores de estado, dar a una skin acceso a
  la cara, poses, reacciones, tiempos o colores de estado de la pilita. Un
  gesto o animación nueva de la pilita no es de una skin: se especifica en
  "Mascota: la pilita" y vale para todas, `classic` incluida.

## Temperatura de caché

Muestra si la caché del prompt de la **conversación principal** de cada
sesión sigue caliente, se está enfriando o ya está fría, y avisa antes de que
expire para decidir si seguir o compactar. Los subagentes no cuentan.

### Lo que dice Claude Code

Verificado el 30 oct 2026 con Claude Code 2.1.314 en
https://code.claude.com/docs/en/prompt-caching, `/hooks`, `/statusline` y
`/settings-reference#idlecompaction`.

- TTL por petición, en dos cubos. Conversación principal: 1 h con suscripción
  dentro del plan; 5 min con usage credits, API key o proveedor cloud (al
  pasar a usage credits baja a 5 min). "Todo lo demás" (subagentes,
  compactación, títulos): 5 min, salvo ayudantes que decide el servidor.
- Precedencia: `FORCE_PROMPT_CACHING_5M=1` → variable del cubo
  (`CLAUDE_CODE_PROMPT_CACHE_TTL`) → ajuste (`promptCacheTtl`) →
  `ENABLE_PROMPT_CACHING_1H=1` → defecto. Solo `5m` o `1h`.
  `DISABLE_PROMPT_CACHING*` la apaga.
- Cada petición que acierta reinicia el timer.
- `usage.cache_creation.ephemeral_1h_input_tokens` /
  `ephemeral_5m_input_tokens` dice el TTL de cada escritura. La doc lo cita
  en la salida de `claude -p --output-format json`; en el transcript JSONL
  aparece en cada línea `type: "assistant"` junto a `timestamp` e
  `isSidechain`, pero **ese formato no es un contrato documentado**: se lee
  de forma tolerante.
- La statusline recibe `prompt_cache` (`warm`, `ttl`, `expires_at`,
  `hit_ratio`…) desde 2.1.251. No se usa: solo cabe una statusline y la del
  usuario no se toca.
- Compactación por inactividad (`idleCompaction`, 2.1.290+): solo con TTL de
  1 h, autocompactación activa, contexto ≥ 200 000 tokens y ~1 h ociosa. Los
  hooks `PreCompact`/`PostCompact` la reportan con `trigger: "auto"`, **igual
  que la autocompactación por ventana**; no hay valor propio.
- `/compact` invalida la capa de conversación (system prompt y contexto del
  proyecto pueden seguir en caché). `/clear` empieza conversación nueva:
  `SessionEnd` `reason: "clear"` y `SessionStart` `source: "clear"`.

### Estados

`caliente`, `enfriandose`, `fria` y `compactada`. Sin dato (sesión sin
respuestas todavía, caché apagada o ajuste desactivado) no hay estado.

```
restante = ultima_peticion + ttl − ahora
aviso    = min(umbral, ttl / 2)
compactada si hubo compactación por inactividad
fria       si restante ≤ 0
enfriandose si restante ≤ aviso
caliente   en otro caso
```

El tope `ttl / 2` evita que con TTL de 5 min y umbral de 5 o más la sesión
nazca "enfriándose".

### Reloj por sesión (`crates/pilas-core/src/cache.rs`)

Solo cuentan los eventos **sin `agent_id`** (los de subagentes no tocan la
caché principal).

| Evento | Efecto |
| --- | --- |
| `UserPromptSubmit` | Última petición = ahora; quita `compactada` |
| `PreToolUse`, `PostToolUse` | Última petición = ahora |
| `Stop` | Última petición = ahora; lee el transcript a los 1,5 s |
| `StopFailure` | Nada (la petición falló) |
| `PreCompact` `auto` en espera | Marca `compactada` (inactividad) |
| `PreCompact` `auto` trabajando | Nada (compactación por ventana) |
| `SessionStart` `startup`/`clear` | Sin dato |
| `SessionStart` `compact` | Mantiene `compactada`; si no, sin dato |
| `SessionStart` `resume` | Lee el transcript |
| `SessionEnd` | Elimina la sesión (como hoy) |

`PreToolUse` y `PostToolUse` cuentan porque marcan el final de una respuesta
y el inicio de la siguiente petición. Con un permiso pendiente o una
herramienta larga el reloj corre desde ahí, que es lo real.

**Lectura del transcript** (`transcript_path` del payload; función pura sobre
el texto, IO en la app): los últimos 512 KB, de abajo arriba, solo líneas
`type: "assistant"` con `isSidechain` distinto de `true`. Devuelve el
`timestamp` más reciente (afina la última petición), el TTL de la escritura
más reciente con tokens de creación (1 h si `ephemeral_1h_input_tokens` > 0,
si no 5 min) y si se observó caché (lecturas o escrituras > 0). Líneas
inválidas se saltan; un fallo deja el reloj como estaba. Nunca se guarda ni
se registra contenido del transcript.

**TTL**: el detectado; si aún no hay, el último conocido de la sesión; si no,
**5 min estimado** (nunca dice "caliente" cuando podría estar fría). La UI
marca el estimado con `~`.

**Override de desarrollo**: `PILAS_CACHE_TTL_SECS=<n>` sustituye el TTL de
todas las sesiones, solo en builds de debug (`cfg!(debug_assertions)`); la
release lo ignora.

### Casos borde

- **Varias sesiones**: un reloj por sesión. La pilita muestra el peor estado
  accionable: `enfriandose` > `fria` > `caliente` (`compactada` no cuenta).
- **Pilas abierta tarde**: al arrancar, `claude agents --json` (local, ya se
  usa en `pilas-jump`) da las sesiones `interactive` con `pid`, `cwd` y
  `sessionId`. Se registran (en espera, o trabajando si `status: "busy"`) y
  el reloj se arranca con su transcript
  (`~/.claude/projects/*/<sessionId>.jsonl`). Una sesión desconocida que
  llega por un evento también lee su transcript la primera vez.
- **Sesiones que mueren sin `SessionEnd`**: la poda por `pid` de siempre
  (cada 30 s) se lleva también su reloj.
- **Mac dormido**: se usa reloj de pared; al despertar el estado es el real.

### Interfaz

- **Pilita**: atributo observado `cache="caliente|enfriandose|fria"`. Solo
  se ve en estado Dormida (en Cargando la sesión trabaja; Alerta y Llena
  mandan). Nunca cambia el color del cuerpo; vale para todas las skins.
  - Caliente: dos hilitos de vapor sobre el borne (2,4 s, sutil).
  - Enfriándose: tirita (ráfagas cortas cada 2 s) y un copo junto al borne.
  - Fría: copo quieto, sin movimiento.
  - Con `prefers-reduced-motion`: solo los iconos, sin animación.
- **Píldora**: con `enfriandose` y sin pendientes, a la derecha una cuenta
  atrás `m:ss` de la sesión que expira antes. Clic en ella salta a la
  terminal de esa sesión sin abrir la isla.
- **Lista**: cada fila suma un chip tras el tiempo: "warm · 48m",
  "cooling · 3:12", "cold" o "compacted" (vía `t()`). `title` con el TTL y si
  es detectado o estimado. Clic en la fila salta, como hoy.
- **Sonido** `frio`: tres notas triangulares descendentes (~0,5 s), una vez
  por cada paso a `enfriandose`, si el ajuste lo permite y el sonido global
  está activado. Nunca en bucle. Las skins pueden darle variante.
- La isla **no** se abre sola por la caché: no es un pendiente.

### Contrato con la app

- `Session.cache: { temp, expires_ms, ttl_s, ttl_detected } | null` y
  `Snapshot.cache_mascot: temp | null` en `src-tauri/src/lib.rs`, reflejados
  en `ui/src/island/api.ts`.
- Un hilo revisa los relojes cada 5 s; si algún estado cambia emite el
  snapshot, y por cada paso a `enfriandose` emite `pilas://cache-cooling`
  con el `session_id`.
- El instalador añade `PreCompact` (sin matcher, sin timeout). Las
  instalaciones previas salen como "necesita reparar" y el flujo de reparar
  de siempre muestra el diff. Los hooks ajenos y la statusline no se tocan.

### Ajustes

| Ajuste | Opciones | Por defecto |
| --- | --- | --- |
| Temperatura de caché | Sí / No | Sí |
| Avisar antes de enfriarse | 1 / 2 / 5 / 10 / 15 min | 5 min |
| Sonido al enfriarse | Sí / No | Sí |

En `settings.json`: `cache_enabled`, `cache_warn_min`, `cache_sound`.
Desactivado: sin estados, sin lecturas de transcript y sin sonido.

### ¿Lanzar `/compact` desde Pilas?

No en esta versión. Claude Code no ofrece ninguna vía para ordenarle una
compactación: los hooks solo responden a eventos y ninguna salida dispara
`/compact`. Lo único posible sería escribir `/compact⏎` en la terminal con
AppleScript de Ghostty, y no es seguro:

- El salto solo es exacto por id o TTY; los respaldos (cwd, `activate`)
  podrían escribir en otra pestaña o en una shell.
- Se pegaría a lo que el usuario tenga a medio escribir en el prompt.
- Con Claude trabajando se encola como mensaje en vez de compactar.
- Abre un canal de escritura en la terminal que hoy Pilas no tiene.

Desde el aviso se salta a la terminal y el usuario decide.

### Pruebas

- `pilas-core`: fronteras de `temperature` (justo antes y después del
  aviso y del TTL) y el tope `ttl / 2`; eventos con `agent_id` ignorados;
  `PreCompact` `auto` en espera → `compactada`, trabajando → nada;
  `SessionStart` por cada `source`; `UserPromptSubmit` quita `compactada`.
- Transcript: 1 h, 5 min, una lectura sin escritura conserva el TTL de la
  escritura anterior, `isSidechain: true` ignorado, líneas inválidas,
  caché apagada (todo a 0) → sin estado.
- Instalador: `PreCompact` se añade y se quita sin tocar lo ajeno.
- A mano, con `PILAS_CACHE_TTL_SECS=120 npm run app` y umbral de 1 min:
  caliente → enfriándose a los 60 s (tirita, sonido, cuenta atrás) → fría a
  los 120 s; un prompt nuevo la vuelve caliente; dos sesiones a la vez con
  estados distintos; abrir Pilas con una sesión ya abierta.

Trabajo futuro (fuera de alcance): pings de keep-alive que mantengan la
caché caliente; lanzar `/compact` solo con coincidencia exacta de terminal,
sesión en espera y confirmación.

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
| Disfraz | Automático / Clásico / Halloween / Navidad | Automático |

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
siguen mandando · sin privilegios · el instalador siempre muestra diff y hace backup · logs mínimos sin prompts ·
la temperatura de caché solo lee números de uso del transcript local de cada sesión.

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
- Con el disfraz en Automático, la pilita lleva el de Halloween del 15 oct al
  2 nov y el de Navidad del 1 al 26 dic, y vuelve a la clásica sola;
  Clásico, Halloween y Navidad lo fijan. En los 4
  estados el color del cuerpo es el de siempre y el sombrero no se recorta en
  la píldora.

## Roadmap y riesgos

La v1 está completa (v0.3.0). El siguiente alcance aún no está definido; la
lista de abajo son solo ideas, no compromisos.

Después de la v1: más terminales (iTerm2, Terminal.app, VS Code) y notificación de respaldo · Linux y Windows ·
Codex como segundo agente · diff antes de permitir, atajos globales, más skins de temporada.

| Riesgo | Mitigación |
| --- | --- |
| Tauri no logra el comportamiento NSPanel | M1 es el primer hito; respaldo con `objc2` |
| Cambios en el contrato de hooks | El hook reenvía todo; interpretación en `pilas-core` con tests |
| TTY no coincide con Ghostty | Respaldo por cwd y fallo cerrado |

Preguntas abiertas: ¿el diccionario de Ghostty expone el TTY? · ¿qué plugin NSPanel para Tauri 2 está mantenido? ·
¿nombre y mascota bajo MIT o con derechos reservados antes de la primera release pública?
