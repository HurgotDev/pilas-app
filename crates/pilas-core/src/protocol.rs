//! Protocolo IPC v1 entre `pilas-hook` y la app: una línea JSON por mensaje.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const AGENT_CLAUDE_CODE: &str = "claude-code";

/// Mensaje hook → app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookMessage {
    pub v: u32,
    /// UUID generado por el hook; la respuesta debe traer el mismo.
    pub id: String,
    /// Fijo a `claude-code` en la v1; punto de extensión para otros agentes.
    pub agent: String,
    /// `hook_event_name` del payload (`PermissionRequest`, `Stop`, …).
    pub event: String,
    /// Si el hook se queda esperando una respuesta.
    pub blocking: bool,
    #[serde(default)]
    pub tty: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub term_program: Option<String>,
    /// JSON original de Claude Code, sin modificar.
    pub payload: Value,
}

/// Mensaje app → hook (solo para eventos bloqueantes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppReply {
    pub v: u32,
    pub id: String,
    /// Lo que el hook imprime tal cual en stdout. `None` = sin decisión (responder en la terminal).
    pub stdout: Option<Value>,
}

impl AppReply {
    pub fn new(id: impl Into<String>, stdout: Option<Value>) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id: id.into(),
            stdout,
        }
    }
}

pub fn tool_name(payload: &Value) -> Option<&str> {
    payload.get("tool_name")?.as_str()
}

pub fn session_id(payload: &Value) -> Option<&str> {
    payload.get("session_id")?.as_str()
}

/// Decide si un evento debe esperar respuesta de la app.
///
/// - `PermissionRequest` bloquea, salvo para `AskUserQuestion`: Claude Code también dispara
///   `PermissionRequest` para esa herramienta y debemos ignorarlo para no duplicar la tarjeta.
/// - `PreToolUse` bloquea solo para `AskUserQuestion` (las respuestas van por `updatedInput`).
pub fn is_blocking(event: &str, payload: &Value) -> bool {
    match event {
        "PermissionRequest" => tool_name(payload) != Some("AskUserQuestion"),
        "PreToolUse" => tool_name(payload) == Some("AskUserQuestion"),
        _ => false,
    }
}

/// Eventos que Pilas ignora por completo (sin cambios de estado ni tarjetas).
pub fn is_ignored(event: &str, payload: &Value) -> bool {
    event == "PermissionRequest" && tool_name(payload) == Some("AskUserQuestion")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bloqueantes() {
        assert!(is_blocking("PermissionRequest", &json!({"tool_name": "Bash"})));
        assert!(!is_blocking(
            "PermissionRequest",
            &json!({"tool_name": "AskUserQuestion"})
        ));
        assert!(is_blocking("PreToolUse", &json!({"tool_name": "AskUserQuestion"})));
        assert!(!is_blocking("PreToolUse", &json!({"tool_name": "Edit"})));
        assert!(!is_blocking("Stop", &json!({})));
    }

    #[test]
    fn ignora_permission_request_de_preguntas() {
        assert!(is_ignored(
            "PermissionRequest",
            &json!({"tool_name": "AskUserQuestion"})
        ));
        assert!(!is_ignored("PermissionRequest", &json!({"tool_name": "Bash"})));
    }

    #[test]
    fn ida_y_vuelta_json() {
        let m = HookMessage {
            v: PROTOCOL_VERSION,
            id: "abc".into(),
            agent: AGENT_CLAUDE_CODE.into(),
            event: "Stop".into(),
            blocking: false,
            tty: Some("/dev/ttys004".into()),
            pid: Some(42),
            term_program: Some("ghostty".into()),
            payload: json!({"session_id": "s1"}),
        };
        let line = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<HookMessage>(&line).unwrap(), m);
    }
}
