//! Installer IO: reads `~/.claude/settings.json`, copies `pilas-hook` to the data folder,
//! backs up and writes. The JSON merge lives in `pilas_core::installer`.

use crate::error::CommandError;
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
    pub fn parse(action: &str) -> Result<Self, CommandError> {
        match action {
            "install" => Ok(Self::Install),
            "uninstall" => Ok(Self::Uninstall),
            other => Err(CommandError::InvalidAction(other.to_string())),
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
fn paths() -> Result<Paths, CommandError> {
    let claude_settings = match std::env::var_os("PILAS_CLAUDE_SETTINGS") {
        Some(p) => PathBuf::from(p),
        None => {
            let home = std::env::var_os("HOME").ok_or(CommandError::PathUnavailable)?;
            PathBuf::from(home).join(".claude/settings.json")
        }
    };
    let hook_dest = data_dir()
        .ok_or(CommandError::PathUnavailable)?
        .join("bin")
        .join(HOOK_NAME);
    Ok(Paths {
        claude_settings,
        hook_dest,
    })
}

/// No file means `{}`. A file that is not valid JSON is an error, so nothing gets written.
fn read_claude_settings(path: &Path) -> Result<Value, CommandError> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => Ok(serde_json::from_str(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
    }
}

fn compute(action: Action, current: &Value, hook_dest: &Path) -> Result<Value, CommandError> {
    let dest = hook_dest.to_str().ok_or(CommandError::PathUnavailable)?;
    Ok(match action {
        Action::Install => installer::install(current, dest)?,
        Action::Uninstall => installer::uninstall(current, dest)?,
    })
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
pub fn preview(action: Action) -> Result<Vec<DiffLine>, CommandError> {
    let p = paths()?;
    let before = read_claude_settings(&p.claude_settings)?;
    let after = compute(action, &before, &p.hook_dest)?;
    let before = serde_json::to_string_pretty(&before)?;
    let after = serde_json::to_string_pretty(&after)?;
    Ok(installer::line_diff(&before, &after))
}

/// Returns the backup path if `settings.json` already existed and was written.
pub fn apply(action: Action) -> Result<Option<PathBuf>, CommandError> {
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
fn bundled_hook() -> Result<PathBuf, CommandError> {
    let exe = std::env::current_exe()?;
    Ok(exe.parent().ok_or(CommandError::PathUnavailable)?.join(HOOK_NAME))
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
fn copy_hook(dest: &Path) -> Result<(), CommandError> {
    use std::os::unix::fs::PermissionsExt;
    // shortcut: in development this copies target/debug/pilas-hook; in release the publish workflow bundles it (bundle.macOS.files → MacOS/pilas-hook, universal and ad-hoc signed)
    let src = bundled_hook()?;
    if !src.exists() {
        return Err(CommandError::HookMissing);
    }
    let dir = dest.parent().ok_or(CommandError::PathUnavailable)?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{HOOK_NAME}.tmp"));
    fs::copy(&src, &tmp)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    Ok(fs::rename(&tmp, dest)?)
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
fn write_claude_settings(path: &Path, value: &Value) -> Result<Option<PathBuf>, CommandError> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(CommandError::PathUnavailable)?;
    let dir = path.parent().ok_or(CommandError::PathUnavailable)?;
    fs::create_dir_all(dir)?;

    let backup = if path.exists() {
        let b = dir.join(format!("{name}.pilas-backup-{}", timestamp()));
        fs::copy(path, &b)?;
        Some(b)
    } else {
        None
    };

    let text = serde_json::to_string_pretty(value)? + "\n";
    let tmp = dir.join(format!("{name}.pilas-tmp"));
    fs::write(&tmp, text)?;
    if let Ok(meta) = fs::metadata(path) {
        fs::set_permissions(&tmp, meta.permissions())?;
    }
    fs::rename(&tmp, path)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_action_is_error() {
        assert!(matches!(Action::parse("install"), Ok(Action::Install)));
        assert!(matches!(Action::parse("uninstall"), Ok(Action::Uninstall)));
        assert_eq!(Action::parse("delete").unwrap_err().code(), "invalid_action");
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
        assert_eq!(read_claude_settings(&missing).unwrap(), json!({}));
        let _ = fs::remove_dir_all(&dir);
    }
}
