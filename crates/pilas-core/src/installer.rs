//! Merges the Pilas hooks into Claude Code's `settings.json`. No IO: takes and returns `Value`.
//! Reference: https://code.claude.com/docs/en/hooks (exec form: `command` + `args`).
//!
//! Pilas hooks are recognised by their `command`: the exact binary path or
//! any path ending in `/Pilas/bin/pilas-hook` (earlier installs).

use serde::Serialize;
use serde_json::{json, Map, Value};
use thiserror::Error;

const PILAS_BIN_SUFFIX: &str = "/Pilas/bin/pilas-hook";

/// (event, has `matcher` "*", `timeout` in seconds).
const EVENTS: &[(&str, bool, Option<u64>)] = &[
    ("SessionStart", false, None),
    ("UserPromptSubmit", false, None),
    ("PreToolUse", true, Some(3600)),
    ("PermissionRequest", true, Some(3600)),
    ("PostToolUse", true, None),
    ("Notification", false, None),
    ("Stop", false, None),
    ("StopFailure", false, None),
    ("SessionEnd", false, None),
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstallError {
    #[error("settings.json must be a JSON object")]
    NotObject,
    #[error("\"hooks\" must be a JSON object")]
    HooksNotObject,
    #[error("hooks.{0} must be an array")]
    EventNotArray(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    Ok,
    Missing,
    NeedsRepair,
}

/// `op`: `' '` unchanged, `'+'` added, `'-'` removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub op: char,
    pub text: String,
}

fn pilas_groups(hook_path: &str) -> Vec<(&'static str, Value)> {
    EVENTS
        .iter()
        .map(|&(event, matcher, timeout)| {
            let mut hook = json!({ "type": "command", "command": hook_path, "args": [] });
            if let Some(t) = timeout {
                hook["timeout"] = json!(t);
            }
            let group = if matcher {
                json!({ "matcher": "*", "hooks": [hook] })
            } else {
                json!({ "hooks": [hook] })
            };
            (event, group)
        })
        .collect()
}

fn is_pilas(hook: &Value, hook_path: &str) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c == hook_path || c.ends_with(PILAS_BIN_SUFFIX))
}

/// Only cleans up what this removal left empty (event arrays, groups, the `hooks`
/// object); anything not ours is left untouched.
fn strip_pilas(obj: &mut Map<String, Value>, hook_path: &str) -> Result<(), InstallError> {
    let Some(hooks_val) = obj.get_mut("hooks") else {
        return Ok(());
    };
    let hooks = hooks_val.as_object_mut().ok_or(InstallError::HooksNotObject)?;
    let mut removed_any = false;
    let mut emptied_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let groups = groups
            .as_array_mut()
            .ok_or_else(|| InstallError::EventNotArray(event.clone()))?;
        let mut touched = false;
        groups.retain_mut(|group| {
            let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = inner.len();
            inner.retain(|h| !is_pilas(h, hook_path));
            if inner.len() == before {
                return true;
            }
            touched = true;
            !inner.is_empty()
        });
        if touched {
            removed_any = true;
            if groups.is_empty() {
                emptied_events.push(event.clone());
            }
        }
    }
    for event in emptied_events {
        hooks.remove(&event);
    }
    let drop_hooks = removed_any && hooks.is_empty();
    if drop_hooks {
        obj.remove("hooks");
    }
    Ok(())
}

/// Removes earlier Pilas hooks first, so nothing is duplicated and foreign hooks keep their order.
pub fn install(settings: &Value, hook_path: &str) -> Result<Value, InstallError> {
    let mut out = settings.clone();
    let obj = out.as_object_mut().ok_or(InstallError::NotObject)?;
    strip_pilas(obj, hook_path)?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or(InstallError::HooksNotObject)?;
    for (event, group) in pilas_groups(hook_path) {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| InstallError::EventNotArray(event.to_string()))?;
        groups.push(group);
    }
    Ok(out)
}

pub fn uninstall(settings: &Value, hook_path: &str) -> Result<Value, InstallError> {
    let mut out = settings.clone();
    let obj = out.as_object_mut().ok_or(InstallError::NotObject)?;
    strip_pilas(obj, hook_path)?;
    Ok(out)
}

/// `Ok` if installing would change nothing; `Missing` if there is no Pilas hook;
/// `NeedsRepair` otherwise (old, incomplete or reordered entries).
pub fn status(settings: &Value, hook_path: &str) -> HookStatus {
    let Ok(expected) = install(settings, hook_path) else {
        return HookStatus::NeedsRepair;
    };
    if &expected == settings {
        return HookStatus::Ok;
    }
    // Uninstalling changes something only if Pilas hooks were present.
    if uninstall(settings, hook_path).ok().as_ref() == Some(settings) {
        HookStatus::Missing
    } else {
        HookStatus::NeedsRepair
    }
}

/// Line diff (LCS). Meant for small JSON: O(n·m) memory.
pub fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let (n, m) = (a.len(), b.len());
    // lcs[i][j] = length of the longest common subsequence of a[i..] and b[j..]
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push(DiffLine {
                op: ' ',
                text: a[i].to_string(),
            });
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            out.push(DiffLine {
                op: '-',
                text: a[i].to_string(),
            });
            i += 1;
        } else {
            out.push(DiffLine {
                op: '+',
                text: b[j].to_string(),
            });
            j += 1;
        }
    }
    for line in &a[i..] {
        out.push(DiffLine {
            op: '-',
            text: line.to_string(),
        });
    }
    for line in &b[j..] {
        out.push(DiffLine {
            op: '+',
            text: line.to_string(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = "/Users/x/Library/Application Support/Pilas/bin/pilas-hook";

    fn count_pilas(settings: &Value) -> usize {
        settings["hooks"]
            .as_object()
            .into_iter()
            .flat_map(|h| h.values())
            .flat_map(|groups| groups.as_array().into_iter().flatten())
            .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
            .filter(|h| is_pilas(h, BIN))
            .count()
    }

    #[test]
    fn installs_nine_events_into_empty_object() {
        let out = install(&json!({}), BIN).unwrap();
        let hooks = out["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 9);
        assert_eq!(count_pilas(&out), 9);
        let pre = &out["hooks"]["PreToolUse"][0];
        assert_eq!(pre["matcher"], "*");
        assert_eq!(pre["hooks"][0]["timeout"], 3600);
        assert_eq!(pre["hooks"][0]["args"], json!([]));
        assert_eq!(pre["hooks"][0]["type"], "command");
        assert!(out["hooks"]["Stop"][0].get("matcher").is_none());
        assert!(out["hooks"]["PostToolUse"][0]["hooks"][0].get("timeout").is_none());
    }

    #[test]
    fn keeps_foreign_hooks_their_order_and_other_keys() {
        let foreign = json!({ "hooks": [{ "type": "command", "command": "/usr/bin/lint" }] });
        let settings = json!({
            "model": "opus",
            "hooks": { "Stop": [foreign.clone()] }
        });
        let out = install(&settings, BIN).unwrap();
        assert_eq!(out["model"], "opus");
        let stop = out["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0], foreign);
        assert_eq!(stop.len(), 2);
    }

    #[test]
    fn keeps_key_order() {
        let settings: Value =
            serde_json::from_str(r#"{"z":1,"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/x"}]}]},"a":2}"#)
                .unwrap();
        let installed = install(&settings, BIN).unwrap();
        let keys: Vec<&String> = installed.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["z", "hooks", "a"]);
        let stripped = uninstall(&installed, BIN).unwrap();
        let keys: Vec<&String> = stripped.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["z", "hooks", "a"]);
        assert_eq!(stripped, settings);
    }

    #[test]
    fn installing_twice_is_idempotent() {
        let foreign = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "/x" }] }] } });
        let once = install(&foreign, BIN).unwrap();
        let twice = install(&once, BIN).unwrap();
        assert_eq!(once, twice);
        assert_eq!(count_pilas(&twice), 9);
    }

    #[test]
    fn replaces_old_pilas_paths_without_duplicating() {
        let old = "/Applications/Pilas.app/Contents/Pilas/bin/pilas-hook";
        let with_old = install(&json!({}), old).unwrap();
        let new = install(&with_old, BIN).unwrap();
        assert_eq!(count_pilas(&new), 9);
        assert_eq!(new, install(&json!({}), BIN).unwrap());
    }

    #[test]
    fn uninstall_keeps_foreign_hooks_and_other_keys() {
        let foreign = json!({ "hooks": [{ "type": "command", "command": "/usr/bin/lint" }] });
        let settings = json!({ "theme": "dark", "hooks": { "Stop": [foreign] } });
        let installed = install(&settings, BIN).unwrap();
        let out = uninstall(&installed, BIN).unwrap();
        assert_eq!(out, settings);
    }

    #[test]
    fn uninstall_drops_empty_hooks_object() {
        let installed = install(&json!({ "theme": "dark" }), BIN).unwrap();
        let out = uninstall(&installed, BIN).unwrap();
        assert_eq!(out, json!({ "theme": "dark" }));
    }

    #[test]
    fn uninstall_without_pilas_hooks_changes_nothing() {
        let settings = json!({ "hooks": {} });
        assert_eq!(uninstall(&settings, BIN).unwrap(), settings);
    }

    #[test]
    fn status_covers_three_cases() {
        let empty = json!({});
        assert_eq!(status(&empty, BIN), HookStatus::Missing);
        let installed = install(&empty, BIN).unwrap();
        assert_eq!(status(&installed, BIN), HookStatus::Ok);
        let mut broken = installed.clone();
        broken["hooks"]["Stop"] = json!([]);
        assert_eq!(status(&broken, BIN), HookStatus::NeedsRepair);
        // An old Pilas path counts as ours but does not match.
        let old = install(&empty, "/Applications/Pilas.app/Pilas/bin/pilas-hook").unwrap();
        assert_eq!(status(&old, BIN), HookStatus::NeedsRepair);
        let foreign_path = install(&empty, "/other/pilas-hook").unwrap();
        assert_eq!(status(&foreign_path, BIN), HookStatus::Missing);
    }

    #[test]
    fn status_with_only_foreign_hooks_is_missing() {
        let foreign = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "/x" }] }] } });
        assert_eq!(status(&foreign, BIN), HookStatus::Missing);
    }

    #[test]
    fn non_object_input_is_error() {
        assert_eq!(install(&json!([]), BIN), Err(InstallError::NotObject));
        assert_eq!(install(&json!("x"), BIN), Err(InstallError::NotObject));
        assert_eq!(uninstall(&json!(1), BIN), Err(InstallError::NotObject));
        assert_eq!(install(&json!({ "hooks": [] }), BIN), Err(InstallError::HooksNotObject));
        assert_eq!(
            install(&json!({ "hooks": { "Stop": {} } }), BIN),
            Err(InstallError::EventNotArray("Stop".into()))
        );
        assert_eq!(status(&json!([]), BIN), HookStatus::NeedsRepair);
    }

    #[test]
    fn basic_diff() {
        let d = line_diff("a\nb\nc", "a\nB\nc\nd");
        let ops: Vec<(char, &str)> = d.iter().map(|l| (l.op, l.text.as_str())).collect();
        assert_eq!(ops, vec![(' ', "a"), ('-', "b"), ('+', "B"), (' ', "c"), ('+', "d")]);
    }

    #[test]
    fn diff_of_equal_texts_is_only_context() {
        let d = line_diff("x\ny", "x\ny");
        assert!(d.iter().all(|l| l.op == ' '));
        assert_eq!(d.len(), 2);
    }
}
