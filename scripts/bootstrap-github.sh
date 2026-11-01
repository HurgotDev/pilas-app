#!/usr/bin/env bash
# Creates the repo on your GitHub and pushes the first commit.
# Requires an authenticated GitHub CLI: brew install gh && gh auth login
# Usage: ./scripts/bootstrap-github.sh [repo-name] [--private]
set -euo pipefail

REPO="${1:-pilas-app}"
VISIBILITY="--public"
[[ "${2:-}" == "--private" ]] && VISIBILITY="--private"

cd "$(dirname "$0")/.."
command -v gh >/dev/null || { echo "Install GitHub CLI: brew install gh"; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "Log in first: gh auth login"; exit 1; }

OWNER="$(gh api user --jq .login)"
sed -i.bak "s#github.com/OWNER/pilas-app#github.com/${OWNER}/${REPO}#" Cargo.toml && rm -f Cargo.toml.bak

if [[ ! -d .git ]]; then
  git init -b main
fi
git add -A
git commit -m "Initialize Pilas: spec, plan, Rust core, Tauri app and the mascot" || true

gh repo create "${REPO}" ${VISIBILITY} --source=. --remote=origin --push \
  --description "Notch mascot that tells you when Claude Code needs you"

echo
echo "Done: https://github.com/${OWNER}/${REPO}"
echo "Next step: cd $(pwd) && claude"
