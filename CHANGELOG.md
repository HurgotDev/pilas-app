# Changelog

Todos los cambios notables de Pilas se anotan en este archivo. El formato se
basa en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el
proyecto sigue [Versionado Semántico](https://semver.org/lang/es/).

Las secciones se generan con `npm run release:prepare` a partir de los
fragmentos de `.changes/` (ver `docs/RELEASING.md`).

## [0.2.0] - 2026-10-28

### Novedades

- Añade actualizaciones automáticas firmadas, instalación con Homebrew y el refresco del hook al arrancar.

### Correcciones

- El CI también compila, revisa y prueba la app (src-tauri).
- Actualiza el README (qué hace, privacidad y comandos) y añade la regla de mantenerlo al día.
- Añade el flujo de ramas y releases con changelog por cambio.

## [0.1.0] - 2026-10-28

### Novedades

- Isla sobre el notch como panel flotante que no roba el foco, con
  transición fluida de píldora a tarjeta.
- Lista de sesiones de Claude Code con estado, proyecto, última acción y
  limpieza de sesiones huérfanas.
- Tarjetas de permiso con Permitir, Siempre y Denegar, en cola FIFO y con
  atajos de teclado.
- Respuesta a preguntas de AskUserQuestion (una a cuatro, selección única o
  múltiple) sin salir de la isla.
- La pilita animada con estados, reacciones y sonidos configurables.
- Interfaz en inglés y español según el idioma del sistema.
- Salto a la pestaña exacta de Ghostty y activación de otras terminales.
- Instalador de hooks con backup y diff previo, onboarding y ajustes; `.dmg`
  universal para Apple Silicon e Intel.
