//! Rutas compartidas por la app y el hook. Sin dependencias externas a propósito:
//! el hook debe arrancar rápido.

use std::path::PathBuf;

/// Carpeta de datos de Pilas.
/// macOS: `~/Library/Application Support/Pilas`. Linux: `$XDG_DATA_HOME/pilas` o `~/.local/share/pilas`.
pub fn data_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    {
        home.map(|h| h.join("Library/Application Support/Pilas"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".local/share")))
            .map(|d| d.join("pilas"))
    }
}

/// Ruta del socket. Se puede sobrescribir con `PILAS_SOCKET` (útil en tests y desarrollo).
pub fn socket_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PILAS_SOCKET") {
        return Some(PathBuf::from(p));
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
            return Some(PathBuf::from(rt).join("pilas.sock"));
        }
    }
    data_dir().map(|d| d.join("pilas.sock"))
}
