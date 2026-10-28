//! Saltar a la terminal de una sesión. Regla: fallar cerrado. Si no hay coincidencia segura,
//! solo se trae la app al frente; nunca se enfoca una pestaña equivocada.

use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct JumpTarget {
    pub term_program: Option<String>,
    pub tty: Option<String>,
    pub cwd: Option<String>,
    /// Id de terminal de Ghostty cacheado en un salto anterior.
    pub cached_terminal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JumpOutcome {
    /// Se enfocó la terminal exacta (con su id, para cachearlo).
    Exact { terminal_id: Option<String> },
    /// Solo se trajo la app al frente.
    AppOnly,
}

#[derive(Debug, thiserror::Error)]
pub enum JumpError {
    #[error("no se pudo ejecutar osascript: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("AppleScript falló: {0}")]
    Script(String),
}

pub trait TerminalJumper {
    fn can_handle(&self, term_program: Option<&str>) -> bool;
    fn jump(&self, target: &JumpTarget) -> Result<JumpOutcome, JumpError>;
}

/// Elige el jumper según `TERM_PROGRAM`.
pub fn jumper_for(term_program: Option<&str>) -> Box<dyn TerminalJumper> {
    let ghostty = GhosttyJumper;
    if ghostty.can_handle(term_program) {
        Box::new(ghostty)
    } else {
        Box::new(GenericJumper)
    }
}

fn osascript(script: &str) -> Result<String, JumpError> {
    let out = Command::new("osascript").arg("-e").arg(script).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(JumpError::Script(String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

/// Escapa un string para un literal AppleScript entre comillas dobles.
fn as_literal(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Ghostty ≥ 1.3 en macOS (https://ghostty.org/docs/features/applescript).
pub struct GhosttyJumper;

impl GhosttyJumper {
    fn activate() -> Result<JumpOutcome, JumpError> {
        osascript("tell application \"Ghostty\" to activate")?;
        Ok(JumpOutcome::AppOnly)
    }

    fn focus_by_id(id: &str) -> Result<bool, JumpError> {
        // TODO(M6): confirmar en Script Editor (Ghostty.sdef) la sintaxis exacta para referenciar por id.
        let script = format!(
            "tell application \"Ghostty\"\n\
               set ts to (every terminal whose id is {id})\n\
               if (count of ts) is 1 then\n\
                 focus (item 1 of ts)\n\
                 activate\n\
                 return \"ok\"\n\
               end if\n\
               return \"none\"\n\
             end tell",
            id = as_literal(id)
        );
        Ok(osascript(&script)? == "ok")
    }

    /// Busca por directorio de trabajo; devuelve el id solo si hay exactamente una coincidencia.
    fn focus_by_cwd(cwd: &str) -> Result<Option<String>, JumpError> {
        let script = format!(
            "tell application \"Ghostty\"\n\
               set ts to (every terminal whose working directory is {cwd})\n\
               if (count of ts) is 1 then\n\
                 set t to item 1 of ts\n\
                 focus t\n\
                 activate\n\
                 return (id of t) as text\n\
               end if\n\
               return \"\"\n\
             end tell",
            cwd = as_literal(cwd)
        );
        let id = osascript(&script)?;
        Ok((!id.is_empty()).then_some(id))
    }
}

impl TerminalJumper for GhosttyJumper {
    fn can_handle(&self, term_program: Option<&str>) -> bool {
        term_program.is_some_and(|t| t.eq_ignore_ascii_case("ghostty"))
    }

    fn jump(&self, target: &JumpTarget) -> Result<JumpOutcome, JumpError> {
        if let Some(id) = &target.cached_terminal_id {
            if Self::focus_by_id(id).unwrap_or(false) {
                return Ok(JumpOutcome::Exact { terminal_id: Some(id.clone()) });
            }
        }
        // TODO(M6): coincidencia por TTY. Pendiente verificar si el diccionario de Ghostty expone
        // el TTY de cada terminal (pregunta abierta en la spec). Hasta entonces: cwd + fallo cerrado.
        if let Some(cwd) = &target.cwd {
            if let Ok(Some(id)) = Self::focus_by_cwd(cwd) {
                return Ok(JumpOutcome::Exact { terminal_id: Some(id) });
            }
        }
        Self::activate()
    }
}

/// Cualquier otra terminal: traerla al frente por su nombre de app.
pub struct GenericJumper;

fn app_name(term_program: Option<&str>) -> Option<&'static str> {
    match term_program?.to_ascii_lowercase().as_str() {
        "iterm.app" => Some("iTerm"),
        "apple_terminal" => Some("Terminal"),
        "vscode" => Some("Visual Studio Code"),
        "warpterminal" => Some("Warp"),
        "ghostty" => Some("Ghostty"),
        _ => None,
    }
}

impl TerminalJumper for GenericJumper {
    fn can_handle(&self, _term_program: Option<&str>) -> bool {
        true
    }

    fn jump(&self, target: &JumpTarget) -> Result<JumpOutcome, JumpError> {
        if let Some(app) = app_name(target.term_program.as_deref()) {
            osascript(&format!("tell application {} to activate", as_literal(app)))?;
        }
        Ok(JumpOutcome::AppOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elige_ghostty() {
        assert!(GhosttyJumper.can_handle(Some("ghostty")));
        assert!(!GhosttyJumper.can_handle(Some("iTerm.app")));
        assert!(!GhosttyJumper.can_handle(None));
    }

    #[test]
    fn escapa_literales() {
        assert_eq!(as_literal(r#"/a "b"\c"#), r#""/a \"b\"\\c""#);
    }

    #[test]
    fn nombres_de_apps() {
        assert_eq!(app_name(Some("iTerm.app")), Some("iTerm"));
        assert_eq!(app_name(Some("desconocida")), None);
    }
}
