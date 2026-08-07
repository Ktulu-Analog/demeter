#!/usr/bin/env bash
# ============================================================================
# Demeter — fetch-mcp-pack.sh
# ============================================================================
# Télécharge le pack de MCP officiels (depuis le dépôt demeter-mcp-pack), à un
# tag figé par MCP_PACK_VERSION, vérifie son checksum SHA-256 puis le
# déballe dans src-tauri/mcp-servers/ — c'est ce dossier que `tauri build`
# embarque ensuite comme resources (voir tauri.conf.json → bundle.resources).
#
# À exécuter avant tout `tauri build` / `tauri dev` si src-tauri/mcp-servers/
# est absent ou périmé. Fonctionne aussi bien en local qu'en CI.
# Idempotent : ne retélécharge rien si la version cible est déjà présente
# (voir plus bas) — passez --force pour forcer une réinstallation complète.
#
# Le résultat est 100% reproductible : la version du pack embarquée est fixée
# par le fichier MCP_PACK_VERSION à la racine du dépôt, pas par la "latest"
# du moment — pas de build cassé par une release amont surprise.
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/src-tauri/mcp-servers"

# À adapter à votre organisation GitHub.
PACK_REPO="${MCP_PACK_REPO:-Ktulu-Analog/demeter-mcp-pack}"
VERSION="$(cat "$ROOT/MCP_PACK_VERSION")"

# Idempotent par défaut : si le pack demandé est déjà installé, on ne
# retélécharge rien (permet d'appeler ce script sans frais à chaque
# `cargo tauri dev`, voir beforeDevCommand dans tauri.conf.json).
# `--force` force une réinstallation complète.
if [[ "${1:-}" != "--force" && -f "$DEST/VERSION" && "$(cat "$DEST/VERSION")" == "${VERSION#v}" ]]; then
  echo "✅ Pack MCP v${VERSION#v} déjà présent dans src-tauri/mcp-servers/ (rien à faire)."
  exit 0
fi

BASE_URL="https://github.com/${PACK_REPO}/releases/download/${VERSION}"
ARCHIVE="mcp-pack-${VERSION#v}.zip"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "📦 Récupération de $PACK_REPO @ $VERSION"
curl -fsSL "${BASE_URL}/${ARCHIVE}"        -o "$TMP_DIR/${ARCHIVE}"
curl -fsSL "${BASE_URL}/checksums.txt"     -o "$TMP_DIR/checksums.txt"

echo "🔒 Vérification du checksum SHA-256…"
EXPECTED="$(grep " ${ARCHIVE}\$" "$TMP_DIR/checksums.txt" | awk '{print $1}')"
if [[ -z "$EXPECTED" ]]; then
  echo "❌ Aucune entrée pour ${ARCHIVE} dans checksums.txt" >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$TMP_DIR/${ARCHIVE}" | awk '{print $1}')"
else
  ACTUAL="$(shasum -a 256 "$TMP_DIR/${ARCHIVE}" | awk '{print $1}')"
fi

if [[ "$EXPECTED" != "$ACTUAL" ]]; then
  echo "❌ Checksum invalide pour ${ARCHIVE}" >&2
  echo "   attendu : $EXPECTED" >&2
  echo "   obtenu  : $ACTUAL" >&2
  exit 1
fi
echo "  ✅ Checksum OK"

echo "📂 Déballage dans $DEST"
rm -rf "$DEST"
mkdir -p "$DEST"
unzip -q "$TMP_DIR/${ARCHIVE}" -d "$DEST"

# Trace la version installée — relue par Demeter au runtime pour proposer
# une mise à jour si une version plus récente est disponible (panel MCP).
echo "${VERSION#v}" > "$DEST/VERSION"

echo "✅ Pack MCP v${VERSION#v} installé dans src-tauri/mcp-servers/"
