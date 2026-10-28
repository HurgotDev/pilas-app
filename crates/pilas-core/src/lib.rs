//! Núcleo de Pilas: todo lo que no depende de Tauri ni de la plataforma.
//!
//! - [`protocol`]: mensajes NDJSON entre `pilas-hook` y la app (ver docs/SPEC.md, "Protocolo IPC").
//! - [`session`]: sesiones de Claude Code y su máquina de estados.
//! - [`decision`]: construcción de la salida JSON que el hook imprime para Claude Code.
//! - [`paths`]: rutas del socket y de los datos de la app.

pub mod decision;
pub mod paths;
pub mod protocol;
pub mod session;
