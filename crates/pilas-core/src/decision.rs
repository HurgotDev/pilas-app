//! JSON that `pilas-hook` prints to stdout for Claude Code.
//! Reference: https://code.claude.com/docs/en/hooks (PermissionRequest and PreToolUse).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChoice {
    Allow,
    Deny,
    /// Allow and save the `permission_suggestions` Claude Code proposed.
    Always,
    /// Hand control back to the terminal's native dialog.
    Terminal,
}

/// Read by Claude, not the user, so it is never translated.
pub const DENY_MESSAGE: &str = "Denied from Pilas";

/// `None` means no output, so the native dialog shows.
pub fn permission_output(choice: &PermissionChoice, payload: &Value) -> Option<Value> {
    let decision = match choice {
        PermissionChoice::Allow => json!({ "behavior": "allow" }),
        PermissionChoice::Deny => json!({ "behavior": "deny", "message": DENY_MESSAGE }),
        PermissionChoice::Always => {
            // Only forward what Claude Code suggested: never invent rules.
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

/// `AskUserQuestion` answers: question text → chosen label(s), comma-separated.
pub type Answers = BTreeMap<String, String>;

/// Keeps the original `tool_input` and adds `answers` to it.
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

/// `ask` brings up the native picker in the terminal.
pub fn question_to_terminal() -> Value {
    json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "ask" } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_and_deny() {
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
    fn always_forwards_suggestions() {
        let sugg = json!([{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test" }],
                            "behavior": "allow", "destination": "localSettings" }]);
        let p = json!({ "permission_suggestions": sugg });
        let out = permission_output(&PermissionChoice::Always, &p).unwrap();
        assert_eq!(out["hookSpecificOutput"]["decision"]["updatedPermissions"], sugg);
    }

    #[test]
    fn question_answers() {
        let p = json!({ "tool_name": "AskUserQuestion", "tool_input": { "questions": [{ "question": "Format?" }] } });
        let mut a = Answers::new();
        a.insert("Format?".into(), "Summary".into());
        let out = question_output(&p, &a).unwrap();
        let input = &out["hookSpecificOutput"]["updatedInput"];
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(input["answers"]["Format?"], "Summary");
        assert_eq!(input["questions"][0]["question"], "Format?");
    }
}
