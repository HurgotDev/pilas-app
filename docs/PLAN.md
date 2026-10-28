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

## M6 · Saltar a Ghostty
- [x] Verificar en Script Editor (Ghostty.sdef) si cada terminal expone su TTY; si sí, emparejar por TTY.
- [x] Cachear el id de terminal tras un salto exacto; respaldo por cwd solo con coincidencia única.
- [x] Fallo cerrado: sin coincidencia segura, solo `activate`.

## M7 · Instalador, onboarding y ajustes
- [ ] Copiar `pilas-hook` a `~/Library/Application Support/Pilas/bin/`.
- [ ] Instalar hooks en `~/.claude/settings.json` con backup fechado, diff previo y sin tocar hooks ajenos.
- [ ] Reparar y desinstalar solo las entradas de Pilas. Onboarding de 4 pasos. Ajustes en JSON (incluye idioma
  y sonido: activado + volumen vía `configure()`).

## M8 · Publicación
- [ ] Workflow de release con `.dmg` universal en GitHub Actions.
- [ ] README con instrucciones para abrir una app sin notarizar.
