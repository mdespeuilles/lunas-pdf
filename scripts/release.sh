#!/usr/bin/env bash
# Publie une version : `bun run release 0.2.0` (ajouter `--edit` pour relire les
# notes avant publication). Repris de Lunas Mail ; voir docs/versions.md.
# Met le numéro dans package.json, Cargo.toml (racine) et tauri.conf.json, ajoute en tête
# de CHANGELOG.md les nouveautés depuis la version précédente (en anglais :
# commits rédigés pour les utilisateurs par Claude Code s'il est installé,
# sinon à écrire dans l'éditeur), commite, crée le tag
# v0.2.0 et pousse : GitHub Actions construit et publie (release.yml), avec ces
# notes sur la page de la version et dans la mise à jour proposée par l'app.
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:-}"
edit="${2:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage : bun run release <x.y.z>" >&2
  exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Des modifications ne sont pas commitées." >&2
  exit 1
fi
if [[ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]]; then
  echo "Une version se publie depuis main." >&2
  exit 1
fi
if git rev-parse "v$version" >/dev/null 2>&1; then
  echo "Le tag v$version existe déjà." >&2
  exit 1
fi

# Numéro de version aux trois endroits (le premier « version » de chaque fichier).
perl -0pi -e "s/\"version\": \"[^\"]+\"/\"version\": \"$version\"/" package.json src-tauri/tauri.conf.json
# Cargo.toml racine : [workspace.package] (repris par les crates).
perl -0pi -e "s/^version = \"[^\"]+\"/version = \"$version\"/m" Cargo.toml
(cargo update -p lunas-pdf --offline >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null)

# Nouveautés : sujets des commits depuis le dernier tag, hors numéros de version.
prev=$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)
commits=$(git log --no-merges --pretty='- %s' "${prev:+$prev..}HEAD" | grep -vE '^- Version [0-9]' || true)
notes=""
if [[ -n "$commits" ]] && command -v claude >/dev/null 2>&1; then
  echo "Rédaction des notes de version (Claude Code)…"
  system="You write release notes in English for Lunas PDF, a desktop PDF reader and editor with an AI assistant. You always answer in English, with only a Markdown bullet list."
  prompt="Commit subjects (in French) since the previous release:
$commits

Write the release notes: one short, user-facing English bullet per meaningful change, related commits merged, purely internal changes (build, CI, lockfiles, refactoring, tests) left out. Output only the bullet list."
  # Sans les réglages de l'utilisateur (langue de réponse…), deux minutes au plus
  # (perl : pas de `timeout` sous macOS).
  notes=$(perl -e 'alarm shift; exec @ARGV' 120 claude -p --tools "" --setting-sources "" --strict-mcp-config \
    --system-prompt "$system" "$prompt" < /dev/null 2>/dev/null | grep -E '^\s*[-*] ' || true)
fi
if [[ -z "$notes" ]]; then
  # Sans Claude Code : brouillon à traduire dans l'éditeur.
  notes="${commits:-- Bug fixes and improvements.}"
  edit="--edit"
fi
header="# Lunas PDF changelog"
previous=""
[[ -f CHANGELOG.md ]] && previous=$(tail -n +3 CHANGELOG.md)
printf '%s\n\n## %s — %s\n\n%s\n\n%s\n' "$header" "$version" "$(date +%Y-%m-%d)" "$notes" "$previous" > CHANGELOG.md
if [[ "$edit" == "--edit" ]]; then
  "${EDITOR:-vi}" CHANGELOG.md
fi

git add package.json src-tauri/tauri.conf.json Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "Version $version"
git tag "v$version"
git push origin main "v$version"
echo "v$version poussée : suivez la construction dans l'onglet Actions du dépôt."
