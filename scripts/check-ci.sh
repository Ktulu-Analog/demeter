#!/usr/bin/env bash
# ============================================================================
# Demeter — check-ci.sh
# ============================================================================
# Rejoue en local, dans le même ordre et avec les mêmes flags, les jobs du
# CI GitHub Actions (.github/workflows/ci.yml : jobs "frontend" et "rust")
# avant de pousser.
#
#   chmod +x scripts/check-ci.sh
#   ./scripts/check-ci.sh          # vérifie seulement
#   ./scripts/check-ci.sh --fix    # applique cargo fmt au lieu de le vérifier
# ============================================================================
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FIX=false
[[ "${1:-}" == "--fix" ]] && FIX=true

FAILED=0
step() { echo -e "\n▶ $1"; }
ok()   { echo "  ✅ $1"; }
fail() { echo "  ❌ $1"; FAILED=1; }

# ══════════════════════════════════════════════════════════════════════════
# Job "frontend" (Node 22, comme en CI)
# ══════════════════════════════════════════════════════════════════════════

step "[frontend] npm ci"
if (cd frontend && npm ci); then
  ok "Dépendances installées (npm ci)"
else
  fail "npm ci a échoué — package-lock.json désynchronisé de package.json ?"
fi

step "[frontend] npm run typecheck"
if (cd frontend && npm run typecheck); then
  ok "Typecheck OK"
else
  fail "Erreurs TypeScript à corriger"
fi

step "[frontend] npm run build"
if (cd frontend && npm run build); then
  ok "Build frontend OK"
else
  fail "Le build frontend échoue"
fi

# ══════════════════════════════════════════════════════════════════════════
# Job "rust" (fmt, clippy, check, test — tous en --no-default-features,
# comme en CI, pour éviter le check frontendDist déclenché par custom-protocol)
# ══════════════════════════════════════════════════════════════════════════

step "[rust] cargo fmt --check"
if $FIX; then
  (cd src-tauri && cargo fmt) && ok "Formatage appliqué"
else
  if (cd src-tauri && cargo fmt --check); then
    ok "Formatage OK"
  else
    fail "Formatage à corriger — lancez : (cd src-tauri && cargo fmt)  ou  ./scripts/check-ci.sh --fix"
  fi
fi

step "[rust] cargo clippy --no-default-features -- -D warnings"
if (cd src-tauri && cargo clippy --no-default-features -- -D warnings); then
  ok "Clippy OK"
else
  fail "Clippy a relevé des avertissements/erreurs (voir ci-dessus)"
fi

step "[rust] cargo check --no-default-features"
if (cd src-tauri && cargo check --no-default-features); then
  ok "Check OK"
else
  fail "cargo check échoue"
fi

step "[rust] cargo test --no-default-features"
if (cd src-tauri && cargo test --no-default-features); then
  ok "Tests Rust OK"
else
  fail "Des tests Rust échouent"
fi

# ── Résumé ───────────────────────────────────────────────────────────────────
echo
if [[ "$FAILED" -eq 0 ]]; then
  echo "✅ Toutes les vérifications sont passées — identique à ce que fera le CI GitHub."
  exit 0
else
  echo "❌ Au moins une vérification a échoué — corrigez avant de pousser (le CI échouera sinon)."
  exit 1
fi
