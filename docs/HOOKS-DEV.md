# Probar hooks sin romper tu sesión de Claude Code

Vas a desarrollar Pilas **con** Claude Code, así que no instales los hooks de desarrollo en
`~/.claude/settings.json`: tu propia sesión de trabajo empezaría a pasar por la app a medio construir.

## Opción 1: carpeta de pruebas con hooks locales

```bash
mkdir -p ~/pilas-sandbox/.claude && cd ~/pilas-sandbox
cargo build -p pilas-hook --manifest-path ~/dev/pilas-app/Cargo.toml   # ajusta la ruta
```

Crea `~/pilas-sandbox/.claude/settings.local.json` (la ruta del binario debe ser absoluta):

```json
{
  "hooks": {
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }],
    "PreToolUse":       [{ "matcher": "*", "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [], "timeout": 3600 }] }],
    "PermissionRequest":[{ "matcher": "*", "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [], "timeout": 3600 }] }],
    "PostToolUse":      [{ "matcher": "*", "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }],
    "Notification":     [{ "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }],
    "StopFailure":      [{ "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }],
    "PreCompact":       [{ "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "/RUTA/target/debug/pilas-hook", "args": [] }] }]
  }
}
```

Con `npm run app` corriendo, abre Claude Code en `~/pilas-sandbox` desde Ghostty y pídele algo que
necesite permiso (por ejemplo, "crea un archivo hola.txt").

## Opción 2: simular eventos sin Claude Code

```bash
echo '{"session_id":"s1","cwd":"/tmp/demo","hook_event_name":"SessionStart"}' | target/debug/pilas-hook
echo '{"session_id":"s1","cwd":"/tmp/demo","hook_event_name":"UserPromptSubmit"}' | target/debug/pilas-hook
echo '{"session_id":"s1","cwd":"/tmp/demo","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"rm -rf build"}}' \
  | target/debug/pilas-hook     # queda esperando: responde desde la isla y verás el JSON en stdout
```

`PILAS_SOCKET=/tmp/pilas-dev.sock` cambia la ruta del socket en la app y en el hook (útil para tests).

## Comprobaciones de seguridad

- Con la app cerrada, el hook debe terminar al instante y sin salida: `echo '{}' | pilas-hook; echo $?` → `0`.
- Si un hook "no hace nada", ejecuta Claude Code con `claude --debug` y revisa el log de hooks.

## Probar el instalador sin tocar tu configuración

El instalador de la app escribe en `~/.claude/settings.json`. Para probarlo,
apunta `PILAS_CLAUDE_SETTINGS` a un archivo de prueba antes de abrir la app:

```bash
echo '{ "theme": "dark" }' > /tmp/claude-test.json
cargo build -p pilas-hook   # el instalador copia target/debug/pilas-hook
PILAS_CLAUDE_SETTINGS=/tmp/claude-test.json npm run app
```

Desde la ventana de ajustes verás el diff; al aplicarlo se crea
`/tmp/claude-test.json.pilas-backup-AAAAMMDD-HHMMSS`. Sin la variable, la app
usa tu configuración real.

## Probar la temperatura de caché sin esperar una hora

En builds de debug, `PILAS_CACHE_TTL_SECS` sustituye el TTL de todas las
sesiones (la release lo ignora). Con el aviso en 1 min en Ajustes:

```bash
PILAS_CACHE_TTL_SECS=120 npm run app
```

Tras una respuesta en el sandbox la sesión sale caliente; a los 60 s
enfriándose (tirita, sonido y cuenta atrás en la píldora) y a los 120 s fría.
