//! Installer IO: reads `~/.claude/settings.json`, copies `pilas-hook` to the data folder,
//! backs up and writes. The JSON merge lives in `pilas_core::installer`.

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
            other => Err(format!("unknown action: {other}")),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HooksInfo {
    /// "ok" | "missing" | "needs_repair" | "invalid_json"
    pub status: &'static str,
    pub settings_path: String,
}

struct Paths {
    claude_settings: PathBuf,
    /// `data_dir()/bin/pilas-hook`: where Claude Code finds the hook.
    hook_dest: PathBuf,
}

/// `$PILAS_CLAUDE_SETTINGS` allows testing without touching the real config (see docs/HOOKS-DEV.md).
fn paths() -> Result<Paths, String> {
    let claude_settings = match std::env::var_os("PILAS_CLAUDE_SETTINGS") {
        Some(p) => PathBuf::from(p),
        None => {
            let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
            PathBuf::from(home).join(".claude/settings.json")
        }
    };
    let hook_dest = data_dir()
        .ok_or("could not resolve the data folder")?
        .join("bin")
        .join(HOOK_NAME);
    Ok(Paths {
        claude_settings,
        hook_dest,
    })
}

/// No file means `{}`. A file that is not valid JSON is an error, so nothing gets written.
fn read_claude_settings(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|_| format!("{} is not valid JSON; nothing was written", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn compute(action: Action, current: &Value, hook_dest: &Path) -> Result<Value, String> {
    let dest = hook_dest.to_str().ok_or("the hook path is not UTF-8")?;
    match action {
        Action::Install => installer::install(current, dest),
        Action::Uninstall => installer::uninstall(current, dest),
    }
    .map_err(|e| e.to_string())
}

/// If the path cannot be resolved, "missing" with an empty path.
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
            // Entries in place but the binary was deleted: Claude Code would fail silently.
            HookStatus::Ok if !p.hook_dest.exists() => "needs_repair",
            HookStatus::Ok => "ok",
            HookStatus::Missing => "missing",
            HookStatus::NeedsRepair => "needs_repair",
        },
    };
    HooksInfo { status, settings_path }
}

/// Pretty JSON before/after diff of what the action would do. Writes nothing.
pub fn preview(action: Action) -> Result<Vec<DiffLine>, String> {
    let p = paths()?;
    let before = read_claude_settings(&p.claude_settings)?;
    let after = compute(action, &before, &p.hook_dest)?;
    let before = serde_json::to_string_pretty(&before).map_err(|e| e.to_string())?;
    let after = serde_json::to_string_pretty(&after).map_err(|e| e.to_string())?;
    Ok(installer::line_diff(&before, &after))
}

/// Returns the backup path if `settings.json` already existed and was written.
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
        let _ = fs::remove_file(&p.hook_dest); // already gone is fine
    }
    Ok(backup)
}

/// Next to the app executable; may not exist.
fn bundled_hook() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.parent().ok_or("the executable has no folder")?.join(HOOK_NAME))
}

/// Recopy only if Pilas is connected, a bundled hook exists and it differs from
/// the installed copy (or the copy is missing).
pub fn needs_refresh(connected: bool, bundled: Option<&[u8]>, installed: Option<&[u8]>) -> bool {
    connected && bundled.is_some() && bundled != installed
}

/// Never touches settings.json and reports no errors.
pub fn refresh_hook() {
    let Ok(p) = paths() else { return };
    let Ok(current) = read_claude_settings(&p.claude_settings) else {
        return;
    };
    let dest = p.hook_dest.to_str().unwrap_or_default();
    let connected = !matches!(installer::status(&current, dest), HookStatus::Missing);
    let bundled = bundled_hook().ok().and_then(|src| fs::read(src).ok());
    let installed = fs::read(&p.hook_dest).ok();
    if needs_refresh(connected, bundled.as_deref(), installed.as_deref()) {
        let _ = copy_hook(&p.hook_dest);
    }
}

/// Temp write + rename.
fn copy_hook(dest: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    // shortcut: in development this copies target/debug/pilas-hook; in release the publish workflow bundles it (bundle.macOS.files → MacOS/pilas-hook, universal and ad-hoc signed)
    let src = bundled_hook()?;
    if !src.exists() {
        return Err(format!(
            "{} not found next to the app; build it with `cargo build -p pilas-hook`",
            src.display()
        ));
    }
    let dir = dest.parent().ok_or("hook path has no folder")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{HOOK_NAME}.tmp"));
    fs::copy(&src, &tmp).map_err(|e| e.to_string())?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Local `YYYYMMDD-HHMMSS` for backup names.
fn timestamp() -> String {
    // SAFETY: `time` with a null pointer and `localtime_r` with valid, correctly sized buffers.
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

/// Backup (if it existed) + atomic write keeping the original's permissions.
fn write_claude_settings(path: &Path, value: &Value) -> Result<Option<PathBuf>, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid settings.json name")?;
    let dir = path.parent().ok_or("settings.json has no folder")?;
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
    fn unknown_action_is_error() {
        assert_eq!(Action::parse("install"), Ok(Action::Install));
        assert_eq!(Action::parse("uninstall"), Ok(Action::Uninstall));
        assert!(Action::parse("delete").is_err());
    }

    #[test]
    fn refresh_only_if_connected_and_different() {
        let new: &[u8] = b"v2";
        let old: &[u8] = b"v1";
        assert!(!needs_refresh(false, Some(new), Some(old)), "not connected: leave it");
        assert!(
            !needs_refresh(true, None, Some(old)),
            "no bundled binary: nothing to copy"
        );
        assert!(!needs_refresh(true, Some(new), Some(new)), "already up to date");
        assert!(needs_refresh(true, Some(new), Some(old)), "different binary");
        assert!(needs_refresh(true, Some(new), None), "copy missing");
    }

    #[test]
    fn invalid_json_is_not_read_as_empty() {
        let dir = std::env::temp_dir().join(format!("pilas-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("broken.json");
        fs::write(&broken, "{ no json").unwrap();
        assert!(read_claude_settings(&broken).is_err());
        let missing = dir.join("missing.json");
        assert_eq!(read_claude_settings(&missing), Ok(json!({})));
        let _ = fs::remove_dir_all(&dir);
    }
}
