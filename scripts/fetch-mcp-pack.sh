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

# ── Authenticité (D-13) ──────────────────────────────────────────────────────
# Si une clé publique minisign est configurée dans src-tauri/mcp-pack.pub,
# checksums.txt DOIT être signé (checksums.txt.minisig). Sans clé configurée,
# seul le SHA-256 de la même release est vérifié (intégrité, pas authenticité).
PUBFILE="$ROOT/src-tauri/mcp-pack.pub"
PUBKEY="$(grep -v -e '^[[:space:]]*#' -e '^untrusted comment:' -e '^[[:space:]]*$' "$PUBFILE" 2>/dev/null | head -n1 || true)"
if [[ -n "$PUBKEY" ]]; then
  if [[ "${MCP_PACK_ALLOW_UNSIGNED:-}" == "1" ]]; then
    echo "⚠️  MCP_PACK_ALLOW_UNSIGNED=1 : signature NON vérifiée." >&2
  else
    command -v minisign >/dev/null || { echo "❌ minisign requis (clé publique configurée) : https://jedisct1.github.io/minisign/" >&2; exit 1; }
    curl -fsSL "${BASE_URL}/checksums.txt.minisig" -o "$TMP_DIR/checksums.txt.minisig" \
      || { echo "❌ Release non signée : checksums.txt.minisig introuvable" >&2; exit 1; }
    echo "🔏 Vérification de la signature minisign…"
    TRUSTED="$(minisign -V -P "$PUBKEY" -m "$TMP_DIR/checksums.txt" -x "$TMP_DIR/checksums.txt.minisig" 2>&1)" \
      || { echo "❌ Signature invalide : $TRUSTED" >&2; exit 1; }
    if ! grep -q "version:${VERSION#v}\([[:space:]]\|\$\)" <<<"$TRUSTED"; then
      echo "❌ La signature ne porte pas version:${VERSION#v} (rétrogradation ?)" >&2
      echo "$TRUSTED" >&2
      exit 1
    fi
    echo "  ✅ Signature OK"
  fi
else
  echo "ℹ️  Aucune clé publique configurée (src-tauri/mcp-pack.pub) : authenticité non vérifiée." >&2
fi

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
