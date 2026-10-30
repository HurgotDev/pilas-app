# Plan: temperatura de caché

Fuente: `docs/SPEC.md` → "Temperatura de caché" (aprobada el 30 oct 2026).
Rama `feat/cache-temperature`, worktree `.claude/worktrees/cache-temperature`.

## Grafo de dependencias

```
T1 tracer: reloj + estados + chip (TTL estimado)
 ├─▶ T2 TTL real desde el transcript
 ├─▶ T3 ajustes + aviso (evento, sonido)
 │    └─▶ T4 pilita + cuenta atrás en la píldora
 ├─▶ T5 compactación (PreCompact, idle, SessionStart)
 └─▶ T6 descubrimiento al arrancar (claude agents)
T7 docs y cierre (después de todo)
```

T2, T3, T5 y T6 dependen solo de T1, pero todas tocan
`crates/pilas-core/src/session.rs` o `src-tauri/src/lib.rs`: se hacen en
serie para no pisarse. T4 es solo UI y puede ir en paralelo con T5 o T6.

## Fase 1 · Tracer bullet

### T1 · Estados de caché de punta a punta con TTL estimado

Una sesión viva muestra caliente → enfriándose → fría en la lista, con el TTL
de respaldo (5 min) o el override de desarrollo.

- `crates/pilas-core/src/cache.rs` (nuevo, exportado en `lib.rs`):
  `CacheTemp` (`caliente`, `enfriandose`, `fria`, `compactada`, snake_case),
  `CacheClock { last_ms, ttl_s, ttl_detected, idle_compacted,
  caching_off }` y `temperature(&clock, now_ms, warn_s) -> Option<CacheTemp>`
  pura, con el tope `min(warn, ttl / 2)`.
- `session.rs`: `Session.cache: CacheClock`; `apply` actualiza el reloj solo
  con payloads sin `agent_id`: `UserPromptSubmit`, `PreToolUse`,
  `PostToolUse`, `Stop` → `last_ms = now`; `SessionStart` `startup`/`clear`
  → reloj vacío; `StopFailure` no toca.
- `src-tauri/src/lib.rs`: en `Snapshot`, cada sesión lleva
  `cache: { temp, expires_ms, ttl_s, ttl_detected } | null` y el snapshot,
  `cache_mascot` (peor accionable: `enfriandose` > `fria` > `caliente`).
  Hilo nuevo cada 5 s que recalcula y emite el snapshot solo si algún estado
  cambió. Umbral fijo de 5 min en esta tarea. `PILAS_CACHE_TTL_SECS` solo con
  `cfg!(debug_assertions)`.
- `ui/src/island/api.ts`: tipos espejo. `ui/src/island/island.ts`: chip en
  la fila ("warm · 48m", "cooling · 3:12", "cold") con `title` del TTL
  (`~` si estimado); `ui/src/i18n.ts` en/es; estilos en `ui/src/styles.css`.

Aceptación:
- Tests en `pilas-core`: fronteras de `temperature` (antes/después del
  aviso y del TTL), tope `ttl / 2`, eventos con `agent_id` ignorados,
  `SessionStart` `clear` vacía el reloj, `StopFailure` no lo toca.
- Con `PILAS_CACHE_TTL_SECS=120 npm run app` y una sesión del sandbox: tras
  una respuesta, chip caliente; a los 60 s enfriándose con cuenta atrás; a
  los 120 s fría; un prompt nuevo la vuelve caliente.

Verificar: `cargo test -p pilas-core -p pilas`,
`cargo clippy --workspace -- -D warnings`, `npm run typecheck`.

### Checkpoint 1

Tests y clippy en verde, prueba a mano de arriba. Revisar el diff antes de
seguir: el contrato de `Snapshot` queda fijado aquí.

## Fase 2 · Completar el comportamiento (en serie)

### T2 · TTL real desde el transcript

- `cache.rs`: `parse_transcript_tail(&str) -> TranscriptCache { last_ts_ms,
  ttl_s, caching_observed }` pura según la spec (solo `assistant` sin
  `isSidechain: true`, escritura más reciente con tokens de creación, líneas
  inválidas saltadas).
- App: lee los últimos 512 KB de `transcript_path` en un hilo 1,5 s después
  de `Stop`, en `SessionStart` `resume` y la primera vez que llega una sesión
  desconocida. Aplica el resultado al reloj; un fallo no cambia nada.
  Nada del transcript se guarda ni se registra. Respeta el override de debug.

Aceptación: tests de la spec (1 h, 5 min, lectura sin escritura conserva el
TTL previo, sidechain ignorado, inválidas, caché apagada → sin estado). En
la app, una sesión real con suscripción muestra TTL 1 h detectado (sin `~`).

### T3 · Ajustes y aviso

- `src-tauri/src/settings.rs`: `cache_enabled` (true), `cache_warn_min` (5;
  1/2/5/10/15), `cache_sound` (true), con tests de valores que faltan. El
  hilo de T1 usa el umbral; desactivado → `cache: null`, sin lecturas de
  transcript.
- Por cada paso a `enfriandose`, `pilas://cache-cooling` con el
  `session_id` (una vez por episodio).
- `ui/src/sound/sfx.ts`: sonido `frio` (tres notas triangulares
  descendentes, ~0,5 s); la isla lo toca al recibir el evento si
  `cache_sound` y el sonido global están activos.
- `ui/src/settings.ts` + `i18n.ts`: los tres controles.

Aceptación: cambiar el umbral cambia cuándo empieza "enfriándose" sin
reiniciar; con el ajuste apagado no hay chips ni sonido; el sonido suena una
sola vez por sesión y episodio.

### T4 · La pilita y la cuenta atrás (solo UI; paralelo a T5/T6)

- `ui/src/mascot/pilita.ts`: atributo observado `cache`; vapor (caliente),
  tiritar + copo (enfriándose), copo quieto (fría); solo en Dormida; sin
  tocar el color del cuerpo; con reduced-motion solo iconos. Selector en
  `ui/playground.html`.
- `island.ts`: pasa `cache_mascot` a todas las `<pilas-mascot>`; en la
  píldora, con `enfriandose` y sin pendientes, cuenta atrás `m:ss` de la
  sesión que expira antes; clic → `jumpToSession` sin abrir la isla.
- `docs/SPEC.md` → "Mascota: la pilita": una línea que remite al gesto.

Aceptación: playground con los 3 valores × 4 estados × 3 skins; en Alerta,
Cargando y Llena no se ve nada de caché; el clic en la cuenta atrás enfoca la
pestaña de Ghostty.

### T5 · Compactación

- `crates/pilas-core/src/installer.rs`: `("PreCompact", false, None)` en
  `EVENTS`, con test de instalar/quitar sin tocar lo ajeno.
- `session.rs`: `PreCompact` `trigger: "auto"` con la sesión en espera →
  `idle_compacted`; trabajando → nada. `SessionStart` `compact` mantiene
  `compactada` o vacía el reloj; `UserPromptSubmit` la quita.
- Chip "compacted" en la lista; `compactada` no cuenta para la pilita.

Aceptación: tests de cada caso; una instalación previa sale "necesita
reparar" y el diff solo añade `PreCompact`.

### T6 · Descubrir sesiones al arrancar

- `crates/pilas-jump`: exponer la lectura de `claude agents --json`.
- `pilas-core`: función pura que convierte esa salida en sesiones a
  registrar (`kind: "interactive"` con `pid`; `busy` → trabajando).
- App: al arrancar registra las que falten y lee su transcript
  (`~/.claude/projects/*/<sessionId>.jsonl`).

Aceptación: test de la conversión (background ignoradas, sin `pid`
ignoradas); abrir Pilas con una sesión ya abierta la muestra con su
temperatura sin esperar a un evento.

### Checkpoint 2

Todo en verde y la prueba a mano completa de la spec, con dos sesiones a la
vez y una sesión matada con `kill` (desaparece en ≤ 30 s).

## Fase 3 · Cierre

### T7 · Docs

`README.md` (función, ajustes, hook `PreCompact`, lectura local del
transcript, reparar hooks), `.changes/cache-temperature.md` (`bump:
minor`), casillas de M11 en `docs/PLAN.md`.

### Checkpoint final

Revisión del diff completo, `npm run build`, PR a `develop` con la plantilla.

## Riesgos

- Formato del transcript no documentado → parser tolerante; sin TTL
  detectado queda el estimado de 5 min.
- `claude agents --json` puede cambiar → cualquier fallo = no descubrir.
- La compactación por inactividad se infiere (auto + en espera); si Claude
  Code añade un `trigger` propio, usarlo.
