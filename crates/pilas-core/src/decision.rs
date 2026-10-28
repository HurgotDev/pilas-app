//! Construye la salida JSON que `pilas-hook` imprime en stdout para Claude Code.
//! Referencia: https://code.claude.com/docs/en/hooks (PermissionRequest y PreToolUse).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Lo que el usuario eligió en la tarjeta de permiso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChoice {
    Allow,
    Deny,
    /// Permitir y guardar las `permission_suggestions` que Claude Code propuso.
    Always,
    /// Devolver el control al diálogo nativo de la terminal.
    Terminal,
}

pub const DENY_MESSAGE: &str = "Denegado desde Pilas";

/// Salida para un evento `PermissionRequest`. `None` = sin salida (diálogo nativo).
pub fn permission_output(choice: &PermissionChoice, payload: &Value) -> Option<Value> {
    let decision = match choice {
        PermissionChoice::Allow => json!({ "behavior": "allow" }),
        PermissionChoice::Deny => json!({ "behavior": "deny", "message": DENY_MESSAGE }),
        PermissionChoice::Always => {
            // Solo reenviamos lo que Claude Code sugirió: nunca inventamos reglas.
            let suggestions = payload
                .get("permission_suggestions")
                .cloned()
                .unwrap_or_else(|| json!([]));
            json!({ "behavior": "allow", "updatedPermissions": suggestions })
        }
        PermissionChoice::Terminal => return None,
    };
    Some(json!({
        "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision }
    }))
}

/// Respuestas a `AskUserQuestion`: texto de la pregunta → etiqueta(s) elegida(s), separadas por coma.
pub type Answers = BTreeMap<String, String>;

/// Salida para `PreToolUse` de `AskUserQuestion` con las respuestas del usuario.
/// Conserva el `tool_input` original y le añade `answers`.
pub fn question_output(payload: &Value, answers: &Answers) -> Option<Value> {
    let mut input = payload.get("tool_input")?.clone();
    input.as_object_mut()?.insert("answers".into(), json!(answers));
    Some(json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": input
        }
    }))
}

/// "Responder en la terminal" para una pregunta: `ask` hace aparecer el selector nativo.
pub fn question_to_terminal() -> Value {
    json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "ask" } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permitir_y_denegar() {
        let p = json!({});
        assert_eq!(
            permission_output(&PermissionChoice::Allow, &p).unwrap()["hookSpecificOutput"]["decision"]["behavior"],
            "allow"
        );
        let deny = permission_output(&PermissionChoice::Deny, &p).unwrap();
        assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert_eq!(deny["hookSpecificOutput"]["decision"]["message"], DENY_MESSAGE);
        assert!(permission_output(&PermissionChoice::Terminal, &p).is_none());
    }

    #[test]
    fn siempre_reenvia_sugerencias() {
        let sugg = json!([{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test" }],
                            "behavior": "allow", "destination": "localSettings" }]);
        let p = json!({ "permission_suggestions": sugg });
        let out = permission_output(&PermissionChoice::Always, &p).unwrap();
        assert_eq!(out["hookSpecificOutput"]["decision"]["updatedPermissions"], sugg);
    }

    #[test]
    fn respuestas_de_preguntas() {
        let p = json!({ "tool_name": "AskUserQuestion", "tool_input": { "questions": [{ "question": "¿Formato?" }] } });
        let mut a = Answers::new();
        a.insert("¿Formato?".into(), "Resumen".into());
        let out = question_output(&p, &a).unwrap();
        let input = &out["hookSpecificOutput"]["updatedInput"];
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(input["answers"]["¿Formato?"], "Resumen");
        assert_eq!(input["questions"][0]["question"], "¿Formato?");
    }
}
