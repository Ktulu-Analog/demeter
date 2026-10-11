#!/usr/bin/env bash
# ============================================================================
# Demeter — sign-mcp-pack.sh
# ============================================================================
# Signe le fichier checksums.txt d'une release du pack MCP avec minisign
# (constat D-13). À exécuter par le mainteneur du pack, SUR SON POSTE (la clé
# privée ne doit jamais être stockée dans les secrets de GitHub Actions).
#
# Usage :
#   scripts/sign-mcp-pack.sh <version> <checksums.txt> [clé-privée]
#   ex. : scripts/sign-mcp-pack.sh 0.3.2 dist/checksums.txt
#
# Produit <checksums.txt>.minisig, à publier comme asset de la release, à côté
# de checksums.txt et de mcp-pack-<version>.zip.
#
# Le « commentaire de confiance » signé contient `version:<version>` : Demeter
# refuse une signature dont la version ne correspond pas à l'étiquette de la
# release (protection contre le rejeu d'une ancienne release vulnérable).
# ============================================================================
set -euo pipefail

VERSION="${1:-}"; FILE="${2:-}"; KEY="${3:-$HOME/.minisign/demeter-mcp-pack.key}"
if [[ -z "$VERSION" || -z "$FILE" ]]; then
  echo "Usage : $0 <version> <checksums.txt> [clé-privée]" >&2; exit 2
fi
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  echo "❌ Version invalide : $VERSION (attendu 1.2.3)" >&2; exit 2
fi
command -v minisign >/dev/null || { echo "❌ minisign introuvable (https://jedisct1.github.io/minisign/)" >&2; exit 1; }
[[ -f "$FILE" ]] || { echo "❌ Fichier introuvable : $FILE" >&2; exit 1; }
[[ -f "$KEY"  ]] || { echo "❌ Clé privée introuvable : $KEY" >&2; exit 1; }

minisign -S -s "$KEY" -m "$FILE" -t "demeter-mcp-pack version:${VERSION}"
echo "✅ Signature écrite : ${FILE}.minisig"
echo "   Publiez-la comme asset de la release v${VERSION}."
