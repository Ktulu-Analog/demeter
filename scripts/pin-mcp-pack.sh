#!/usr/bin/env bash
# ============================================================================
# Demeter — pin-mcp-pack.sh  (constat F-02)
# ============================================================================
# Calcule l'empreinte SHA-256 de l'archive du pack MCP désigné par MCP_PACK_VERSION
# et l'écrit dans MCP_PACK_SHA256, ancre de confiance contrôlée par ce dépôt :
# scripts/fetch-mcp-pack.sh refuse ensuite toute archive dont l'empreinte diffère
# (tag déplacé, release altérée).
#
# ⚠ Ce script fait confiance à ce que la release contient AU MOMENT où vous le lancez
#   (trust on first use). Avant de committer MCP_PACK_SHA256 :
#     - vérifiez la release sur GitHub (auteur, date, notes) ;
#     - idéalement, comparez l'empreinte avec celle d'une compilation reproductible
#       du dépôt demeter-mcp-pack, ou avec une signature minisign (docs/SECURITE_PACK_MCP.md) ;
#     - faites relire le diff de MCP_PACK_SHA256 comme du code.
#
# Usage :  ./scripts/pin-mcp-pack.sh
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACK_REPO="${MCP_PACK_REPO:-Ktulu-Analog/demeter-mcp-pack}"
VERSION="$(cat "$ROOT/MCP_PACK_VERSION")"
ARCHIVE="mcp-pack-${VERSION#v}.zip"
BASE_URL="https://github.com/${PACK_REPO}/releases/download/${VERSION}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "📦 Téléchargement de ${PACK_REPO} @ ${VERSION}"
curl -fsSL "${BASE_URL}/${ARCHIVE}"    -o "$TMP_DIR/${ARCHIVE}"
curl -fsSL "${BASE_URL}/checksums.txt" -o "$TMP_DIR/checksums.txt"

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$TMP_DIR/${ARCHIVE}" | awk '{print $1}')"
else
  ACTUAL="$(shasum -a 256 "$TMP_DIR/${ARCHIVE}" | awk '{print $1}')"
fi

EXPECTED="$(grep " ${ARCHIVE}\$" "$TMP_DIR/checksums.txt" | awk '{print $1}' || true)"
if [[ -z "$EXPECTED" || "$EXPECTED" != "$ACTUAL" ]]; then
  echo "❌ checksums.txt de la release ne correspond pas à l'archive (attendu : ${EXPECTED:-absent}, obtenu : $ACTUAL)" >&2
  exit 1
fi

cat > "$ROOT/MCP_PACK_SHA256" <<PIN
# Empreinte SHA-256 épinglée de l'archive du pack MCP (constat F-02).
# Générée par scripts/pin-mcp-pack.sh — à relire avant de committer.
# Format : <sha256>  <nom de l'archive>
${ACTUAL}  ${ARCHIVE}
PIN

echo "✅ MCP_PACK_SHA256 écrit : ${ACTUAL}  ${ARCHIVE}"
echo "   Vérifiez la release sur GitHub, puis committez MCP_PACK_SHA256."
