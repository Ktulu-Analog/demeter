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

# ── Authenticité (D-13, F-02) ────────────────────────────────────────────────
# Le SHA-256 de checksums.txt est publié dans la MÊME release que l'archive : il
# prouve l'intégrité du transfert, PAS l'authenticité (qui contrôle la release remplace
# les deux). L'authenticité repose donc sur au moins UNE des deux ancres de confiance
# gérées dans CE dépôt :
#   1. une signature minisign : clé publique dans src-tauri/mcp-pack.pub, signature
#      checksums.txt.minisig dans la release ;
#   2. une EMPREINTE ÉPINGLÉE : fichier MCP_PACK_SHA256 (SHA-256 de l'archive),
#      validé en revue de code lors de chaque changement de version.
# Si aucune des deux n'est en place, le build ÉCHOUE (défaillance fermée). Les deux
# peuvent coexister : dans ce cas, les deux contrôles sont exigés.
# Dérogation explicite et bruyante, pour le développement seulement :
#   MCP_PACK_ALLOW_UNSIGNED=1
PUBFILE="$ROOT/src-tauri/mcp-pack.pub"
PUBKEY="$(grep -v -e '^[[:space:]]*#' -e '^untrusted comment:' -e '^[[:space:]]*$' "$PUBFILE" 2>/dev/null | head -n1 || true)"

PIN_FILE="$ROOT/MCP_PACK_SHA256"
PIN_EXPECTED=""
if [[ -f "$PIN_FILE" ]]; then
  PIN_HASH="$(awk 'NF && $1 !~ /^#/ {print tolower($1); exit}' "$PIN_FILE")"
  PIN_NAME="$(awk 'NF && $1 !~ /^#/ {print $2; exit}' "$PIN_FILE" | sed 's/^\*//')"
  if [[ -n "$PIN_HASH" && "$PIN_NAME" == "$ARCHIVE" ]]; then
    PIN_EXPECTED="$PIN_HASH"
  else
    echo "❌ MCP_PACK_SHA256 ne correspond pas à ${ARCHIVE} (épinglé : '${PIN_NAME:-?}')." >&2
    echo "   MCP_PACK_VERSION a changé : relancez ./scripts/pin-mcp-pack.sh et faites relire le diff." >&2
    exit 1
  fi
fi

if [[ -z "$PUBKEY" && -z "$PIN_EXPECTED" ]]; then
  if [[ "${MCP_PACK_ALLOW_UNSIGNED:-}" == "1" ]]; then
    echo "⚠️  MCP_PACK_ALLOW_UNSIGNED=1 : AUCUNE vérification d'authenticité (ni clé minisign, ni empreinte épinglée)." >&2
  else
    echo "❌ Authenticité du pack MCP non vérifiable : ni clé publique (src-tauri/mcp-pack.pub), ni empreinte épinglée (MCP_PACK_SHA256)." >&2
    echo "   Les binaires de ce pack seront exécutés avec vos droits : le build est refusé." >&2
    echo "   → Épingler la version actuelle :  ./scripts/pin-mcp-pack.sh   (puis relire et committer MCP_PACK_SHA256)" >&2
    echo "   → ou configurer une clé minisign : voir docs/SECURITE_PACK_MCP.md" >&2
    exit 1
  fi
fi

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

if [[ -n "$PIN_EXPECTED" ]]; then
  if [[ "$PIN_EXPECTED" != "$ACTUAL" ]]; then
    echo "❌ Empreinte épinglée (MCP_PACK_SHA256) non respectée pour ${ARCHIVE}" >&2
    echo "   épinglée : $PIN_EXPECTED" >&2
    echo "   obtenue  : $ACTUAL" >&2
    echo "   L'archive de la release a changé depuis l'épinglage (tag déplacé ou release altérée)." >&2
    exit 1
  fi
  echo "  ✅ Empreinte épinglée OK"
fi

echo "📂 Déballage dans $DEST"
rm -rf "$DEST"
mkdir -p "$DEST"
unzip -q "$TMP_DIR/${ARCHIVE}" -d "$DEST"

# Trace la version installée — relue par Demeter au runtime pour proposer
# une mise à jour si une version plus récente est disponible (panel MCP).
echo "${VERSION#v}" > "$DEST/VERSION"

echo "✅ Pack MCP v${VERSION#v} installé dans src-tauri/mcp-servers/"
