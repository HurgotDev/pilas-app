# Plan de desarrollo de la v1

Cada hito termina con algo que se puede probar a mano. El orden ataca primero el riesgo más alto
(la ventana sobre el notch). Los criterios de aceptación finales están en `docs/SPEC.md`.

## M0 · Fundaciones
- [x] `cargo check --workspace` y `cargo clippy --workspace -- -D warnings` sin errores (el esqueleto no se compiló al generarse).
- [x] `cargo test -p pilas-core -p pilas-jump` en verde.
- [x] `npm run app` abre la ventana transparente sin ícono en el Dock.

## M1 · La isla en su sitio (riesgo alto: hacerlo primero)
- [x] Convertir la ventana en `NSPanel`: nivel por encima de la barra de menú, en todos los Spaces, sin robar foco.
      Evaluar `tauri-nspanel` (comunidad) y, si no basta, un módulo propio con `objc2`.
- [ ] Posicionar sobre el notch de la pantalla del MacBook; en un monitor externo principal, centrada bajo la barra de menú.
- [ ] Ajuste de pantalla (principal / la del notch / una concreta) y recolocar al conectar o desconectar monitores.
      Pendiente: probar con monitor externo y la recolocación; con notch ya funciona.
- [x] Click-through fuera de la píldora y área de hover de 6 px cuando está oculta.
- [x] Transición fluida de píldora a abierta y vuelta (ancho y alto animados
      en CSS; la ventana nativa crece antes y encoge después).
- Prueba: escribir en otra app mientras la isla cambia de tamaño; el foco nunca se pierde.

## M2 · Hook y sesiones de punta a punta
- [x] `pilas-hook` en release < 20 ms de arranque; con la app cerrada sale al instante.
- [x] Lista de sesiones con estado, proyecto, última acción y "hace N min".
- [x] Limpieza de sesiones huérfanas por `pid` cada 30 s.
- Prueba: 3 sesiones en paralelo en `~/pilas-sandbox` (ver `docs/HOOKS-DEV.md`).

## M3 · Permisos
- [x] Tarjeta de permiso con Permitir / Siempre / Denegar y "Responder en la terminal".
- [x] "Siempre" muestra qué regla se guardará (de `permission_suggestions`) antes de confirmar.
- [x] Cola FIFO con "1 de N"; al responder en la terminal la tarjeta desaparece sola.
- [x] `Enter` permite y `Esc` cierra cuando la isla tiene foco.

## M4 · Preguntas (AskUserQuestion)
- [x] Tarjeta con 1 a 4 preguntas en pasos, selección única y múltiple, botón Enviar.
- [x] Respuestas por `updatedInput.answers` (texto de la pregunta → etiquetas separadas por coma).
- [x] Sin tarjetas duplicadas (el `PermissionRequest` de AskUserQuestion se ignora).

## M4b · Idiomas (inglés y español)
- [x] `ui/src/i18n.ts` con `t()` y diccionarios `en`/`es`; idioma del sistema.
- [x] Todos los textos de la isla pasan por `t()`; inglés por defecto.
- [x] `last_action` como datos (`{ kind, arg }`) en `pilas-core` y `Snapshot`.
- [x] "Denied from Pilas" fijo en inglés.

## M5 · La pilita y los sonidos
- [x] Estados y reacciones integrados con la prioridad Alerta > Llena > Cargando > Dormida.
- [x] Sonidos con ajuste de activado y volumen (`configure()` en `sfx.ts`;
  se persiste en M7); un solo recordatorio a los 60 s de un pendiente.
- [x] Sonido de mareo: `"mareo"` en `sfx.ts`; `pilita-click` lleva la
  reacción en `detail.reaction` y la isla toca mareo en vez de toque en el
  3.er clic.

## M6 · Saltar a Ghostty
- [x] Verificar en Script Editor (Ghostty.sdef) si cada terminal expone su TTY; si sí, emparejar por TTY.
- [x] Cachear el id de terminal tras un salto exacto; respaldo por cwd solo con coincidencia única.
- [x] Fallo cerrado: sin coincidencia segura, solo `activate`.

## M7 · Instalador, onboarding y ajustes
- [x] Copiar `pilas-hook` a `~/Library/Application Support/Pilas/bin/`.
- [x] Instalar hooks en `~/.claude/settings.json` con backup fechado, diff previo y sin tocar hooks ajenos.
- [x] Reparar y desinstalar solo las entradas de Pilas. Onboarding de 4 pasos. Ajustes en JSON (incluye idioma
  y sonido: activado + volumen vía `configure()`).

## M8 · Publicación
- [x] Workflow de release con `.dmg` universal en GitHub Actions.
- [x] README con instrucciones para abrir una app sin notarizar.

## M9 · Actualizaciones
- [x] Refrescar el `pilas-hook` copiado al arrancar si difiere del de la app.
- [x] `tauri-plugin-updater`: ajuste "Buscar actualizaciones" (sí por
  defecto), búsqueda al arrancar y cada 24 h, aviso en el menú y en Ajustes,
  instalar y reiniciar solo a petición.
- [ ] Release: artefactos firmados del updater y `latest.json` en cada versión.
- [ ] Cask en `HurgotDev/homebrew-tap` actualizado por el workflow de release.

## M10 · Skins de temporada (ver `docs/SPEC.md` → "Skins")

Orden: T1 → (T2 ∥ T3) → T4 → T5. T2 y T3 tocan archivos distintos.

- [x] T1 · Ajuste `skin` y `resolveSkin`.
  - Hecho: enum `Skin` en `src-tauri/src/settings.rs` (falta o desconocido
    → `auto`, sin perder otros ajustes) reflejado en `ui/src/island/api.ts`;
    `ui/src/mascot/skins.ts` con `resolveSkin(choice, date)`; script
    `test:ui` en `package.json` y en Comandos de `CLAUDE.md`.
  - Verificar: `cargo test -p pilas`, `npm run test:ui` (bordes 14/15 oct y
    2/3 nov), `npm run typecheck`, `cargo clippy --workspace -- -D warnings`.
- [x] T2 · Pilita con disfraz de Halloween.
  - Hecho: atributo observado `skin` en `ui/src/mascot/pilita.ts`; sombrero
    dentro del `rig`, murciélago sin solaparse con "z", exclamación ni
    destellos, LEDs naranja; quieto con reduced-motion; encuadre ajustado
    para que el sombrero no se recorte a 28 px; selector de skin en
    `ui/playground.html`.
  - Verificar: `npm run dev` → `/playground.html`, 4 estados × 3 reacciones
    en las dos skins; colores de cuerpo idénticos a la clásica.
- [x] T3 · Sonidos de Halloween.
  - Hecho: `ui/src/sound/sfx.ts` con `configure({ skin })` y notas propias
    de Alerta, Listo, Toque y Mareo (murciélagos); el resto cae en la
    clásica; duración ±20 %.
  - Verificar: `npm run typecheck`; a oído desde el playground.
- [x] T4 · Conectar la skin en la app.
  - Hecho: selector "Costume"/"Disfraz" en `ui/src/settings.ts` +
    `ui/src/i18n.ts`; `ui/src/island/island.ts` resuelve la skin al
    arrancar, al cambiar el ajuste y cada hora, la pone en todas las
    `<pilas-mascot>`, en `data-skin` de la raíz y en `configure`; `.dot` de
    calabaza en `ui/src/styles.css`.
  - Verificar: `npm run app`; cambiar el ajuste cambia pilita, puntos y
    sonidos al instante; en Automático hoy (28 oct) sale Halloween.
- [x] T5 · Docs y cierre.
  - Hecho: `README.md` (ajuste nuevo), `.changes/seasonal-skins.md`
    (`bump: minor`), casillas marcadas.
  - Verificar: releer el README; todos los comandos de T1 en verde.
- [ ] T6 · Contador oculto en 0 y ambiente Halloween en la isla.
  - Hecho: `.badge[hidden]` no se ve (bug previo); badge de calabaza
    naranja; murciélago que cruza la isla (sale de la pilita); resplandor,
    telaraña, fantasmitas y acentos naranja según `docs/SPEC.md` → Skins.
  - Verificar: render a tamaño real de píldora e isla abierta en ambas
    skins; `npm run typecheck`, `npm run build`.
