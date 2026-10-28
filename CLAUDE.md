# Pilas

Isla minimalista en el notch de macOS con una mascota (la pilita) que avisa cuando Claude Code necesita
atención y permite aprobar permisos y responder preguntas sin cambiar de ventana.

- Especificación completa: `docs/SPEC.md` (la fuente de verdad; léela antes de diseñar algo nuevo).
- Plan por hitos y estado actual: `docs/PLAN.md` (marca las casillas al terminar cada tarea).
- Cómo probar hooks sin romper tu propia sesión: `docs/HOOKS-DEV.md`.

## Arquitectura en una línea por pieza

- `crates/pilas-core`: protocolo IPC, sesiones y máquina de estados, salida JSON para Claude Code. Sin Tauri, 100 % testeable.
- `crates/pilas-hook`: binario que Claude Code ejecuta en cada evento. stdin → socket → stdout. **Siempre sale con 0.**
- `crates/pilas-jump`: saltar a la terminal (Ghostty exacto vía AppleScript, el resto solo `activate`).
- `src-tauri`: app Tauri 2. Servidor del socket (`server.rs`), cola FIFO de pendientes y comandos (`lib.rs`).
- `ui/`: frontend TypeScript + Vite sin framework. `mascot/pilita.ts` es el web component `<pilas-mascot>`.

## Comandos

```bash
cargo test -p pilas-core -p pilas-jump      # tests rápidos del núcleo
cargo clippy --workspace -- -D warnings
cargo build -p pilas-hook --release          # binario del hook
npm install && npm run typecheck
npm run app                                  # tauri dev (abre la isla)
npm run dev                                  # solo la UI; playground de la pilita en /playground.html
```

## Reglas que no se negocian

1. **Nunca bloquear Claude Code.** Si Pilas no corre o algo falla, el hook sale con 0 y sin salida.
   Nada de `onFailure: "block"`, nada de reintentos, nada de `exit 2` en PermissionRequest.
2. **Nada se aprueba solo.** No hay timeouts ni modos que aprueben. "Siempre" solo reenvía las
   `permission_suggestions` de Claude Code, nunca reglas inventadas.
3. **Una decisión solo va a su solicitud.** Se entrega por `request_id` exacto; si el hook se fue, se descarta.
4. **Sin red.** La app no abre puertos TCP ni hace peticiones externas. Socket Unix con permisos 0600.
5. **El hook es tonto.** Reenvía el payload íntegro; toda interpretación vive en `pilas-core`, con tests.
6. **Minimalismo.** Si algo no está en `docs/SPEC.md` → "Alcance v1", no se agrega sin preguntar.

## Convenciones

- Rust estable, `cargo fmt` y `clippy` limpios. Errores con `thiserror` en librerías; nada de `unwrap()`
  fuera de tests salvo en locks de Mutex.
- Textos de la interfaz siempre vía `t()` de `ui/src/i18n.ts` (en/es, inglés por defecto);
  comentarios en español; identificadores en inglés salvo los nombres del dominio
  ya fijados (estados `dormida`, `cargando`, `alerta`, `llena`; reacciones `saludo`, `aplastada`, `mareo`).
- Los tipos de `ui/src/island/api.ts` deben reflejar `Snapshot` de `src-tauri/src/lib.rs`: si cambias uno, cambia el otro.
- Antes de tocar el contrato con Claude Code, verifica en https://code.claude.com/docs/en/hooks y añade un test en `pilas-core`.
- Commits pequeños y en español, en imperativo: "Añade la tarjeta de preguntas".

## Estado inicial (importante)

El esqueleto se generó sin compilar Rust (no había toolchain). Lo primero es la tarea M0 de `docs/PLAN.md`:
`cargo check --workspace`, corregir lo que falle y dejar los tests en verde. El frontend sí compila.
