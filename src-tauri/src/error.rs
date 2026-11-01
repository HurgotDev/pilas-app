//! Errors that Tauri commands return to the UI. The wire format is the code only; the UI
//! shows each one through `err.<code>` in `ui/src/i18n.ts`. The `Display` text is for logs.

use pilas_core::installer::InstallError;
use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("unknown hook action: {0}")]
    InvalidAction(String),
    #[error("a folder Pilas needs could not be resolved")]
    PathUnavailable,
    #[error("the hook program is missing next to the app")]
    HookMissing,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Install(#[from] InstallError),
    #[error(transparent)]
    Jump(#[from] pilas_jump::JumpError),
    #[error(transparent)]
    File(#[from] std::io::Error),
    #[error(transparent)]
    Window(#[from] tauri::Error),
    #[error("update check failed: {0}")]
    UpdateCheck(String),
    #[error("update install failed: {0}")]
    UpdateInstall(String),
    #[error("no update is available")]
    UpdateUnavailable,
    #[error("updates can only be installed from Pilas.app")]
    UpdateNotInAppBundle,
    #[error("the request no longer exists")]
    RequestGone,
    #[error("unknown session")]
    SessionUnknown,
}

impl CommandError {
    /// Stable identifier. Every code needs an `err.<code>` key in both languages.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidAction(_) => "invalid_action",
            Self::PathUnavailable => "path_unavailable",
            Self::HookMissing => "hook_missing",
            Self::Json(_) => "settings_not_json",
            Self::Install(InstallError::NotObject) => "settings_not_object",
            Self::Install(InstallError::HooksNotObject) => "settings_hooks_not_object",
            Self::Install(InstallError::EventNotArray(_)) => "settings_event_not_array",
            Self::Jump(_) => "jump_failed",
            Self::File(_) => "file_failed",
            Self::Window(_) => "window_failed",
            Self::UpdateCheck(_) => "update_check_failed",
            Self::UpdateInstall(_) => "update_install_failed",
            Self::UpdateUnavailable => "update_unavailable",
            Self::UpdateNotInAppBundle => "update_not_in_app",
            Self::RequestGone => "request_gone",
            Self::SessionUnknown => "session_unknown",
        }
    }
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_variant_maps_to_its_code() {
        let cases: Vec<(CommandError, &str)> = vec![
            (CommandError::InvalidAction("delete".into()), "invalid_action"),
            (CommandError::PathUnavailable, "path_unavailable"),
            (CommandError::HookMissing, "hook_missing"),
            (
                CommandError::Json(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
                "settings_not_json",
            ),
            (CommandError::Install(InstallError::NotObject), "settings_not_object"),
            (
                CommandError::Install(InstallError::HooksNotObject),
                "settings_hooks_not_object",
            ),
            (
                CommandError::Install(InstallError::EventNotArray("PreToolUse".into())),
                "settings_event_not_array",
            ),
            (
                CommandError::Jump(pilas_jump::JumpError::Script("failed".into())),
                "jump_failed",
            ),
            (CommandError::File(std::io::Error::other("disk full")), "file_failed"),
            (
                CommandError::Window(tauri::Error::WindowLabelAlreadyExists("settings".into())),
                "window_failed",
            ),
            (CommandError::UpdateCheck("offline".into()), "update_check_failed"),
            (CommandError::UpdateInstall("offline".into()), "update_install_failed"),
            (CommandError::UpdateUnavailable, "update_unavailable"),
            (CommandError::UpdateNotInAppBundle, "update_not_in_app"),
            (CommandError::RequestGone, "request_gone"),
            (CommandError::SessionUnknown, "session_unknown"),
        ];
        for (error, code) in cases {
            assert_eq!(error.code(), code, "{error}");
        }
    }

    #[test]
    fn serializes_as_the_bare_code() {
        assert_eq!(
            serde_json::to_string(&CommandError::RequestGone).unwrap(),
            "\"request_gone\""
        );
    }
}
