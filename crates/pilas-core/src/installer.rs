//! Fusión de los hooks de Pilas en `settings.json` de Claude Code. Sin IO: recibe y devuelve `Value`.
//! Referencia: https://code.claude.com/docs/en/hooks (forma exec: `command` + `args`).
//!
//! Los hooks de Pilas se reconocen por su `command`: la ruta exacta del binario o
//! cualquier ruta que termine en `/Pilas/bin/pilas-hook` (instalaciones anteriores).

use serde::Serialize;
use serde_json::{json, Map, Value};
use thiserror::Error;

/// Sufijo que identifica un hook de Pilas aunque la ruta del binario haya cambiado.
const PILAS_BIN_SUFFIX: &str = "/Pilas/bin/pilas-hook";

/// Eventos que Pilas escucha: (evento, lleva `matcher` "*", `timeout` en segundos).
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
    #[error("settings.json debe ser un objeto JSON")]
    NotObject,
    #[error("\"hooks\" debe ser un objeto JSON")]
    HooksNotObject,
    #[error("hooks.{0} debe ser un array")]
    EventNotArray(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    Ok,
    Missing,
    NeedsRepair,
}

/// Una línea del diff: `' '` igual, `'+'` añadida, `'-'` quitada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub op: char,
    pub text: String,
}

/// Grupos (evento, entrada) que Pilas añade para `hook_path`.
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

/// Quita los hooks de Pilas. Solo limpia lo que quedó vacío por esa quitada:
/// arrays de evento, grupos y el objeto `hooks`. Lo ajeno no se toca.
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

/// Añade los hooks de Pilas, quitando antes los suyos previos (sin duplicar ni reordenar lo ajeno).
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

/// Quita solo los hooks de Pilas.
pub fn uninstall(settings: &Value, hook_path: &str) -> Result<Value, InstallError> {
    let mut out = settings.clone();
    let obj = out.as_object_mut().ok_or(InstallError::NotObject)?;
    strip_pilas(obj, hook_path)?;
    Ok(out)
}

/// `Ok` si instalar no cambiaría nada; `Missing` si no hay ningún hook de Pilas;
/// `NeedsRepair` en el resto (entradas viejas, incompletas o en otro orden).
pub fn status(settings: &Value, hook_path: &str) -> HookStatus {
    let Ok(expected) = install(settings, hook_path) else {
        return HookStatus::NeedsRepair;
    };
    if &expected == settings {
        return HookStatus::Ok;
    }
    // quitar cambia algo solo si había hooks de Pilas
    if uninstall(settings, hook_path).ok().as_ref() == Some(settings) {
        HookStatus::Missing
    } else {
        HookStatus::NeedsRepair
    }
}

/// Diff por líneas (LCS). Pensado para JSON pequeño: O(n·m) en memoria.
pub fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let (n, m) = (a.len(), b.len());
    // lcs[i][j] = longitud de la subsecuencia común más larga de a[i..] y b[j..]
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
    fn instala_en_objeto_vacio_con_los_nueve_eventos() {
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
    fn conserva_hooks_ajenos_y_su_orden_y_otras_claves() {
        let ajeno = json!({ "hooks": [{ "type": "command", "command": "/usr/bin/lint" }] });
        let settings = json!({
            "model": "opus",
            "hooks": { "Stop": [ajeno.clone()] }
        });
        let out = install(&settings, BIN).unwrap();
        assert_eq!(out["model"], "opus");
        let stop = out["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0], ajeno);
        assert_eq!(stop.len(), 2);
    }

    #[test]
    fn conserva_el_orden_de_las_claves() {
        let settings: Value =
            serde_json::from_str(r#"{"z":1,"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/x"}]}]},"a":2}"#)
                .unwrap();
        let instalado = install(&settings, BIN).unwrap();
        let claves: Vec<&String> = instalado.as_object().unwrap().keys().collect();
        assert_eq!(claves, vec!["z", "hooks", "a"]);
        let sin = uninstall(&instalado, BIN).unwrap();
        let claves: Vec<&String> = sin.as_object().unwrap().keys().collect();
        assert_eq!(claves, vec!["z", "hooks", "a"]);
        assert_eq!(sin, settings);
    }

    #[test]
    fn instalar_dos_veces_es_idempotente() {
        let ajeno = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "/x" }] }] } });
        let una = install(&ajeno, BIN).unwrap();
        let dos = install(&una, BIN).unwrap();
        assert_eq!(una, dos);
        assert_eq!(count_pilas(&dos), 9);
    }

    #[test]
    fn reemplaza_rutas_antiguas_de_pilas_sin_duplicar() {
        let vieja = "/Applications/Pilas.app/Contents/Pilas/bin/pilas-hook";
        let con_vieja = install(&json!({}), vieja).unwrap();
        let nueva = install(&con_vieja, BIN).unwrap();
        assert_eq!(count_pilas(&nueva), 9);
        assert_eq!(nueva, install(&json!({}), BIN).unwrap());
    }

    #[test]
    fn uninstall_deja_ajenos_y_otras_claves() {
        let ajeno = json!({ "hooks": [{ "type": "command", "command": "/usr/bin/lint" }] });
        let settings = json!({ "theme": "dark", "hooks": { "Stop": [ajeno] } });
        let instalado = install(&settings, BIN).unwrap();
        let out = uninstall(&instalado, BIN).unwrap();
        assert_eq!(out, settings);
    }

    #[test]
    fn uninstall_quita_hooks_object_si_queda_vacio() {
        let instalado = install(&json!({ "theme": "dark" }), BIN).unwrap();
        let out = uninstall(&instalado, BIN).unwrap();
        assert_eq!(out, json!({ "theme": "dark" }));
    }

    #[test]
    fn uninstall_sin_hooks_de_pilas_no_cambia_nada() {
        let settings = json!({ "hooks": {} });
        assert_eq!(uninstall(&settings, BIN).unwrap(), settings);
    }

    #[test]
    fn status_cubre_los_tres_casos() {
        let vacio = json!({});
        assert_eq!(status(&vacio, BIN), HookStatus::Missing);
        let instalado = install(&vacio, BIN).unwrap();
        assert_eq!(status(&instalado, BIN), HookStatus::Ok);
        let mut roto = instalado.clone();
        roto["hooks"]["Stop"] = json!([]);
        assert_eq!(status(&roto, BIN), HookStatus::NeedsRepair);
        // ruta antigua de Pilas: cuenta como nuestra, pero no coincide
        let vieja = install(&vacio, "/Applications/Pilas.app/Pilas/bin/pilas-hook").unwrap();
        assert_eq!(status(&vieja, BIN), HookStatus::NeedsRepair);
        // ruta ajena a Pilas: no es nuestra
        let ajena = install(&vacio, "/otra/pilas-hook").unwrap();
        assert_eq!(status(&ajena, BIN), HookStatus::Missing);
    }

    #[test]
    fn status_con_hooks_ajenos_solo_es_missing() {
        let ajeno = json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "/x" }] }] } });
        assert_eq!(status(&ajeno, BIN), HookStatus::Missing);
    }

    #[test]
    fn entrada_no_objeto_es_error() {
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
    fn diff_basico() {
        let d = line_diff("a\nb\nc", "a\nB\nc\nd");
        let ops: Vec<(char, &str)> = d.iter().map(|l| (l.op, l.text.as_str())).collect();
        assert_eq!(ops, vec![(' ', "a"), ('-', "b"), ('+', "B"), (' ', "c"), ('+', "d")]);
    }

    #[test]
    fn diff_de_textos_iguales_solo_tiene_contexto() {
        let d = line_diff("x\ny", "x\ny");
        assert!(d.iter().all(|l| l.op == ' '));
        assert_eq!(d.len(), 2);
    }
}
