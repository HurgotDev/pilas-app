//! IO del instalador: lee `~/.claude/settings.json`, copia `pilas-hook` a la carpeta de datos,
//! hace backup y escribe. La fusión JSON vive en `pilas_core::installer`.

use pilas_core::installer::{self, DiffLine, HookStatus};
use pilas_core::paths::data_dir;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const HOOK_NAME: &str = "pilas-hook";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Uninstall,
}

impl Action {
    pub fn parse(action: &str) -> Result<Self, String> {
        match action {
            "install" => Ok(Self::Install),
            "uninstall" => Ok(Self::Uninstall),
            otra => Err(format!("acción desconocida: {otra}")),
        }
    }
}

/// Lo que ve la UI en `hooks_status`.
#[derive(Debug, Serialize)]
pub struct HooksInfo {
    /// "ok" | "missing" | "needs_repair" | "invalid_json"
    pub status: &'static str,
    pub settings_path: String,
}

struct Paths {
    claude_settings: PathBuf,
    /// `data_dir()/bin/pilas-hook`: donde Claude Code encontrará el hook.
    hook_dest: PathBuf,
}

/// `$PILAS_CLAUDE_SETTINGS` permite probar sin tocar la configuración real (ver docs/HOOKS-DEV.md).
fn paths() -> Result<Paths, String> {
    let claude_settings = match std::env::var_os("PILAS_CLAUDE_SETTINGS") {
        Some(p) => PathBuf::from(p),
        None => {
            let home = std::env::var_os("HOME").ok_or("HOME no definido")?;
            PathBuf::from(home).join(".claude/settings.json")
        }
    };
    let hook_dest = data_dir()
        .ok_or("no se pudo resolver la carpeta de datos")?
        .join("bin")
        .join(HOOK_NAME);
    Ok(Paths {
        claude_settings,
        hook_dest,
    })
}

/// Sin archivo = `{}`. Archivo que no es JSON válido = error (no se escribe nada).
fn read_claude_settings(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|_| format!("{} no es JSON válido; no se ha escrito nada", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn compute(action: Action, current: &Value, hook_dest: &Path) -> Result<Value, String> {
    let dest = hook_dest.to_str().ok_or("la ruta del hook no es UTF-8")?;
    match action {
        Action::Install => installer::install(current, dest),
        Action::Uninstall => installer::uninstall(current, dest),
    }
    .map_err(|e| e.to_string())
}

/// Estado actual para la UI. Si no se puede resolver la ruta, "missing" con ruta vacía.
pub fn info() -> HooksInfo {
    let Ok(p) = paths() else {
        return HooksInfo {
            status: "missing",
            settings_path: String::new(),
        };
    };
    let settings_path = p.claude_settings.display().to_string();
    let status = match read_claude_settings(&p.claude_settings) {
        Err(_) => "invalid_json",
        Ok(current) => match installer::status(&current, p.hook_dest.to_str().unwrap_or_default()) {
            // Entradas en su sitio pero el binario se ha borrado: Claude Code fallaría en silencio.
            HookStatus::Ok if !p.hook_dest.exists() => "needs_repair",
            HookStatus::Ok => "ok",
            HookStatus::Missing => "missing",
            HookStatus::NeedsRepair => "needs_repair",
        },
    };
    HooksInfo { status, settings_path }
}

/// Diff (pretty JSON antes/después) de lo que haría la acción. No escribe nada.
pub fn preview(action: Action) -> Result<Vec<DiffLine>, String> {
    let p = paths()?;
    let before = read_claude_settings(&p.claude_settings)?;
    let after = compute(action, &before, &p.hook_dest)?;
    let before = serde_json::to_string_pretty(&before).map_err(|e| e.to_string())?;
    let after = serde_json::to_string_pretty(&after).map_err(|e| e.to_string())?;
    Ok(installer::line_diff(&before, &after))
}

/// Aplica la acción. Devuelve la ruta del backup si se escribió `settings.json` y ya existía.
pub fn apply(action: Action) -> Result<Option<PathBuf>, String> {
    let p = paths()?;
    let current = read_claude_settings(&p.claude_settings)?;
    let next = compute(action, &current, &p.hook_dest)?;
    if action == Action::Install {
        copy_hook(&p.hook_dest)?;
    }
    let backup = if next != current {
        write_claude_settings(&p.claude_settings, &next)?
    } else {
        None
    };
    if action == Action::Uninstall {
        let _ = fs::remove_file(&p.hook_dest); // si ya no está, no pasa nada
    }
    Ok(backup)
}

/// Copia el binario del hook junto a la app a la carpeta de datos (escritura temporal + rename).
fn copy_hook(dest: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    // shortcut: en desarrollo copia target/debug/pilas-hook; en release lo empaqueta el workflow de publicación (bundle.macOS.files → MacOS/pilas-hook, universal y firmado ad-hoc)
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let src = exe.parent().ok_or("el ejecutable no tiene carpeta")?.join(HOOK_NAME);
    if !src.exists() {
        return Err(format!(
            "no se encuentra {} junto a la app; compila con `cargo build -p pilas-hook`",
            src.display()
        ));
    }
    let dir = dest.parent().ok_or("ruta del hook sin carpeta")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{HOOK_NAME}.tmp"));
    fs::copy(&src, &tmp).map_err(|e| e.to_string())?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Fecha local `YYYYMMDD-HHMMSS` para los nombres de backup.
fn timestamp() -> String {
    // SAFETY: `time` con puntero nulo y `localtime_r` con búferes válidos del tamaño correcto.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// Backup (si ya existía) + escritura atómica conservando los permisos del original.
fn write_claude_settings(path: &Path, value: &Value) -> Result<Option<PathBuf>, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("nombre de settings.json no válido")?;
    let dir = path.parent().ok_or("settings.json sin carpeta")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let backup = if path.exists() {
        let b = dir.join(format!("{name}.pilas-backup-{}", timestamp()));
        fs::copy(path, &b).map_err(|e| e.to_string())?;
        Some(b)
    } else {
        None
    };

    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())? + "\n";
    let tmp = dir.join(format!("{name}.pilas-tmp"));
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    if let Ok(meta) = fs::metadata(path) {
        fs::set_permissions(&tmp, meta.permissions()).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accion_desconocida_es_error() {
        assert_eq!(Action::parse("install"), Ok(Action::Install));
        assert_eq!(Action::parse("uninstall"), Ok(Action::Uninstall));
        assert!(Action::parse("borrar").is_err());
    }

    #[test]
    fn json_invalido_no_se_lee_como_vacio() {
        let dir = std::env::temp_dir().join(format!("pilas-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let roto = dir.join("roto.json");
        fs::write(&roto, "{ no json").unwrap();
        assert!(read_claude_settings(&roto).is_err());
        let ausente = dir.join("ausente.json");
        assert_eq!(read_claude_settings(&ausente), Ok(json!({})));
        let _ = fs::remove_dir_all(&dir);
    }
}
