#!/usr/bin/env bash
# Crea el repo en tu GitHub y sube el primer commit.
# Requiere GitHub CLI autenticado: brew install gh && gh auth login
# Uso: ./scripts/bootstrap-github.sh [nombre-del-repo] [--private]
set -euo pipefail

REPO="${1:-pilas-app}"
VISIBILITY="--public"
[[ "${2:-}" == "--private" ]] && VISIBILITY="--private"

cd "$(dirname "$0")/.."
command -v gh >/dev/null || { echo "Instala GitHub CLI: brew install gh"; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "Inicia sesión primero: gh auth login"; exit 1; }

OWNER="$(gh api user --jq .login)"
# Pone tu usuario en la URL del repositorio del workspace.
sed -i.bak "s#github.com/OWNER/pilas-app#github.com/${OWNER}/${REPO}#" Cargo.toml && rm -f Cargo.toml.bak

if [[ ! -d .git ]]; then
  git init -b main
fi
git add -A
git commit -m "Inicializa Pilas: spec, plan, núcleo en Rust, app Tauri y la pilita" || true

gh repo create "${REPO}" ${VISIBILITY} --source=. --remote=origin --push \
  --description "La pilita del notch que te avisa cuando Claude Code te necesita"

echo
echo "Listo: https://github.com/${OWNER}/${REPO}"
echo "Siguiente paso: cd $(pwd) && claude"
